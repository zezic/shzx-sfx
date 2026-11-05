use serde::Serialize;

mod rain;

use rain::RainSpace;

use crate::{
    BackgroundSnapshot, Chord, Command, GeneratedEvent, HarmonySnapshot, MarketContext, Mix, Mode,
    ParameterSnapshot, RenderConfig, Result, ReverbParameters, ReverbSettings, SpatialMode, Symbol,
    TransmissionSnapshot, Unit,
    background::Background,
    dsp::{HighPass, LowPass, Smooth, limit_stereo},
    harmony::Harmony,
    reverb::SpaceReverb,
    sfx::SfxBank,
    sfx_eq::SfxEqFilter,
    transmission::Transmission,
};

// Calibrate source buses before the user-controlled master. Layer controls
// remain linear 0..1, with normal listening levels near their default positions.
const ENVIRONMENT_GAIN: f32 = 4.0;
const EVENT_GAIN: f32 = 16.0;
const TRANSMISSION_GAIN: f32 = 256.0;
const MELODY_DRY_GAIN: f32 = 0.025;

/// A cheap copy of the current synthesis state. Levels are post-master linear
/// peak and RMS since startup; event parameters describe the last admitted voice.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Snapshot {
    pub symbol: Symbol,
    pub time_seconds: f64,
    pub sample_rate: u32,
    pub seed: u64,
    pub market: MarketContext,
    pub background: BackgroundSnapshot,
    pub transmission: TransmissionSnapshot,
    /// Frequency multiplier of the one-second startup glide.
    pub startup_pitch: Unit,
    pub mode: Mode,
    pub mix: Mix,
    pub output_gain_db: f32,
    pub reverb: ReverbSettings,
    pub spatial: SpatialMode,
    pub chord: Chord,
    pub cutoff_hz: f32,
    pub focus_amount: f32,
    pub zoom_amount: f32,
    pub active_voices: usize,
    pub generated_events: u64,
    pub stolen_voices: u64,
    pub coalesced_events: u64,
    pub last_event: Option<GeneratedEvent>,
    pub peak: f32,
    pub rms: f32,
}

/// Device-independent stereo synthesis. All memory is reserved in [`Self::new`].
/// Rendering and typed command application perform bounded work without locks,
/// allocations, logging, or disk access.
pub struct Renderer {
    symbol: Symbol,
    config: RenderConfig,
    sample: u64,
    background: Background,
    sfx: SfxBank,
    sfx_eq: [SfxEqFilter; 2],
    transmission: Transmission,
    harmony: Harmony,
    market: MarketContext,
    market_smoothers: [Smooth; 5],
    mode: Mode,
    focus: Smooth,
    mix: Mix,
    mix_smoothers: [Smooth; 13],
    current_mix: Mix,
    master: Smooth,
    output_gain: Smooth,
    reverb: ReverbSettings,
    reverb_smoothers: [ReverbSmoothers; 6],
    current_reverb: ReverbSettings,
    spatial: SpatialMode,
    environment_reverb: SpaceReverb,
    ui_reverb: SpaceReverb,
    transmission_reverb: SpaceReverb,
    melody_reverb: SpaceReverb,
    accent_reverb: SpaceReverb,
    rain: RainSpace,
    lowpass: [LowPass; 2],
    highpass: [HighPass; 2],
    wet_lowpass: [LowPass; 2],
    output_highpass: [HighPass; 2],
    cutoff_hz: f32,
    zoom_amount: f32,
    last_event: Option<GeneratedEvent>,
    peak: f32,
    sum_squares: f64,
}

