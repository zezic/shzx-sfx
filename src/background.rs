use fundsp::hacker32::{AudioNode, AudioUnit, Biquad, BiquadCoefs, chorus};

use crate::{
    AmbientSnapshot, BackgroundVoice, Chord, ChordTransition, ClockState, DelaySnapshot,
    DetailSnapshot, MarketContext, MelodySnapshot, Mix, RootTransition, Symbol, Unit,
    dsp::{Echo, EchoParameters, LowPass, Osc, SineTable},
    harmony::midi_hz,
};

mod tones;
mod voices;

use tones::{ToneFormation, ToneKind};
use voices::DemonicVoices;

struct ScheduledTone {
    sound: ToneFormation,
    next: u64,
    generated: u64,
}

impl ScheduledTone {
    fn new(rate: u32, kind: ToneKind, first_seconds: u64) -> Self {
        Self {
            sound: ToneFormation::new(rate, kind),
            next: u64::from(rate) * first_seconds,
            generated: 0,
        }
    }

    fn control(&mut self, sample: u64, rate: f32, focus: f32, rng: &mut fastrand::Rng) {
        if sample >= self.next {
            self.next = sample + ((16.0 + rng.f32() * 18.0) * rate) as u64;
            if focus <= 0.7 && !self.sound.is_active() {
                self.sound.start(rng);
                self.generated += u64::from(self.sound.is_active());
            }
        }
    }

    fn snapshot(&self, sample: u64, rate: f32, focus: f32) -> AmbientSnapshot {
        AmbientSnapshot {
            generated: self.generated,
            active: usize::from(self.sound.is_active()),
            clock: ClockState::at(self.next, sample, rate, focus > 0.7),
        }
    }
}

const UNISON: usize = 12;
const SUB_UNISON: usize = 16;
const MELODY_ATTACK_SECONDS: f32 = 3.0;
const MELODY_DECAY_SECONDS: f32 = 8.0;
const MELODY_DURATION_SECONDS: f32 = MELODY_ATTACK_SECONDS + MELODY_DECAY_SECONDS;

fn melody_envelope(seconds: f32) -> f32 {
    let progress = if seconds < MELODY_ATTACK_SECONDS {
        seconds / MELODY_ATTACK_SECONDS
    } else {
        (MELODY_DURATION_SECONDS - seconds) / MELODY_DECAY_SECONDS
    };
    progress * progress * (3.0 - 2.0 * progress)
}

fn melody_vibrato(table: &SineTable, seconds: f32) -> f32 {
    1.0 + 0.025 * table.at((seconds * 4.3).fract())
}

/// Each note makes a smooth excursion, then rests before choosing a new one.
enum PitchDrift {
    Resting {
        until: u64,
    },
    Excursion {
        start: u64,
        duration: u64,
        semitones: f32,
    },
}

impl PitchDrift {
    fn new(rate: f32, rng: &mut fastrand::Rng) -> Self {
        Self::Resting {
            until: ((12.0 + rng.f32() * 24.0) * rate) as u64,
        }
    }

    fn semitones(&mut self, sample: u64, rate: f32, rng: &mut fastrand::Rng) -> f32 {
        match *self {
            Self::Resting { until } if sample >= until => {
                *self = Self::Excursion {
                    start: sample,
                    duration: ((6.0 + rng.f32() * 6.0) * rate) as u64,
                    semitones: rng.f32() - 0.5,
                };
                0.0
            }
            Self::Resting { .. } => 0.0,
            Self::Excursion {
                start,
                duration,
                semitones,
            } => {
                let elapsed = sample - start;
                if elapsed >= duration {
                    *self = Self::Resting {
                        until: sample + ((18.0 + rng.f32() * 32.0) * rate) as u64,
                    };
                    0.0
                } else {
                    let phase = elapsed as f32 / duration as f32;
                    semitones * (std::f32::consts::PI * phase).sin().powi(2)
                }
            }
        }
    }
}

#[derive(Clone)]
struct SubLayer {
    oscillator: Osc,
    pan: [f32; 2],
}

#[derive(Clone)]
struct SubBank {
    layers: [SubLayer; SUB_UNISON],
    lowpass: [LowPass; 2],
    root_bell: [Biquad<f32>; 2],
}

const SUB_BANK_CUTOFF: f32 = 400.0;

impl SubBank {
    fn new(chord: Chord, rate: f32, rng: &mut fastrand::Rng) -> Self {
        let root_hz = midi_hz(chord.pedal_note() as f32);
        Self {
            layers: std::array::from_fn(|i| {
                let angle = sub_position(i) * std::f32::consts::FRAC_PI_2;
                SubLayer {
                    oscillator: Osc::new(sub_frequency(root_hz, i), rate, rng.f32()),
                    pan: [angle.cos(), angle.sin()],
                }
            }),
            lowpass: std::array::from_fn(|_| LowPass::resonant(rate, SUB_BANK_CUTOFF, 3.0)),
            root_bell: std::array::from_fn(|_| Biquad::with_coefs(sub_bell(rate, root_hz))),
        }
    }

