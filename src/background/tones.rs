use arrayvec::ArrayVec;
use fundsp::hacker32::{AudioNode, Biquad, BiquadCoefs};

use crate::{
    Pan, SpatialMode,
    dsp::{HighPass, LowPass, Osc, PanGains, SineTable, Smooth},
};

const MAX_PARTIALS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToneKind {
    FlyingCoins,
    LaserBeams,
}

impl ToneKind {
    fn band(self) -> (f32, f32) {
        match self {
            Self::FlyingCoins => (1_000.0, 10_000.0),
            Self::LaserBeams => (10_000.0, 14_000.0),
        }
    }
}

enum Envelope {
    Swell { attack: u32, release: u32 },
    Beam { duration: u32, edge: u32 },
}

impl Envelope {
    fn duration(&self) -> u32 {
        match *self {
            Self::Swell { attack, release } => attack + release,
            Self::Beam { duration, .. } => duration,
        }
    }

    fn amplitude(&self, age: u32) -> f32 {
        let progress = match *self {
            Self::Swell { attack, release } => {
                if age < attack {
                    age as f32 / attack as f32
                } else {
                    (attack + release - age) as f32 / release as f32
                }
            }
            Self::Beam { duration, edge } => {
                let attack = age as f32 / edge as f32;
                let release = (duration - age) as f32 / (edge * 30) as f32;
                attack.min(release).min(1.0)
            }
        };
        progress * progress * (3.0 - 2.0 * progress)
    }
}

struct Pulse {
    phase: f64,
    step: f64,
    gain: Smooth,
}

impl Pulse {
    fn new(rate: f32, rng: &mut fastrand::Rng) -> Self {
        Self {
            phase: 0.0,
            step: (3.0 + f64::from(rng.f32()) * 7.0) / f64::from(rate),
            gain: Smooth::new(1.0, 0.002, rate),
        }
    }

    fn tick(&mut self) -> f32 {
        self.gain.target(if self.phase < 0.5 { 1.0 } else { 0.0 });
        self.phase += self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        self.gain.tick()
    }
}

enum PitchMotion {
    Steady,
    OnsetShake { oscillator: Osc, depth: f32 },
}

struct Partial {
    carrier: Osc,
    level_lfo: Osc,
    index_lfo: Osc,
    level_depth: f32,
    index_depth: f32,
    index: f32,
    pan: [f32; 2],
    pitch: PitchMotion,
    pulse: Option<Pulse>,
}

struct Formation {
    age: u32,
    envelope: Envelope,
    partials: ArrayVec<Partial, MAX_PARTIALS>,
    matrix: [[f32; MAX_PARTIALS]; MAX_PARTIALS],
    previous: [f32; MAX_PARTIALS],
    gain: f32,
}

/// A bounded PM network. Every matrix row reads the preceding sample, so its
/// result is independent of partial iteration order and contains no algebraic loop.
pub(super) struct ToneFormation {
    rate: f32,
    kind: ToneKind,
    source: Option<Formation>,
    band: Option<Band>,
}

impl ToneFormation {
    pub(super) fn new(rate: u32, kind: ToneKind) -> Self {
        let rate = rate as f32;
        let (_, high) = kind.band();
        Self {
            rate,
            kind,
            source: None,
            band: (rate * 0.45 >= high).then(|| Band::new(rate, kind)),
        }
    }