impl Renderer {
    pub fn new(config: RenderConfig) -> Result<Self> {
        let config = config.validate()?;
        let rate = config.sample_rate as f32;
        let control_rate = rate / 64.0;
        let market = MarketContext::default();
        let environment_reverb = new_reverb(config.sample_rate, config.reverb.environment);
        let ui_reverb = new_reverb(config.sample_rate, config.reverb.events);
        let transmission_reverb = new_reverb(config.sample_rate, config.reverb.transmission);
        let melody_reverb = new_reverb(config.sample_rate, config.reverb.melody);
        let accent_reverb = new_reverb(config.sample_rate, config.reverb.accents);
        let mut master = Smooth::new(0.0, 0.025, rate);
        master.target(config.mix.master.get());
        let harmony = Harmony::new(
            config.sample_rate,
            Chord::from_symbol(config.symbol, config.harmony_mode),
        );
        Ok(Self {
            symbol: config.symbol,
            config,
            sample: 0,
            background: Background::new(config.sample_rate, config.seed, harmony.chord()),
            sfx: SfxBank::new(
                config.sample_rate,
                config.max_voices,
                config.seed ^ 0x5358_4658,
                config.spatial,
            ),
            sfx_eq: std::array::from_fn(|_| SfxEqFilter::new(config.sample_rate)),
            transmission: Transmission::new(
                config.sample_rate,
                config.seed ^ 0x4441_5441,
                config.spatial,
            ),
            harmony,
            market,
            market_smoothers: market_values(market).map(|v| Smooth::new(v, 0.4, control_rate)),
            mode: Mode::Browse,
            focus: Smooth::new(0.0, 0.55, control_rate),
            mix: config.mix,
            mix_smoothers: mix_values(config.mix).map(|v| Smooth::new(v, 0.04, control_rate)),
            current_mix: config.mix,
            master,
            output_gain: Smooth::new(config.mix.output_gain.amplitude(), 0.025, rate),
            reverb: config.reverb,
            reverb_smoothers: reverb_buses(config.reverb)
                .map(|parameters| ReverbSmoothers::new(parameters, control_rate)),
            current_reverb: config.reverb,
            spatial: config.spatial,
            environment_reverb,
            ui_reverb,
            transmission_reverb,
            melody_reverb,
            accent_reverb,
            rain: RainSpace::new(config.sample_rate, config.reverb.rain),
            lowpass: std::array::from_fn(|_| LowPass::new(rate, 1400.0)),
            highpass: std::array::from_fn(|_| HighPass::new(rate, 32.0)),
            wet_lowpass: std::array::from_fn(|_| LowPass::new(rate, 1800.0)),
            output_highpass: std::array::from_fn(|_| HighPass::new(rate, 22.0)),
            cutoff_hz: 1400.0,
            zoom_amount: 0.0,
            last_event: None,
            peak: 0.0,
            sum_squares: 0.0,
        })
    }

    pub fn apply(&mut self, command: Command) {
        match command {
            Command::Symbol(symbol) => {
                self.symbol = symbol;
                if self.harmony.set_symbol(symbol) {
                    self.background.chord(self.harmony.chord());
                }
            }
            Command::Harmony(mode) => {
                if self
                    .harmony
                    .set_mode(mode, self.symbol, self.config.sample_rate, self.sample)
                {
                    self.background.chord(self.harmony.chord());
                }
            }
            Command::Market(market) => {
                self.market = market;
                for (smooth, target) in self.market_smoothers.iter_mut().zip(market_values(market))
                {
                    smooth.target(target);
                }
            }
            Command::Mode(mode) => {
                self.mode = mode;
                self.focus.target(mode_focus(mode));
            }
            Command::Mix(mix) => {
                self.mix = mix;
                self.master.target(mix.master.get());
                self.output_gain.target(mix.output_gain.amplitude());
                for (smooth, target) in self.mix_smoothers.iter_mut().zip(mix_values(mix)) {
                    smooth.target(target);
                }
            }
            Command::Reverb(settings) => {
                self.reverb = settings;
                for (smooth, target) in self.reverb_smoothers.iter_mut().zip(reverb_buses(settings))
                {
                    smooth.target(target);
                }
            }
            Command::Spatial(spatial) => {
                self.spatial = spatial;
                self.sfx.set_spatial(spatial);
                self.transmission.set_spatial(spatial);
            }
            Command::Trigger(event) => {
                if let Some(generated) = self.sfx.trigger(event) {
                    self.last_event = Some(generated);
                }
            }
            Command::DataTransmission(command) => self.transmission.apply(command),
        }
    }