    fn tune(&mut self, chord: Chord, rate: f32) {
        let root_hz = midi_hz(chord.pedal_note() as f32);
        for (index, layer) in self.layers.iter_mut().enumerate() {
            layer
                .oscillator
                .frequency(sub_frequency(root_hz, index), rate);
        }
        for filter in &mut self.lowpass {
            filter.set_cutoff(SUB_BANK_CUTOFF);
        }
        for filter in &mut self.root_bell {
            filter.set_coefs(sub_bell(rate, root_hz));
        }
    }

    fn tick(&mut self, pitch_ratio: f32) -> [f32; 2] {
        let mut output = [0.0; 2];
        for layer in &mut self.layers {
            let value =
                layer.oscillator.saw_scaled(pitch_ratio) * 0.12 * pitch_ratio / SUB_UNISON as f32;
            output[0] += value * layer.pan[0];
            output[1] += value * layer.pan[1];
        }
        std::array::from_fn(|channel| {
            self.root_bell[channel].tick(&[self.lowpass[channel].tick(output[channel])].into())[0]
        })
    }
}

fn sub_position(index: usize) -> f32 {
    // Interleave detuning and panorama so both sides contain the whole spread.
    (index * 7 % SUB_UNISON) as f32 / (SUB_UNISON - 1) as f32
}

fn sub_frequency(root_hz: f32, index: usize) -> f32 {
    let semitones = (index as f32 / (SUB_UNISON - 1) as f32 * 2.0 - 1.0) * 0.24;
    root_hz * 0.5 * 2.0_f32.powf(semitones / 12.0)
}

fn sub_bell(rate: f32, root_hz: f32) -> BiquadCoefs<f32> {
    BiquadCoefs::bell(rate, root_hz, 1.3, 10.0_f32.powf(-15.0 / 20.0))
}

struct PadBank {
    oscillators: [Osc; UNISON],
    chord: Chord,
}

impl PadBank {
    fn frequencies(chord: Chord) -> [f32; UNISON] {
        let notes = chord.notes();
        std::array::from_fn(|i| midi_hz(f32::from(notes[i / 3]) + [-0.055, 0.0, 0.055][i % 3]))
    }

    fn new(chord: Chord, rate: f32, rng: &mut fastrand::Rng) -> Self {
        Self {
            oscillators: Self::frequencies(chord).map(|hz| Osc::new(hz, rate, rng.f32())),
            chord,
        }
    }

    fn tune(&mut self, chord: Chord, rate: f32) {
        for (osc, hz) in self.oscillators.iter_mut().zip(Self::frequencies(chord)) {
            osc.frequency(hz, rate);
        }
        self.chord = chord;
    }

    fn tick(
        &mut self,
        table: &SineTable,
        density: f32,
        pitch_ratio: f32,
        drift_ratios: [f32; 4],
    ) -> [f32; 2] {
        let mut out = [0.0; 2];
        for (i, osc) in self.oscillators.iter_mut().enumerate() {
            let tone_gain = [1.0, 0.85, 0.35 + density * 0.65, 0.1 + density * 0.9][i / 3];
            let value =
                osc.tick_scaled(table, pitch_ratio * drift_ratios[i / 3]) * tone_gain * pitch_ratio
                    / UNISON as f32;
            let width = [0.7, 0.5, 0.3][i % 3];
            out[0] += value * width;
            out[1] += value * (1.0 - width);
        }
        out
    }
}

pub(crate) struct BackgroundFrame {
    pub environment: [f32; 2],
    pub melody: [f32; 2],
    pub accents: [f32; 2],
}

pub(crate) struct Background {
    rate: f32,
    table: SineTable,
    rng: fastrand::Rng,
    pads: [PadBank; 2],
    current_pad: usize,
    transition: f32,
    pending_chord: Option<Chord>,
    pedals: [[Osc; 3]; 2],
    subs: [SubBank; 2],
    drift: [PitchDrift; 4],
    drift_ratios: [f32; 4],
    drift_rng: fastrand::Rng,
    chorus: [Box<dyn AudioUnit>; 2],
    noise_filter: [LowPass; 2],
    detail_filter: [LowPass; 2],
    melody_filter: [LowPass; 2],
    detail_echo: Echo,
    melody_echo: Echo,
    slow: [Osc; 3],
    slow_values: [f32; 3],
    next_detail: u64,
    detail_generated: u64,
    detail_left: u32,
    detail_amplitude: f32,
    detail_decay: f32,
    detail_pan: f32,
    detail_osc: Osc,
    next_melody: u64,
    melody_generated: u64,
    melody: Osc,
    melody_age: Option<u32>,
    melody_pan: f32,
    market: MarketContext,
    focus: f32,
    harmonic_swell: f32,
    voices: DemonicVoices,
    coins: ScheduledTone,
    lasers: ScheduledTone,
    accent_rng: fastrand::Rng,
}