    pub(super) fn start(&mut self, rng: &mut fastrand::Rng) {
        assert!(self.source.is_none(), "a tone formation is already active");
        if self.band.is_none() {
            return;
        }
        let count = rng.usize(2..=MAX_PARTIALS);
        let center = match self.kind {
            ToneKind::FlyingCoins => {
                (0.35 + rng.f32() * 0.25) * if rng.bool() { 1.0 } else { -1.0 }
            }
            ToneKind::LaserBeams => rng.f32() * 1.5 - 0.75,
        };
        let opposite = if count >= 5 { rng.usize(1..=2) } else { 1 };
        let (low, high) = self.kind.band();
        let mut partials = ArrayVec::new();
        for index in 0..count {
            let position = match self.kind {
                ToneKind::FlyingCoins => {
                    let group = if index < opposite { -center } else { center };
                    group + (rng.f32() - 0.5) * 0.34
                }
                ToneKind::LaserBeams => center + (rng.f32() - 0.5) * 0.1,
            };
            let (level_depth, pm_index, pitch, pulse) = match self.kind {
                ToneKind::FlyingCoins => (
                    0.25 + rng.f32() * 0.65,
                    0.025 + rng.f32() * 0.055,
                    PitchMotion::Steady,
                    None,
                ),
                ToneKind::LaserBeams => (
                    0.005 + rng.f32() * 0.025,
                    0.001 + rng.f32() * 0.004,
                    PitchMotion::OnsetShake {
                        oscillator: Osc::new(40.0 + rng.f32() * 160.0, self.rate, rng.f32()),
                        depth: 0.003 + rng.f32() * 0.012,
                    },
                    Some(Pulse::new(self.rate, rng)),
                ),
            };
            partials.push(Partial {
                carrier: Osc::new(low + rng.f32() * (high - low), self.rate, rng.f32()),
                level_lfo: Osc::new(0.3 + rng.f32() * 3.7, self.rate, rng.f32()),
                index_lfo: Osc::new(0.3 + rng.f32() * 3.7, self.rate, rng.f32()),
                level_depth,
                index_depth: 0.15 + rng.f32() * 0.85,
                index: pm_index,
                pan: PanGains::new(Pan::new(position).expect("bounded formation position"))
                    .for_mode(SpatialMode::Pan),
                pitch,
                pulse,
            });
        }
        let mut matrix = [[0.0; MAX_PARTIALS]; MAX_PARTIALS];
        for (row_index, row) in matrix.iter_mut().enumerate().take(count) {
            for (column, weight) in row.iter_mut().enumerate().take(count) {
                if column != row_index {
                    *weight = rng.f32() * 2.0 - 1.0;
                }
            }
            let scale = row.iter().map(|weight| weight.abs()).sum::<f32>();
            assert!(scale > 0.0, "PM matrix row must have an input");
            for weight in row.iter_mut().take(count) {
                *weight /= scale;
            }
        }
        let envelope = match self.kind {
            ToneKind::FlyingCoins => Envelope::Swell {
                attack: ((3.0 + rng.f32() * 4.0) * self.rate) as u32,
                release: ((3.0 + rng.f32() * 4.0) * self.rate) as u32,
            },
            ToneKind::LaserBeams => Envelope::Beam {
                duration: ((6.0 + rng.f32() * 8.0) * self.rate) as u32,
                edge: (self.rate * 0.001) as u32,
            },
        };
        self.source = Some(Formation {
            age: 0,
            envelope,
            partials,
            matrix,
            previous: [0.0; MAX_PARTIALS],
            gain: match self.kind {
                ToneKind::FlyingCoins => 0.012,
                ToneKind::LaserBeams => 0.006,
            } / (count as f32).sqrt(),
        });
    }

    pub(super) fn is_active(&self) -> bool {
        self.source.is_some()
    }

    pub(super) fn tick(&mut self, table: &SineTable) -> [f32; 2] {
        let Some(band) = &mut self.band else {
            return [0.0; 2];
        };
        let mut output = [0.0; 2];
        if let Some(source) = &mut self.source {
            let mut current = [0.0; MAX_PARTIALS];
            for (index, partial) in source.partials.iter_mut().enumerate() {
                let pm = source.matrix[index]
                    .iter()
                    .zip(source.previous)
                    .map(|(weight, previous)| weight * previous)
                    .sum::<f32>();
                let index_gain =
                    1.0 - partial.index_depth * (0.5 + partial.index_lfo.tick(table) * 0.5);
                let pitch = match &mut partial.pitch {
                    PitchMotion::Steady => 1.0,
                    PitchMotion::OnsetShake { oscillator, depth } => {
                        let envelope = (1.0 - source.age as f32 / (self.rate * 0.1)).max(0.0);
                        1.0 + oscillator.tick(table) * *depth * envelope * envelope
                    }
                };
                let wave =
                    partial
                        .carrier
                        .tick_modulated(table, pm * partial.index * index_gain, pitch);
                current[index] = wave;
                let gate = match &mut partial.pulse {
                    Some(pulse) => pulse.tick(),
                    None => 1.0,
                };
                let amplitude = (1.0
                    - partial.level_depth * (0.5 + partial.level_lfo.tick(table) * 0.5))
                    * gate;
                for (channel, value) in output.iter_mut().enumerate() {
                    *value += wave * amplitude * partial.pan[channel];
                }
            }
            source.previous = current;
            let gain = source.envelope.amplitude(source.age) * source.gain;
            output = output.map(|value| value * gain);
            source.age += 1;
            if source.age >= source.envelope.duration() {
                self.source = None;
            }
        }
        band.tick(output)
    }
}

struct Band {
    highpass: [[HighPass; 2]; 2],
    lowpass: [[LowPass; 2]; 2],
    coin_eq: Option<[[Biquad<f32>; 2]; 2]>,
}