    fn control(&mut self) {
        let values: [f32; 5] = std::array::from_fn(|i| self.market_smoothers[i].tick());
        let market = MarketContext {
            trend: self.market.trend,
            trend_strength: Unit::clamped(values[0]),
            volume: Unit::clamped(values[1]),
            volatility: Unit::clamped(values[2]),
            zoom: Unit::clamped(values[3]),
            activity: Unit::clamped(values[4]),
        };
        let focus = self.focus.tick();
        if self
            .harmony
            .update(self.sample, self.config.sample_rate, market, focus > 0.4)
        {
            self.background.chord(self.harmony.chord());
        }
        self.background
            .control(self.sample, market, focus, self.symbol);
        let [environment, events, transmission, melody, accents, rain] =
            self.reverb_smoothers.each_mut().map(|bus| bus.tick());
        self.current_reverb = ReverbSettings {
            environment,
            events,
            transmission,
            melody,
            accents,
            rain,
        };
        let parameters = environment_parameters(
            market.zoom.get(),
            focus,
            self.current_reverb.environment.room_size.get(),
        );
        self.cutoff_hz = parameters.lowpass_hz;
        for filter in &mut self.lowpass {
            filter.set_cutoff(self.cutoff_hz);
        }
        for filter in &mut self.highpass {
            filter.set_cutoff(parameters.highpass_hz);
        }
        self.zoom_amount = market.zoom.get();
        let width_scale = reverb_width_scale(focus);
        let mut environment = self.current_reverb.environment;
        environment.width = Unit::clamped(environment.width.get() * width_scale);
        let mut events = self.current_reverb.events;
        events.width = Unit::clamped(events.width.get() * width_scale);
        configure_reverb(
            &mut self.environment_reverb,
            parameters.reverb_room,
            environment,
        );
        configure_reverb(
            &mut self.ui_reverb,
            self.current_reverb.events.room_size.get(),
            events,
        );
        configure_reverb(
            &mut self.transmission_reverb,
            self.current_reverb.transmission.room_size.get(),
            self.current_reverb.transmission,
        );
        let mut melody = self.current_reverb.melody;
        melody.width = Unit::clamped(melody.width.get() * width_scale);
        configure_reverb(&mut self.melody_reverb, melody.room_size.get(), melody);
        let mut accents = self.current_reverb.accents;
        accents.width = Unit::clamped(accents.width.get() * width_scale);
        configure_reverb(&mut self.accent_reverb, accents.room_size.get(), accents);
        self.rain
            .control(market.zoom.get(), focus, self.current_reverb.rain);
        let levels: [f32; 13] = std::array::from_fn(|i| self.mix_smoothers[i].tick());
        self.current_mix = Mix {
            master: self.mix.master,
            output_gain: self.mix.output_gain,
            drone: Unit::clamped(levels[0]),
            texture: Unit::clamped(levels[1]),
            harmony: Unit::clamped(levels[2]),
            melody: Unit::clamped(levels[3]),
            ui: Unit::clamped(levels[4]),
            reverb: Unit::clamped(levels[5]),
            sub: Unit::clamped(levels[6]),
            transmission: Unit::clamped(levels[7]),
            demonic_voices: Unit::clamped(levels[8]),
            demonic_stutters: Unit::clamped(levels[9]),
            flying_coins: Unit::clamped(levels[10]),
            laser_beams: Unit::clamped(levels[11]),
            rain: Unit::clamped(levels[12]),
        };
    }

    /// Generate a block. Control-rate updates follow the sample clock so changing
    /// block sizes does not change the generated arrangement or random choices.
    pub fn render(&mut self, output: &mut [[f32; 2]]) {
        for frame in output {
            *frame = self.next_frame();
        }
    }