impl Background {
    pub fn new(rate: u32, seed: u64, chord: Chord) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed);
        let rate_f = rate as f32;
        let mut sub_rng = fastrand::Rng::with_seed(seed ^ 0x5355_4254_4f4e_4500);
        let sub = SubBank::new(chord, rate_f, &mut sub_rng);
        let mut drift_rng = fastrand::Rng::with_seed(seed ^ 0x4452_4946_5400_0000);
        let drift = std::array::from_fn(|_| PitchDrift::new(rate_f, &mut drift_rng));
        let [detail_delay, melody_delay] = delay_parameters(MarketContext::default(), 0.0);
        let mut chorus_nodes: [Box<dyn AudioUnit>; 2] = [
            Box::new(chorus(seed, 0.012, 0.004, 0.11)),
            Box::new(chorus(seed.wrapping_add(1), 0.015, 0.004, 0.14)),
        ];
        for node in &mut chorus_nodes {
            node.set_sample_rate(rate as f64);
            node.allocate();
        }
        Self {
            rate: rate_f,
            table: SineTable::new(),
            pads: std::array::from_fn(|_| PadBank::new(chord, rate_f, &mut rng)),
            current_pad: 0,
            transition: 1.0,
            pending_chord: None,
            pedals: [pedal_oscillators(chord, rate_f); 2],
            subs: [sub.clone(), sub],
            drift,
            drift_ratios: [1.0; 4],
            drift_rng,
            chorus: chorus_nodes,
            noise_filter: std::array::from_fn(|_| LowPass::new(rate_f, 480.0)),
            detail_filter: std::array::from_fn(|_| LowPass::new(rate_f, 1900.0)),
            melody_filter: std::array::from_fn(|_| LowPass::new(rate_f, 950.0)),
            detail_echo: Echo::new(rate, detail_delay, 1_650.0),
            melody_echo: Echo::new(rate, melody_delay, 1_150.0),
            slow: [
                Osc::new(0.037, rate_f / 64.0, rng.f32()),
                Osc::new(0.021, rate_f / 64.0, rng.f32()),
                Osc::new(0.081, rate_f / 64.0, rng.f32()),
            ],
            slow_values: [0.0; 3],
            next_detail: rate as u64 * 2,
            detail_generated: 0,
            detail_left: 0,
            detail_amplitude: 0.0,
            detail_decay: 0.999_f32.powf(48_000.0 / rate_f),
            detail_pan: 0.5,
            detail_osc: Osc::new(420.0, rate_f, 0.0),
            next_melody: rate as u64 * 7,
            melody_generated: 0,
            melody: Osc::new(440.0, rate_f, 0.0),
            melody_age: None,
            melody_pan: 0.5,
            market: MarketContext::default(),
            focus: 0.0,
            harmonic_swell: 0.0,
            voices: DemonicVoices::new(rate, seed ^ 0x564f_4943_4553),
            coins: ScheduledTone::new(rate, ToneKind::FlyingCoins, 4),
            lasers: ScheduledTone::new(rate, ToneKind::LaserBeams, 12),
            accent_rng: fastrand::Rng::with_seed(seed ^ 0x4143_4345_4e54),
            rng,
        }
    }

    pub fn chord(&mut self, chord: Chord) {
        if self.transition < 1.0 {
            self.pending_chord = Some(chord);
            return;
        }
        self.start_chord(chord);
    }

    fn start_chord(&mut self, chord: Chord) {
        let previous = self.current_pad;
        self.current_pad = 1 - self.current_pad;
        self.pads[self.current_pad].tune(chord, self.rate);
        self.pedals[self.current_pad] = self.pedals[previous];
        self.subs[self.current_pad] = self.subs[previous].clone();
        self.subs[self.current_pad].tune(chord, self.rate);
        let frequency = midi_hz(chord.pedal_note() as f32);
        for (oscillator, hz) in self.pedals[self.current_pad].iter_mut().zip([
            frequency,
            frequency + 0.07,
            frequency * 2.0,
        ]) {
            oscillator.frequency(hz, self.rate);
        }
        self.transition = 0.0;
    }

    /// Run at a fixed 64-frame control rate, independent of device block size.
    pub fn control(&mut self, sample: u64, market: MarketContext, focus: f32, symbol: Symbol) {
        self.market = market;
        self.focus = focus;
        self.voices.control(sample, symbol, focus);
        self.coins
            .control(sample, self.rate, focus, &mut self.accent_rng);
        self.lasers
            .control(sample, self.rate, focus, &mut self.accent_rng);
        self.drift_ratios = std::array::from_fn(|i| {
            let semitones = self.drift[i].semitones(sample, self.rate, &mut self.drift_rng);
            2.0_f32.powf(semitones / 12.0)
        });
        let [detail_delay, melody_delay] = delay_parameters(market, focus);
        self.detail_echo.target(detail_delay);
        self.melody_echo.target(melody_delay);
        self.slow_values = std::array::from_fn(|i| self.slow[i].tick(&self.table));
        self.harmonic_swell = (0.5 + 0.5 * self.slow_values[0]).powi(2);
        let noise_cutoff = 260.0 + 220.0 * (0.5 + self.slow_values[1] * 0.5);
        for filter in &mut self.noise_filter {
            filter.set_cutoff(noise_cutoff);
        }
        for filter in &mut self.detail_filter {
            filter.set_cutoff(1100.0 + 1000.0 * market.volatility.get());
        }
        if sample >= self.next_detail {
            self.detail_generated += 1;
            let density =
                0.3 + market.volume.get() + market.activity.get() * 1.5 + market.zoom.get();
            let wait = (0.6 + self.rng.f32() * 3.5) / density;
            self.next_detail = sample + (wait * self.rate) as u64;
            self.detail_left = ((0.012 + self.rng.f32() * 0.08) * self.rate) as u32;
            self.detail_amplitude = 0.032 + 0.08 * self.rng.f32();
            self.detail_pan = 0.15 + self.rng.f32() * 0.7;
            self.detail_osc
                .frequency(190.0 + self.rng.f32() * 940.0, self.rate);
        }
        if sample >= self.next_melody {
            self.next_melody =
                sample + ((MELODY_DURATION_SECONDS + self.rng.f32() * 18.0) * self.rate) as u64;
            if focus < 0.3 {
                self.melody_generated += 1;
                let note = f32::from(self.rng.u8(72..84));
                self.melody.frequency(midi_hz(note), self.rate);
                self.melody_age = Some(0);
                self.melody_pan = 0.25 + self.rng.f32() * 0.5;
            }
        }
    }

    pub fn market(&self) -> MarketContext {
        self.market
    }

    pub fn harmonic_swell(&self) -> Unit {
        Unit::clamped(self.harmonic_swell)
    }

    pub fn accents(&self, sample: u64) -> [AmbientSnapshot; 4] {
        let [voices, stutters] = self.voices.snapshots(sample, self.focus);
        [
            voices,
            stutters,
            self.coins.snapshot(sample, self.rate, self.focus),
            self.lasers.snapshot(sample, self.rate, self.focus),
        ]
    }

    pub fn delay_snapshots(&self, market: MarketContext, focus: f32) -> [DelaySnapshot; 2] {
        let [details, melody] = delay_parameters(market, focus);
        [
            self.detail_echo.snapshot(details),
            self.melody_echo.snapshot(melody),
        ]
    }

    pub fn chord_transition(&self) -> ChordTransition {
        let to = self.pads[self.current_pad].chord;
        if self.transition >= 1.0 {
            ChordTransition::Steady { chord: to }
        } else {
            ChordTransition::Crossfading {
                from: self.pads[1 - self.current_pad].chord,
                to,
                progress: Unit::clamped(self.transition),
                remaining_seconds: (1.0 - self.transition) * 3.0,
            }
        }
    }

    pub fn root_transition(&self) -> RootTransition {
        let to = self.pads[self.current_pad].chord.root();
        let from = self.pads[1 - self.current_pad].chord.root();
        if self.transition >= 1.0 || from == to {
            RootTransition::Steady { root: to }
        } else {
            RootTransition::Crossfading {
                from,
                to,
                progress: Unit::clamped(self.transition),
                remaining_seconds: (1.0 - self.transition) * 3.0,
            }
        }
    }

    pub fn details(&self, sample: u64) -> DetailSnapshot {
        DetailSnapshot {
            generated: self.detail_generated,
            next_in_seconds: self.next_detail.saturating_sub(sample) as f32 / self.rate,
            voice: if self.detail_left == 0 {
                BackgroundVoice::Silent
            } else {
                BackgroundVoice::Playing {
                    frequency_hz: self.detail_osc.frequency_hz(self.rate),
                    remaining_seconds: self.detail_left as f32 / self.rate,
                }
            },
        }
    }

    pub fn melody(&self, sample: u64) -> MelodySnapshot {
        MelodySnapshot {
            generated: self.melody_generated,
            clock: ClockState::at(self.next_melody, sample, self.rate, self.focus >= 0.3),
            voice: match self.melody_age {
                Some(age) => BackgroundVoice::Playing {
                    frequency_hz: self.melody.frequency_hz(self.rate)
                        * melody_vibrato(&self.table, age as f32 / self.rate),
                    remaining_seconds: MELODY_DURATION_SECONDS - age as f32 / self.rate,
                },
                None => BackgroundVoice::Silent,
            },
        }
    }

    pub fn tick(&mut self, mix: Mix, pitch_ratio: f32) -> BackgroundFrame {
        if self.transition >= 1.0
            && let Some(chord) = self.pending_chord.take()
        {
            self.start_chord(chord);
        }
        let blend = (self.transition < 1.0)
            .then_some(self.transition * self.transition * (3.0 - 2.0 * self.transition));
        let pedal = self.pedals[self.current_pad]
            .each_mut()
            .map(|osc| osc.tick_scaled(&self.table, pitch_ratio));
        let pedal = if let Some(t) = blend {
            let previous = self.pedals[1 - self.current_pad]
                .each_mut()
                .map(|osc| osc.tick_scaled(&self.table, pitch_ratio));
            std::array::from_fn(|i| previous[i] * (1.0 - t) + pedal[i] * t)
        } else {
            pedal
        };
        let air_movement = 0.8 + self.slow_values[1] * 0.2;
        let drone = [
            (pedal[0] * 0.0175 + pedal[2] * 0.004) * air_movement * pitch_ratio,
            (pedal[1] * 0.0175 + pedal[2] * 0.004) * air_movement * pitch_ratio,
        ];

        let current_sub = self.subs[self.current_pad].tick(pitch_ratio);
        let sub = if let Some(t) = blend {
            let previous = self.subs[1 - self.current_pad].tick(pitch_ratio);
            std::array::from_fn(|i| previous[i] * (1.0 - t) + current_sub[i] * t)
        } else {
            current_sub
        };
        let current = self.pads[self.current_pad].tick(
            &self.table,
            self.market.volume.get(),
            pitch_ratio,
            self.drift_ratios,
        );
        let pad = if let Some(t) = blend {
            let previous = self.pads[1 - self.current_pad].tick(
                &self.table,
                self.market.volume.get(),
                pitch_ratio,
                self.drift_ratios,
            );
            self.transition = (self.transition + 1.0 / (self.rate * 0.5)).min(1.0);
            std::array::from_fn(|i| previous[i] * (1.0 - t) + current[i] * t)
        } else {
            current
        };
        let harmonic_gain = 0.18 * (0.12 + self.harmonic_swell * 0.88);
        let harmony: [f32; 2] = std::array::from_fn(|i| {
            let wet = self.chorus[i].filter_mono(pad[i]);
            (pad[i] * 0.75 + wet * 0.25) * harmonic_gain
        });

        let mut texture = [0.0; 2];
        for (i, value) in texture.iter_mut().enumerate() {
            *value = self.noise_filter[i].tick(self.rng.f32() * 2.0 - 1.0)
                * 0.10
                * (0.8 + 0.2 * self.slow_values[2]);
        }
        let mut detail = [0.0; 2];
        if self.detail_left > 0 {
            let age_gate = if self.detail_left % (self.rate as u32 / 180).max(1)
                < (self.rate as u32 / 360).max(1)
            {
                1.0
            } else {
                0.15
            };
            let impulse = (self.rng.f32() * 0.7 - 0.35
                + self.detail_osc.tick_scaled(&self.table, pitch_ratio) * 0.3 * pitch_ratio)
                * self.detail_amplitude
                * age_gate
                * (1.0 - self.focus * 0.65);
            self.detail_amplitude *= self.detail_decay;
            self.detail_left -= 1;
            detail[0] = self.detail_filter[0].tick(impulse * (1.0 - self.detail_pan));
            detail[1] = self.detail_filter[1].tick(impulse * self.detail_pan);
        } else {
            detail[0] = self.detail_filter[0].tick(0.0);
            detail[1] = self.detail_filter[1].tick(0.0);
        }
        let detail_wet = self.detail_echo.tick(detail);
        for channel in 0..2 {
            texture[channel] += detail[channel] + detail_wet[channel];
        }

        let mut melodic = 0.0;
        if let Some(age) = &mut self.melody_age {
            let seconds = *age as f32 / self.rate;
            let vibrato = melody_vibrato(&self.table, seconds);
            melodic = self.melody.tick_scaled(&self.table, pitch_ratio * vibrato)
                * melody_envelope(seconds)
                * 0.10
                * pitch_ratio;
            *age += 1;
            if *age >= (MELODY_DURATION_SECONDS * self.rate) as u32 {
                self.melody_age = None;
            }
        }
        let melody = [
            self.melody_filter[0].tick(melodic * (1.0 - self.melody_pan)),
            self.melody_filter[1].tick(melodic * self.melody_pan),
        ];
        let melody_wet = self.melody_echo.tick(melody);
        let voices = self.voices.tick(
            &self.table,
            [mix.demonic_voices.get(), mix.demonic_stutters.get()],
        );
        let coins = self.coins.sound.tick(&self.table);
        let lasers = self.lasers.sound.tick(&self.table);
        BackgroundFrame {
            environment: std::array::from_fn(|i| {
                drone[i] * mix.drone.get()
                    + sub[i] * mix.sub.get()
                    + texture[i] * mix.texture.get()
                    + harmony[i] * mix.harmony.get()
            }),
            melody: std::array::from_fn(|i| (melody[i] + melody_wet[i]) * mix.melody.get()),
            accents: std::array::from_fn(|i| {
                voices[i] + coins[i] * mix.flying_coins.get() + lasers[i] * mix.laser_beams.get()
            }),
        }
    }
}