impl Band {
    fn new(rate: f32, kind: ToneKind) -> Self {
        let (low, high) = kind.band();
        Self {
            highpass: std::array::from_fn(|_| {
                std::array::from_fn(|_| HighPass::new(rate, low * 0.9))
            }),
            lowpass: std::array::from_fn(|_| {
                std::array::from_fn(|_| LowPass::new(rate, high * 1.1))
            }),
            coin_eq: (kind == ToneKind::FlyingCoins).then(|| {
                std::array::from_fn(|_| {
                    [(3_300.0, -12.0), (9_000.0, -9.0)].map(|(hz, gain_db)| {
                        Biquad::with_coefs(BiquadCoefs::bell(
                            rate,
                            hz,
                            0.8,
                            10.0_f32.powf(gain_db / 20.0),
                        ))
                    })
                })
            }),
        }
    }

    fn tick(&mut self, input: [f32; 2]) -> [f32; 2] {
        std::array::from_fn(|channel| {
            let mut value = input[channel];
            for filter in &mut self.highpass[channel] {
                value = filter.tick(value);
            }
            for filter in &mut self.lowpass[channel] {
                value = filter.tick(value);
            }
            if let Some(eq) = &mut self.coin_eq {
                for filter in &mut eq[channel] {
                    value = filter.tick(&[value].into())[0];
                }
            }
            value
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Band, Envelope, PitchMotion, Pulse, ToneFormation, ToneKind};
    use crate::dsp::SineTable;
    use fundsp::{fft::real_fft, hacker32::AudioNode, math::Complex32};

    fn formation(rate: u32, kind: ToneKind, seed: u64) -> ToneFormation {
        let mut result = ToneFormation::new(rate, kind);
        result.start(&mut fastrand::Rng::with_seed(seed));
        result
    }

    #[test]
    fn formations_keep_the_requested_register_and_stereo_groups() {
        for kind in [ToneKind::FlyingCoins, ToneKind::LaserBeams] {
            for seed in 0..128 {
                let mut formation = formation(48_000, kind, seed);
                let source = formation.source.as_ref().unwrap();
                let partials = &source.partials;
                assert!((2..=10).contains(&partials.len()));
                let (low, high) = kind.band();
                for partial in partials {
                    assert!((low..=high).contains(&partial.carrier.frequency_hz(48_000.0)));
                    for lfo in [&partial.level_lfo, &partial.index_lfo] {
                        assert!((0.3..=4.0).contains(&lfo.frequency_hz(48_000.0)));
                    }
                    if let PitchMotion::OnsetShake { oscillator, .. } = &partial.pitch {
                        assert!((40.0..=200.0).contains(&oscillator.frequency_hz(48_000.0)));
                    }
                    match kind {
                        ToneKind::FlyingCoins => assert!(partial.pulse.is_none()),
                        ToneKind::LaserBeams => {
                            let pulse = partial.pulse.as_ref().unwrap();
                            assert!((3.0..=10.0).contains(&(pulse.step * 48_000.0)));
                        }
                    }
                }
                let left = partials
                    .iter()
                    .filter(|partial| partial.pan[0] > partial.pan[1])
                    .count();
                match kind {
                    ToneKind::FlyingCoins => {
                        assert!((1..partials.len()).contains(&left));
                        assert!(left.min(partials.len() - left) <= 2);
                        let Envelope::Swell { attack, release } = source.envelope else {
                            panic!()
                        };
                        assert!((144_000..=336_000).contains(&attack));
                        assert!((144_000..=336_000).contains(&release));
                    }
                    ToneKind::LaserBeams => {
                        let pans: Vec<_> = partials.iter().map(|partial| partial.pan[0]).collect();
                        let min = pans.iter().copied().fold(f32::INFINITY, f32::min);
                        let max = pans.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                        assert!(max - min < 0.08);
                        let source = formation.source.as_mut().unwrap();
                        let (first, others) = source.partials.split_at_mut(1);
                        let first = first[0].pulse.as_mut().unwrap();
                        let second = others[0].pulse.as_mut().unwrap();
                        assert!(
                            (0..48_000).any(|_| (first.tick() >= 0.5) != (second.tick() >= 0.5)),
                            "seed {seed}: partial pulse patterns must move independently"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn tone_audio_is_quiet_reproducible_and_concentrated_in_its_high_band() {
        let table = SineTable::new();
        for rate in [44_100, 48_000, 192_000] {
            for kind in [ToneKind::FlyingCoins, ToneKind::LaserBeams] {
                let mut first = formation(rate, kind, 123);
                let mut second = formation(rate, kind, 123);
                for _ in 0..rate * 2 {
                    assert_eq!(first.tick(&table), second.tick(&table));
                }
                let mut input = [0.0; 8192];
                let mut energy = 0.0_f64;
                let mut peak = 0.0_f32;
                for (index, value) in input.iter_mut().enumerate() {
                    let frame = first.tick(&table);
                    assert_eq!(frame, second.tick(&table));
                    assert!(frame.iter().all(|value| value.is_finite()));
                    energy += frame
                        .iter()
                        .map(|value| f64::from(*value).powi(2))
                        .sum::<f64>();
                    peak = peak.max(frame[0].abs()).max(frame[1].abs());
                    let hann = 0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / 8191.0).cos();
                    *value = frame[0] * hann;
                }
                let rms = (energy / 16_384.0).sqrt();
                assert!((0.0001..0.006).contains(&rms), "{rate} {kind:?}: RMS {rms}");
                assert!(peak < 0.03, "{rate} {kind:?}: peak {peak}");
                let mut spectrum = [Complex32::new(0.0, 0.0); 4097];
                real_fft(&input, &mut spectrum);
                let total = spectrum
                    .iter()
                    .map(|bin| f64::from(bin.norm_sqr()))
                    .sum::<f64>();
                let (low, high) = kind.band();
                let in_band = spectrum
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        (low - 300.0..high + 300.0)
                            .contains(&(*index as f32 * rate as f32 / 8192.0))
                    })
                    .map(|(_, bin)| f64::from(bin.norm_sqr()))
                    .sum::<f64>();
                assert!(
                    in_band / total > 0.95,
                    "{rate} {kind:?}: spectral share {}",
                    in_band / total
                );
            }
        }
    }

    #[test]
    fn coin_pm_matrix_changes_the_audio_more_than_the_weak_laser_matrix() {
        let table = SineTable::new();
        let mut relative_changes = Vec::new();
        for kind in [ToneKind::FlyingCoins, ToneKind::LaserBeams] {
            let mut modulated = formation(48_000, kind, 123);
            let mut unmodulated = formation(48_000, kind, 123);
            for partial in &mut unmodulated.source.as_mut().unwrap().partials {
                partial.index = 0.0;
            }
            let mut energy = 0.0_f64;
            let mut error = 0.0_f64;
            for sample in 0..144_000 {
                let with_pm = modulated.tick(&table);
                let without_pm = unmodulated.tick(&table);
                if sample >= 96_000 {
                    for channel in 0..2 {
                        energy += f64::from(without_pm[channel]).powi(2);
                        error += f64::from(with_pm[channel] - without_pm[channel]).powi(2);
                    }
                }
            }
            relative_changes.push((error / energy).sqrt());
        }
        assert!(
            relative_changes[0] > 0.01,
            "coin PM relative RMS {}",
            relative_changes[0]
        );
        assert!(
            relative_changes[1] < relative_changes[0] * 0.2,
            "coin/laser PM relative RMS {relative_changes:?}"
        );
    }

    #[test]
    fn laser_pulses_hold_their_rate_and_half_duty_with_smooth_edges() {
        for rate in [8_000, 48_000, 192_000] {
            for hz in [3.0, 10.0] {
                let mut pulse = Pulse::new(rate as f32, &mut fastrand::Rng::with_seed(123));
                pulse.step = f64::from(hz) / f64::from(rate);
                let mut previous = 1.0;
                let mut crossings = Vec::new();
                let mut high = true;
                let mut quiet = 0;
                let mut full = 0;
                let mut largest_step = 0.0_f32;
                let frames = rate * 3;
                for age in 0..frames {
                    let value = pulse.tick();
                    assert!((0.0..=1.0).contains(&value));
                    let step = (value - previous).abs();
                    assert!(step <= 1.0 / (rate as f32 * 0.002) + 1e-6);
                    largest_step = largest_step.max(step);
                    if (value >= 0.5) != high {
                        high = !high;
                        crossings.push((age, high));
                    }
                    quiet += u32::from(value < 0.0001);
                    full += u32::from(value > 0.9999);
                    previous = value;
                }
                let expected_crossings = (hz * 6.0) as usize;
                assert!(
                    (expected_crossings - 1..=expected_crossings + 1).contains(&crossings.len())
                );
                let expected_period = rate as f32 / hz;
                let mut period_error = 0.0_f32;
                let mut duty_error = 0.0_f32;
                for cycle in crossings[1..].windows(3) {
                    let period = (cycle[2].0 - cycle[0].0) as f32;
                    let on = if cycle[0].1 {
                        cycle[1].0 - cycle[0].0
                    } else {
                        cycle[2].0 - cycle[1].0
                    };
                    let error = (period / expected_period - 1.0).abs();
                    let duty = (on as f32 / period - 0.5).abs();
                    assert!(
                        (period - expected_period).abs() <= 2.0,
                        "{rate} Hz, {hz} Hz pulse: period {period}"
                    );
                    assert!(
                        duty <= 2.0 / period,
                        "{rate} Hz, {hz} Hz pulse: duty {}",
                        on as f32 / period
                    );
                    period_error = period_error.max(error);
                    duty_error = duty_error.max(duty);
                }
                assert!(quiet > frames / 4 && full > frames / 4);
                eprintln!(
                    "{rate} Hz, {hz} Hz pulse: period error {:.4}%, duty error {:.4}%, largest step {largest_step:.6}",
                    period_error * 100.0,
                    duty_error * 100.0
                );
            }
        }
    }

    #[test]
    fn coin_bells_cut_their_centers_and_shape_the_stereo_source_before_routing() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            for (index, frequency, gain_db) in [(0, 3_300.0, -12.0), (1, 9_000.0, -9.0)] {
                let band = Band::new(rate as f32, ToneKind::FlyingCoins);
                let mut bell = band.coin_eq.unwrap()[0][index].clone();
                let mut shaped = Band::new(rate as f32, ToneKind::FlyingCoins);
                let mut bypassed = Band::new(rate as f32, ToneKind::FlyingCoins);
                bypassed.coin_eq = None;
                let mut input_energy = 0.0_f64;
                let mut bell_energy = 0.0_f64;
                let mut shaped_energy = 0.0_f64;
                let mut bypassed_energy = 0.0_f64;
                for sample in 0..rate / 4 {
                    let phase =
                        std::f64::consts::TAU * frequency * f64::from(sample) / f64::from(rate);
                    let input = phase.sin() as f32 * 0.1;
                    let filtered = bell.tick(&[input].into())[0];
                    let actual = shaped.tick([input, 0.0]);
                    let bypass = bypassed.tick([input, 0.0]);
                    assert!(actual[0].is_finite() && filtered.is_finite());
                    assert_eq!(
                        actual[1], 0.0,
                        "coin EQ must keep channel histories independent"
                    );
                    if sample >= rate / 20 {
                        input_energy += f64::from(input).powi(2);
                        bell_energy += f64::from(filtered).powi(2);
                        shaped_energy += f64::from(actual[0]).powi(2);
                        bypassed_energy += f64::from(bypass[0]).powi(2);
                    }
                }
                let single_gain = 10.0 * (bell_energy / input_energy).log10();
                assert!(
                    (single_gain - gain_db).abs() < 0.1,
                    "{rate} Hz, {frequency} Hz bell: {single_gain:.3} dB"
                );
                let combined_gain = 10.0 * (shaped_energy / bypassed_energy).log10();
                assert!(
                    (gain_db - 3.5..gain_db + 0.05).contains(&combined_gain),
                    "{rate} Hz, {frequency} Hz combined bells: {combined_gain:.3} dB"
                );
            }
        }
    }

    #[test]
    fn coin_swell_retires_and_unsupported_registers_stay_silent() {
        let table = SineTable::new();
        let mut coins = formation(48_000, ToneKind::FlyingCoins, 123);
        let duration = coins.source.as_ref().unwrap().envelope.duration();
        let mut edge = 0.0_f64;
        let mut middle = 0.0_f64;
        for age in 0..duration {
            let frame = coins.tick(&table);
            let energy = frame
                .iter()
                .map(|value| f64::from(*value).powi(2))
                .sum::<f64>();
            if age < 4_800 || age >= duration - 4_800 {
                edge += energy;
            }
            if (duration / 2 - 4_800..duration / 2 + 4_800).contains(&age) {
                middle += energy;
            }
        }
        assert!(edge < middle * 0.0001);
        assert!(!coins.is_active());
        for _ in 0..48_000 {
            coins.tick(&table);
        }
        assert!(coins.tick(&table).iter().all(|value| value.abs() < 1e-12));
        for (rate, kind) in [
            (8_000, ToneKind::FlyingCoins),
            (22_050, ToneKind::FlyingCoins),
            (24_000, ToneKind::LaserBeams),
            (30_000, ToneKind::LaserBeams),
        ] {
            let mut unsupported = formation(rate, kind, 123);
            assert!(!unsupported.is_active());
            for _ in 0..rate {
                assert_eq!(unsupported.tick(&table), [0.0; 2]);
            }
        }
        for (rate, kind) in [
            (24_000, ToneKind::FlyingCoins),
            (32_000, ToneKind::LaserBeams),
        ] {
            assert!(formation(rate, kind, 123).is_active());
        }
    }
}