    #[inline]
    pub fn next_frame(&mut self) -> [f32; 2] {
        if self.sample.is_multiple_of(64) {
            self.control();
        }
        let pitch_ratio = self.startup_pitch();
        let background = self.background.tick(self.current_mix, pitch_ratio);
        let melody = background.melody;
        let accents = background.accents;
        let rain = self.rain.tick(
            background.rain,
            self.current_mix.rain.get(),
            self.current_reverb.rain.send.get(),
            self.current_mix.reverb.get(),
        );
        let background = background.environment;
        let raw_ui = self.sfx.tick_scaled(pitch_ratio);
        let transmission = self.transmission.tick(pitch_ratio);
        let ui =
            std::array::from_fn::<_, 2, _>(|channel| self.sfx_eq[channel].tick(raw_ui[channel]));
        let focus = self.focus.value();
        let bg_send = self.current_reverb.environment.send.get();
        let ui_send = self.current_reverb.events.send.get();
        let bg_wet = self
            .environment_reverb
            .tick((background[0] * bg_send, background[1] * bg_send));
        let accent_send = self.current_reverb.accents.send.get();
        let accent_wet = self
            .accent_reverb
            .tick((accents[0] * accent_send, accents[1] * accent_send));
        let ui_wet = self.ui_reverb.tick((ui[0] * ui_send, ui[1] * ui_send));
        let transmission_send = self.current_reverb.transmission.send.get();
        let transmission_wet = self.transmission_reverb.tick((
            transmission[0] * transmission_send,
            transmission[1] * transmission_send,
        ));
        let melody_send = self.current_reverb.melody.send.get();
        let melody_wet = self
            .melody_reverb
            .tick((melody[0] * melody_send, melody[1] * melody_send));
        let wet = self.current_mix.reverb.get();
        let master = self.master.tick() * self.output_gain.tick();
        let bg_wet = [bg_wet.0, bg_wet.1];
        let ui_wet = [ui_wet.0, ui_wet.1];
        let transmission_wet = [transmission_wet.0, transmission_wet.1];
        let melody_wet = [melody_wet.0, melody_wet.1];
        let accent_wet = [accent_wet.0, accent_wet.1];
        let frame = std::array::from_fn(|i| {
            let distant = background[i] * (0.9 - focus * 0.65 - self.zoom_amount * 0.2)
                + bg_wet[i] * wet * (0.5 + focus * 2.0 + self.zoom_amount * 0.35)
                + melody[i] * MELODY_DRY_GAIN
                + melody_wet[i] * wet;
            // High-register accents have source band-limiting and a brighter
            // space. The scene's low-pass continues to darken the bed.
            let bg = self.lowpass[i].tick(self.highpass[i].tick(distant))
                + rain[i]
                + accents[i] * (0.9 - focus * 0.65 - self.zoom_amount * 0.2)
                + accent_wet[i] * wet;
            let effect =
                (ui[i] + self.wet_lowpass[i].tick(ui_wet[i]) * wet) * self.current_mix.ui.get();
            let data = (transmission[i] * 0.08 + transmission_wet[i] * wet)
                * self.current_mix.transmission.get();
            self.output_highpass[i].tick(
                (bg * ENVIRONMENT_GAIN + effect * EVENT_GAIN + data * TRANSMISSION_GAIN) * master,
            )
        });
        let out = limit_stereo(frame);
        for value in out {
            self.peak = self.peak.max(value.abs());
            self.sum_squares += (value as f64).powi(2);
        }
        self.sample += 1;
        out
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            symbol: self.symbol,
            time_seconds: self.sample as f64 / self.config.sample_rate as f64,
            sample_rate: self.config.sample_rate,
            seed: self.config.seed,
            market: self.market,
            background: self.background_snapshot(),
            transmission: self.transmission.snapshot(),
            startup_pitch: Unit::clamped(self.startup_pitch()),
            mode: self.mode,
            mix: self.mix,
            output_gain_db: 20.0 * self.output_gain.value().log10(),
            reverb: self.reverb,
            spatial: self.spatial,
            chord: self.harmony.chord(),
            cutoff_hz: self.cutoff_hz,
            focus_amount: self.focus.value(),
            zoom_amount: self.zoom_amount,
            active_voices: self.sfx.active_voices(),
            generated_events: self.sfx.generated_events(),
            stolen_voices: self.sfx.stolen_voices(),
            coalesced_events: self.sfx.coalesced_events(),
            last_event: self.last_event,
            peak: self.peak,
            rms: if self.sample == 0 {
                0.0
            } else {
                (self.sum_squares / (self.sample as f64 * 2.0)).sqrt() as f32
            },
        }
    }

    fn startup_pitch(&self) -> f32 {
        if self.sample >= u64::from(self.config.sample_rate) {
            return 1.0;
        }
        let progress = self.sample as f64 / f64::from(self.config.sample_rate);
        (progress * progress * (3.0 - 2.0 * progress)) as f32
    }

    fn background_snapshot(&self) -> BackgroundSnapshot {
        let focus = self.focus.value();
        let target_focus = mode_focus(self.mode);
        let current = environment_parameters(
            self.zoom_amount,
            focus,
            self.current_reverb.environment.room_size.get(),
        );
        let target = environment_parameters(
            self.market.zoom.get(),
            target_focus,
            self.reverb.environment.room_size.get(),
        );
        let market = self.background.market();
        let [detail_delay, melody_delay] =
            self.background.delay_snapshots(self.market, target_focus);
        let [demonic_voices, demonic_stutters, flying_coins, laser_beams] =
            self.background.accents(self.sample);
        BackgroundSnapshot {
            market,
            focus: ParameterSnapshot {
                current: focus,
                target: target_focus,
            },
            lowpass_hz: ParameterSnapshot {
                current: self.cutoff_hz,
                target: target.lowpass_hz,
            },
            highpass_hz: ParameterSnapshot {
                current: current.highpass_hz,
                target: target.highpass_hz,
            },
            reverb_room: ParameterSnapshot {
                current: current.reverb_room,
                target: target.reverb_room,
            },
            reverb_width: ParameterSnapshot {
                current: self.current_reverb.environment.width.get() * reverb_width_scale(focus),
                target: self.reverb.environment.width.get() * reverb_width_scale(target_focus),
            },
            harmony: HarmonySnapshot {
                mode: self.harmony.chord().mode(),
                root: self.harmony.chord().root(),
                root_transition: self.background.root_transition(),
                transition: self.background.chord_transition(),
                next: self.harmony.next_choice(market),
                clock: self
                    .harmony
                    .clock(self.sample, self.config.sample_rate, focus > 0.4),
            },
            harmonic_swell: self.background.harmonic_swell(),
            details: self.background.details(self.sample),
            melody: self.background.melody(self.sample),
            detail_delay,
            melody_delay,
            rain: self.rain.snapshot(
                self.zoom_amount,
                focus,
                self.market.zoom.get(),
                target_focus,
            ),
            demonic_voices,
            demonic_stutters,
            flying_coins,
            laser_beams,
        }
    }
}

