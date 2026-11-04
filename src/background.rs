use fundsp::hacker32::{AudioUnit, chorus};

use crate::{
    BackgroundVoice, Chord, ChordTransition, ClockState, DelaySnapshot, DetailSnapshot,
    MarketContext, MelodySnapshot, Mix, RootTransition, Unit,
    dsp::{Echo, EchoParameters, LowPass, Osc, SineTable},
    harmony::midi_hz,
};

const UNISON: usize = 12;

struct PadBank {
    oscillators: [Osc; UNISON],
    chord: Chord,
}

impl PadBank {
    fn new(chord: Chord, rate: f32, rng: &mut fastrand::Rng) -> Self {
        let notes = chord.notes();
        Self {
            oscillators: std::array::from_fn(|i| {
                let detune = [-0.055, 0.0, 0.055][i % 3];
                Osc::new(midi_hz(notes[i / 3] as f32 + detune), rate, rng.f32())
            }),
            chord,
        }
    }

    fn tune(&mut self, chord: Chord, rate: f32) {
        let notes = chord.notes();
        for (i, osc) in self.oscillators.iter_mut().enumerate() {
            osc.frequency(
                midi_hz(notes[i / 3] as f32 + [-0.055, 0.0, 0.055][i % 3]),
                rate,
            );
        }
        self.chord = chord;
    }

    fn tick(&mut self, table: &SineTable, density: f32) -> [f32; 2] {
        let mut out = [0.0; 2];
        for (i, osc) in self.oscillators.iter_mut().enumerate() {
            let tone_gain = [1.0, 0.85, 0.35 + density * 0.65, 0.1 + density * 0.9][i / 3];
            let value = osc.tick(table) * tone_gain / UNISON as f32;
            let width = [0.7, 0.5, 0.3][i % 3];
            out[0] += value * width;
            out[1] += value * (1.0 - width);
        }
        out
    }
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
}

impl Background {
    pub fn new(rate: u32, seed: u64, chord: Chord) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed);
        let rate_f = rate as f32;
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
        let frequency = midi_hz(chord.root.pedal_note() as f32);
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
    pub fn control(&mut self, sample: u64, market: MarketContext, focus: f32, chord: Chord) {
        self.market = market;
        self.focus = focus;
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
            self.next_melody = sample + ((11.0 + self.rng.f32() * 18.0) * self.rate) as u64;
            if focus < 0.3 {
                self.melody_generated += 1;
                let note = chord.notes()[self.rng.usize(1..4)] as f32 + 12.0;
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
        let to = self.pads[self.current_pad].chord.root;
        let from = self.pads[1 - self.current_pad].chord.root;
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
                Some(age) if (age as f32 / self.rate) < 2.6 => BackgroundVoice::Playing {
                    frequency_hz: self.melody.frequency_hz(self.rate),
                    remaining_seconds: 2.6 - age as f32 / self.rate,
                },
                _ => BackgroundVoice::Silent,
            },
        }
    }

    pub fn tick(&mut self, mix: Mix) -> [f32; 2] {
        if self.transition >= 1.0
            && let Some(chord) = self.pending_chord.take()
        {
            self.start_chord(chord);
        }
        let blend = (self.transition < 1.0)
            .then_some(self.transition * self.transition * (3.0 - 2.0 * self.transition));
        let pedal = self.pedals[self.current_pad]
            .each_mut()
            .map(|osc| osc.tick(&self.table));
        let pedal = if let Some(t) = blend {
            let previous = self.pedals[1 - self.current_pad]
                .each_mut()
                .map(|osc| osc.tick(&self.table));
            std::array::from_fn(|i| previous[i] * (1.0 - t) + pedal[i] * t)
        } else {
            pedal
        };
        let air_movement = 0.8 + self.slow_values[1] * 0.2;
        let drone = [
            (pedal[0] * 0.0175 + pedal[2] * 0.004) * air_movement,
            (pedal[1] * 0.0175 + pedal[2] * 0.004) * air_movement,
        ];

        let current = self.pads[self.current_pad].tick(&self.table, self.market.volume.get());
        let pad = if let Some(t) = blend {
            let previous =
                self.pads[1 - self.current_pad].tick(&self.table, self.market.volume.get());
            self.transition = (self.transition + 1.0 / (self.rate * 3.0)).min(1.0);
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
            let impulse = (self.rng.f32() * 0.7 - 0.35 + self.detail_osc.tick(&self.table) * 0.3)
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
            let attack = (seconds / 0.22).min(1.0);
            let release = (1.0 - seconds / 2.6).max(0.0);
            melodic = self.melody.tick(&self.table) * attack * release * release * 0.10;
            *age += 1;
            if seconds >= 2.6 {
                self.melody_age = None;
            }
        }
        let melody = [
            self.melody_filter[0].tick(melodic * (1.0 - self.melody_pan)),
            self.melody_filter[1].tick(melodic * self.melody_pan),
        ];
        let melody_wet = self.melody_echo.tick(melody);
        std::array::from_fn(|i| {
            drone[i] * mix.drone.get()
                + texture[i] * mix.texture.get()
                + harmony[i] * mix.harmony.get()
                + (melody[i] + melody_wet[i]) * mix.melody.get()
        })
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
    let frequency = midi_hz(chord.root.pedal_note() as f32);
    [
        Osc::new(frequency, rate, 0.0),
        Osc::new(frequency + 0.07, rate, 0.3),
        Osc::new(frequency * 2.0, rate, 0.1),
    ]
}
