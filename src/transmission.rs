use crate::{
    DataTransmission, DataTransmissionCommand, Pan, SfxEvent, SfxKind, SfxScale, SpatialMode,
    TransmissionPhase, TransmissionRate, TransmissionSnapshot,
    dsp::{HighPass, PanGains, Smooth},
    sfx::SfxBank,
};

#[derive(Clone, Copy)]
enum AfterOpening {
    Repeat,
    Close,
}

enum Sequence {
    Idle,
    Opening { until: u64, next: AfterOpening },
    Repeating { at: u64 },
    AwaitingEnd,
    Closing { until: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BurstSounds {
    opening: SfxKind,
    tick: SfxKind,
    closing: SfxKind,
}

impl BurstSounds {
    // Complex sounds share navigation priority; short ticks have interaction
    // priority. None uses cooldown admission, so every scheduled part can enter
    // the pool even when an earlier burst is still sounding.
    const OPENINGS: [SfxKind; 4] = [
        SfxKind::Connect,
        SfxKind::DialogOpen,
        SfxKind::Notification,
        SfxKind::TabOpen,
    ];
    const TICKS: [SfxKind; 3] = [SfxKind::Focus, SfxKind::Click, SfxKind::SecondaryClick];
    const CLOSINGS: [SfxKind; 3] = [
        SfxKind::DialogClose,
        SfxKind::TabClose,
        SfxKind::DataRefresh,
    ];

    fn choose(rng: &mut fastrand::Rng) -> Self {
        Self {
            opening: Self::OPENINGS[rng.usize(..Self::OPENINGS.len())],
            tick: Self::TICKS[rng.usize(..Self::TICKS.len())],
            closing: Self::CLOSINGS[rng.usize(..Self::CLOSINGS.len())],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BurstRates([TransmissionRate; 4]);

impl BurstRates {
    fn choose(nominal: TransmissionRate, rng: &mut fastrand::Rng) -> Self {
        Self(std::array::from_fn(|_| {
            let hz = (nominal.get() * (0.75 + rng.f32() * 0.5))
                .clamp(TransmissionRate::MIN_HZ, TransmissionRate::MAX_HZ);
            TransmissionRate::new(hz).expect("bounded transmission group rate")
        }))
    }

    fn after(self, repeated: u16) -> TransmissionRate {
        match repeated {
            0..=3 => self.0[0],
            4..=7 => self.0[1],
            8..=11 => self.0[2],
            _ => self.0[3],
        }
    }
}

/// An externally delimited sequence on an independent, preallocated SFX bus.
pub(crate) struct Transmission {
    rate: u32,
    frame: u64,
    bank: SfxBank,
    sequence: Sequence,
    settings: DataTransmission,
    repeated: u16,
    sounds: BurstSounds,
    rates: BurstRates,
    current_rate: TransmissionRate,
    rng: fastrand::Rng,
    highpass: Option<[[HighPass; 2]; 2]>,
    pan: [Smooth; 2],
}

impl Transmission {
    pub fn new(rate: u32, seed: u64, spatial: SpatialMode) -> Self {
        Self {
            rate,
            frame: 0,
            bank: SfxBank::new(rate, 8, seed ^ 0x4441_5441_5f53_4658, spatial),
            sequence: Sequence::Idle,
            settings: DataTransmission::default(),
            repeated: 0,
            sounds: BurstSounds {
                opening: SfxKind::Connect,
                tick: SfxKind::Focus,
                closing: SfxKind::DialogClose,
            },
            rates: BurstRates([DataTransmission::default().rate; 4]),
            current_rate: DataTransmission::default().rate,
            rng: fastrand::Rng::with_seed(seed),
            highpass: (rate > 16_000).then(|| {
                std::array::from_fn(|_| {
                    std::array::from_fn(|_| HighPass::new(rate as f32, 8_000.0))
                })
            }),
            pan: std::array::from_fn(|_| Smooth::new(1.0, 0.02, rate as f32)),
        }
    }

    pub fn apply(&mut self, command: DataTransmissionCommand) {
        match command {
            DataTransmissionCommand::Start(settings) => {
                self.settings = settings;
                self.set_pan(settings.pan);
                self.repeated = 0;
                self.sounds = BurstSounds::choose(&mut self.rng);
                self.rates = BurstRates::choose(settings.rate, &mut self.rng);
                self.current_rate = self.rates.after(0);
                let until = self.trigger(self.sounds.opening, 0.25);
                self.sequence = Sequence::Opening {
                    until,
                    next: AfterOpening::Repeat,
                };
            }
            DataTransmissionCommand::Pan(pan) => self.set_pan(pan),
            DataTransmissionCommand::End => match self.sequence {
                Sequence::Opening { until, .. } => {
                    self.sequence = Sequence::Opening {
                        until,
                        next: AfterOpening::Close,
                    };
                }
                Sequence::Repeating { .. } | Sequence::AwaitingEnd => self.close(),
                Sequence::Idle | Sequence::Closing { .. } => {}
            },
        }
    }

    fn set_pan(&mut self, pan: Pan) {
        self.settings.pan = pan;
        let gains = PanGains::new(pan).for_mode(self.bank.spatial());
        for (smooth, target) in self.pan.iter_mut().zip(gains) {
            smooth.target(target * std::f32::consts::SQRT_2);
        }
    }

    fn trigger(&mut self, kind: SfxKind, duration: f32) -> u64 {
        let mut event = SfxEvent::new(kind);
        event.intensity = self.settings.intensity;
        event.edits.duration = SfxScale::new(duration).expect("valid transmission articulation");
        let generated = self
            .bank
            .trigger(event)
            .expect("bounded transmission schedule fits its voice pool");
        self.frame + (generated.duration_seconds * self.rate as f32).ceil() as u64
    }

    fn close(&mut self) {
        let until = self.trigger(self.sounds.closing, 0.25);
        self.sequence = Sequence::Closing { until };
    }

    fn repeat(&mut self) {
        self.trigger(self.sounds.tick, 0.25);
        self.repeated += 1;
        if self.repeated == self.settings.max_triggers.get() {
            self.sequence = Sequence::AwaitingEnd;
        } else {
            self.current_rate = self.rates.after(self.repeated);
            self.sequence = Sequence::Repeating {
                at: self.frame + (self.rate as f32 / self.current_rate.get()).round() as u64,
            };
        }
    }

    pub fn set_spatial(&mut self, spatial: SpatialMode) {
        self.bank.set_spatial(spatial);
        self.set_pan(self.settings.pan);
    }

    pub fn tick(&mut self, pitch_ratio: f32) -> [f32; 2] {
        match self.sequence {
            Sequence::Opening { until, next } if self.frame >= until => match next {
                AfterOpening::Repeat => self.repeat(),
                AfterOpening::Close => self.close(),
            },
            Sequence::Repeating { at } if self.frame >= at => self.repeat(),
            Sequence::Closing { until } if self.frame >= until => self.sequence = Sequence::Idle,
            _ => {}
        }
        let source = self.bank.tick_scaled(pitch_ratio);
        let gains = self.pan.each_mut().map(Smooth::tick);
        self.frame += 1;
        match &mut self.highpass {
            Some(filters) => std::array::from_fn(|i| {
                let first = filters[i][0].tick(source[i]);
                filters[i][1].tick(first) * gains[i]
            }),
            None => [0.0; 2],
        }
    }

    pub fn snapshot(&self) -> TransmissionSnapshot {
        TransmissionSnapshot {
            phase: match self.sequence {
                Sequence::Idle => TransmissionPhase::Idle,
                Sequence::Opening { .. } => TransmissionPhase::Opening,
                Sequence::Repeating { .. } => TransmissionPhase::Repeating,
                Sequence::AwaitingEnd => TransmissionPhase::AwaitingEnd,
                Sequence::Closing { .. } => TransmissionPhase::Closing,
            },
            repeated: self.repeated,
            max_triggers: self.settings.max_triggers,
            pan: self.settings.pan,
            rate_hz: self.current_rate.get(),
            active_voices: self.bank.active_voices(),
            generated_events: self.bank.generated_events(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransmissionCount;

    #[test]
    fn burst_sound_choices_are_independent_seeded_and_retained_until_closing() {
        let mut a = Transmission::new(48_000, 123, SpatialMode::Pan);
        let mut b = Transmission::new(48_000, 123, SpatialMode::Pan);
        let mut sounds = Vec::new();
        let mut rates = Vec::new();
        let mut waveforms = Vec::new();
        for _ in 0..12 {
            for transmission in [&mut a, &mut b] {
                transmission.apply(DataTransmissionCommand::Start(DataTransmission::default()));
            }
            let selected = a.sounds;
            let selected_rates = a.rates;
            assert_eq!(selected, b.sounds);
            assert_eq!(selected_rates, b.rates);
            let mut waveform = 0_u64;
            for frame in 0..24_000 {
                if frame == 12_000 {
                    a.apply(DataTransmissionCommand::End);
                    b.apply(DataTransmissionCommand::End);
                }
                let output = a.tick(1.0);
                assert_eq!(output, b.tick(1.0));
                for sample in output {
                    waveform = waveform.rotate_left(5) ^ u64::from(sample.to_bits());
                }
            }
            assert_eq!(a.sounds, selected);
            assert_eq!(a.rates, selected_rates);
            assert_eq!(a.snapshot().phase, TransmissionPhase::Idle);
            sounds.push(selected);
            rates.push(selected_rates);
            waveforms.push(waveform);
        }
        assert!(
            sounds
                .iter()
                .any(|sound| sound.opening != sounds[0].opening)
        );
        assert!(sounds.iter().any(|sound| sound.tick != sounds[0].tick));
        assert!(
            sounds
                .iter()
                .any(|sound| sound.closing != sounds[0].closing)
        );
        assert!(sounds.iter().any(|a| {
            sounds
                .iter()
                .any(|b| a.opening == b.opening && a.tick != b.tick && a.closing != b.closing)
        }));
        assert!(waveforms.windows(2).all(|pair| pair[0] != pair[1]));
        assert!(rates.windows(2).all(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn extreme_rates_admit_every_capped_tick_and_closing_after_rapid_starts() {
        for sample_rate in [8_000, 48_000, 192_000] {
            for hz in [TransmissionRate::MIN_HZ, TransmissionRate::MAX_HZ] {
                let mut transmission = Transmission::new(sample_rate, 71, SpatialMode::Pan);
                let settings = DataTransmission {
                    max_triggers: TransmissionCount::new(64).unwrap(),
                    rate: TransmissionRate::new(hz).unwrap(),
                    ..DataTransmission::default()
                };
                let mut starts = 0;
                for frame in 0..sample_rate / 10 {
                    if frame % 7 == 0 {
                        transmission.apply(DataTransmissionCommand::Start(settings));
                        starts += 1;
                    }
                    assert!(transmission.tick(1.0).into_iter().all(f32::is_finite));
                    assert!(transmission.snapshot().active_voices <= 8);
                }
                let limit = u64::from(sample_rate) * 40;
                while transmission.snapshot().phase != TransmissionPhase::AwaitingEnd {
                    assert!(
                        transmission.frame < limit,
                        "pulse budget did not finish at {hz} Hz"
                    );
                    assert!(transmission.tick(1.0).into_iter().all(f32::is_finite));
                    assert!(transmission.snapshot().active_voices <= 8);
                }
                assert_eq!(transmission.snapshot().repeated, 64);
                assert_eq!(transmission.snapshot().generated_events, starts + 64);
                transmission.apply(DataTransmissionCommand::End);
                for _ in 0..sample_rate / 2 {
                    assert!(transmission.tick(1.0).into_iter().all(f32::is_finite));
                }
                assert_eq!(transmission.snapshot().generated_events, starts + 65);
                assert_eq!(transmission.snapshot().phase, TransmissionPhase::Idle);
            }
        }
    }

    #[test]
    fn highpass_preserves_air_and_rejects_low_frequency_content() {
        let gain = |rate: u32, hz: f64| {
            let mut transmission = Transmission::new(rate, 123, SpatialMode::Pan);
            let filters = &mut transmission.highpass.as_mut().unwrap()[0];
            let mut input = 0.0;
            let mut output = 0.0;
            for sample in 0..rate {
                let value =
                    (std::f64::consts::TAU * hz * f64::from(sample) / f64::from(rate)).sin() as f32;
                let first = filters[0].tick(value);
                let result = filters[1].tick(first);
                assert!(result.is_finite());
                if sample > rate / 2 {
                    input += f64::from(value).powi(2);
                    output += f64::from(result).powi(2);
                }
            }
            10.0 * (output / input).log10()
        };
        for rate in [44_100, 48_000, 96_000, 192_000] {
            assert!(gain(rate, 1_000.0) < -65.0);
            assert!(gain(rate, 12_000.0) > -2.0);
            assert!((gain(rate, 8_000.0) + 6.0206).abs() < 0.03);
        }
    }

    #[test]
    fn repeated_start_replaces_schedule_without_unbounded_voices() {
        let mut transmission = Transmission::new(48_000, 123, SpatialMode::Pan);
        for frame in 0..48_000 {
            if frame < 2000 && frame % 7 == 0 {
                transmission.apply(DataTransmissionCommand::Start(DataTransmission::default()));
            }
            if frame == 20_000 {
                transmission.apply(DataTransmissionCommand::End);
            }
            let output = transmission.tick(1.0);
            assert!(output.into_iter().all(f32::is_finite));
            assert!(transmission.snapshot().active_voices <= 8);
        }
        assert_eq!(transmission.snapshot().phase, TransmissionPhase::Idle);
    }
}