fn delay_parameters(market: MarketContext, focus: f32) -> [EchoParameters; 2] {
    let zoom = market.zoom.get();
    let activity = market.activity.get();
    let volume = market.volume.get();
    let volatility = market.volatility.get();
    [
        EchoParameters {
            seconds: (0.14 + zoom * 0.19 + volatility * 0.09 + focus * 0.08 - activity * 0.05)
                .clamp(0.1, 0.6),
            feedback: (0.2 + volatility * 0.15 + zoom * 0.15 + focus * 0.12 + volume * 0.03
                - activity * 0.07)
                .clamp(0.15, 0.65),
            send: (0.18 + volatility * 0.11 + activity * 0.1 + zoom * 0.06 - focus * 0.12)
                .clamp(0.06, 0.5),
        },
        EchoParameters {
            seconds: (0.34 + volume * 0.08 + zoom * 0.38 + focus * 0.2 + volatility * 0.1
                - activity * 0.08)
                .clamp(0.25, 1.15),
            feedback: (0.32 + zoom * 0.16 + focus * 0.12 + volume * 0.08
                - activity * 0.06
                - volatility * 0.05)
                .clamp(0.15, 0.65),
            send: (0.34 + zoom * 0.1 + focus * 0.1 + volume * 0.06 - activity * 0.06)
                .clamp(0.15, 0.6),
        },
    ]
}

