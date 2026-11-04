use serde::Serialize;

use crate::{
    BackgroundSnapshot, Chord, ClockState, Command, GeneratedEvent, HarmonySnapshot, MarketContext,
    Mix, Mode, ParameterSnapshot, RenderConfig, Result, ReverbParameters, ReverbSettings,
    SpatialMode, Symbol, Unit,
    background::Background,
    dsp::{HighPass, LowPass, Smooth, limit_stereo},
    harmony::Harmony,
    reverb::SpaceReverb,
    sfx::SfxBank,
    sfx_eq::SfxEqFilter,
};

// Calibrate source buses before the user-controlled master. Layer controls
// remain linear 0..1, with normal listening levels near their default positions.
const ENVIRONMENT_GAIN: f32 = 4.0;
const EVENT_GAIN: f32 = 16.0;

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
    harmony: Harmony,
    market: MarketContext,
    market_smoothers: [Smooth; 5],
    mode: Mode,
    focus: Smooth,
    mix: Mix,
    mix_smoothers: [Smooth; 6],
    current_mix: Mix,
    master: Smooth,
    output_gain: Smooth,
    reverb: ReverbSettings,
    reverb_smoothers: [Smooth; 10],
    current_reverb: ReverbSettings,
    spatial: SpatialMode,
    environment_reverb: SpaceReverb,
    ui_reverb: SpaceReverb,
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
        let mut environment_reverb = SpaceReverb::new(config.sample_rate);
        configure_reverb(
            &mut environment_reverb,
            config.reverb.environment.room_size.get(),
            config.reverb.environment,
        );
        let mut ui_reverb = SpaceReverb::new(config.sample_rate);
        configure_reverb(
            &mut ui_reverb,
            config.reverb.events.room_size.get(),
            config.reverb.events,
        );
        let mut master = Smooth::new(0.0, 0.025, rate);
        master.target(config.mix.master.get());
        let harmony = Harmony::new(config.sample_rate, config.symbol.root());
        Ok(Self {
            symbol: config.symbol,
            config,
            sample: 0,
            background: Background::new(config.sample_rate, config.seed, harmony.chord),
            sfx: SfxBank::new(
                config.sample_rate,
                config.max_voices,
                config.seed ^ 0x5358_4658,
                config.spatial,
            ),
            sfx_eq: std::array::from_fn(|_| SfxEqFilter::new(config.sample_rate)),
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
            reverb_smoothers: reverb_values(config.reverb)
                .map(|v| Smooth::new(v, 0.12, control_rate)),
            current_reverb: config.reverb,
            spatial: config.spatial,
            environment_reverb,
            ui_reverb,
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
                if self.harmony.set_root(symbol.root()) {
                    self.background.chord(self.harmony.chord);
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
                for (smooth, target) in self
                    .reverb_smoothers
                    .iter_mut()
                    .zip(reverb_values(settings))
                {
                    smooth.target(target);
                }
            }
            Command::Spatial(spatial) => {
                self.spatial = spatial;
                self.sfx.set_spatial(spatial);
            }
            Command::Trigger(event) => {
                if let Some(generated) = self.sfx.trigger(event) {
                    self.last_event = Some(generated);
                }
            }
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
            self.background.chord(self.harmony.chord);
        }
        self.background
            .control(self.sample, market, focus, self.harmony.chord);
        let reverb_values = std::array::from_fn(|i| self.reverb_smoothers[i].tick());
        self.current_reverb = reverb_from_values(reverb_values);
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
        let levels: [f32; 6] = std::array::from_fn(|i| self.mix_smoothers[i].tick());
        self.current_mix = Mix {
            master: self.mix.master,
            output_gain: self.mix.output_gain,
            drone: Unit::clamped(levels[0]),
            texture: Unit::clamped(levels[1]),
            harmony: Unit::clamped(levels[2]),
            melody: Unit::clamped(levels[3]),
            ui: Unit::clamped(levels[4]),
            reverb: Unit::clamped(levels[5]),
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
        let background = self.background.tick(self.current_mix);
        let raw_ui = self.sfx.tick();
        let ui =
            std::array::from_fn::<_, 2, _>(|channel| self.sfx_eq[channel].tick(raw_ui[channel]));
        let focus = self.focus.value();
        let bg_send = self.current_reverb.environment.send.get();
        let ui_send = self.current_reverb.events.send.get();
        let bg_wet = self
            .environment_reverb
            .tick((background[0] * bg_send, background[1] * bg_send));
        let ui_wet = self.ui_reverb.tick((ui[0] * ui_send, ui[1] * ui_send));
        let wet = self.current_mix.reverb.get();
        let master = self.master.tick() * self.output_gain.tick();
        let bg_wet = [bg_wet.0, bg_wet.1];
        let ui_wet = [ui_wet.0, ui_wet.1];
        let frame = std::array::from_fn(|i| {
            let distant = background[i] * (0.9 - focus * 0.65 - self.zoom_amount * 0.2)
                + bg_wet[i] * wet * (0.5 + focus * 2.0 + self.zoom_amount * 0.35);
            let bg = self.lowpass[i].tick(self.highpass[i].tick(distant));
            let effect =
                (ui[i] + self.wet_lowpass[i].tick(ui_wet[i]) * wet) * self.current_mix.ui.get();
            self.output_highpass[i].tick((bg * ENVIRONMENT_GAIN + effect * EVENT_GAIN) * master)
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
            mode: self.mode,
            mix: self.mix,
            output_gain_db: 20.0 * self.output_gain.value().log10(),
            reverb: self.reverb,
            spatial: self.spatial,
            chord: self.harmony.chord,
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
                root: self.harmony.chord.root,
                root_transition: self.background.root_transition(),
                transition: self.background.chord_transition(),
                next: self.harmony.next_choice(market),
                clock: ClockState::at(
                    self.harmony.next_change,
                    self.sample,
                    self.config.sample_rate as f32,
                    focus > 0.4,
                ),
            },
            harmonic_swell: self.background.harmonic_swell(),
            details: self.background.details(self.sample),
            melody: self.background.melody(self.sample),
            detail_delay,
            melody_delay,
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

fn reverb_values(settings: ReverbSettings) -> [f32; 10] {
    let a = settings.environment;
    let b = settings.events;
    [
        a.room_size.get(),
        a.damping.get(),
        a.width.get(),
        a.wet.get(),
        a.send.get(),
        b.room_size.get(),
        b.damping.get(),
        b.width.get(),
        b.wet.get(),
        b.send.get(),
    ]
}

fn reverb_from_values(values: [f32; 10]) -> ReverbSettings {
    let [a, b, c, d, e, f, g, h, i, j] = values.map(Unit::clamped);
    ReverbSettings {
        environment: ReverbParameters {
            room_size: a,
            damping: b,
            width: c,
            wet: d,
            send: e,
        },
        events: ReverbParameters {
            room_size: f,
            damping: g,
            width: h,
            wet: i,
            send: j,
        },
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

fn mix_values(mix: Mix) -> [f32; 6] {
    [
        mix.drone.get(),
        mix.texture.get(),
        mix.harmony.get(),
        mix.melody.get(),
        mix.ui.get(),
        mix.reverb.get(),
    ]
}

fn reverb_width_scale(focus: f32) -> f32 {
    1.0 - 0.8 * focus
}
