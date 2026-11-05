use fundsp::hacker32::{AudioNode, Biquad, BiquadCoefs, Pinkpass};
use fundsp::math::softsign;

use crate::{
    AmbientSnapshot, ClockState, Pan, SpatialMode, Symbol,
    dsp::{HighPass, LowPass, Osc, PanGains, SineTable, Smooth},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VoiceKind {
    Murmur,
    Stutter,
}

const EXCITATION_GAIN_6_DB: f32 = 1.995_262_3;
const FORMANT_TRANSITION_SECONDS: f32 = 0.030;
const PRIMARY_FORMANTS: usize = 3;
const RESONANCE_COUNT: usize = PRIMARY_FORMANTS + 4;

struct Excitation {
    pink: Pinkpass<f32>,
    sine: Osc,
}

impl Excitation {
    fn new(rate: f32, rng: &mut fastrand::Rng) -> Self {
        let mut pink = Pinkpass::new();
        pink.set_sample_rate(f64::from(rate));
        Self {
            pink,
            sine: Osc::new(30.0 + rng.f32() * 80.0, rate, rng.f32()),
        }
    }

    fn tick(&mut self, white: f32, table: &SineTable) -> f32 {
        let noise = softsign(self.pink.tick(&[white].into())[0]);
        let mixed = noise + self.sine.tick(table) * 0.1;
        softsign(mixed * EXCITATION_GAIN_6_DB)
    }
}

#[derive(Clone, Copy)]
enum Vowel {
    A,
    E,
    I,
    O,
    U,
    Eh,
    Uh,
    Schwa,
}

#[derive(Clone, Copy)]
enum VoiceProfile {
    Low,
    High,
}

#[derive(Clone, Copy)]
struct Formant {
    frequency_hz: f32,
    /// Nominal per-section bandwidth; broad bands are calibrated by their response.
    bandwidth_hz: f32,
    peak_db: f32,
}

impl Formant {
    const fn new(frequency_hz: f32, bandwidth_hz: f32, peak_db: f32) -> Self {
        Self {
            frequency_hz,
            bandwidth_hz,
            peak_db,
        }
    }

    fn interpolate(self, to: Self, t: f32) -> Self {
        Self {
            frequency_hz: self.frequency_hz + (to.frequency_hz - self.frequency_hz) * t,
            bandwidth_hz: self.bandwidth_hz + (to.bandwidth_hz - self.bandwidth_hz) * t,
            peak_db: self.peak_db + (to.peak_db - self.peak_db) * t,
        }
    }
}

#[must_use]
#[derive(Clone, Copy)]
struct VocalShape {
    formants: [Formant; PRIMARY_FORMANTS],
    low: Option<Formant>,
    body: Option<Formant>,
    upper: [Option<Formant>; 2],
}

impl VocalShape {
    fn new(hz: [f32; 3], bandwidths: [f32; 3], peaks_db: [f32; 3]) -> Self {
        Self {
            formants: std::array::from_fn(|i| Formant::new(hz[i], bandwidths[i], peaks_db[i])),
            low: None,
            body: None,
            upper: [None; 2],
        }
    }

    fn with_low(mut self, formant: Formant) -> Self {
        self.low = Some(formant);
        self
    }

    fn with_body(mut self, formant: Formant) -> Self {
        self.body = Some(formant);
        self
    }

    fn with_upper(mut self, formants: [Option<Formant>; 2]) -> Self {
        self.upper = formants;
        self
    }

    fn bands(self) -> [Option<Formant>; RESONANCE_COUNT] {
        let [first, second, third] = self.formants;
        [
            Some(first),
            Some(second),
            Some(third),
            self.low,
            self.body,
            self.upper[0],
            self.upper[1],
        ]
    }
}

#[derive(Clone, Copy)]
struct VocalMotion {
    formant_scale: f32,
    flutter_phase: f32,
}

#[derive(Clone, Copy)]
enum RippleFlank {
    Lower,
    Upper,
}

struct SpectralRipple {
    filter: Biquad<f32>,
    offset_bandwidths: f32,
    bandwidth_ratio: f32,
    peak_gain: f32,
    phase_offset: f32,
}

impl SpectralRipple {
    fn new(rate: f32, flank: RippleFlank, rng: &mut fastrand::Rng) -> Self {
        let direction = match flank {
            RippleFlank::Lower => -1.0,
            RippleFlank::Upper => 1.0,
        };
        Self {
            filter: Biquad::with_coefs(BiquadCoefs::bell(rate, 400.0, 1.0, 1.0)),
            offset_bandwidths: direction * (0.4 + rng.f32() * 0.55),
            bandwidth_ratio: 0.08 + rng.f32() * 0.08,
            peak_gain: 10.0_f32.powf((0.8 + rng.f32()) / 20.0),
            phase_offset: rng.f32(),
        }
    }

    fn control(&mut self, center: f32, bandwidth: f32, phase: f32, rate: f32) {
        let drift = (std::f32::consts::TAU * (phase + self.phase_offset)).sin() * 0.04;
        let hz = center + (self.offset_bandwidths + drift) * bandwidth;
        let width = (bandwidth * self.bandwidth_ratio).clamp(8.0, 60.0);
        let available = smoothstep(((rate * 0.42 - hz) / width).clamp(0.0, 1.0));
        let gain = 1.0 + (self.peak_gain - 1.0) * available;
        let hz = hz.min(rate * 0.42);
        self.filter
            .set_coefs(BiquadCoefs::bell(rate, hz, hz / width, gain));
    }

    fn tick(&mut self, input: f32) -> f32 {
        self.filter.tick(&[input].into())[0]
    }
}

struct Resonance {
    sections: [Biquad<f32>; 2],
    ripples: Option<[SpectralRipple; 2]>,
    gain: Smooth,
}

impl Resonance {
    fn new(rate: f32) -> Self {
        Self {
            sections: std::array::from_fn(|_| {
                Biquad::with_coefs(formant_filter(rate, 400.0, 50.0))
            }),
            ripples: None,
            gain: Smooth::new(0.0, 0.005, rate),
        }
    }

    #[must_use]
    fn with_ripples(mut self, rate: f32, rng: &mut fastrand::Rng) -> Self {
        self.ripples = Some(
            [RippleFlank::Lower, RippleFlank::Upper]
                .map(|flank| SpectralRipple::new(rate, flank, rng)),
        );
        self
    }

    fn control(
        &mut self,
        from: Option<Formant>,
        to: Option<Formant>,
        transition: f32,
        motion: VocalMotion,
        rate: f32,
        lowpass: &LowPass,
    ) {
        let (formant, amplitude) = match (from, to) {
            (Some(from), Some(to)) => (from.interpolate(to, transition), 1.0),
            (Some(from), None) => (from, 1.0 - transition),
            (None, Some(to)) => (to, transition),
            (None, None) => {
                self.gain.target(0.0);
                return;
            }
        };
        let hz = formant.frequency_hz * motion.formant_scale;
        let bandwidth = formant.bandwidth_hz * hz / formant.frequency_hz;
        let fade_width = bandwidth.min(rate * 0.02);
        let available = smoothstep(((rate * 0.42 - hz) / fade_width).clamp(0.0, 1.0));
        if available == 0.0 {
            self.gain.target(0.0);
            return;
        }
        let coefs = formant_filter(rate, hz, bandwidth);
        for section in &mut self.sections {
            section.set_coefs(coefs);
        }
        if let Some(ripples) = &mut self.ripples {
            for ripple in ripples {
                ripple.control(hz, bandwidth, motion.flutter_phase, rate);
            }
        }
        // Correct source tilt and output attenuation once for the complete
        // branch. Two sections keep its skirts out of neighboring valleys.
        let gain =
            amplitude * available * 10.0_f32.powf(formant.peak_db / 20.0) * (hz / 1000.0).sqrt()
                / lowpass.magnitude_at(hz);
        self.gain.target(gain);
    }

    fn tick(&mut self, excitation: f32) -> f32 {
        let mut filtered = self
            .sections
            .iter_mut()
            .fold(excitation, |sample, section| {
                section.tick(&[sample].into())[0]
            });
        if let Some(ripples) = &mut self.ripples {
            for ripple in ripples {
                filtered = ripple.tick(filtered);
            }
        }
        filtered * self.gain.tick()
    }
}

impl Vowel {
    // A and rounded vowels follow the reference's spectral envelope; other
    // vowel identities use whisper-informed anchors. These are synthesis settings,
    // not demographic measurements; see demonic-voices-research.md.
    fn shape(self, profile: VoiceProfile) -> VocalShape {
        let hz = match (self, profile) {
            (Self::A, VoiceProfile::Low) => [820.0, 1190.0, 2540.0],
            (Self::A, VoiceProfile::High) => [840.0, 1220.0, 2600.0],
            (Self::E, VoiceProfile::Low) => [623.0, 2183.5, 2848.0],
            (Self::E, VoiceProfile::High) => [674.5, 2489.5, 3123.0],
            (Self::I, VoiceProfile::Low) => [495.0, 2412.0, 2979.0],
            (Self::I, VoiceProfile::High) => [511.0, 2738.0, 3239.0],
            (Self::O, VoiceProfile::Low) => [365.0, 780.0, 2370.0],
            (Self::O, VoiceProfile::High) => [385.0, 805.0, 2420.0],
            (Self::U, VoiceProfile::Low) => [345.0, 730.0, 2320.0],
            (Self::U, VoiceProfile::High) => [365.0, 760.0, 2370.0],
            (Self::Eh, VoiceProfile::Low) => [751.0, 1955.0, 2717.0],
            (Self::Eh, VoiceProfile::High) => [838.0, 2241.0, 3007.0],
            (Self::Uh, VoiceProfile::Low) => [756.0, 1328.0, 2639.0],
            (Self::Uh, VoiceProfile::High) => [800.0, 1450.0, 2770.0],
            (Self::Schwa, VoiceProfile::Low) => [758.0, 1485.5, 2660.5],
            (Self::Schwa, VoiceProfile::High) => [795.0, 1560.0, 2800.0],
        };
        match self {
            Self::A => VocalShape::new(hz, [110.0, 100.0, 513.0], [13.0, 2.5, -3.6])
                .with_low(Formant::new(134.0, 222.0, -2.0))
                .with_body(Formant::new(725.0, 262.0, -5.7))
                .with_upper([Some(Formant::new(3090.0, 353.0, -4.6)), None]),
            Self::O => VocalShape::new(hz, [130.0, 69.0, 256.0], [12.0, -4.0, -33.0])
                .with_body(Formant::new(2800.0, 1756.0, -27.0))
                .with_upper([
                    Some(Formant::new(2720.0, 232.0, -27.1)),
                    Some(Formant::new(3700.0, 260.0, -32.5)),
                ]),
            Self::U => VocalShape::new(hz, [120.0, 65.0, 250.0], [12.0, -5.0, -34.0])
                .with_body(Formant::new(2740.0, 1650.0, -28.0))
                .with_upper([
                    Some(Formant::new(2660.0, 225.0, -28.0)),
                    Some(Formant::new(3670.0, 250.0, -33.5)),
                ]),
            Self::E | Self::I => VocalShape::new(hz, [140.0, 160.0, 190.0], [12.0, 2.0, -5.0]),
            Self::Eh | Self::Uh | Self::Schwa => {
                VocalShape::new(hz, [150.0, 180.0, 220.0], [12.0, 1.0, -6.0])
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Phone {
    Vowel(Vowel),
    LabialStop,
    AlveolarStop,
    VelarStop,
    Sibilant,
    Fricative,
    Affricate,
    Lateral,
    Nasal,
    Rhotic,
    PalatalGlide,
    LabialGlide,
}

impl Phone {
    fn weight(self) -> f32 {
        match self {
            Self::Vowel(_) => 1.0,
            Self::LabialStop | Self::AlveolarStop | Self::VelarStop => 0.22,
            _ => 0.45,
        }
    }

    fn shape(self, profile: VoiceProfile) -> VocalShape {
        let (hz, bandwidths) = match self {
            Self::Vowel(vowel) => return vowel.shape(profile),
            Self::LabialStop => ([400.0, 900.0, 2100.0], [150.0, 300.0, 450.0]),
            Self::AlveolarStop | Self::Sibilant => {
                ([3500.0, 4500.0, 5500.0], [500.0, 700.0, 900.0])
            }
            Self::VelarStop => ([1500.0, 2100.0, 3000.0], [200.0, 250.0, 400.0]),
            Self::Affricate => ([2500.0, 3200.0, 4000.0], [400.0, 500.0, 600.0]),
            Self::Fricative => ([1200.0, 3000.0, 5000.0], [700.0, 900.0, 1000.0]),
            Self::Lateral => ([400.0, 1100.0, 2600.0], [60.0, 100.0, 120.0]),
            Self::Nasal => ([250.0, 1200.0, 2100.0], [80.0, 120.0, 140.0]),
            Self::Rhotic => ([450.0, 1100.0, 1600.0], [70.0, 100.0, 120.0]),
            Self::PalatalGlide => return Vowel::I.shape(profile),
            Self::LabialGlide => return Vowel::U.shape(profile),
        };
        VocalShape::new(hz, bandwidths, [0.0, -6.0, -12.0])
    }
}

/// Noise articulations follow English letter names, including double-u.
fn pronunciation(letter: u8) -> &'static [Phone] {
    use self::Vowel as V;
    use Phone::*;
    match letter {
        b'A' => &[Vowel(V::E), Vowel(V::I)],
        b'B' | b'P' => &[LabialStop, Vowel(V::I)],
        b'C' | b'Z' => &[Sibilant, Vowel(V::I)],
        b'D' | b'T' => &[AlveolarStop, Vowel(V::I)],
        b'E' => &[Vowel(V::I)],
        b'F' => &[Vowel(V::Eh), Fricative],
        b'G' => &[Affricate, Vowel(V::I)],
        b'H' => &[Vowel(V::E), Vowel(V::I), Affricate],
        b'I' => &[Vowel(V::A), Vowel(V::I)],
        b'J' => &[Affricate, Vowel(V::E), Vowel(V::I)],
        b'K' => &[VelarStop, Vowel(V::E), Vowel(V::I)],
        b'L' => &[Vowel(V::Eh), Lateral],
        b'M' | b'N' => &[Vowel(V::Eh), Nasal],
        b'O' => &[Vowel(V::O), Vowel(V::U)],
        b'Q' => &[VelarStop, PalatalGlide, Vowel(V::U)],
        b'R' => &[Vowel(V::A), Rhotic],
        b'S' => &[Vowel(V::Eh), Sibilant],
        b'U' => &[PalatalGlide, Vowel(V::U)],
        b'V' => &[Fricative, Vowel(V::I)],
        b'W' => &[
            AlveolarStop,
            Vowel(V::Uh),
            LabialStop,
            Vowel(V::Schwa),
            Lateral,
            PalatalGlide,
            Vowel(V::U),
        ],
        b'X' => &[Vowel(V::Eh), VelarStop, Sibilant],
        b'Y' => &[LabialGlide, Vowel(V::A), Vowel(V::I)],
        b'0' => &[Sibilant, Vowel(V::I), Rhotic, Vowel(V::O), Vowel(V::U)],
        b'1' => &[LabialGlide, Vowel(V::Uh), Nasal],
        b'2' => &[AlveolarStop, Vowel(V::U)],
        b'3' => &[Fricative, Rhotic, Vowel(V::I)],
        b'4' => &[Fricative, Vowel(V::O), Rhotic],
        b'5' => &[Fricative, Vowel(V::A), Vowel(V::I), Fricative],
        b'6' => &[Sibilant, Vowel(V::I), VelarStop, Sibilant],
        b'7' => &[Sibilant, Vowel(V::Eh), Fricative, Vowel(V::Schwa), Nasal],
        b'8' => &[Vowel(V::E), Vowel(V::I), AlveolarStop],
        b'9' => &[Nasal, Vowel(V::A), Vowel(V::I), Nasal],
        _ => unreachable!("ticker punctuation is skipped before pronunciation"),
    }
}

struct Stutter {
    phase: f32,
    rate: f32,
    from_rate: f32,
    target_rate: f32,
    amplitude: f32,
    from_amplitude: f32,
    target_amplitude: f32,
    elapsed: u32,
    duration: u32,
}

impl Stutter {
    fn new(rate: f32, rng: &mut fastrand::Rng) -> Self {
        let hz = 3.0 + rng.f32() * 17.0;
        let mut this = Self {
            phase: 0.0,
            rate: hz,
            from_rate: hz,
            target_rate: hz,
            amplitude: 0.7,
            from_amplitude: 0.7,
            target_amplitude: 0.7,
            elapsed: 0,
            duration: 1,
        };
        this.retarget(rate, rng);
        this
    }

    fn retarget(&mut self, rate: f32, rng: &mut fastrand::Rng) {
        self.from_rate = self.rate;
        self.target_rate = 3.0 + rng.f32() * 17.0;
        self.from_amplitude = self.amplitude;
        self.target_amplitude = 0.25 + rng.f32() * 0.75;
        self.elapsed = 0;
        self.duration = ((0.3 + rng.f32() * 1.2) * rate).round() as u32;
    }

    fn tick(&mut self, rate: f32, rng: &mut fastrand::Rng) -> f32 {
        if self.elapsed >= self.duration {
            self.retarget(rate, rng);
        }
        let t = smoothstep(self.elapsed as f32 / self.duration as f32);
        self.rate = self.from_rate + (self.target_rate - self.from_rate) * t;
        self.amplitude = self.from_amplitude + (self.target_amplitude - self.from_amplitude) * t;
        self.elapsed += 1;
        self.phase = (self.phase + self.rate / rate).fract();
        // Rounded pulse edges avoid broadband clicks at high stuttering rates.
        let gate = (self.phase / 0.04)
            .min((0.56 - self.phase) / 0.04)
            .clamp(0.0, 1.0);
        smoothstep(gate) * self.amplitude
    }
}

enum Delivery {
    Murmur,
    Stutter(Stutter),
}

struct Voice {
    phones: &'static [Phone],
    phone_weight: f32,
    profile: VoiceProfile,
    age: u32,
    duration: u32,
    excitation: Excitation,
    resonances: [Resonance; RESONANCE_COUNT],
    lowpass: LowPass,
    frication: HighPass,
    mutation: f32,
    flutter_phase: f32,
    flutter_hz: f32,
    delivery: Delivery,
    phone_gain: f32,
    fricative_gain: f32,
}

impl Voice {
    fn kind(&self) -> VoiceKind {
        match self.delivery {
            Delivery::Murmur => VoiceKind::Murmur,
            Delivery::Stutter(_) => VoiceKind::Stutter,
        }
    }

    fn new(rate: f32, kind: VoiceKind, letter: u8, rng: &mut fastrand::Rng) -> Self {
        let phones = pronunciation(letter);
        // Detail draws stay independent of the utterance and excitation timeline.
        let mut detail_rng = fastrand::Rng::with_seed(rng.clone().u64(..) ^ 0x0052_4950_504c_4553);
        let mut this = Self {
            phones,
            phone_weight: phones.iter().map(|phone| phone.weight()).sum(),
            profile: if rng.bool() {
                VoiceProfile::Low
            } else {
                VoiceProfile::High
            },
            age: 0,
            duration: ((0.4 + rng.f32() * 6.6) * rate).round() as u32,
            excitation: Excitation::new(rate, rng),
            resonances: std::array::from_fn(|index| {
                let resonance = Resonance::new(rate);
                if index < PRIMARY_FORMANTS {
                    resonance.with_ripples(rate, &mut detail_rng)
                } else {
                    resonance
                }
            }),
            lowpass: LowPass::new(rate, 6200.0),
            frication: HighPass::new(rate, (rate * 0.35).min(2800.0)),
            mutation: if rng.f32() < 0.25 {
                rng.f32() * 0.08 - 0.06
            } else {
                0.0
            },
            flutter_phase: rng.f32(),
            flutter_hz: 0.15 + rng.f32() * 0.65,
            delivery: match kind {
                VoiceKind::Murmur => Delivery::Murmur,
                VoiceKind::Stutter => Delivery::Stutter(Stutter::new(rate, rng)),
            },
            phone_gain: 1.0,
            fricative_gain: 0.0,
        };
        this.control(rate);
        this
    }

    fn control(&mut self, rate: f32) {
        let position = self.age as f32 / self.duration as f32 * self.phone_weight;
        let mut start = 0.0;
        let mut index = 0;
        for (i, phone) in self.phones.iter().enumerate() {
            index = i;
            if position < start + phone.weight() {
                break;
            }
            start += phone.weight();
        }
        let phone = self.phones[index];
        let t = ((position - start) / phone.weight()).clamp(0.0, 1.0);
        let from = phone.shape(self.profile);
        let to = self.phones[(index + 1).min(self.phones.len() - 1)].shape(self.profile);
        let phone_seconds = self.duration as f32 / rate * phone.weight() / self.phone_weight;
        let transition = formant_transition(t, phone_seconds);
        let progress = self.age as f32 / self.duration as f32;
        let excursion = 4.0 * progress * (1.0 - progress);
        let motion = VocalMotion {
            formant_scale: 1.0 + self.mutation * excursion,
            flutter_phase: self.flutter_phase,
        };
        for ((resonance, from), to) in self.resonances.iter_mut().zip(from.bands()).zip(to.bands())
        {
            resonance.control(from, to, transition, motion, rate, &self.lowpass);
        }
        self.phone_gain = match phone {
            Phone::LabialStop | Phone::AlveolarStop | Phone::VelarStop => {
                if t < 0.55 {
                    0.03
                } else {
                    (1.0 - (t - 0.55) / 0.45) * 1.5
                }
            }
            Phone::Nasal => 0.5,
            _ => 1.0,
        };
        self.fricative_gain = match phone {
            Phone::Sibilant | Phone::Affricate => 0.12,
            Phone::Fricative => 0.07,
            _ => 0.0,
        };
    }

    fn tick(&mut self, rate: f32, table: &SineTable, rng: &mut fastrand::Rng) -> f32 {
        if self.age >= self.duration {
            return 0.0;
        }
        if self.age.is_multiple_of(64) {
            self.control(rate);
        }
        let white = rng.f32() * 2.0 - 1.0;
        let excitation = self.excitation.tick(white, table);
        let formants: f32 = self
            .resonances
            .iter_mut()
            .map(|resonance| resonance.tick(excitation))
            .sum();
        let seconds = self.age as f32 / rate;
        let left = (self.duration - self.age) as f32 / rate;
        let envelope = smoothstep((seconds / 0.045).min(1.0)) * smoothstep((left / 0.12).min(1.0));
        let flutter = 0.85 + 0.15 * table.at(self.flutter_phase);
        self.flutter_phase = (self.flutter_phase + self.flutter_hz / rate).fract();
        let delivery = match &mut self.delivery {
            Delivery::Murmur => 1.0,
            Delivery::Stutter(stutter) => stutter.tick(rate, rng) * 0.55,
        };
        self.age += 1;
        let shaped = self.lowpass.tick(
            formants * self.phone_gain + self.frication.tick(excitation) * self.fricative_gain,
        );
        shaped * envelope * flutter * delivery * 0.04
    }
}

struct VoiceSlot {
    voice: Option<Voice>,
    delay: Vec<f32>,
    cursor: usize,
    delay_frames: usize,
    pan: [f32; 2],
    leading: usize,
    tail: usize,
}

impl VoiceSlot {
    fn new(rate: u32) -> Self {
        Self {
            voice: None,
            delay: vec![0.0; (rate as f32 * 0.008).ceil() as usize + 1],
            cursor: 0,
            delay_frames: 0,
            pan: [0.0; 2],
            leading: 0,
            tail: 0,
        }
    }

    fn start(&mut self, voice: Voice, position: f32, rate: f32) {
        assert!(self.voice.is_none());
        self.delay.fill(0.0);
        self.cursor = 0;
        self.delay_frames = ((0.002 + position.abs() * 0.006) * rate).round() as usize;
        self.pan = PanGains::new(Pan::new(position).expect("bounded voice position"))
            .for_mode(SpatialMode::Haas);
        self.leading = usize::from(position > 0.0);
        self.tail = self.delay_frames;
        self.voice = Some(voice);
    }

    fn tick(&mut self, rate: f32, table: &SineTable, rng: &mut fastrand::Rng) -> [f32; 2] {
        let Some(voice) = &mut self.voice else {
            return [0.0; 2];
        };
        let value = voice.tick(rate, table, rng);
        let delayed =
            self.delay[(self.cursor + self.delay.len() - self.delay_frames) % self.delay.len()];
        self.delay[self.cursor] = value;
        self.cursor = (self.cursor + 1) % self.delay.len();
        let mut out = [0.0; 2];
        out[self.leading] = value * self.pan[self.leading];
        out[1 - self.leading] = delayed * self.pan[1 - self.leading];
        if voice.age >= voice.duration {
            if self.tail == 0 {
                self.voice = None;
            } else {
                self.tail -= 1;
            }
        }
        out
    }
}

struct Schedule {
    next: u64,
    generated: u64,
}

pub(super) struct DemonicVoices {
    rate: f32,
    slots: [VoiceSlot; 2],
    murmur: Schedule,
    stutter: Schedule,
    letter: usize,
    rng: fastrand::Rng,
}

impl DemonicVoices {
    pub fn new(rate: u32, seed: u64) -> Self {
        Self {
            rate: rate as f32,
            slots: std::array::from_fn(|_| VoiceSlot::new(rate)),
            murmur: Schedule {
                next: u64::from(rate) * 3,
                generated: 0,
            },
            stutter: Schedule {
                next: u64::from(rate) * 6,
                generated: 0,
            },
            letter: 0,
            rng: fastrand::Rng::with_seed(seed),
        }
    }

    pub fn control(&mut self, sample: u64, symbol: Symbol, focus: f32) {
        for kind in [VoiceKind::Murmur, VoiceKind::Stutter] {
            let schedule = match kind {
                VoiceKind::Murmur => &mut self.murmur,
                VoiceKind::Stutter => &mut self.stutter,
            };
            if sample < schedule.next {
                continue;
            }
            schedule.next = sample + ((2.0 + self.rng.f32() * 9.0) * self.rate) as u64;
            if focus > 0.7 {
                continue;
            }
            let first = self.rng.usize(..2);
            let Some(index) = [first, 1 - first]
                .into_iter()
                .find(|&i| self.slots[i].voice.is_none())
            else {
                continue;
            };
            let bytes = symbol.as_str().as_bytes();
            while !bytes[self.letter % bytes.len()].is_ascii_alphanumeric() {
                self.letter += 1;
            }
            let letter = bytes[self.letter % bytes.len()];
            self.letter = (self.letter + 1) % bytes.len();
            let voice = Voice::new(self.rate, kind, letter, &mut self.rng);
            let position = (0.45 + self.rng.f32() * 0.45) * if index == 0 { -1.0 } else { 1.0 };
            self.slots[index].start(voice, position, self.rate);
            schedule.generated += 1;
        }
    }

    pub fn tick(&mut self, table: &SineTable, levels: [f32; 2]) -> [f32; 2] {
        let mut out = [0.0; 2];
        for slot in &mut self.slots {
            let gain = match slot.voice.as_ref().map(Voice::kind) {
                Some(VoiceKind::Murmur) => levels[0],
                Some(VoiceKind::Stutter) => levels[1],
                None => continue,
            };
            let frame = slot.tick(self.rate, table, &mut self.rng);
            for i in 0..2 {
                out[i] += frame[i] * gain;
            }
        }
        out
    }

    pub fn snapshots(&self, sample: u64, focus: f32) -> [AmbientSnapshot; 2] {
        [
            (&self.murmur, VoiceKind::Murmur),
            (&self.stutter, VoiceKind::Stutter),
        ]
        .map(|(schedule, kind)| AmbientSnapshot {
            generated: schedule.generated,
            active: self
                .slots
                .iter()
                .filter(|slot| {
                    slot.voice
                        .as_ref()
                        .is_some_and(|voice| voice.kind() == kind)
                })
                .count(),
            clock: ClockState::at(schedule.next, sample, self.rate, focus > 0.7),
        })
    }
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn formant_transition(progress: f32, phone_seconds: f32) -> f32 {
    let window = FORMANT_TRANSITION_SECONDS.min(phone_seconds * 0.55);
    let remaining = (1.0 - progress) * phone_seconds;
    smoothstep((1.0 - remaining / window).clamp(0.0, 1.0))
}

fn formant_filter(rate: f32, hz: f32, bandwidth: f32) -> BiquadCoefs<f32> {
    let mut coefs = BiquadCoefs::resonator(rate, hz, hz / bandwidth);
    // Peak normalization keeps distorted excitation from gaining excessive level
    // at narrow resonances, particularly at high sample rates.
    let gain = (1.0 - coefs.a2).sqrt();
    coefs.b0 *= gain;
    coefs.b2 *= gain;
    coefs
}

#[cfg(test)]
mod tests {
    use super::*;
    use fundsp::{fft::real_fft, math::Complex32};

    const FFT_LEN: usize = 32_768;

    fn averaged_spectrum(samples: &[f32], hann: &[f32]) -> Vec<f64> {
        let mut input = vec![0.0; FFT_LEN];
        let mut output = vec![Complex32::new(0.0, 0.0); FFT_LEN / 2 + 1];
        let mut power = vec![0.0; output.len()];
        for window in samples.windows(FFT_LEN).step_by(FFT_LEN / 4) {
            for ((sample, value), weight) in input.iter_mut().zip(window).zip(hann) {
                *sample = value * weight;
            }
            real_fft(&input, &mut output);
            for (sum, bin) in power.iter_mut().zip(&output) {
                *sum += f64::from(bin.norm_sqr());
            }
        }
        power
    }

    fn spectral_band(power: &[f64], rate: f32, low_hz: f32, high_hz: f32) -> &[f64] {
        let bin_hz = rate / FFT_LEN as f32;
        let first = (low_hz / bin_hz).ceil() as usize;
        let end = (high_hz / bin_hz).ceil() as usize;
        &power[first..end.min(power.len())]
    }

    fn spectral_peak(power: &[f64], rate: f32, low_hz: f32, high_hz: f32) -> (f32, f64) {
        let bin_hz = rate / FFT_LEN as f32;
        let radius = (20.0 / bin_hz).ceil() as usize;
        let first = (low_hz / bin_hz).ceil() as usize;
        let end = (high_hz / bin_hz).ceil() as usize;
        (first..end)
            .map(|center| {
                let band = &power[center - radius..=center + radius];
                (
                    center as f32 * bin_hz,
                    band.iter().sum::<f64>() / band.len() as f64,
                )
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap()
    }

    fn spectral_width(power: &[f64], rate: f32, peak_hz: f32) -> f32 {
        let bin_hz = rate / FFT_LEN as f32;
        let radius = (20.0 / bin_hz).ceil() as usize;
        let mean = |center| {
            let band = &power[center - radius..=center + radius];
            band.iter().sum::<f64>() / band.len() as f64
        };
        let peak = (peak_hz / bin_hz).round() as usize;
        let half = mean(peak) * 0.5;
        let lower = (radius..peak).rfind(|&bin| mean(bin) < half).unwrap();
        let upper = (peak + 1..power.len() - radius)
            .find(|&bin| mean(bin) < half)
            .unwrap();
        (upper - lower) as f32 * bin_hz
    }

    #[test]
    fn rounded_and_open_vowels_keep_reference_envelopes_and_supporting_resonances() {
        let table = SineTable::new();
        let hann: Vec<_> = (0..FFT_LEN)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_LEN - 1) as f32).cos())
            .collect();
        for (rate, profile, seed, sine_hz) in [
            (8_000.0, VoiceProfile::Low, 11, 30.0),
            (48_000.0, VoiceProfile::Low, 123, 70.0),
            (48_000.0, VoiceProfile::High, 90_210, 110.0),
            (192_000.0, VoiceProfile::High, 123, 30.0),
        ] {
            let render = |phones| {
                let mut rng = fastrand::Rng::with_seed(seed);
                let mut voice = Voice::new(rate, VoiceKind::Murmur, b'E', &mut rng);
                voice.profile = profile;
                voice.phones = phones;
                voice.phone_weight = 1.0;
                voice.mutation = 0.0;
                let warmup = (rate * 0.3) as u32;
                let sample_count = ((rate * 2.0) as usize).max(FFT_LEN * 2);
                voice.duration = warmup + sample_count as u32 + (rate * 0.2) as u32;
                voice.excitation.sine = Osc::new(sine_hz, rate, 0.0);
                for _ in 0..warmup {
                    voice.tick(rate, &table, &mut rng);
                }
                let samples: Vec<_> = (0..sample_count)
                    .map(|_| voice.tick(rate, &table, &mut rng))
                    .collect();
                assert!(samples.iter().all(|sample| sample.is_finite()));
                averaged_spectrum(&samples, &hann)
            };
            let rounded = render(&[Phone::Vowel(Vowel::O)]);
            let open = render(&[Phone::Vowel(Vowel::A)]);
            let share = |power: &[f64], low, high| {
                spectral_band(power, rate, low, high).iter().sum::<f64>()
                    / power.iter().sum::<f64>()
            };
            let mean = |power: &[f64], low, high| {
                let band = spectral_band(power, rate, low, high);
                band.iter().sum::<f64>() / band.len() as f64
            };
            let rounded_main = spectral_peak(&rounded, rate, 250.0, 600.0);
            let open_main = spectral_peak(&open, rate, 650.0, 1000.0);
            let rounded_upper = share(&rounded, 2000.0, 4000.0);
            let open_upper = share(&open, 2000.0, 4000.0);
            let rounded_width = spectral_width(&rounded, rate, rounded_main.0);
            let open_width = spectral_width(&open, rate, open_main.0);
            assert!(
                (320.0..440.0).contains(&rounded_main.0),
                "{rate} Hz seed {seed}: rounded peak {}",
                rounded_main.0
            );
            assert!(
                (760.0..920.0).contains(&open_main.0),
                "{rate} Hz seed {seed}: open peak {}",
                open_main.0
            );
            assert!(share(&rounded, 250.0, 600.0) > 0.90);
            assert!(share(&open, 600.0, 1200.0) > 0.65);
            assert!(
                (60.0..160.0).contains(&rounded_width),
                "{rate} Hz seed {seed}: rounded width {rounded_width}"
            );
            assert!(
                (45.0..150.0).contains(&open_width),
                "{rate} Hz seed {seed}: open width {open_width}"
            );
            assert!(
                (0.0003..0.006).contains(&rounded_upper),
                "{rate} Hz seed {seed}: rounded upper {rounded_upper}"
            );
            assert!(
                (0.04..0.22).contains(&open_upper),
                "{rate} Hz seed {seed}: open upper {open_upper}"
            );
            assert!(open_upper > rounded_upper * 15.0);

            let rounded_second = spectral_peak(&rounded, rate, 700.0, 900.0).1;
            let second_db = 10.0 * (rounded_second / rounded_main.1).log10();
            assert!(
                (-26.0..-12.0).contains(&second_db),
                "{rate} Hz seed {seed}: rounded secondary {second_db} dB"
            );
            let first_upper = spectral_peak(&rounded, rate, 2200.0, 2500.0).1;
            let second_upper = spectral_peak(&rounded, rate, 2600.0, 2850.0).1;
            let bridge = mean(&rounded, 2470.0, 2590.0);
            let valley = mean(&rounded, 1400.0, 1800.0);
            eprintln!(
                "{rate} Hz seed {seed}: O/A peaks {:.1}/{:.1} Hz, widths {rounded_width:.1}/{open_width:.1} Hz, upper shares {rounded_upper:.5}/{open_upper:.5}, secondary {second_db:.1} dB, bridge/valley {:.2}",
                rounded_main.0,
                open_main.0,
                bridge / valley
            );
            assert!(
                bridge > valley,
                "{rate} Hz seed {seed}: bridge/valley {}",
                bridge / valley
            );
            assert!(bridge > first_upper.max(second_upper) * 0.06);
            assert!(second_upper > rounded_main.1 * 0.00003);
            if rate >= 48_000.0 {
                let high_detail = spectral_peak(&rounded, rate, 3500.0, 3950.0);
                assert!(
                    (3600.0..3900.0).contains(&high_detail.0),
                    "{rate} Hz seed {seed}: high detail peak {}",
                    high_detail.0
                );
                assert!(
                    (0.08..0.7).contains(&(high_detail.1 / second_upper)),
                    "{rate} Hz seed {seed}: high/upper detail {}",
                    high_detail.1 / second_upper
                );
            }

            let open_upper_peak = spectral_peak(&open, rate, 2400.0, 2800.0).1;
            let open_support = spectral_peak(&open, rate, 2950.0, 3300.0).1;
            assert!((0.2..2.0).contains(&(open_support / open_upper_peak)));
        }
    }

    #[test]
    fn murmurs_and_stutters_keep_the_approved_register_through_live_excursions() {
        let rate = 48_000.0;
        let table = SineTable::new();
        let hann: Vec<_> = (0..FFT_LEN)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_LEN - 1) as f32).cos())
            .collect();
        let candidates = || {
            (0..128).map(|seed| {
                let mut rng = fastrand::Rng::with_seed(seed);
                let voice = Voice::new(rate, VoiceKind::Murmur, b'O', &mut rng);
                (seed, voice.mutation)
            })
        };
        let lower_seed = candidates().min_by(|a, b| a.1.total_cmp(&b.1)).unwrap().0;
        let upper_seed = candidates().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap().0;
        for kind in [VoiceKind::Murmur, VoiceKind::Stutter] {
            for seed in [lower_seed, upper_seed] {
                for (phones, low, high) in [
                    (&[Phone::Vowel(Vowel::O)][..], 300.0, 440.0),
                    (&[Phone::Vowel(Vowel::A)][..], 740.0, 900.0),
                    (&[Phone::Vowel(Vowel::Schwa)][..], 690.0, 870.0),
                ] {
                    let mut rng = fastrand::Rng::with_seed(seed);
                    let mut voice = Voice::new(rate, kind, b'O', &mut rng);
                    voice.profile = VoiceProfile::High;
                    voice.phones = phones;
                    voice.phone_weight = 1.0;
                    voice.duration = (rate * 6.0) as u32;
                    let samples: Vec<_> = (0..voice.duration)
                        .map(|_| voice.tick(rate, &table, &mut rng))
                        .collect();
                    for start in [0.35, 2.5, 4.7] {
                        let start = (start * rate) as usize;
                        let end = start + rate as usize;
                        let power = averaged_spectrum(&samples[start..end], &hann);
                        let peak = spectral_peak(&power, rate, 250.0, 1100.0).0;
                        assert!(
                            (low..high).contains(&peak),
                            "{kind:?} seed {seed}, frame {start}: main peak {peak} outside {low}..{high}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn spectral_ripples_are_seeded_details_without_moving_the_main_resonance() {
        let rate = 48_000.0;
        let formant = Some(Formant::new(800.0, 120.0, 0.0));
        let response = |seed| {
            let mut resonance = Resonance::new(rate);
            if let Some(seed) = seed {
                resonance = resonance.with_ripples(rate, &mut fastrand::Rng::with_seed(seed));
            }
            resonance.control(
                formant,
                formant,
                0.0,
                VocalMotion {
                    formant_scale: 1.0,
                    flutter_phase: 0.3,
                },
                rate,
                &LowPass::new(rate, 6200.0),
            );
            for _ in 0..rate as u32 / 5 {
                resonance.tick(0.0);
            }
            let samples: Vec<_> = (0..FFT_LEN)
                .map(|frame| resonance.tick(f32::from(frame == 0)))
                .collect();
            let mut spectrum = vec![Complex32::new(0.0, 0.0); FFT_LEN / 2 + 1];
            real_fft(&samples, &mut spectrum);
            let power: Vec<_> = spectrum
                .iter()
                .map(|bin| f64::from(bin.norm_sqr()))
                .collect();
            (samples, power)
        };
        let (_, plain) = response(None);
        let (first, detailed) = response(Some(123));
        assert_eq!(first, response(Some(123)).0);
        assert_ne!(first, response(Some(90_210)).0);
        let first_bin = (600.0 / rate * FFT_LEN as f32) as usize;
        let last_bin = (1000.0 / rate * FFT_LEN as f32) as usize;
        let max_detail_db = (first_bin..last_bin)
            .map(|bin| 10.0 * (detailed[bin] / plain[bin]).log10())
            .fold(0.0_f64, f64::max);
        assert!(
            (0.6..3.5).contains(&max_detail_db),
            "ripple lift {max_detail_db} dB"
        );
        let plain_peak = spectral_peak(&plain, rate, 650.0, 950.0).0;
        let detailed_peak = spectral_peak(&detailed, rate, 650.0, 950.0).0;
        assert!((plain_peak - detailed_peak).abs() < 12.0);
        let energy_ratio = spectral_band(&detailed, rate, 500.0, 1100.0)
            .iter()
            .sum::<f64>()
            / spectral_band(&plain, rate, 500.0, 1100.0)
                .iter()
                .sum::<f64>();
        assert!(
            (0.95..1.2).contains(&energy_ratio),
            "ripple energy ratio {energy_ratio}"
        );
    }

    #[test]
    fn upper_detail_keeps_its_register_and_fades_when_the_rate_cannot_support_it() {
        for rate in [8_000.0, 44_100.0, 48_000.0] {
            let mut resonance = Resonance::new(rate);
            let formant = Vowel::O.shape(VoiceProfile::Low).upper[1];
            resonance.control(
                formant,
                formant,
                0.0,
                VocalMotion {
                    formant_scale: 1.0,
                    flutter_phase: 0.0,
                },
                rate,
                &LowPass::new(rate, 6200.0),
            );
            for _ in 0..rate as u32 / 5 {
                resonance.tick(0.0);
            }
            let samples: Vec<_> = (0..FFT_LEN)
                .map(|frame| resonance.tick(f32::from(frame == 0)))
                .collect();
            if rate == 8_000.0 {
                assert!(samples.iter().all(|sample| *sample == 0.0));
            } else {
                let mut spectrum = vec![Complex32::new(0.0, 0.0); FFT_LEN / 2 + 1];
                real_fft(&samples, &mut spectrum);
                let power: Vec<_> = spectrum
                    .iter()
                    .map(|bin| f64::from(bin.norm_sqr()))
                    .collect();
                let peak = spectral_peak(&power, rate, 3400.0, 4000.0).0;
                assert!(
                    (3600.0..3900.0).contains(&peak),
                    "{rate} Hz: detail peak {peak}"
                );
                let in_band = spectral_band(&power, rate, 3500.0, 3900.0)
                    .iter()
                    .sum::<f64>()
                    / power.iter().sum::<f64>();
                assert!(in_band > 0.8, "{rate} Hz: detail band share {in_band}");
            }
        }
    }

    #[test]
    fn vowel_morphs_hold_the_source_until_the_last_30_ms_of_each_phone() {
        let table = SineTable::new();
        let profile = VoiceProfile::Low;
        let from = Vowel::A.shape(profile).formants;
        let to = Vowel::I.shape(profile).formants;
        let halfway: [Formant; 3] = std::array::from_fn(|i| Formant {
            frequency_hz: (from[i].frequency_hz + to[i].frequency_hz) * 0.5,
            bandwidth_hz: (from[i].bandwidth_hz + to[i].bandwidth_hz) * 0.5,
            peak_db: (from[i].peak_db + to[i].peak_db) * 0.5,
        });
        for rate in [8_000.0, 48_000.0, 192_000.0] {
            for seconds in [0.4, 7.0] {
                let mut rng = fastrand::Rng::with_seed(123);
                let mut reference_rng = fastrand::Rng::with_seed(123);
                let mut voice = Voice::new(rate, VoiceKind::Murmur, b'I', &mut rng);
                let mut reference = Voice::new(rate, VoiceKind::Murmur, b'I', &mut reference_rng);
                let duration = (rate * seconds).round() as u32;
                for item in [&mut voice, &mut reference] {
                    item.profile = profile;
                    item.mutation = 0.0;
                    item.duration = duration;
                }
                reference.phones = &[Phone::Vowel(Vowel::A)];
                reference.phone_weight = 1.0;
                let boundary = duration / 2;
                let transition_start = boundary - (rate * 0.030).round() as u32;
                for sample in 0..transition_start {
                    assert_eq!(
                        voice.tick(rate, &table, &mut rng),
                        reference.tick(rate, &table, &mut reference_rng),
                        "{rate} Hz, {seconds}s: vowel changed before the short morph at {sample}"
                    );
                }
                for (age, expected) in [
                    (transition_start, from),
                    (boundary - (rate * 0.015).round() as u32, halfway),
                    (boundary, to),
                ] {
                    voice.age = age;
                    voice.control(rate);
                    for (resonance, formant) in voice.resonances.iter().zip(expected) {
                        let expected =
                            formant_filter(rate, formant.frequency_hz, formant.bandwidth_hz);
                        for filter in &resonance.sections {
                            let actual = filter.coefs();
                            for (actual, expected) in
                                [actual.a1, actual.a2, actual.b0, actual.b1, actual.b2]
                                    .into_iter()
                                    .zip([
                                        expected.a1,
                                        expected.a2,
                                        expected.b0,
                                        expected.b1,
                                        expected.b2,
                                    ])
                            {
                                assert!(
                                    (actual - expected).abs() < 1e-5,
                                    "{rate} Hz, {seconds}s at {age}: {actual} vs {expected}"
                                );
                            }
                        }
                    }
                    assert_eq!(voice.duration, duration);
                }
            }
        }
    }

    fn band_power(samples: &[f32], center: f32, rate: f32) -> f64 {
        (-3..=3)
            .map(|offset| {
                let hz = center + offset as f32 * 20.0;
                let coefficient = 2.0 * (std::f32::consts::TAU * hz / rate).cos();
                let (mut a, mut b) = (0.0_f64, 0.0_f64);
                for &sample in samples {
                    let next = f64::from(sample) + f64::from(coefficient) * a - b;
                    b = a;
                    a = next;
                }
                a * a + b * b - f64::from(coefficient) * a * b
            })
            .sum()
    }

    #[test]
    fn formant_filters_hold_unit_peaks_and_engineered_bandwidths_across_rates() {
        let mut max_peak_error = 0.0_f64;
        let mut max_width_error = 0.0_f64;
        for rate in [8_000.0, 48_000.0, 192_000.0] {
            for (center, bandwidth) in [(495.0, 200.0), (2412.0, 150.0), (2979.0, 175.0)] {
                let mut filter = Biquad::with_coefs(formant_filter(rate, center, bandwidth));
                let input: Vec<_> = (0..FFT_LEN)
                    .map(|sample| filter.tick(&[f32::from(sample == 0)].into())[0])
                    .collect();
                let mut output = vec![Complex32::new(0.0, 0.0); FFT_LEN / 2 + 1];
                real_fft(&input, &mut output);
                let power: Vec<_> = output.iter().map(|bin| f64::from(bin.norm_sqr())).collect();
                let peak = (0..power.len())
                    .max_by(|&a, &b| power[a].total_cmp(&power[b]))
                    .unwrap();
                let peak_db = 10.0 * power[peak].log10();
                assert!(
                    peak_db.abs() < 0.05,
                    "{rate} Hz, center {center}: peak {peak_db:.3} dB"
                );
                let bin_hz = f64::from(rate) / FFT_LEN as f64;
                let measured_center = peak as f64 * bin_hz;
                assert!((measured_center - f64::from(center)).abs() < f64::from(bandwidth) * 0.1);
                let half = power[peak] * 0.5;
                let lower = (0..peak).rfind(|&bin| power[bin] < half).unwrap();
                let upper = (peak + 1..power.len())
                    .find(|&bin| power[bin] < half)
                    .unwrap();
                let low_hz = (lower as f64
                    + (half - power[lower]) / (power[lower + 1] - power[lower]))
                    * bin_hz;
                let high_hz = ((upper - 1) as f64
                    + (power[upper - 1] - half) / (power[upper - 1] - power[upper]))
                    * bin_hz;
                let measured_width = high_hz - low_hz;
                let width_error = (measured_width / f64::from(bandwidth) - 1.0).abs();
                assert!(
                    width_error < 0.015,
                    "{rate} Hz, center {center}: -3dB width {measured_width:.3} Hz vs {bandwidth}"
                );
                max_peak_error = max_peak_error.max(peak_db.abs());
                max_width_error = max_width_error.max(width_error);
            }
        }
        eprintln!(
            "formant peak error <= {max_peak_error:.4} dB; bandwidth error <= {:.3}%",
            max_width_error * 100.0
        );
    }

    #[test]
    fn front_and_rounded_vowels_keep_distinct_articulations() {
        let rate = 48_000.0;
        let table = SineTable::new();
        let render = |letter| {
            let mut rng = fastrand::Rng::with_seed(123);
            let mut voice = Voice::new(rate, VoiceKind::Murmur, letter, &mut rng);
            voice.profile = VoiceProfile::Low;
            voice.mutation = 0.0;
            // Use sustained vowels so consonant gestures cannot mask the formants.
            voice.phones = if letter == b'E' {
                &[Phone::Vowel(Vowel::I)]
            } else {
                &[Phone::Vowel(Vowel::U)]
            };
            voice.phone_weight = 1.0;
            voice.duration = 48_000;
            let mut samples = Vec::new();
            for i in 0..48_000 {
                let sample = voice.tick(rate, &table, &mut rng);
                assert!(sample.is_finite());
                if (12_000..36_000).contains(&i) {
                    samples.push(sample);
                }
            }
            samples
        };
        let front = render(b'E');
        let back = render(b'U');
        assert!(band_power(&front, 2412.0, rate) > band_power(&back, 2412.0, rate) * 3.0);
        assert!(band_power(&back, 350.0, rate) > band_power(&front, 350.0, rate) * 3.0);
        let rms =
            (front.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / front.len() as f64).sqrt();
        assert!((0.001..0.012).contains(&rms), "voice RMS {rms}");
    }

    #[test]
    fn excitation_has_a_bounded_low_oscillator_with_nonlinear_harmonics() {
        let table = SineTable::new();
        for rate in [8_000.0, 48_000.0, 192_000.0] {
            let mut rng = fastrand::Rng::with_seed(98);
            let mut excitation = Excitation::new(rate, &mut rng);
            assert!((30.0..=110.0).contains(&excitation.sine.frequency_hz(rate)));
            excitation.sine = Osc::new(70.0, rate, 0.0);
            // Isolating the periodic source exposes the clipper's added harmonics.
            let samples: Vec<_> = (0..rate as u32)
                .map(|_| excitation.tick(0.0, &table))
                .collect();
            assert!(samples.iter().all(|value| value.abs() < 1.0));
            let fundamental = band_power(&samples, 70.0, rate);
            let third = band_power(&samples, 210.0, rate);
            assert!(
                third > fundamental * 0.0005,
                "{rate} Hz: third/fundamental {}",
                third / fundamental
            );
            let mut noisy = Excitation::new(rate, &mut fastrand::Rng::with_seed(98));
            for _ in 0..rate as u32 {
                assert!(noisy.tick(rng.f32() * 2.0 - 1.0, &table).abs() < 1.0);
            }
        }
    }

    #[test]
    fn voices_follow_live_tickers_and_share_two_opposed_haas_slots() {
        let rate = 8_000;
        let table = SineTable::new();
        let mut voices = DemonicVoices::new(rate, 42);
        let mut same = DemonicVoices::new(rate, 42);
        let mut overlapped = false;
        let mut energy = 0.0_f64;
        for sample in 0..u64::from(rate) * 40 {
            let symbol = Symbol::new(if sample < 80_000 { "A.W" } else { "^QI-9" }).unwrap();
            if sample.is_multiple_of(64) {
                voices.control(sample, symbol, 0.0);
                same.control(sample, symbol, 0.0);
            }
            let frame = voices.tick(&table, [1.0; 2]);
            assert_eq!(frame, same.tick(&table, [1.0; 2]));
            assert!(frame.iter().all(|v| v.is_finite() && v.abs() < 0.15));
            energy += frame.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
            if voices.slots.iter().all(|slot| slot.voice.is_some()) {
                overlapped = true;
                assert_ne!(voices.slots[0].leading, voices.slots[1].leading);
                assert!(voices.slots.iter().all(|slot| slot.delay_frames > 0));
            }
            for slot in &voices.slots {
                if let Some(voice) = &slot.voice {
                    assert!((3200..=56_000).contains(&voice.duration));
                }
            }
        }
        assert!(overlapped && energy > 0.01);
        assert!(voices.murmur.generated > 2 && voices.stutter.generated > 2);
    }

    #[test]
    fn stutters_change_rate_and_volume_within_short_bounded_phrases() {
        let rate = 48_000.0;
        let mut rng = fastrand::Rng::with_seed(66);
        let mut stutter = Stutter::new(rate, &mut rng);
        let mut peaks = 0;
        let mut previous = 0.0;
        let mut silent = 0;
        let initial = stutter.target_rate;
        for _ in 0..480_000 {
            let value = stutter.tick(rate, &mut rng);
            assert!((3.0..=20.0).contains(&stutter.rate));
            assert!((14_400..=72_000).contains(&stutter.duration));
            assert!((0.0..=1.0).contains(&value));
            if value == 0.0 {
                silent += 1;
            }
            if value > 0.0 && previous == 0.0 {
                peaks += 1;
            }
            previous = value;
        }
        assert!((30..=200).contains(&peaks));
        assert!(silent > 480_000 / 3);
        assert_ne!(initial, stutter.target_rate);
    }

    #[test]
    fn ticker_articulations_render_and_retire_at_extreme_rates() {
        let table = SineTable::new();
        for rate in [8_000.0, 192_000.0] {
            let mut rng = fastrand::Rng::with_seed(1234);
            for letter in b'A'..=b'Z' {
                let mut voice = Voice::new(rate, VoiceKind::Murmur, letter, &mut rng);
                voice.duration = (rate * 0.4) as u32;
                let mut energy = 0.0_f64;
                for _ in 0..voice.duration + 16 {
                    let value = voice.tick(rate, &table, &mut rng);
                    assert!(
                        value.is_finite() && value.abs() < 0.15,
                        "letter {} rate {rate} age {} value {value}",
                        char::from(letter),
                        voice.age
                    );
                    energy += f64::from(value).powi(2);
                }
                assert!(
                    energy > 1.0e-6,
                    "silent letter {} at {rate}",
                    char::from(letter)
                );
                assert_eq!(voice.tick(rate, &table, &mut rng), 0.0);
            }
        }
    }
}
