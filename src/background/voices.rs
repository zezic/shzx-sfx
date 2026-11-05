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
enum WhisperProfile {
    Male,
    Female,
}

#[derive(Clone, Copy)]
struct Formant {
    frequency_hz: f32,
    bandwidth_hz: f32,
    peak_db: f32,
}

impl Formant {
    fn interpolate(self, to: Self, t: f32) -> Self {
        Self {
            frequency_hz: self.frequency_hz + (to.frequency_hz - self.frequency_hz) * t,
            bandwidth_hz: self.bandwidth_hz + (to.bandwidth_hz - self.bandwidth_hz) * t,
            peak_db: self.peak_db + (to.peak_db - self.peak_db) * t,
        }
    }
}

fn formants(hz: [f32; 3], bandwidths: [f32; 3], peaks_db: [f32; 3]) -> [Formant; 3] {
    std::array::from_fn(|i| Formant {
        frequency_hz: hz[i],
        bandwidth_hz: bandwidths[i],
        peak_db: peaks_db[i],
    })
}

impl Vowel {
    // English-whisper means from Sharifzadeh et al., Tables 1/2. AY onset and
    // schwa are design blends. Bandwidths/peak targets are engineering settings,
    // not population norms; see sound-design-research.md.
    fn formants(self, profile: WhisperProfile) -> [Formant; 3] {
        let hz = match (self, profile) {
            (Self::A, WhisperProfile::Male) => [878.0, 1294.0, 2628.0],
            (Self::A, WhisperProfile::Female) => [966.0, 1346.0, 2950.0],
            (Self::E, WhisperProfile::Male) => [623.0, 2183.5, 2848.0],
            (Self::E, WhisperProfile::Female) => [674.5, 2489.5, 3123.0],
            (Self::I, WhisperProfile::Male) => [495.0, 2412.0, 2979.0],
            (Self::I, WhisperProfile::Female) => [511.0, 2738.0, 3239.0],
            (Self::O, WhisperProfile::Male) => [601.0, 1035.0, 2550.0],
            (Self::O, WhisperProfile::Female) => [579.0, 1056.0, 2928.0],
            (Self::U, WhisperProfile::Male) => [653.0, 1696.0, 2583.0],
            (Self::U, WhisperProfile::Female) => [636.0, 1772.0, 2759.0],
            (Self::Eh, WhisperProfile::Male) => [751.0, 1955.0, 2717.0],
            (Self::Eh, WhisperProfile::Female) => [838.0, 2241.0, 3007.0],
            (Self::Uh, WhisperProfile::Male) => [756.0, 1328.0, 2639.0],
            (Self::Uh, WhisperProfile::Female) => [949.0, 1532.0, 2844.0],
            (Self::Schwa, WhisperProfile::Male) => [758.0, 1485.5, 2660.5],
            (Self::Schwa, WhisperProfile::Female) => [927.0, 1653.0, 2870.5],
        };
        formants(hz, [200.0, 150.0, 175.0], [12.0, -4.0, -12.0])
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

    fn formants(self, profile: WhisperProfile) -> [Formant; 3] {
        let (hz, bandwidths) = match self {
            Self::Vowel(vowel) => return vowel.formants(profile),
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
            Self::PalatalGlide => return Vowel::I.formants(profile),
            Self::LabialGlide => return Vowel::U.formants(profile),
        };
        formants(hz, bandwidths, [0.0, -6.0, -12.0])
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
    profile: WhisperProfile,
    age: u32,
    duration: u32,
    excitation: Excitation,
    filters: [Biquad<f32>; 3],
    formant_gains: [Smooth; 3],
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
        let mut this = Self {
            phones,
            phone_weight: phones.iter().map(|phone| phone.weight()).sum(),
            profile: if rng.bool() {
                WhisperProfile::Male
            } else {
                WhisperProfile::Female
            },
            age: 0,
            duration: ((0.4 + rng.f32() * 6.6) * rate).round() as u32,
            excitation: Excitation::new(rate, rng),
            filters: std::array::from_fn(|_| Biquad::with_coefs(formant_filter(rate, 400.0, 50.0))),
            formant_gains: std::array::from_fn(|_| Smooth::new(0.0, 0.005, rate)),
            lowpass: LowPass::new(rate, 6200.0),
            frication: HighPass::new(rate, (rate * 0.35).min(2800.0)),
            mutation: if rng.f32() < 0.25 {
                rng.f32() * 0.66 - 0.33
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
        let from = phone.formants(self.profile);
        let to = self.phones[(index + 1).min(self.phones.len() - 1)].formants(self.profile);
        let phone_seconds = self.duration as f32 / rate * phone.weight() / self.phone_weight;
        let transition = formant_transition(t, phone_seconds);
        let progress = self.age as f32 / self.duration as f32;
        // An excursion returns to the original voice, never exceeding +/-33%.
        let excursion = 4.0 * progress * (1.0 - progress);
        for i in 0..3 {
            let formant = from[i].interpolate(to[i], transition);
            let hz = (formant.frequency_hz * (1.0 + self.mutation * excursion)).min(rate * 0.42);
            let bandwidth = formant.bandwidth_hz * hz / formant.frequency_hz;
            self.filters[i].set_coefs(formant_filter(rate, hz, bandwidth));
            // Compensate the pink source's 1/f power tilt for comparable spectral
            // peaks, then account for the actual output band's attenuation.
            let output_gain = self.lowpass.magnitude_at(hz);
            let gain = 10.0_f32.powf(formant.peak_db / 20.0) * (hz / 1000.0).sqrt() / output_gain;
            self.formant_gains[i].target(gain);
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
            .filters
            .iter_mut()
            .zip(&mut self.formant_gains)
            .map(|(filter, gain)| filter.tick(&[excitation].into())[0] * gain.tick())
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

    #[test]
    fn whisper_vowels_keep_the_first_formant_stronger_across_profiles_and_excitation_pitches() {
        const VOWELS: [(&str, Vowel, &[Phone]); 4] = [
            ("I", Vowel::I, &[Phone::Vowel(Vowel::I)]),
            ("A", Vowel::A, &[Phone::Vowel(Vowel::A)]),
            ("U", Vowel::U, &[Phone::Vowel(Vowel::U)]),
            ("O", Vowel::O, &[Phone::Vowel(Vowel::O)]),
        ];
        let rate = 48_000.0;
        let table = SineTable::new();
        let bin_hz = rate / FFT_LEN as f32;
        let hann: Vec<_> = (0..FFT_LEN)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_LEN - 1) as f32).cos())
            .collect();
        let mut f2_range = [f64::INFINITY, f64::NEG_INFINITY];
        let mut f3_range = f2_range;
        for (profile_name, profile) in [
            ("male", WhisperProfile::Male),
            ("female", WhisperProfile::Female),
        ] {
            for (vowel_name, vowel, phones) in VOWELS {
                let formants = vowel.formants(profile);
                for sine_hz in [30.0, 70.0, 110.0] {
                    for seed in [11, 123, 90_210] {
                        let mut rng = fastrand::Rng::with_seed(seed);
                        let mut voice = Voice::new(rate, VoiceKind::Murmur, b'E', &mut rng);
                        voice.profile = profile;
                        voice.phones = phones;
                        voice.phone_weight = 1.0;
                        voice.mutation = 0.0;
                        voice.duration = (rate * 3.0) as u32;
                        voice.excitation.sine = Osc::new(sine_hz, rate, 0.0);
                        for _ in 0..rate as u32 / 2 {
                            voice.tick(rate, &table, &mut rng);
                        }
                        let samples: Vec<_> = (0..rate as u32 * 2)
                            .map(|_| voice.tick(rate, &table, &mut rng))
                            .collect();
                        assert!(samples.iter().all(|sample| sample.is_finite()));
                        let power = averaged_spectrum(&samples, &hann);
                        let clean: Vec<_> = (0..power.len())
                            .map(|i| {
                                let hz = i as f32 * bin_hz;
                                // A two-bin guard excludes the Hann main lobe of each
                                // coherent harmonic while retaining the noise envelope.
                                (hz - (hz / sine_hz).round() * sine_hz).abs() > bin_hz * 2.0
                            })
                            .collect();
                        let peaks = formants.map(|formant| {
                            let first = ((formant.frequency_hz - formant.bandwidth_hz * 0.5)
                                / bin_hz)
                                .ceil() as usize;
                            let last = ((formant.frequency_hz + formant.bandwidth_hz * 0.5)
                                / bin_hz)
                                .floor() as usize;
                            let radius = (15.0 / bin_hz).round() as usize;
                            (first..=last)
                                .map(|center| {
                                    let mut sum = 0.0;
                                    let mut count = 0;
                                    for bin in center - radius..=center + radius {
                                        if clean[bin] {
                                            sum += power[bin];
                                            count += 1;
                                        }
                                    }
                                    assert!(count > 0);
                                    sum / f64::from(count)
                                })
                                .fold(0.0_f64, f64::max)
                        });
                        let f2 = 10.0 * (peaks[1] / peaks[0]).log10();
                        let f3 = 10.0 * (peaks[2] / peaks[0]).log10();
                        // Nearby OH resonances overlap, reducing the peak difference
                        // from the nominal 6 dB branch-level separation.
                        assert!(
                            (-9.0..=-2.5).contains(&f2),
                            "{profile_name} {vowel_name}, sine {sine_hz}, seed {seed}: F2/F1 {f2:.2} dB"
                        );
                        assert!(
                            (-12.0..=-6.0).contains(&f3),
                            "{profile_name} {vowel_name}, sine {sine_hz}, seed {seed}: F3/F1 {f3:.2} dB"
                        );
                        f2_range = [f2_range[0].min(f2), f2_range[1].max(f2)];
                        f3_range = [f3_range[0].min(f3), f3_range[1].max(f3)];
                        if sine_hz == 110.0 && seed == 123 {
                            eprintln!(
                                "{profile_name} {vowel_name}: F2/F1 {f2:.2} dB, F3/F1 {f3:.2} dB"
                            );
                        }
                    }
                }
            }
        }
        eprintln!("whisper F2/F1 range {f2_range:?} dB; F3/F1 range {f3_range:?} dB");
    }

    #[test]
    fn vowel_morphs_hold_the_source_until_the_last_30_ms_of_each_phone() {
        let table = SineTable::new();
        let profile = WhisperProfile::Male;
        let from = Vowel::E.formants(profile);
        let to = Vowel::I.formants(profile);
        let halfway: [Formant; 3] = std::array::from_fn(|i| Formant {
            frequency_hz: (from[i].frequency_hz + to[i].frequency_hz) * 0.5,
            bandwidth_hz: (from[i].bandwidth_hz + to[i].bandwidth_hz) * 0.5,
            peak_db: (from[i].peak_db + to[i].peak_db) * 0.5,
        });
        for rate in [8_000.0, 48_000.0, 192_000.0] {
            for seconds in [0.4, 7.0] {
                let mut rng = fastrand::Rng::with_seed(123);
                let mut reference_rng = fastrand::Rng::with_seed(123);
                let mut voice = Voice::new(rate, VoiceKind::Murmur, b'A', &mut rng);
                let mut reference = Voice::new(rate, VoiceKind::Murmur, b'A', &mut reference_rng);
                let duration = (rate * seconds).round() as u32;
                for item in [&mut voice, &mut reference] {
                    item.profile = profile;
                    item.mutation = 0.0;
                    item.duration = duration;
                }
                reference.phones = &[Phone::Vowel(Vowel::E)];
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
                    for (filter, formant) in voice.filters.iter().zip(expected) {
                        let actual = filter.coefs();
                        let expected =
                            formant_filter(rate, formant.frequency_hz, formant.bandwidth_hz);
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
    fn noise_vowels_place_energy_at_the_researched_formants() {
        let rate = 48_000.0;
        let table = SineTable::new();
        let render = |letter| {
            let mut rng = fastrand::Rng::with_seed(123);
            let mut voice = Voice::new(rate, VoiceKind::Murmur, letter, &mut rng);
            voice.profile = WhisperProfile::Male;
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
        assert!(band_power(&back, 1696.0, rate) > band_power(&front, 1696.0, rate) * 3.0);
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
                    assert!(voice.mutation.abs() <= 0.33);
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