struct EnvironmentParameters {
    lowpass_hz: f32,
    highpass_hz: f32,
    reverb_room: f32,
}

fn environment_parameters(zoom: f32, focus: f32, room: f32) -> EnvironmentParameters {
    EnvironmentParameters {
        lowpass_hz: (1400.0 - zoom * 1120.0) * (1.0 - focus) + 650.0 * focus,
        highpass_hz: 32.0 + focus * 288.0,
        reverb_room: room + (1.0 - room) * (focus * 0.8 + zoom * 0.2),
    }
}

fn configure_reverb(reverb: &mut SpaceReverb, room: f32, parameters: ReverbParameters) {
    reverb.set_dry(0.0);
    reverb.set_room_size(room);
    reverb.set_dampening(parameters.damping.get());
    reverb.set_width(parameters.width.get());
    reverb.set_wet(parameters.wet.get());
}

fn new_reverb(sample_rate: u32, parameters: ReverbParameters) -> SpaceReverb {
    let mut reverb = SpaceReverb::new(sample_rate);
    configure_reverb(&mut reverb, parameters.room_size.get(), parameters);
    reverb
}

fn reverb_buses(settings: ReverbSettings) -> [ReverbParameters; 6] {
    [
        settings.environment,
        settings.events,
        settings.transmission,
        settings.melody,
        settings.accents,
        settings.rain,
    ]
}

struct ReverbSmoothers([Smooth; 5]);

impl ReverbSmoothers {
    fn values(parameters: ReverbParameters) -> [f32; 5] {
        [
            parameters.room_size.get(),
            parameters.damping.get(),
            parameters.width.get(),
            parameters.wet.get(),
            parameters.send.get(),
        ]
    }

    fn new(parameters: ReverbParameters, rate: f32) -> Self {
        Self(Self::values(parameters).map(|value| Smooth::new(value, 0.12, rate)))
    }

    fn target(&mut self, parameters: ReverbParameters) {
        for (smoother, value) in self.0.iter_mut().zip(Self::values(parameters)) {
            smoother.target(value);
        }
    }

    fn tick(&mut self) -> ReverbParameters {
        let [room_size, damping, width, wet, send] =
            self.0.each_mut().map(|value| Unit::clamped(value.tick()));
        ReverbParameters {
            room_size,
            damping,
            width,
            wet,
            send,
        }
    }
}

fn mode_focus(mode: Mode) -> f32 {
    match mode {
        Mode::Browse => 0.0,
        Mode::Dialog => 1.0,
        Mode::Focus => 0.65,
    }
}

fn market_values(market: MarketContext) -> [f32; 5] {
    [
        market.trend_strength.get(),
        market.volume.get(),
        market.volatility.get(),
        market.zoom.get(),
        market.activity.get(),
    ]
}

fn mix_values(mix: Mix) -> [f32; 13] {
    [
        mix.drone.get(),
        mix.texture.get(),
        mix.harmony.get(),
        mix.melody.get(),
        mix.ui.get(),
        mix.reverb.get(),
        mix.sub.get(),
        mix.transmission.get(),
        mix.demonic_voices.get(),
        mix.demonic_stutters.get(),
        mix.flying_coins.get(),
        mix.laser_beams.get(),
        mix.rain.get(),
    ]
}

fn reverb_width_scale(focus: f32) -> f32 {
    1.0 - 0.8 * focus
}