fn pedal_oscillators(chord: Chord, rate: f32) -> [Osc; 3] {
    let frequency = midi_hz(chord.pedal_note() as f32);
    [
        Osc::new(frequency, rate, 0.0),
        Osc::new(frequency + 0.07, rate, 0.3),
        Osc::new(frequency * 2.0, rate, 0.1),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Background, PadBank, PitchDrift, SubBank, sub_bell};
    use crate::{
        BackgroundVoice, Chord, ChordShape, MarketContext, Mix, ScaleDegree, Unit,
        dsp::{Echo, EchoParameters, HighPass, SineTable},
        harmony::midi_hz,
    };
    use fundsp::hacker32::{AudioNode, Biquad};

    fn dry_melody(rate: u32) -> Background {
        let mut background = Background::new(
            rate,
            123,
            Chord::new(ScaleDegree::I, ChordShape::MinorNinth),
        );
        background.melody.frequency(220.0, rate as f32);
        background.melody_age = Some(0);
        background.melody_echo = Echo::new(
            rate,
            EchoParameters {
                seconds: 0.3,
                feedback: 0.0,
                send: 0.0,
            },
            1_150.0,
        );
        background
    }

    #[test]
    fn melody_scheduler_uses_all_twelve_chromatic_notes_independently_of_harmony() {
        let rate = 8_000;
        let mut first = Background::new(
            rate,
            123,
            Chord::new(ScaleDegree::I, ChordShape::MinorNinth),
        );
        let mut second =
            Background::new(rate, 123, Chord::new(ScaleDegree::VII, ChordShape::Tension));
        let mut notes = BTreeSet::new();
        for index in 0..128 {
            let sample = u64::from(rate) * (7 + index * 12);
            for background in [&mut first, &mut second] {
                background.next_melody = sample;
                background.melody_age = None;
                background.control(
                    sample,
                    MarketContext::default(),
                    0.0,
                    crate::Symbol::default(),
                );
                assert_eq!(background.melody_generated, index + 1);
                assert_eq!(background.melody_age, Some(0));
                assert!(background.next_melody >= sample + u64::from(rate) * 11);
            }
            let frequency = first.melody.frequency_hz(rate as f32);
            assert_eq!(frequency, second.melody.frequency_hz(rate as f32));
            let midi = 69.0 + 12.0 * (frequency / 440.0).log2();
            assert!((midi - midi.round()).abs() < 0.0001);
            assert!((72.0..84.0).contains(&midi.round()));
            notes.insert(midi.round() as u8);
        }
        assert_eq!(notes, (72..84).collect());

        let sample = u64::from(rate) * (7 + 128 * 12);
        for background in [&mut first, &mut second] {
            background.next_melody = sample;
            background.melody_age = None;
            background.control(
                sample,
                MarketContext::default(),
                0.3,
                crate::Symbol::default(),
            );
            assert_eq!(background.melody_generated, 128);
            assert_eq!(background.melody_age, None);
            assert!(background.next_melody > sample);
        }
    }

    #[test]
    fn pad_oscillators_stay_below_420_hz_through_tuning_drift_and_crossfades() {
        let drift = 2.0_f32.powf(0.5 / 12.0);
        let assert_ceiling = |bank: &PadBank, rate: f32| {
            for oscillator in &bank.oscillators {
                let hz = oscillator.frequency_hz(rate) * drift;
                assert!(hz > 0.0 && hz <= 420.0, "{}: {hz} Hz", bank.chord);
            }
        };
        let table = SineTable::new();
        for rate in [8_000, 48_000, 192_000] {
            let mut rng = fastrand::Rng::with_seed(123);
            let mut retuned = PadBank::new(
                Chord::new(ScaleDegree::I, ChordShape::MinorNinth),
                rate as f32,
                &mut rng,
            );
            let mut highest = 0.0_f32;
            for root in ScaleDegree::ALL {
                for shape in ChordShape::ALL {
                    let chord = Chord::new(root, shape);
                    let mut created = PadBank::new(chord, rate as f32, &mut rng);
                    retuned.tune(chord, rate as f32);
                    for bank in [&mut created, &mut retuned] {
                        assert_ceiling(bank, rate as f32);
                        for oscillator in &bank.oscillators {
                            highest = highest.max(oscillator.frequency_hz(rate as f32) * drift);
                        }
                        for startup in [0.0, 0.5, 1.0] {
                            assert!(
                                bank.tick(&table, 1.0, startup, [drift; 4])
                                    .into_iter()
                                    .all(f32::is_finite)
                            );
                        }
                    }
                }
            }
            assert!(highest > 400.0, "exercise the highest supported voices");
        }

        let rate = 8_000;
        let mut background = Background::new(
            rate,
            123,
            Chord::new(ScaleDegree::VI, ChordShape::MinorNinth),
        );
        background.drift_ratios = [drift; 4];
        background.chord(Chord::new(ScaleDegree::I, ChordShape::MinorEleventh));
        let mix = Mix {
            master: Unit::ZERO,
            ..Mix::default()
        };
        for frame in 0..rate * 6 + 256 {
            if frame == rate {
                background.chord(Chord::new(ScaleDegree::VII, ChordShape::Seventh));
            }
            for bank in &background.pads {
                assert_ceiling(bank, rate as f32);
            }
            let output = background.tick(mix, 1.0);
            assert!(
                output
                    .environment
                    .into_iter()
                    .chain(output.melody)
                    .all(f32::is_finite)
            );
        }
        assert_eq!(
            background.pads[background.current_pad].chord.root(),
            ScaleDegree::VII.pitch_class()
        );
        assert!(background.transition >= 1.0);
    }

    #[test]
    fn melody_waveform_rises_slowly_and_keeps_its_long_decay_until_retirement() {
        let windows = [
            (0.0, 0.1),
            (0.4, 0.6),
            (1.4, 1.6),
            (2.8, 3.2),
            (6.9, 7.1),
            (9.9, 10.1),
            (10.8, 11.0),
            (11.05, 11.2),
        ];
        for rate in [8_000, 48_000, 192_000] {
            let mut background = dry_melody(rate);
            let mix = Mix {
                drone: Unit::ZERO,
                sub: Unit::ZERO,
                texture: Unit::ZERO,
                harmony: Unit::ZERO,
                melody: Unit::ONE,
                ..Mix::default()
            };
            let mut energy = [0.0_f64; 8];
            let mut counts = [0; 8];
            let mut peak = 0.0_f32;
            for frame in 0..(rate as f32 * 11.2) as u32 {
                let source = background.tick(mix, 1.0);
                assert_eq!(source.environment, [0.0; 2]);
                let output = source.melody;
                assert!(output.into_iter().all(f32::is_finite));
                peak = peak.max(output[0].abs());
                let seconds = frame as f32 / rate as f32;
                for (index, (start, end)) in windows.iter().enumerate() {
                    if (*start..*end).contains(&seconds) {
                        energy[index] += f64::from(output[0]).powi(2);
                        counts[index] += 1;
                    }
                }
                if frame + 1 == rate * 11 {
                    assert_eq!(background.melody_age, None);
                    assert_eq!(
                        background.melody(u64::from(frame) + 1).voice,
                        BackgroundVoice::Silent
                    );
                }
            }
            let rms = std::array::from_fn::<_, 8, _>(|i| (energy[i] / f64::from(counts[i])).sqrt());
            assert!(
                (0.049..0.051).contains(&peak),
                "source gain must remain 0.10: {peak}"
            );
            assert!(rms[0] < rms[3] * 0.005);
            assert!(rms[1] < rms[3] * 0.1);
            assert!((0.45..0.55).contains(&(rms[2] / rms[3])));
            assert!((0.45..0.55).contains(&(rms[4] / rms[3])));
            assert!((0.03..0.06).contains(&(rms[5] / rms[3])));
            assert!(rms[6] > 0.0 && rms[6] < rms[3] * 0.005);
            assert!(
                rms[7] < 1.0e-10,
                "the source must end without a residual step"
            );
        }
    }

    #[test]
    fn melody_vibrato_changes_actual_periods_and_remains_seed_reproducible() {
        for rate in [8_000, 48_000, 192_000] {
            let mut background = dry_melody(rate);
            let mut twin = dry_melody(rate);
            background.melody_age = Some(rate * 2);
            twin.melody_age = Some(rate * 2);
            let mix = Mix {
                melody: Unit::ONE,
                ..Mix::default()
            };
            let mut previous = 0.0_f32;
            let mut previous_crossing: Option<f64> = None;
            let mut previous_frequency = 220.0;
            let mut minima = f64::MAX;
            let mut maxima = 0.0_f64;
            let mut vibrato_crossings = [0.0; 12];
            let mut crossings = 0;
            for frame in 0..rate * 2 {
                let output = background.tick(mix, 1.0);
                let repeated = twin.tick(mix, 1.0);
                assert_eq!(output.environment, repeated.environment);
                assert_eq!(output.melody, repeated.melody);
                let sample = output.melody[0];
                if frame > rate / 20 && previous < 0.0 && sample >= 0.0 {
                    let crossing =
                        f64::from(frame) - 1.0 + f64::from(-previous / (sample - previous));
                    if let Some(last) = previous_crossing {
                        let frequency = f64::from(rate) / (crossing - last);
                        minima = minima.min(frequency);
                        maxima = maxima.max(frequency);
                        if previous_frequency < 220.0 && frequency >= 220.0 {
                            vibrato_crossings[crossings] = crossing / f64::from(rate);
                            crossings += 1;
                        }
                        previous_frequency = frequency;
                    }
                    previous_crossing = Some(crossing);
                }
                previous = sample;
            }
            assert!(minima > 220.0 * 0.974 && minima < 220.0 * 0.977, "{minima}");
            assert!(maxima > 220.0 * 1.023 && maxima < 220.0 * 1.026, "{maxima}");
            assert!(crossings >= 7);
            for period in vibrato_crossings[..crossings].windows(2) {
                assert!(((period[1] - period[0]) - 1.0 / 4.3).abs() < 0.005);
            }
        }
    }

    #[test]
    fn drift_is_rare_smooth_bounded_and_independent_for_each_note() {
        let rate = 8_000.0;
        let mut rng = fastrand::Rng::with_seed(123);
        let mut drifts: [PitchDrift; 4] = std::array::from_fn(|_| PitchDrift::new(rate, &mut rng));
        let mut previous = [0.0_f32; 4];
        let mut peaks = [0.0_f32; 4];
        let mut active_frames = [0; 4];
        let mut starts = [0; 4];
        let mut first_start = [None; 4];
        let mut opposed = false;
        for sample in (0..8_000 * 240).step_by(64) {
            let offsets: [f32; 4] = std::array::from_fn(|i| {
                let resting = matches!(drifts[i], PitchDrift::Resting { .. });
                let offset = drifts[i].semitones(sample, rate, &mut rng);
                if resting && matches!(drifts[i], PitchDrift::Excursion { .. }) {
                    starts[i] += 1;
                    first_start[i].get_or_insert(sample);
                }
                assert!(offset.abs() <= 0.5);
                assert!((offset - previous[i]).abs() < 0.003);
                if offset != 0.0 {
                    active_frames[i] += 1;
                }
                peaks[i] = peaks[i].max(offset.abs());
                previous[i] = offset;
                offset
            });
            opposed |= offsets.iter().any(|v| *v > 0.05) && offsets.iter().any(|v| *v < -0.05);
            if sample < 8_000 * 12 {
                assert_eq!(offsets, [0.0; 4]);
            }
        }
        assert!(
            opposed,
            "independent notes should sometimes bend in opposite directions"
        );
        assert!(peaks.iter().all(|peak| *peak > 0.1));
        assert!(starts.iter().all(|count| (3..=9).contains(count)));
        assert!(active_frames.iter().all(|count| *count < 240 * 125 / 2));
        assert!(first_start.windows(2).all(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn sub_is_a_quiet_stereo_rumble_an_octave_below_each_root() {
        for rate in [8_000, 48_000, 192_000] {
            for root in [ScaleDegree::I, ScaleDegree::III, ScaleDegree::VII] {
                let chord = Chord::new(root, ChordShape::Sus2);
                let root_hz = midi_hz(root.pedal_note() as f32);
                let mut rng = fastrand::Rng::with_seed(123);
                let mut sub = SubBank::new(chord, rate as f32, &mut rng);
                for layer in &sub.layers {
                    let hz = layer.oscillator.frequency_hz(rate as f32);
                    assert!((hz / (root_hz * 0.5) - 1.0).abs() < 0.008);
                }
                assert_ne!(sub.layers[0].pan, sub.layers[1].pan);
                let mut highpass = [
                    HighPass::new(rate as f32, 200.0),
                    HighPass::new(rate as f32, 200.0),
                ];
                let mut energy = [0.0_f64; 2];
                let mut high_energy = 0.0;
                let mut cross = 0.0;
                let mut mono_energy = 0.0;
                let mut peak = 0.0_f32;
                for sample in 0..rate * 2 {
                    let frame = sub.tick(1.0);
                    assert!(frame.iter().all(|v| v.is_finite()));
                    let high = std::array::from_fn::<_, 2, _>(|i| highpass[i].tick(frame[i]));
                    if sample >= rate / 2 {
                        for i in 0..2 {
                            energy[i] += f64::from(frame[i]).powi(2);
                            high_energy += f64::from(high[i]).powi(2);
                            peak = peak.max(frame[i].abs());
                        }
                        cross += f64::from(frame[0] * frame[1]);
                        mono_energy += f64::from((frame[0] + frame[1]) * 0.5).powi(2);
                    }
                }
                let total = energy.iter().sum::<f64>();
                let rms = (total / f64::from(rate * 3)).sqrt();
                let correlation = cross / (energy[0] * energy[1]).sqrt();
                assert!(
                    (0.001..0.04).contains(&rms),
                    "rate {rate}, root {root}: rms {rms}"
                );
                assert!(peak < 0.15);
                assert!(
                    correlation < 0.98,
                    "sub unison should retain stereo width: {correlation}"
                );
                assert!(mono_energy / (total * 0.5) > 0.25);
                assert!(
                    high_energy / total < 0.03,
                    "the sub should remain low-pass filtered: {}",
                    high_energy / total
                );
            }
        }
    }

    #[test]
    fn sub_bell_attenuates_the_original_root_and_tracks_retuning() {
        let rate = 48_000.0;
        for root in ScaleDegree::ALL {
            let hz = midi_hz(root.pedal_note() as f32);
            let gain = |frequency: f32| {
                let mut filter = Biquad::with_coefs(sub_bell(rate, hz));
                let mut input_energy = 0.0_f64;
                let mut output_energy = 0.0_f64;
                for sample in 0..48_000 {
                    let input = (sample as f32 * std::f32::consts::TAU * frequency / rate).sin();
                    let output = filter.tick(&[input].into())[0];
                    if sample >= 24_000 {
                        input_energy += f64::from(input).powi(2);
                        output_energy += f64::from(output).powi(2);
                    }
                }
                10.0 * (output_energy / input_energy).log10()
            };
            assert!((gain(hz) + 15.0).abs() < 0.2);
            assert!(gain(hz * 0.5) > -5.0);
        }
        let mut rng = fastrand::Rng::with_seed(123);
        let mut sub = SubBank::new(Chord::new(ScaleDegree::I, ChordShape::Sus2), rate, &mut rng);
        sub.tune(Chord::new(ScaleDegree::VII, ChordShape::Sus2), rate);
        let root_hz = midi_hz(ScaleDegree::VII.pedal_note() as f32);
        assert!(sub.layers.iter().all(|layer| {
            (layer.oscillator.frequency_hz(rate) / (root_hz * 0.5) - 1.0).abs() < 0.008
        }));
    }
}
