use std::f32::consts::{FRAC_PI_4, TAU};

use crate::dsp::{HighPass, LowPass, SineTable};
use crate::{GeneratedEvent, SFX_EQ, SfxEq, SfxEvent, SfxKind, SfxLayerSelection, SpatialMode};

/// Independently auditable components of an event voice.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    strum::Display,
    strum::EnumString,
    strum::EnumIter,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum SfxLayer {
    Body,
    Texture,
    Tonal,
    Flecks,
    Shimmer,
    Crisp,
}

impl SfxLayer {
    pub const ALL: [Self; 6] = [
        Self::Body,
        Self::Texture,
        Self::Tonal,
        Self::Flecks,
        Self::Shimmer,
        Self::Crisp,
    ];
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::Texture => "Texture",
            Self::Tonal => "Tonal",
            Self::Flecks => "Flecks",
            Self::Shimmer => "Shimmer",
            Self::Crisp => "Crisp",
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, strum::Display)]
#[strum(serialize_all = "kebab-case")]
pub enum SfxGenerator {
    #[strum(to_string = "gated tonal body")]
    PulseBody,
    Intermodulation,
    TonalSequence,
    FilteredNoise,
    PhaseTone,
    HighIntermodulation,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SfxModulation {
    Intermodulation {
        fm_deviation: f32,
        pm_cycles: f32,
        quantizer_step: f32,
        ring_ratios: [f32; 2],
        sync_ratio: f32,
        pulse_duty: f32,
        fold_base_radians: f32,
        fold_depth_radians: f32,
    },
    Fm {
        index: f32,
        ratio: f32,
    },
    Phase {
        index: f32,
        rate_hz: f32,
    },
    WideIntermodulation {
        low_hz: [f32; 2],
        high_hz: [f32; 2],
        auxiliary_ratio: f32,
        pm_cycles: [f32; 3],
        noise_blend: [f32; 3],
        noise_lowpass_hz: f32,
        ratchet_hz: f32,
    },
}

impl std::fmt::Display for SfxModulation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Intermodulation {
                fm_deviation,
                pm_cycles,
                quantizer_step,
                ring_ratios,
                sync_ratio,
                pulse_duty,
                fold_base_radians,
                fold_depth_radians,
            } => write!(
                f,
                "FM deviation {fm_deviation:.2}x carrier; PM {pm_cycles:.2} cycles; crush step {quantizer_step:.3}; ring {:.2}x/{:.2}x; sync {sync_ratio:.2}x; pulse {:.0}%; fold PM {fold_base_radians:.1}+/-{fold_depth_radians:.1} rad",
                ring_ratios[0],
                ring_ratios[1],
                pulse_duty * 100.0
            ),
            Self::Fm { index, ratio } => {
                write!(f, "low-index FM {index:.2}, modulator {ratio:.2}x")
            }
            Self::Phase { index, rate_hz } => write!(f, "PM {index:.3} cycles at {rate_hz:.2} Hz"),
            Self::WideIntermodulation {
                low_hz,
                high_hz,
                auxiliary_ratio,
                pm_cycles,
                noise_blend,
                noise_lowpass_hz,
                ratchet_hz,
            } => write!(
                f,
                "wide PM/ring: low {:.0}>{:.0} Hz, high {:.0}>{:.0} Hz, aux {auxiliary_ratio:.2}x; PM {:.2}>{:.2}>{:.2} cycles; noise {:.2}>{:.2}>{:.2} via LP {noise_lowpass_hz:.0} Hz; accents {ratchet_hz:.0} Hz",
                low_hz[0],
                low_hz[1],
                high_hz[0],
                high_hz[1],
                pm_cycles[0],
                pm_cycles[1],
                pm_cycles[2],
                noise_blend[0],
                noise_blend[1],
                noise_blend[2]
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct SfxEnvelope {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SfxPitch {
    Fixed,
    Settle { semitones: f32, milliseconds: f32 },
}

impl std::fmt::Display for SfxPitch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed => f.write_str("fixed"),
            Self::Settle {
                semitones,
                milliseconds,
            } => write!(f, "{semitones:+.2} st settling in {milliseconds:.1} ms"),
        }
    }
}

/// Nominal settings used by the same generators that render the voice.
/// Bands are clamped to the selected device's Nyquist limit.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct SfxLayerRecipe {
    pub layer: SfxLayer,
    pub level: f32,
    pub highpass_hz: f32,
    pub lowpass_hz: f32,
    pub carrier_hz: Option<f32>,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub start_ms: f32,
    pub duration_ms: f32,
    pub minimum_sample_rate: u32,
    pub generator: SfxGenerator,
    pub modulation: Option<SfxModulation>,
}

/// A timed noise injection within each macro phrase part.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct SfxBurst {
    pub start_ms: f32,
    pub duration_ms: f32,
    pub level: f32,
}

/// A procedural recipe before seeded pitch, duration, and detail-rate variation.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct SfxRecipe {
    pub kind: SfxKind,
    pub patch: SfxPatch,
    pub pitch_hz: f32,
    pub duration_seconds: f32,
    pub gain: f32,
    pub eq: SfxEq,
    pub envelope: SfxEnvelope,
    pub pitch: SfxPitch,
    pub phrase: &'static [SfxStep],
    pub micro_pattern: &'static [SfxDetail],
    pub detail_rate_hz: [f32; 2],
    pub hold_rate_hz: [f32; 2],
    pub tonal_pitch_ratios: &'static [f32],
    pub tonal_base_hz: f32,
    pub filter_gesture: SfxFilterGesture,
    pub fleck_bursts: &'static [SfxBurst],
    pub layers: [SfxLayerRecipe; 6],
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    Detail,
    Interaction,
    Navigation,
    Transaction,
    Alert,
}

impl Priority {
    fn for_kind(kind: SfxKind) -> Self {
        use SfxKind::*;
        match kind {
            HoverEnter | HoverLeave | Scroll | ScrollbarMove | DragOver | Resize => Self::Detail,
            OrderCreate | OrderFill | OrderCancel => Self::Transaction,
            Warning | Error | DropRejected | Disconnect => Self::Alert,
            TabOpen | TabClose | DialogOpen | DialogClose | Search | DataRefresh | Notification
            | Connect => Self::Navigation,
            _ => Self::Interaction,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct SfxStep {
    pub start_fraction: f32,
    pub length_fraction: f32,
    pub pitch_ratio: f32,
    pub level: f32,
}

impl SfxStep {
    const fn new(start: f32, length: f32, pitch_ratio: f32, level: f32) -> Self {
        Self {
            start_fraction: start,
            length_fraction: length,
            pitch_ratio,
            level,
        }
    }
}

const SINGLE: &[SfxStep] = &[SfxStep::new(0.0, 1.0, 1.0, 1.0)];
const ASCENT: &[SfxStep] = &[
    SfxStep::new(0.0, 0.48, 1.0, 0.8),
    SfxStep::new(0.32, 0.68, 1.5, 1.0),
];
const DESCENT: &[SfxStep] = &[
    SfxStep::new(0.0, 0.46, 1.0, 0.8),
    SfxStep::new(0.33, 0.67, 0.75, 1.0),
];
const REJECTION: &[SfxStep] = &[
    SfxStep::new(0.0, 0.3, 1.0, 1.0),
    SfxStep::new(0.36, 0.64, 0.89, 0.9),
];
const WARNING: &[SfxStep] = &[
    SfxStep::new(0.0, 0.38, 1.0, 0.85),
    SfxStep::new(0.47, 0.53, 1.0, 1.0),
];
const CREATE: &[SfxStep] = &[
    SfxStep::new(0.0, 0.37, 0.5, 0.8),
    SfxStep::new(0.22, 0.43, 1.0, 0.7),
    SfxStep::new(0.55, 0.45, 1.25, 1.0),
];
const FILL: &[SfxStep] = &[
    SfxStep::new(0.0, 0.28, 1.0, 0.75),
    SfxStep::new(0.19, 0.35, 1.5, 0.8),
    SfxStep::new(0.43, 0.57, 2.0, 1.0),
];
const CANCEL_ORDER: &[SfxStep] = &[
    SfxStep::new(0.0, 0.4, 1.0, 0.8),
    SfxStep::new(0.3, 0.42, 0.75, 0.75),
    SfxStep::new(0.6, 0.4, 0.5, 0.9),
];
const LINK: &[SfxStep] = &[
    SfxStep::new(0.0, 0.28, 1.0, 0.7),
    SfxStep::new(0.27, 0.3, 1.0, 0.75),
    SfxStep::new(0.56, 0.44, 1.5, 1.0),
];

const DETAIL_RATE_HZ: [f32; 2] = [230.0, 670.0];
const HOLD_RATE_HZ: [f32; 2] = [1_200.0, 4_400.0];
const TEXTURE_FM_DEVIATION: f32 = 0.6;
const TEXTURE_PM_CYCLES: f32 = 0.28;
const QUANTIZER_DIVISOR: f32 = 7.0;
const SYNC_RATIO: f32 = 2.37;
const PULSE_DUTY: f32 = 0.24;
const FOLD_BASE_RADIANS: f32 = 1.2;
const FOLD_DEPTH_RADIANS: f32 = 1.8;
const TONAL_FM_INDEX: f32 = 0.08;
const TONAL_FM_RATIO: f32 = 1.5;
const SHIMMER_PM_CYCLES: f32 = 0.035;
const SHIMMER_MOD_HZ: f32 = 0.7;

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SfxPatch {
    Contact { metal: f32 },
    Digital { modulation: f32, gate_hz: f32 },
    Servo { friction: f32 },
    Air,
}

#[derive(Clone, Copy, Debug, serde::Serialize, strum::Display)]
#[strum(serialize_all = "kebab-case")]
pub enum SfxDetail {
    Noise,
    Ring,
    Crush,
    Metal,
    Clean,
    Fm,
    Phase,
    Sync,
    Pulse,
    Fold,
    Gap,
}

impl std::fmt::Display for SfxPatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contact { metal } => write!(f, "contact: texture inharmonic blend {metal:.2}"),
            Self::Digital {
                modulation,
                gate_hz,
            } => {
                write!(f, "digital: texture PM {:.2} rad", 1.2 + modulation)?;
                if *gate_hz > 0.0 {
                    write!(f, "; gate {gate_hz:.0} Hz")
                } else {
                    f.write_str("; no slow gate")
                }
            }
            Self::Servo { friction } => {
                write!(f, "servo: texture held-noise friction {friction:.2}")
            }
            Self::Air => f.write_str("air: held and edge-shaped noise"),
        }
    }
}

impl SfxPatch {
    fn crisp(self) -> CrispParameters {
        let (low_hz, high_hz, pm_cycles, noise_blend, noise_lowpass_hz, ratchet_hz, level) =
            match self {
                Self::Contact { .. } => (
                    [92.0, 210.0],
                    [8_300.0, 11_300.0],
                    [0.32, 0.12, 0.025],
                    [0.06, 0.27, 0.035],
                    800.0,
                    360.0,
                    0.055,
                ),
                Self::Digital { .. } => (
                    [145.0, 85.0],
                    [12_100.0, 7_600.0],
                    [0.24, 0.42, 0.04],
                    [0.18, 0.36, 0.035],
                    1_100.0,
                    470.0,
                    0.045,
                ),
                Self::Servo { .. } => (
                    [74.0, 235.0],
                    [7_600.0, 12_600.0],
                    [0.14, 0.34, 0.06],
                    [0.04, 0.33, 0.08],
                    650.0,
                    290.0,
                    0.060,
                ),
                Self::Air => (
                    [115.0, 175.0],
                    [10_800.0, 8_100.0],
                    [0.12, 0.24, 0.02],
                    [0.25, 0.40, 0.08],
                    1_250.0,
                    620.0,
                    0.035,
                ),
            };
        CrispParameters {
            low_hz,
            high_hz,
            auxiliary_ratio: 1.31,
            pm_cycles,
            noise_blend,
            noise_lowpass_hz,
            ratchet_hz,
            level,
        }
    }

    fn details(self) -> &'static [SfxDetail; 6] {
        use SfxDetail::*;
        match self {
            Self::Contact { .. } => &[Noise, Sync, Metal, Gap, Clean, Pulse],
            Self::Digital { .. } => &[Crush, Phase, Clean, Fm, Noise, Sync],
            Self::Servo { .. } => &[Sync, Ring, Pulse, Gap, Fold, Noise],
            Self::Air => &[Noise, Gap, Crush, Clean, Gap, Phase],
        }
    }

    fn tonal_ratios(self) -> &'static [f32; 6] {
        match self {
            Self::Contact { .. } => &[1.0, 1.5, 2.0, 1.25, 1.0, 1.75],
            Self::Digital { .. } => &[1.0, 1.25, 1.5, 2.0, 1.5, 1.0],
            Self::Servo { .. } => &[2.0, 1.5, 1.0, 1.25, 1.75, 1.0],
            Self::Air => &[1.0, 2.0, 1.5, 1.0, 1.25, 1.75],
        }
    }

    fn ratios(self) -> [f32; 4] {
        match self {
            Self::Contact { .. } => [1.0, 1.73, 2.91, 2.07],
            Self::Digital { .. } => [1.0, 2.0, 3.0, 1.71],
            Self::Servo { .. } => [1.0, 2.0, 3.0, 0.93],
            Self::Air => [1.0, 1.5, 2.0, 0.5],
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, strum::Display)]
#[strum(serialize_all = "kebab-case")]
pub enum SfxFilterGesture {
    Punch,
    BandStep,
    Ratchet,
    Thin,
}

impl SfxFilterGesture {
    fn cutoffs(self, time: f32, nominal: f32) -> (f32, f32) {
        let onset = (1.0 - time / 0.006).clamp(0.0, 1.0);
        let rebound = (1.0 - (time - 0.011).abs() / 0.002).max(0.0);
        match self {
            Self::Punch => (
                nominal * (1.0 + onset * 1.3 + rebound * 0.5),
                140.0 + rebound * 550.0,
            ),
            Self::BandStep => {
                let step = (time * 150.0).fract();
                let opened = if step < 0.28 { 1.0 } else { 0.0 };
                (nominal * (1.0 + opened * 0.7), 220.0 + opened * 750.0)
            }
            Self::Ratchet => {
                let pulse = (1.0 - (time * 95.0).fract() * 3.0).max(0.0);
                (
                    nominal * (0.95 + pulse * 0.8),
                    180.0 + (1.0 - pulse) * 300.0,
                )
            }
            Self::Thin => (nominal * (1.1 + onset * 1.4), 650.0 + onset * 900.0),
        }
    }
}

#[derive(Clone, Copy)]
enum AccentPattern {
    Single,
    Double,
    Ratchet,
}

impl AccentPattern {
    fn gain(self, time: f32) -> f32 {
        let pulse = |center: f32, width: f32| (1.0 - (time - center).abs() / width).max(0.0);
        match self {
            Self::Single => 1.0,
            Self::Double => 1.0 + pulse(0.008, 0.0012) * 0.8 + pulse(0.015, 0.001) * 0.35,
            Self::Ratchet => {
                1.0 + pulse(0.004, 0.0008) * 0.7
                    + pulse(0.008, 0.0008) * 0.5
                    + pulse(0.012, 0.0008) * 0.3
            }
        }
    }
}

#[derive(Clone, Copy)]
enum FleckPattern {
    Tick,
    Double,
    Ratchet,
}

impl FleckPattern {
    fn bursts(self) -> &'static [SfxBurst] {
        match self {
            Self::Tick => &[SfxBurst {
                start_ms: 0.0,
                duration_ms: 1.2,
                level: 1.0,
            }],
            Self::Double => &[
                SfxBurst {
                    start_ms: 0.0,
                    duration_ms: 1.4,
                    level: 1.0,
                },
                SfxBurst {
                    start_ms: 3.0,
                    duration_ms: 0.8,
                    level: 0.55,
                },
            ],
            Self::Ratchet => &[
                SfxBurst {
                    start_ms: 0.0,
                    duration_ms: 0.8,
                    level: 1.0,
                },
                SfxBurst {
                    start_ms: 1.8,
                    duration_ms: 0.7,
                    level: 0.65,
                },
                SfxBurst {
                    start_ms: 3.5,
                    duration_ms: 0.6,
                    level: 0.35,
                },
            ],
        }
    }

    fn envelope(self, time: f32) -> f32 {
        self.bursts()
            .iter()
            .map(|burst| {
                let age = time * 1000.0 - burst.start_ms;
                if (0.0..burst.duration_ms).contains(&age) {
                    (age / 0.12).min(1.0)
                        * ((burst.duration_ms - age) / 0.35).min(1.0)
                        * burst.level
                } else {
                    0.0
                }
            })
            .sum()
    }
}

#[derive(Clone, Copy)]
struct Articulation {
    filter: SfxFilterGesture,
    accents: AccentPattern,
    flecks: FleckPattern,
    bright_highpass: f32,
    bright_lowpass: f32,
    bright_level: f32,
    filter_tail: f32,
}

impl Articulation {
    fn for_kind(kind: SfxKind) -> Self {
        use SfxKind::*;
        let (filter, accents, flecks, bright_highpass, bright_lowpass, bright_level) = match kind {
            HoverEnter | HoverLeave | Blur => (
                SfxFilterGesture::Thin,
                AccentPattern::Single,
                FleckPattern::Tick,
                3_300.0,
                7_800.0,
                0.045,
            ),
            Focus | TabOpen | TabClose | Search | DataRefresh | Confirm | Cancel => (
                SfxFilterGesture::BandStep,
                AccentPattern::Double,
                FleckPattern::Double,
                2_200.0,
                6_800.0,
                0.06,
            ),
            Scroll | ScrollbarGrab | ScrollbarMove | ScrollbarRelease | Resize | DragOver => (
                SfxFilterGesture::Punch,
                AccentPattern::Single,
                FleckPattern::Tick,
                2_900.0,
                7_400.0,
                0.05,
            ),
            ToggleOn | ToggleOff | DragStart | ZoomIn | ZoomOut => (
                SfxFilterGesture::Ratchet,
                AccentPattern::Ratchet,
                FleckPattern::Ratchet,
                1_700.0,
                5_200.0,
                0.05,
            ),
            OrderCreate | OrderFill | OrderCancel | Warning | Error | DropRejected
            | Notification | Connect | Disconnect | DialogOpen | DialogClose => (
                SfxFilterGesture::Ratchet,
                AccentPattern::Double,
                FleckPattern::Double,
                1_600.0,
                4_800.0,
                0.04,
            ),
            _ => (
                SfxFilterGesture::Punch,
                AccentPattern::Double,
                FleckPattern::Double,
                2_500.0,
                7_200.0,
                0.055,
            ),
        };
        let filter_tail = match kind {
            OrderCreate | OrderFill | OrderCancel | Warning | Error | DropRejected
            | Notification | Connect | Disconnect | DialogOpen | DialogClose => 0.02,
            _ => 0.012,
        };
        Self {
            filter,
            accents,
            flecks,
            bright_highpass,
            bright_lowpass,
            bright_level,
            filter_tail,
        }
    }
}

fn transient_decay(time: f32, seconds: f32) -> f32 {
    let elapsed = time / seconds;
    if elapsed < 8.0 { (-elapsed).exp() } else { 0.0 }
}

#[derive(Clone, Copy)]
struct AmplitudeEnvelope {
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

impl AmplitudeEnvelope {
    const fn new(attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
        assert!(attack > 0.0 && decay > 0.0 && release > 0.0);
        assert!(sustain > 0.0 && sustain < 1.0);
        Self {
            attack,
            decay,
            sustain,
            release,
        }
    }

    fn gain(self, time: f32, duration: f32) -> f32 {
        if time < self.attack {
            time / self.attack
        } else if time < self.attack + self.decay {
            let remaining = 1.0 - (time - self.attack) / self.decay;
            self.sustain + (1.0 - self.sustain) * remaining * remaining
        } else if time < duration - self.release {
            self.sustain
        } else {
            self.sustain * ((duration - time) / self.release).clamp(0.0, 1.0)
        }
    }
}

#[derive(Clone, Copy)]
enum PitchEnvelope {
    Fixed,
    Settle { semitones: f32, seconds: f32 },
}

impl PitchEnvelope {
    fn ratio(self, time: f32) -> f32 {
        match self {
            Self::Fixed => 1.0,
            Self::Settle { semitones, seconds } if time < seconds => {
                let remaining = 1.0 - time / seconds;
                2.0_f32.powf(semitones * remaining * remaining / 12.0)
            }
            Self::Settle { .. } => 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Recipe {
    patch: SfxPatch,
    pitch_hz: f32,
    duration: f32,
    pitch: PitchEnvelope,
    gain: f32,
    amplitude: AmplitudeEnvelope,
    cutoff: f32,
    steps: &'static [SfxStep],
    articulation: Articulation,
}

impl Recipe {
    fn contact(pitch_hz: f32, duration: f32, metal: f32) -> Self {
        Self {
            patch: SfxPatch::Contact { metal },
            pitch_hz,
            duration,
            pitch: PitchEnvelope::Settle {
                semitones: 1.5,
                seconds: 0.002,
            },
            gain: 0.16,
            amplitude: AmplitudeEnvelope::new(0.00035, 0.004, 0.16, 0.002),
            cutoff: 1_350.0,
            steps: SINGLE,
            articulation: Articulation::for_kind(SfxKind::Click),
        }
    }

    fn digital(pitch_hz: f32, duration: f32, modulation: f32, gate_hz: f32) -> Self {
        Self {
            patch: SfxPatch::Digital {
                modulation,
                gate_hz,
            },
            pitch_hz,
            duration,
            pitch: PitchEnvelope::Fixed,
            gain: 0.13,
            amplitude: AmplitudeEnvelope::new(0.0006, 0.007, 0.22, 0.0025),
            cutoff: 1_600.0,
            steps: SINGLE,
            articulation: Articulation::for_kind(SfxKind::Focus),
        }
    }

    fn servo(pitch_hz: f32, duration: f32, friction: f32) -> Self {
        Self {
            patch: SfxPatch::Servo { friction },
            pitch_hz,
            duration,
            pitch: PitchEnvelope::Settle {
                semitones: 0.75,
                seconds: 0.003,
            },
            gain: 0.14,
            amplitude: AmplitudeEnvelope::new(0.0008, 0.009, 0.2, 0.003),
            cutoff: 1_100.0,
            steps: SINGLE,
            articulation: Articulation::for_kind(SfxKind::ToggleOn),
        }
    }

    fn air(pitch_hz: f32, duration: f32) -> Self {
        Self {
            patch: SfxPatch::Air,
            pitch_hz,
            duration,
            pitch: PitchEnvelope::Fixed,
            gain: 0.075,
            amplitude: AmplitudeEnvelope::new(0.0009, 0.006, 0.15, 0.0025),
            cutoff: 1_250.0,
            steps: SINGLE,
            articulation: Articulation::for_kind(SfxKind::HoverEnter),
        }
    }

    fn level(mut self, gain: f32) -> Self {
        self.gain = gain;
        self
    }

    fn pattern(mut self, steps: &'static [SfxStep]) -> Self {
        self.steps = steps;
        self
    }

    fn for_kind(kind: SfxKind) -> Self {
        use SfxKind::*;
        let mut recipe = match kind {
            Click => Self::contact(510.0, 0.065, 0.22),
            SecondaryClick => Self::contact(370.0, 0.085, 0.5).level(0.14),
            Confirm => Self::digital(620.0, 0.23, 0.25, 0.0).pattern(ASCENT),
            Cancel => Self::digital(470.0, 0.19, 0.35, 0.0).pattern(DESCENT),
            DestructiveClick => Self::contact(230.0, 0.19, 0.72).level(0.18),
            ToggleOn => Self::servo(460.0, 0.13, 0.15),
            ToggleOff => Self::servo(580.0, 0.14, 0.2),
            HoverEnter => Self::air(670.0, 0.042).level(0.045),
            HoverLeave => Self::air(530.0, 0.035).level(0.035),
            Focus => Self::digital(700.0, 0.095, 0.15, 0.0).level(0.07),
            Blur => Self::air(420.0, 0.075).level(0.05),
            Scroll => Self::contact(340.0, 0.028, 0.08).level(0.065),
            ScrollBoundary => Self::contact(190.0, 0.13, 0.48).pattern(REJECTION),
            ScrollbarGrab => Self::contact(410.0, 0.07, 0.42).level(0.12),
            ScrollbarMove => Self::servo(240.0, 0.045, 0.75).level(0.055),
            ScrollbarRelease => Self::contact(290.0, 0.09, 0.32).level(0.11),
            DragStart => Self::servo(270.0, 0.16, 0.45),
            DragOver => Self::digital(390.0, 0.038, 0.6, 120.0).level(0.045),
            Drop => Self::contact(320.0, 0.16, 0.65).pattern(ASCENT),
            DropRejected => Self::digital(310.0, 0.28, 1.05, 65.0).pattern(REJECTION),
            Resize => Self::servo(195.0, 0.07, 0.85).level(0.055),
            ZoomIn => Self::servo(330.0, 0.12, 0.32).level(0.09),
            ZoomOut => Self::servo(540.0, 0.14, 0.36).level(0.09),
            TabOpen => Self::digital(540.0, 0.18, 0.65, 130.0).pattern(ASCENT),
            TabClose => Self::digital(610.0, 0.16, 0.55, 150.0).pattern(DESCENT),
            DialogOpen => Self::servo(170.0, 0.44, 0.68).level(0.12),
            DialogClose => Self::servo(350.0, 0.37, 0.62).level(0.11),
            Search => Self::digital(760.0, 0.22, 0.85, 90.0).level(0.09),
            DataRefresh => Self::digital(450.0, 0.27, 1.2, 185.0)
                .level(0.1)
                .pattern(ASCENT),
            Notification => Self::digital(570.0, 0.43, 0.2, 0.0)
                .level(0.13)
                .pattern(ASCENT),
            Warning => Self::digital(360.0, 0.48, 0.8, 0.0)
                .level(0.17)
                .pattern(WARNING),
            Error => Self::digital(250.0, 0.57, 1.6, 48.0)
                .level(0.18)
                .pattern(REJECTION),
            OrderCreate => Self::servo(280.0, 0.72, 0.38).level(0.18).pattern(CREATE),
            OrderFill => Self::digital(390.0, 0.95, 0.3, 0.0)
                .level(0.19)
                .pattern(FILL),
            OrderCancel => Self::contact(440.0, 0.63, 0.52)
                .level(0.17)
                .pattern(CANCEL_ORDER),
            Connect => Self::digital(430.0, 0.52, 0.5, 75.0)
                .level(0.12)
                .pattern(LINK),
            Disconnect => Self::digital(510.0, 0.48, 1.3, 55.0)
                .level(0.15)
                .pattern(DESCENT),
        };
        recipe.articulation = Articulation::for_kind(kind);
        recipe.duration *= match kind {
            OrderCreate | OrderFill | OrderCancel | Warning | Error | DropRejected
            | Notification | Connect | Disconnect | DialogOpen | DialogClose => 1.0,
            _ => 0.65,
        };
        recipe.amplitude = match kind {
            Click => AmplitudeEnvelope::new(0.00025, 0.008, 0.055, 0.004),
            SecondaryClick => AmplitudeEnvelope::new(0.0003, 0.010, 0.045, 0.0045),
            DestructiveClick | Drop => AmplitudeEnvelope::new(0.0003, 0.009, 0.075, 0.004),
            Scroll => AmplitudeEnvelope::new(0.0002, 0.0045, 0.035, 0.0025),
            ScrollBoundary | ScrollbarGrab | ScrollbarRelease => {
                AmplitudeEnvelope::new(0.00025, 0.0065, 0.060, 0.0035)
            }
            ToggleOn | ToggleOff | DragStart | DragOver | Resize => {
                AmplitudeEnvelope::new(0.0004, 0.008, 0.080, 0.004)
            }
            HoverEnter | HoverLeave | Blur => AmplitudeEnvelope::new(0.0005, 0.0075, 0.075, 0.0035),
            TabOpen | TabClose | Search | DataRefresh => {
                AmplitudeEnvelope::new(0.0005, 0.009, 0.12, 0.004)
            }
            _ => recipe.amplitude,
        };
        recipe
    }
}

impl Recipe {
    fn layers(self) -> [SfxLayerRecipe; 6] {
        let (lp, hp) = self.articulation.filter.cutoffs(0.0, self.cutoff);
        let shortest_part = self
            .steps
            .iter()
            .map(|step| self.duration * step.length_fraction)
            .fold(self.duration, f32::min);
        let shimmer_ms = (shortest_part * 550.0).clamp(6.0, 72.0);
        let ratios = self.patch.ratios();
        let crisp = self.patch.crisp();
        [
            SfxLayerRecipe {
                layer: SfxLayer::Body,
                level: 0.35,
                highpass_hz: hp,
                lowpass_hz: lp,
                carrier_hz: Some(self.pitch_hz),
                attack_ms: self.amplitude.attack * 1000.0,
                release_ms: self.amplitude.release * 1000.0,
                start_ms: 0.0,
                duration_ms: self.duration * 1000.0,
                minimum_sample_rate: 8_000,
                generator: SfxGenerator::PulseBody,
                modulation: None,
            },
            SfxLayerRecipe {
                layer: SfxLayer::Texture,
                level: 0.25,
                highpass_hz: hp,
                lowpass_hz: lp,
                carrier_hz: Some(self.pitch_hz),
                attack_ms: self.amplitude.attack * 1000.0,
                release_ms: self.amplitude.release * 1000.0,
                start_ms: 0.0,
                duration_ms: self.duration * 1000.0,
                minimum_sample_rate: 8_000,
                generator: SfxGenerator::Intermodulation,
                modulation: Some(SfxModulation::Intermodulation {
                    fm_deviation: TEXTURE_FM_DEVIATION,
                    pm_cycles: TEXTURE_PM_CYCLES,
                    quantizer_step: 1.0 / QUANTIZER_DIVISOR,
                    ring_ratios: [ratios[1], ratios[3]],
                    sync_ratio: SYNC_RATIO,
                    pulse_duty: PULSE_DUTY,
                    fold_base_radians: FOLD_BASE_RADIANS,
                    fold_depth_radians: FOLD_DEPTH_RADIANS,
                }),
            },
            SfxLayerRecipe {
                layer: SfxLayer::Tonal,
                level: 0.28,
                highpass_hz: 1_200.0,
                lowpass_hz: 8_000.0,
                carrier_hz: Some((self.pitch_hz * 3.8).clamp(1_500.0, 3_000.0)),
                attack_ms: 0.12,
                release_ms: 0.35,
                start_ms: 0.0,
                duration_ms: 2000.0 / (DETAIL_RATE_HZ[0] + DETAIL_RATE_HZ[1]),
                minimum_sample_rate: 8_000,
                generator: SfxGenerator::TonalSequence,
                modulation: Some(SfxModulation::Fm {
                    index: TONAL_FM_INDEX,
                    ratio: TONAL_FM_RATIO,
                }),
            },
            SfxLayerRecipe {
                layer: SfxLayer::Flecks,
                level: self.articulation.bright_level * 0.55,
                highpass_hz: self.articulation.bright_highpass,
                lowpass_hz: self.articulation.bright_lowpass,
                carrier_hz: None,
                attack_ms: 0.12,
                release_ms: 0.35,
                start_ms: 0.0,
                duration_ms: self
                    .articulation
                    .flecks
                    .bursts()
                    .iter()
                    .map(|burst| burst.start_ms + burst.duration_ms)
                    .fold(0.0, f32::max),
                minimum_sample_rate: 8_000,
                generator: SfxGenerator::FilteredNoise,
                modulation: None,
            },
            SfxLayerRecipe {
                layer: SfxLayer::Shimmer,
                level: 0.018,
                highpass_hz: 7_200.0,
                lowpass_hz: 16_000.0,
                carrier_hz: Some((self.pitch_hz * 15.7).clamp(8_400.0, 12_500.0)),
                attack_ms: 2.5,
                release_ms: 4.0,
                start_ms: 3.0,
                duration_ms: shimmer_ms,
                minimum_sample_rate: 20_000,
                generator: SfxGenerator::PhaseTone,
                modulation: Some(SfxModulation::Phase {
                    index: SHIMMER_PM_CYCLES,
                    rate_hz: SHIMMER_MOD_HZ,
                }),
            },
            SfxLayerRecipe {
                layer: SfxLayer::Crisp,
                level: crisp.level,
                highpass_hz: 6_000.0,
                lowpass_hz: 16_000.0,
                carrier_hz: Some(crisp.high_hz[0]),
                attack_ms: 0.35,
                release_ms: 2.0,
                start_ms: 0.0,
                duration_ms: (shortest_part * 750.0).clamp(6.0, 90.0),
                minimum_sample_rate: 20_000,
                generator: SfxGenerator::HighIntermodulation,
                modulation: Some(SfxModulation::WideIntermodulation {
                    low_hz: crisp.low_hz,
                    high_hz: crisp.high_hz,
                    auxiliary_ratio: crisp.auxiliary_ratio,
                    pm_cycles: crisp.pm_cycles,
                    noise_blend: crisp.noise_blend,
                    noise_lowpass_hz: crisp.noise_lowpass_hz,
                    ratchet_hz: crisp.ratchet_hz,
                }),
            },
        ]
    }
}

impl SfxKind {
    #[must_use]
    pub fn recipe(self) -> SfxRecipe {
        let recipe = Recipe::for_kind(self);
        SfxRecipe {
            kind: self,
            patch: recipe.patch,
            pitch_hz: recipe.pitch_hz,
            duration_seconds: recipe.duration,
            gain: recipe.gain,
            eq: SFX_EQ,
            envelope: SfxEnvelope {
                attack_ms: recipe.amplitude.attack * 1000.0,
                decay_ms: recipe.amplitude.decay * 1000.0,
                sustain: recipe.amplitude.sustain,
                release_ms: recipe.amplitude.release * 1000.0,
            },
            pitch: match recipe.pitch {
                PitchEnvelope::Fixed => SfxPitch::Fixed,
                PitchEnvelope::Settle { semitones, seconds } => SfxPitch::Settle {
                    semitones,
                    milliseconds: seconds * 1000.0,
                },
            },
            phrase: recipe.steps,
            micro_pattern: recipe.patch.details(),
            detail_rate_hz: DETAIL_RATE_HZ,
            hold_rate_hz: HOLD_RATE_HZ,
            tonal_pitch_ratios: recipe.patch.tonal_ratios(),
            tonal_base_hz: (recipe.pitch_hz * 3.8).clamp(1_500.0, 3_000.0),
            filter_gesture: recipe.articulation.filter,
            fleck_bursts: recipe.articulation.flecks.bursts(),
            layers: recipe.layers(),
        }
    }
}

struct StepState {
    phase: [f32; 4],
    held_noise: f32,
    hold_frames: u32,
    hold_hz: f32,
    detail_rate: f32,
    detail_offset: f32,
    previous_noise: f32,
    fm_phase: f32,
    sync_phase: f32,
    tonal_phase: f32,
    tonal_mod_phase: f32,
    shimmer_phase: f32,
    shimmer_mod_phase: f32,
}

impl StepState {
    fn new(rng: &mut fastrand::Rng) -> Self {
        Self {
            phase: std::array::from_fn(|_| rng.f32()),
            held_noise: 0.0,
            hold_frames: 0,
            hold_hz: HOLD_RATE_HZ[0] + rng.f32() * (HOLD_RATE_HZ[1] - HOLD_RATE_HZ[0]),
            detail_rate: DETAIL_RATE_HZ[0] + rng.f32() * (DETAIL_RATE_HZ[1] - DETAIL_RATE_HZ[0]),
            detail_offset: rng.f32() * 6.0,
            previous_noise: 0.0,
            fm_phase: rng.f32(),
            sync_phase: rng.f32(),
            tonal_phase: rng.f32(),
            tonal_mod_phase: rng.f32(),
            shimmer_phase: rng.f32(),
            shimmer_mod_phase: rng.f32(),
        }
    }

    fn source(
        &mut self,
        recipe: Recipe,
        pitch_hz: f32,
        time: f32,
        sample_rate: f32,
        rng: &mut fastrand::Rng,
        table: &SineTable,
    ) -> SourceParts {
        let frequency = pitch_hz * recipe.pitch.ratio(time);
        let ratios = recipe.patch.ratios();
        let previous_master = self.phase[0];
        for (phase, ratio) in self.phase.iter_mut().zip(ratios) {
            *phase = (*phase + (frequency * ratio).min(sample_rate * 0.42) / sample_rate).fract();
        }
        let noise = rng.f32() * 2.0 - 1.0;
        let noise_edge = noise - self.previous_noise * 0.65;
        self.previous_noise = noise;
        if self.hold_frames == 0 {
            self.held_noise = noise;
            self.hold_frames = (sample_rate / self.hold_hz).max(1.0) as u32;
        }
        self.hold_frames -= 1;
        let partials = self.phase.map(|phase| table.at(phase));
        self.fm_phase = (self.fm_phase
            + frequency * (1.0 + partials[3] * TEXTURE_FM_DEVIATION) / sample_rate)
            .fract();
        self.sync_phase = if self.phase[0] < previous_master {
            0.0
        } else {
            (self.sync_phase + frequency * SYNC_RATIO / sample_rate).fract()
        };
        let ring = partials[1] * partials[3];
        let folded = table.at((self.phase[0]
            + (FOLD_BASE_RADIANS + partials[3] * FOLD_DEPTH_RADIANS) * partials[2] / TAU
            + 1.0)
            .fract());
        let crushed = (folded * 5.0 + self.held_noise * 2.0).round() / QUANTIZER_DIVISOR;
        let clock = time * self.detail_rate + self.detail_offset;
        let grain_phase = clock.fract();
        let grain_gate = (grain_phase * 12.0).min(1.0) * ((1.0 - grain_phase) * 14.0).min(1.0);
        let grain = match recipe.patch.details()[clock as usize % 6] {
            SfxDetail::Noise => noise_edge * 0.52 + self.held_noise * 0.18,
            SfxDetail::Ring => ring * 0.72 + self.held_noise * 0.18,
            SfxDetail::Crush => crushed * 0.68 + noise_edge * 0.18,
            SfxDetail::Metal => partials[1] * 0.25 + partials[2] * 0.24 + ring * 0.28,
            SfxDetail::Clean => partials[0] * 0.48,
            SfxDetail::Fm => table.at(self.fm_phase) * 0.5 + self.held_noise * 0.18,
            SfxDetail::Phase => {
                table.at((self.phase[0] + partials[1] * TEXTURE_PM_CYCLES + 1.0).fract()) * 0.5
                    + noise_edge * 0.12
            }
            SfxDetail::Sync => table.at(self.sync_phase) * 0.48 + noise_edge * 0.14,
            SfxDetail::Pulse => {
                (if self.sync_phase < PULSE_DUTY {
                    1.0 - PULSE_DUTY
                } else {
                    -PULSE_DUTY
                }) * 0.65
                    + self.held_noise * 0.12
            }
            SfxDetail::Fold => {
                let triangle = 1.0 - 4.0 * (self.phase[2] - 0.5).abs();
                (4.0 * ((triangle * 1.5 + 1.25).fract() - 0.5).abs() - 1.0) * 0.4 + ring * 0.2
            }
            SfxDetail::Gap => 0.0,
        } * grain_gate;
        let dirty_body = match recipe.patch {
            SfxPatch::Contact { metal } => {
                let strike = noise_edge * transient_decay(time, 0.0018);
                let modal = partials[0] * 0.07
                    + ring * (0.2 + metal * 0.2)
                    + metal * 0.2 * partials[2] * transient_decay(time, 0.008);
                strike * 0.5 + modal + crushed * 0.2
            }
            SfxPatch::Digital {
                modulation,
                gate_hz,
            } => {
                let tone = table
                    .at((self.phase[0] + (1.2 + modulation) * partials[3] / TAU + 1.0).fract());
                let gate = if gate_hz > 0.0 && (time * gate_hz).fract() > 0.62 {
                    0.18
                } else {
                    1.0
                };
                (tone * 0.12
                    + ring * 0.2
                    + crushed * 0.2
                    + self.held_noise * (0.12 + modulation * 0.08)
                    + noise_edge * 0.3 * transient_decay(time, 0.0015))
                    * gate
            }
            SfxPatch::Servo { friction } => {
                let mechanism = crushed * 0.3 + ring * 0.32;
                mechanism * (1.0 - friction * 0.3) + self.held_noise * friction * 0.35
            }
            SfxPatch::Air => self.held_noise * 0.3 + noise_edge * 0.2 + ring * 0.04,
        };
        let onset = noise_edge * transient_decay(time, 0.00075) * 0.22;
        let body = match recipe.patch {
            SfxPatch::Contact { .. } => partials[0] * 0.28 + partials[1] * 0.025,
            SfxPatch::Digital { .. } => partials[0] * 0.24 + partials[2] * 0.025,
            SfxPatch::Servo { .. } => partials[0] * 0.17 + table.at(self.sync_phase) * 0.04,
            SfxPatch::Air => partials[0] * 0.05,
        };
        let tonal_hz = ((pitch_hz * 3.8).clamp(1_500.0, 3_000.0)
            * recipe.patch.tonal_ratios()[clock as usize % 6])
            .min(sample_rate * 0.36);
        let modulator_hz = (tonal_hz * TONAL_FM_RATIO).min(sample_rate * 0.42);
        self.tonal_mod_phase = (self.tonal_mod_phase + modulator_hz / sample_rate).fract();
        self.tonal_phase = (self.tonal_phase
            + (tonal_hz + table.at(self.tonal_mod_phase) * TONAL_FM_INDEX * modulator_hz)
                / sample_rate)
            .fract();
        let slot_seconds = 1.0 / self.detail_rate;
        let slot_envelope = (grain_phase * slot_seconds / 0.00012).min(1.0)
            * ((1.0 - grain_phase) * slot_seconds / 0.00035).min(1.0);
        SourceParts {
            body,
            texture: dirty_body * 0.35 + grain * 0.85 + onset,
            tonal: table.at(self.tonal_phase) * slot_envelope,
        }
    }

    fn shimmer(
        &mut self,
        time: f32,
        pitch_hz: f32,
        profile: SfxLayerRecipe,
        sample_rate: f32,
        table: &SineTable,
    ) -> f32 {
        if sample_rate < profile.minimum_sample_rate as f32 {
            return 0.0;
        }
        let age = time - profile.start_ms * 0.001;
        let duration = profile.duration_ms * 0.001;
        if !(0.0..duration).contains(&age) {
            return 0.0;
        }
        let envelope = (age / (profile.attack_ms * 0.001)).min(1.0)
            * ((duration - age) / (profile.release_ms * 0.001)).min(1.0);
        let carrier = (pitch_hz * 15.7)
            .clamp(8_400.0, 12_500.0)
            .min(sample_rate * 0.4);
        self.shimmer_phase = (self.shimmer_phase + carrier / sample_rate).fract();
        self.shimmer_mod_phase = (self.shimmer_mod_phase + SHIMMER_MOD_HZ / sample_rate).fract();
        table.at(
            (self.shimmer_phase + table.at(self.shimmer_mod_phase) * SHIMMER_PM_CYCLES + 1.0)
                .fract(),
        ) * envelope
    }
}

struct SourceParts {
    body: f32,
    texture: f32,
    tonal: f32,
}

struct BodyFilter {
    highpass: HighPass,
    lowpass: LowPass,
    lowpass_second: LowPass,
}

impl BodyFilter {
    fn new(rate: f32, highpass: f32, lowpass: f32) -> Self {
        Self {
            highpass: HighPass::new(rate, highpass.min(rate * 0.35)),
            lowpass: LowPass::new(rate, lowpass),
            lowpass_second: LowPass::new(rate, lowpass),
        }
    }
    fn cutoff(&mut self, rate: f32, highpass: f32, lowpass: f32) {
        self.highpass.set_cutoff(highpass.min(rate * 0.35));
        self.lowpass.set_cutoff(lowpass);
        self.lowpass_second.set_cutoff(lowpass);
    }
    fn tick(&mut self, sample: f32) -> f32 {
        self.lowpass_second
            .tick(self.lowpass.tick(self.highpass.tick(sample)))
    }
}

struct BandFilter {
    highpass: HighPass,
    lowpass: LowPass,
}

impl BandFilter {
    fn new(rate: f32, highpass: f32, lowpass: f32) -> Self {
        Self {
            highpass: HighPass::new(rate, highpass.min(rate * 0.35)),
            lowpass: LowPass::new(rate, lowpass.min(rate * 0.45)),
        }
    }
    fn tick(&mut self, sample: f32) -> f32 {
        self.lowpass.tick(self.highpass.tick(sample))
    }
}

struct BrightDetail {
    highpass: HighPass,
    lowpass: LowPass,
    rng: fastrand::Rng,
    previous_noise: f32,
    level: f32,
}

#[derive(Clone, Copy)]
struct CrispParameters {
    low_hz: [f32; 2],
    high_hz: [f32; 2],
    auxiliary_ratio: f32,
    pm_cycles: [f32; 3],
    noise_blend: [f32; 3],
    noise_lowpass_hz: f32,
    ratchet_hz: f32,
    level: f32,
}

fn parameter_contour(values: [f32; 3], progress: f32) -> f32 {
    let (from, to, position) = if progress < 0.22 {
        (values[0], values[1], progress / 0.22)
    } else {
        (values[1], values[2], (progress - 0.22) / 0.78)
    };
    let smooth = position * position * (3.0 - 2.0 * position);
    from + (to - from) * smooth
}

struct CrispState {
    phase: [f32; 3],
    noise: LowPass,
    rng: fastrand::Rng,
}

impl CrispState {
    fn new(rate: f32, parameters: CrispParameters, seed: u64) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed);
        Self {
            phase: std::array::from_fn(|_| rng.f32()),
            noise: LowPass::new(rate, parameters.noise_lowpass_hz),
            rng,
        }
    }

    fn tick(
        &mut self,
        time: f32,
        duration: f32,
        rate: f32,
        parameters: CrispParameters,
        table: &SineTable,
    ) -> f32 {
        let progress = (time / duration).clamp(0.0, 1.0);
        let sweep = progress * progress * (3.0 - 2.0 * progress);
        let low_hz = parameters.low_hz[0] + (parameters.low_hz[1] - parameters.low_hz[0]) * sweep;
        let high_hz = (parameters.high_hz[0]
            + (parameters.high_hz[1] - parameters.high_hz[0]) * (1.0 - (1.0 - progress).powi(3)))
        .min(rate * 0.35);
        let auxiliary_hz = (high_hz * parameters.auxiliary_ratio).min(rate * 0.37);
        for (phase, hz) in self.phase.iter_mut().zip([low_hz, high_hz, auxiliary_hz]) {
            *phase = (*phase + hz / rate).fract();
        }
        let low = table.at(self.phase[0]);
        let colored = self.noise.tick(self.rng.f32() * 2.0 - 1.0);
        let depth = parameter_contour(parameters.pm_cycles, progress);
        let blend = parameter_contour(parameters.noise_blend, progress);
        let high = table.at((self.phase[1] + low * depth + colored * blend + 1.0).fract());
        let auxiliary =
            table.at((self.phase[2] - low * depth * 0.37 - colored * blend * 0.5 + 1.0).fract());
        let ring = high * low;
        let complex = high * 0.48 + ring * 0.33 + auxiliary * low * low * 0.19;
        let attack = (time / 0.00035).min(1.0);
        let release = ((duration - time) / 0.002).min(1.0);
        let front = 0.2 + 0.8 * (1.0 - (time / 0.005).min(1.0)).powi(2);
        let ratchet = (1.0 - (time * parameters.ratchet_hz).fract() * 3.5).max(0.0);
        complex * attack * release * (front + ratchet * (0.38 - progress * 0.28))
    }
}

struct CrispDetail {
    states: [CrispState; 3],
    parameters: CrispParameters,
    profile: SfxLayerRecipe,
    highpass: [HighPass; 2],
    lowpass: LowPass,
    rate: f32,
}

impl CrispDetail {
    fn new(rate: f32, profile: SfxLayerRecipe, parameters: CrispParameters, seed: u64) -> Self {
        Self {
            states: std::array::from_fn(|index| {
                CrispState::new(
                    rate,
                    parameters,
                    seed ^ (index as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15),
                )
            }),
            parameters,
            profile,
            highpass: std::array::from_fn(|_| {
                HighPass::new(rate, profile.highpass_hz.min(rate * 0.35))
            }),
            lowpass: LowPass::new(rate, profile.lowpass_hz.min(rate * 0.45)),
            rate,
        }
    }

    fn part(&mut self, index: usize, time: f32, table: &SineTable) -> f32 {
        if self.rate < self.profile.minimum_sample_rate as f32 {
            return 0.0;
        }
        let age = time - self.profile.start_ms * 0.001;
        let duration = self.profile.duration_ms * 0.001;
        if !(0.0..duration).contains(&age) {
            return 0.0;
        }
        self.states[index].tick(age, duration, self.rate, self.parameters, table)
    }

    fn filter(&mut self, input: f32) -> f32 {
        let input = self.highpass[0].tick(input);
        self.lowpass.tick(self.highpass[1].tick(input)) * self.profile.level
    }
}

impl BrightDetail {
    fn new(sample_rate: f32, profile: SfxLayerRecipe, seed: u64) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed);
        let variation = 0.9 + rng.f32() * 0.2;
        Self {
            highpass: HighPass::new(
                sample_rate,
                (profile.highpass_hz * variation).min(sample_rate * 0.33),
            ),
            lowpass: LowPass::new(
                sample_rate,
                (profile.lowpass_hz * variation).min(sample_rate * 0.45),
            ),
            previous_noise: 0.0,
            level: profile.level * (0.85 + rng.f32() * 0.3),
            rng,
        }
    }

    fn tick(&mut self, envelope: f32) -> f32 {
        let injection = if envelope > 0.0 {
            let noise = self.rng.f32() * 2.0 - 1.0;
            let edge = noise - self.previous_noise * 0.45;
            self.previous_noise = noise;
            edge * envelope
        } else {
            0.0
        };
        self.lowpass.tick(self.highpass.tick(injection)) * self.level
    }
}

struct LayerProfiles {
    body: SfxLayerRecipe,
    texture: SfxLayerRecipe,
    tonal: SfxLayerRecipe,
    flecks: SfxLayerRecipe,
    shimmer: SfxLayerRecipe,
    crisp: SfxLayerRecipe,
}

impl From<[SfxLayerRecipe; 6]> for LayerProfiles {
    fn from(profiles: [SfxLayerRecipe; 6]) -> Self {
        let [body, texture, tonal, flecks, shimmer, crisp] = profiles;
        Self {
            body,
            texture,
            tonal,
            flecks,
            shimmer,
            crisp,
        }
    }
}

struct LayerSamples {
    body: f32,
    texture: f32,
    tonal: f32,
    flecks: f32,
    shimmer: f32,
    crisp: f32,
}

impl LayerSamples {
    fn select(self, selection: SfxLayerSelection) -> f32 {
        match selection {
            SfxLayerSelection::All => {
                self.body + self.texture + self.tonal + self.flecks + self.shimmer + self.crisp
            }
            SfxLayerSelection::Only(layer) => match layer {
                SfxLayer::Body => self.body,
                SfxLayer::Texture => self.texture,
                SfxLayer::Tonal => self.tonal,
                SfxLayer::Flecks => self.flecks,
                SfxLayer::Shimmer => self.shimmer,
                SfxLayer::Crisp => self.crisp,
            },
        }
    }
}

struct Voice {
    recipe: Recipe,
    pitch_hz: f32,
    duration: f32,
    age: u32,
    end_frame: u32,
    sample_rate: f32,
    level: f32,
    pan: f32,
    pan_gains: [f32; 2],
    delay_frames: usize,
    priority: Priority,
    started: u64,
    steps: [StepState; 3],
    rng: fastrand::Rng,
    body_filter: BodyFilter,
    texture_filter: BodyFilter,
    tonal_filter: BandFilter,
    shimmer_filter: BandFilter,
    bright: BrightDetail,
    crisp: CrispDetail,
    layers: SfxLayerSelection,
    profiles: LayerProfiles,
    base_cutoff: f32,
    filter_interval: u32,
}

impl Voice {
    fn new(event: SfxEvent, sample_rate: u32, seed: u64, started: u64) -> (Self, GeneratedEvent) {
        let mut rng = fastrand::Rng::with_seed(seed);
        let recipe = Recipe::for_kind(event.kind);
        let duration = recipe.duration * (0.92 + rng.f32() * 0.16);
        let sample_rate = sample_rate as f32;
        let pitch_hz = (recipe.pitch_hz * (0.955 + rng.f32() * 0.09)).min(sample_rate * 0.08);
        let pan = event.pan.get();
        let angle = (pan + 1.0) * FRAC_PI_4;
        let delay_frames = ((0.002 + 0.006 * pan.abs()) * sample_rate).round() as usize;
        let end_frame = ((duration + recipe.articulation.filter_tail) * sample_rate).ceil() as u32
            + delay_frames as u32;
        let base_cutoff = recipe.cutoff * (0.94 + rng.f32() * 0.12);
        let (cutoff, highpass) = recipe.articulation.filter.cutoffs(0.0, base_cutoff);
        let level = recipe.gain * event.intensity.get() * (0.93 + rng.f32() * 0.14);
        let steps = std::array::from_fn(|_| StepState::new(&mut rng));
        let profiles = LayerProfiles::from(recipe.layers());
        let bright = BrightDetail::new(sample_rate, profiles.flecks, rng.u64(..));
        let crisp = CrispDetail::new(
            sample_rate,
            profiles.crisp,
            recipe.patch.crisp(),
            seed ^ 0x4352_4953_505f_4846,
        );
        let voice = Self {
            recipe,
            pitch_hz,
            duration,
            age: 0,
            end_frame,
            sample_rate,
            level,
            pan,
            pan_gains: [angle.cos(), angle.sin()],
            delay_frames,
            priority: Priority::for_kind(event.kind),
            started,
            steps,
            rng,
            body_filter: BodyFilter::new(sample_rate, highpass, cutoff),
            texture_filter: BodyFilter::new(sample_rate, highpass, cutoff),
            tonal_filter: BandFilter::new(
                sample_rate,
                profiles.tonal.highpass_hz,
                profiles.tonal.lowpass_hz,
            ),
            shimmer_filter: BandFilter::new(
                sample_rate,
                profiles.shimmer.highpass_hz,
                profiles.shimmer.lowpass_hz,
            ),
            bright,
            crisp,
            layers: event.layers,
            profiles,
            base_cutoff,
            filter_interval: (sample_rate * 0.001).ceil().max(1.0) as u32,
        };
        let generated = GeneratedEvent {
            kind: event.kind,
            pan: event.pan,
            intensity: event.intensity,
            seed,
            pitch_hz,
            duration_seconds: end_frame as f32 / sample_rate,
            cutoff_hz: cutoff.min(sample_rate * 0.45),
            gain: level,
            layers: event.layers,
        };
        (voice, generated)
    }

    fn tick(&mut self, table: &SineTable) -> f32 {
        let time = self.age as f32 / self.sample_rate;
        let mut parts = SourceParts {
            body: 0.0,
            texture: 0.0,
            tonal: 0.0,
        };
        let mut shimmer = 0.0;
        let mut crisp = 0.0;
        let mut bright_envelope = 0.0;
        let mut filter_time = None;
        for (index, (step, state)) in self.recipe.steps.iter().zip(&mut self.steps).enumerate() {
            let local_time = time - step.start_fraction * self.duration;
            let duration = self.duration * step.length_fraction;
            if (0.0..duration).contains(&local_time) {
                let envelope = self.recipe.amplitude.gain(local_time, duration)
                    * self.recipe.articulation.accents.gain(local_time)
                    * step.level;
                let source = state.source(
                    self.recipe,
                    self.pitch_hz * step.pitch_ratio,
                    local_time,
                    self.sample_rate,
                    &mut self.rng,
                    table,
                );
                parts.body += source.body * envelope;
                parts.texture += source.texture * envelope;
                parts.tonal += source.tonal * envelope;
                shimmer += state.shimmer(
                    local_time,
                    self.pitch_hz * step.pitch_ratio,
                    self.profiles.shimmer,
                    self.sample_rate,
                    table,
                ) * step.level;
                crisp += self.crisp.part(index, local_time, table) * step.level;
                bright_envelope +=
                    self.recipe.articulation.flecks.envelope(local_time) * step.level;
                filter_time = Some(local_time);
            }
        }
        if self.age.is_multiple_of(self.filter_interval)
            && let Some(time) = filter_time
        {
            let (lowpass, highpass) = self
                .recipe
                .articulation
                .filter
                .cutoffs(time, self.base_cutoff);
            self.body_filter.cutoff(self.sample_rate, highpass, lowpass);
            self.texture_filter
                .cutoff(self.sample_rate, highpass, lowpass);
        }
        self.age += 1;
        LayerSamples {
            body: self.body_filter.tick(parts.body * self.level) * self.profiles.body.level,
            texture: self.texture_filter.tick(parts.texture * self.level)
                * self.profiles.texture.level,
            tonal: self.tonal_filter.tick(parts.tonal * self.level) * self.profiles.tonal.level,
            flecks: self.bright.tick(bright_envelope) * self.level,
            shimmer: self.shimmer_filter.tick(shimmer * self.level) * self.profiles.shimmer.level,
            crisp: self.crisp.filter(crisp * self.level),
        }
        .select(self.layers)
    }
}

struct VoiceSlot {
    voice: Option<Voice>,
    delay: Vec<f32>,
    cursor: usize,
    last_output: [f32; 2],
    residual: [f32; 2],
    residual_left: u32,
    residual_frames: u32,
}

impl VoiceSlot {
    fn new(sample_rate: u32) -> Self {
        Self {
            voice: None,
            delay: vec![0.0; (sample_rate as f32 * 0.008).ceil() as usize + 1],
            cursor: 0,
            last_output: [0.0; 2],
            residual: [0.0; 2],
            residual_left: 0,
            residual_frames: (sample_rate as f32 * 0.003).ceil() as u32,
        }
    }

    fn start(&mut self, voice: Voice) {
        if self.voice.is_some() {
            self.residual = self.last_output;
            self.residual_left = self.residual_frames;
        }
        self.delay.fill(0.0);
        self.cursor = 0;
        self.voice = Some(voice);
    }

    fn tick(&mut self, spatial_mix: f32, table: &SineTable) -> [f32; 2] {
        let mut output = [0.0; 2];
        if let Some(voice) = &mut self.voice {
            let mono = voice.tick(table);
            let read = (self.cursor + self.delay.len() - voice.delay_frames) % self.delay.len();
            let delayed = self.delay[read];
            self.delay[self.cursor] = mono;
            self.cursor = (self.cursor + 1) % self.delay.len();
            output = [mono * voice.pan_gains[0], mono * voice.pan_gains[1]];
            // Keep the direct signal intact; the quiet delayed opposite-ear copy
            // adds depth while retaining useful dry energy when folded to mono.
            let opposite = usize::from(voice.pan < 0.0);
            output[opposite] += delayed * 0.18 * voice.pan.abs() * spatial_mix;
            if voice.age >= voice.end_frame {
                self.voice = None;
            }
        }
        if self.residual_left > 0 {
            let fade = self.residual_left as f32 / self.residual_frames as f32;
            output[0] += self.residual[0] * fade;
            output[1] += self.residual[1] * fade;
            self.residual_left -= 1;
        }
        self.last_output = output;
        output
    }
}

#[derive(Default)]
struct Cooldowns {
    hover: Option<u64>,
    scroll: Option<u64>,
    motion: Option<u64>,
    zoom: Option<u64>,
    boundary: Option<u64>,
}

impl Cooldowns {
    fn entry(&mut self, kind: SfxKind) -> Option<(&mut Option<u64>, f32)> {
        use SfxKind::*;
        match kind {
            HoverEnter | HoverLeave => Some((&mut self.hover, 0.035)),
            Scroll | ScrollbarMove => Some((&mut self.scroll, 0.018)),
            DragOver | Resize => Some((&mut self.motion, 0.025)),
            ZoomIn | ZoomOut => Some((&mut self.zoom, 0.025)),
            ScrollBoundary => Some((&mut self.boundary, 0.09)),
            _ => None,
        }
    }

    fn is_limited(&mut self, kind: SfxKind, frame: u64, sample_rate: u32) -> bool {
        self.entry(kind).is_some_and(|(last, seconds)| {
            last.is_some_and(|last| frame - last < (seconds * sample_rate as f32) as u64)
        })
    }

    fn admit(&mut self, kind: SfxKind, frame: u64) {
        if let Some((last, _)) = self.entry(kind) {
            *last = Some(frame);
        }
    }
}

/// A fixed-capacity procedural voice pool. Only construction allocates.
pub(crate) struct SfxBank {
    sample_rate: u32,
    voices: Vec<VoiceSlot>,
    rng: fastrand::Rng,
    frame: u64,
    cooldowns: Cooldowns,
    spatial: SpatialMode,
    spatial_mix: f32,
    spatial_smoothing: f32,
    stolen: u64,
    generated: u64,
    coalesced: u64,
    table: SineTable,
}

impl SfxBank {
    pub(crate) fn new(
        sample_rate: u32,
        max_voices: usize,
        seed: u64,
        spatial: SpatialMode,
    ) -> Self {
        Self {
            sample_rate,
            voices: (0..max_voices)
                .map(|_| VoiceSlot::new(sample_rate))
                .collect(),
            rng: fastrand::Rng::with_seed(seed),
            frame: 0,
            cooldowns: Cooldowns::default(),
            spatial,
            spatial_mix: if spatial == SpatialMode::Haas {
                1.0
            } else {
                0.0
            },
            spatial_smoothing: 1.0 - (-1.0 / (sample_rate as f32 * 0.02)).exp(),
            stolen: 0,
            generated: 0,
            coalesced: 0,
            table: SineTable::new(),
        }
    }

    pub(crate) fn trigger(&mut self, event: SfxEvent) -> Option<GeneratedEvent> {
        if self
            .cooldowns
            .is_limited(event.kind, self.frame, self.sample_rate)
        {
            self.coalesced += 1;
            return None;
        }
        let priority = Priority::for_kind(event.kind);
        let index = self
            .voices
            .iter()
            .position(|slot| slot.voice.is_none())
            .or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .filter_map(|(index, slot)| slot.voice.as_ref().map(|voice| (index, voice)))
                    .filter(|(_, voice)| voice.priority <= priority)
                    .min_by_key(|(_, voice)| (voice.priority, voice.started))
                    .map(|(index, _)| index)
            });
        let Some(index) = index else {
            self.coalesced += 1;
            return None;
        };
        let (voice, generated) = Voice::new(event, self.sample_rate, self.rng.u64(..), self.frame);
        self.stolen += u64::from(self.voices[index].voice.is_some());
        self.voices[index].start(voice);
        self.cooldowns.admit(event.kind, self.frame);
        self.generated += 1;
        Some(generated)
    }

    pub(crate) fn tick(&mut self) -> [f32; 2] {
        let target = if self.spatial == SpatialMode::Haas {
            1.0
        } else {
            0.0
        };
        self.spatial_mix += (target - self.spatial_mix) * self.spatial_smoothing;
        let mut output = [0.0; 2];
        for slot in &mut self.voices {
            let sample = slot.tick(self.spatial_mix, &self.table);
            output[0] += sample[0];
            output[1] += sample[1];
        }
        self.frame += 1;
        output
    }

    pub(crate) fn set_spatial(&mut self, spatial: SpatialMode) {
        self.spatial = spatial;
    }

    pub(crate) fn active_voices(&self) -> usize {
        self.voices
            .iter()
            .filter(|slot| slot.voice.is_some())
            .count()
    }

    pub(crate) fn stolen_voices(&self) -> u64 {
        self.stolen
    }

    pub(crate) fn generated_events(&self) -> u64 {
        self.generated
    }

    pub(crate) fn coalesced_events(&self) -> u64 {
        self.coalesced
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Pan, Unit};
    use fundsp::{fft::real_fft, hacker32::Complex32};
    use strum::IntoEnumIterator;

    const SAMPLE_RATE: u32 = 48_000;

    fn render_component(kind: SfxKind, layer: SfxLayer, rate: u32) -> Vec<[f32; 2]> {
        let mut bank = SfxBank::new(rate, 1, 71, SpatialMode::Pan);
        let event = bank
            .trigger(SfxEvent {
                layers: SfxLayerSelection::Only(layer),
                ..SfxEvent::new(kind)
            })
            .unwrap();
        (0..(event.duration_seconds * rate as f32).ceil() as usize + 1)
            .map(|_| bank.tick())
            .collect()
    }

    fn energy(frames: &[[f32; 2]]) -> f64 {
        frames
            .iter()
            .flatten()
            .map(|&value| f64::from(value).powi(2))
            .sum()
    }

    fn spectrum(frames: &[[f32; 2]], rate: u32) -> (f64, f64) {
        const LENGTH: usize = 512;
        let mut input = [0.0_f32; LENGTH];
        let mut output = [Complex32::new(0.0, 0.0); LENGTH / 2 + 1];
        let mut total = 0.0;
        let mut above = 0.0;
        let mut weighted = 0.0;
        for frames in frames.windows(LENGTH).step_by(LENGTH / 4) {
            for (index, (frame, input)) in frames.iter().zip(&mut input).enumerate() {
                let hann = 0.5 - 0.5 * (TAU * index as f32 / (LENGTH - 1) as f32).cos();
                *input = frame[0] * hann;
            }
            real_fft(&input, &mut output);
            for (index, bin) in output.iter().enumerate() {
                let hz = index as f64 * rate as f64 / LENGTH as f64;
                let power = bin.norm_sqr() as f64;
                total += power;
                weighted += power * hz;
                if hz >= 6_000.0 {
                    above += power;
                }
            }
        }
        assert!(total > 0.0);
        (above / total, weighted / total)
    }

    #[test]
    fn crisp_intermodulation_stays_high_quiet_and_unavailable_at_low_rates() {
        let crisp = render_component(SfxKind::OrderFill, SfxLayer::Crisp, SAMPLE_RATE);
        let tonal = render_component(SfxKind::OrderFill, SfxLayer::Tonal, SAMPLE_RATE);
        let (share, _) = spectrum(&crisp, SAMPLE_RATE);
        assert!(
            share > 0.92,
            "crisp detail leaked into the body band: {share}"
        );
        assert!(energy(&crisp) > 0.000001);
        assert!(
            energy(&crisp) < energy(&tonal) * 0.20,
            "high-band fill must remain subordinate"
        );
        assert_eq!(
            energy(&render_component(
                SfxKind::OrderFill,
                SfxLayer::Crisp,
                8_000
            )),
            0.0
        );
        for rate in [20_000, 44_100, 192_000] {
            assert!(
                render_component(SfxKind::Confirm, SfxLayer::Crisp, rate)
                    .iter()
                    .flatten()
                    .all(|value| value.is_finite())
            );
        }
    }

    #[test]
    fn crisp_parameter_contours_change_the_high_band_during_a_part() {
        let crisp = render_component(SfxKind::OrderFill, SfxLayer::Crisp, SAMPLE_RATE);
        let (_, first) = spectrum(&crisp[48..720], SAMPLE_RATE);
        let (_, late) = spectrum(&crisp[2_880..4_128], SAMPLE_RATE);
        assert!(
            first > late + 1_000.0,
            "carrier/depth/noise envelopes should reshape the high band: {first} -> {late}"
        );
    }

    #[test]
    fn sharp_click_body_decays_to_a_low_tail_instead_of_a_flat_plateau() {
        for kind in [SfxKind::Click, SfxKind::SecondaryClick] {
            let body = render_component(kind, SfxLayer::Body, SAMPLE_RATE);
            let front = energy(&body[48..240]) / (240 - 48) as f64;
            let tail = energy(&body[960..1_440]) / (1_440 - 960) as f64;
            assert!(
                tail < front * 0.12,
                "{kind}: short click should fall below its initial body: {tail} / {front}"
            );
        }
    }

    fn bank(seed: u64, max_voices: usize, spatial: SpatialMode) -> SfxBank {
        SfxBank::new(SAMPLE_RATE, max_voices, seed, spatial)
    }

    #[test]
    fn intermodulated_texture_is_at_least_ten_decibels_below_its_unit_layer() {
        let table = SineTable::new();
        for kind in [
            SfxKind::Click,
            SfxKind::Focus,
            SfxKind::Confirm,
            SfxKind::OrderFill,
        ] {
            let event = SfxEvent {
                layers: SfxLayerSelection::Only(SfxLayer::Texture),
                ..SfxEvent::new(kind)
            };
            let (mut quieter, _) = Voice::new(event, SAMPLE_RATE, 71, 0);
            let (mut unit_layer, _) = Voice::new(event, SAMPLE_RATE, 71, 0);
            unit_layer.profiles.texture.level = 1.0;
            let mut quiet_energy = 0.0_f64;
            let mut unit_energy = 0.0_f64;
            for _ in 0..quieter.end_frame {
                quiet_energy += f64::from(quieter.tick(&table)).powi(2);
                unit_energy += f64::from(unit_layer.tick(&table)).powi(2);
            }
            let ratio = quiet_energy / unit_energy;
            assert!(
                ratio > 0.0 && ratio < 0.1,
                "{kind}: texture energy ratio {ratio}"
            );
        }
    }

    #[test]
    fn routine_gestures_are_fast_while_long_signatures_keep_their_shape() {
        for seed in [7, 19, 44, 71, 123] {
            for (kind, maximum) in [
                (SfxKind::Click, 0.064),
                (SfxKind::HoverEnter, 0.045),
                (SfxKind::Scroll, 0.037),
                (SfxKind::ToggleOn, 0.11),
                (SfxKind::TabOpen, 0.145),
                (SfxKind::DragStart, 0.13),
                (SfxKind::ZoomIn, 0.105),
            ] {
                let mut bank = bank(seed, 1, SpatialMode::Pan);
                let event = bank.trigger(SfxEvent::new(kind)).unwrap();
                assert!(
                    event.duration_seconds < maximum,
                    "{kind}: {}s",
                    event.duration_seconds
                );
            }
            for (kind, minimum) in [
                (SfxKind::OrderCreate, 0.66),
                (SfxKind::OrderFill, 0.88),
                (SfxKind::DialogOpen, 0.42),
                (SfxKind::Error, 0.52),
            ] {
                let mut bank = bank(seed, 1, SpatialMode::Pan);
                let event = bank.trigger(SfxEvent::new(kind)).unwrap();
                assert!(
                    event.duration_seconds > minimum,
                    "{kind}: {}s",
                    event.duration_seconds
                );
            }
        }
    }

    #[test]
    fn bright_details_add_a_short_high_band_without_a_continuous_hiss() {
        let table = SineTable::new();
        let mut detail_energy = 0.0;
        let mut high_band_energy = 0.0;
        let mut late_energy = 0.0;
        for seed in [7, 19, 44, 71, 123] {
            let (mut voice, _) = Voice::new(SfxEvent::new(SfxKind::Click), SAMPLE_RATE, seed, 0);
            let (mut body_only, _) =
                Voice::new(SfxEvent::new(SfxKind::Click), SAMPLE_RATE, seed, 0);
            body_only.bright.level = 0.0;
            let mut highpass = HighPass::new(SAMPLE_RATE as f32, 2_500.0);
            for frame in 0..(SAMPLE_RATE / 20) {
                let detail = voice.tick(&table) - body_only.tick(&table);
                let high = highpass.tick(detail);
                detail_energy += detail * detail;
                high_band_energy += high * high;
                if frame > SAMPLE_RATE / 100 {
                    late_energy += detail * detail;
                }
            }
        }
        assert!(
            detail_energy > 0.000001,
            "bright path should contribute audible transient energy"
        );
        assert!(
            high_band_energy > detail_energy * 0.35,
            "flecks should emphasize their high-pass band: {}",
            high_band_energy / detail_energy
        );
        assert!(
            late_energy < detail_energy * 0.0001,
            "high-frequency injection must end quickly: {}",
            late_energy / detail_energy
        );
    }

    #[test]
    fn digital_voice_holds_a_low_body_level_until_its_short_release() {
        let table = SineTable::new();
        let mut early_energy = 0.0;
        let mut late_energy = 0.0;
        let mut onset_energy = 0.0;
        // Average the held level over variations: individual 15 ms windows
        // intentionally contain different rapidly alternating micro-details.
        for seed in [7, 19, 28, 44, 71, 81, 91, 123] {
            let (mut voice, _) = Voice::new(SfxEvent::new(SfxKind::Focus), SAMPLE_RATE, seed, 0);
            let duration = voice.duration;
            let samples: Vec<_> = (0..((duration + 0.02) * SAMPLE_RATE as f32) as usize)
                .map(|_| voice.tick(&table))
                .collect();
            let rms = |start: f32, end: f32| {
                let window = &samples
                    [(start * SAMPLE_RATE as f32) as usize..(end * SAMPLE_RATE as f32) as usize];
                (window.iter().map(|sample| sample * sample).sum::<f32>() / window.len() as f32)
                    .sqrt()
            };
            let early_hold = rms(0.025, 0.04);
            let late_hold = rms(duration - 0.025, duration - 0.01);
            early_energy += early_hold * early_hold;
            late_energy += late_hold * late_hold;
            let onset = samples[..(0.01 * SAMPLE_RATE as f32) as usize]
                .iter()
                .map(|sample| sample.abs())
                .fold(0.0_f32, f32::max);
            onset_energy += onset * onset;
            assert!(rms(duration + 0.008, duration + 0.015) < late_hold * 0.001);
        }
        let hold_ratio = (late_energy / early_energy).sqrt();
        assert!(
            (0.75..1.3).contains(&hold_ratio),
            "sustain body drifted: late/early RMS {hold_ratio}"
        );
        assert!(
            onset_energy > early_energy * 4.0,
            "transient should decay to a lower held body"
        );
    }

    #[test]
    fn every_event_pitch_settles_within_five_milliseconds() {
        let table = SineTable::new();
        for kind in SfxKind::iter() {
            let recipe = Recipe::for_kind(kind);
            let mut rng = fastrand::Rng::with_seed(71);
            let mut state = StepState::new(&mut rng);
            let frequency_at = |time: f32, state: &mut StepState, rng: &mut fastrand::Rng| {
                let before = state.phase[0];
                state.source(recipe, 400.0, time, SAMPLE_RATE as f32, rng, &table);
                (state.phase[0] - before).rem_euclid(1.0) * SAMPLE_RATE as f32
            };
            let early = frequency_at(0.006, &mut state, &mut rng);
            let late = frequency_at(recipe.duration * 0.8, &mut state, &mut rng);
            assert!(
                (early - late).abs() < 0.1,
                "{kind}: pitch kept moving after onset: {early} -> {late} Hz"
            );
        }
    }

    #[test]
    fn digital_details_break_up_a_dominant_clean_carrier() {
        let (mut voice, _) = Voice::new(SfxEvent::new(SfxKind::Focus), SAMPLE_RATE, 71, 0);
        let pitch = voice.pitch_hz;
        let table = SineTable::new();
        let start = (0.02 * SAMPLE_RATE as f32) as usize;
        let end = ((voice.duration - 0.008) * SAMPLE_RATE as f32) as usize;
        let samples: Vec<_> = (0..end).map(|_| voice.tick(&table)).collect();
        let window = &samples[start..];
        let mut projection = [0.0_f64; 2];
        let mut energy = 0.0_f64;
        for (index, &sample) in window.iter().enumerate() {
            let phase = (index + start) as f64 * f64::from(pitch) * std::f64::consts::TAU
                / f64::from(SAMPLE_RATE);
            projection[0] += f64::from(sample) * phase.sin();
            projection[1] += f64::from(sample) * phase.cos();
            energy += f64::from(sample).powi(2);
        }
        let carrier_share =
            2.0 * (projection[0].powi(2) + projection[1].powi(2)) / (window.len() as f64 * energy);
        assert!(
            carrier_share < 0.6,
            "clean carrier dominates the held digital texture: {carrier_share:.3}"
        );
        let detail_energies: Vec<_> = window
            .chunks((SAMPLE_RATE / 500) as usize)
            .filter(|chunk| chunk.len() == (SAMPLE_RATE / 500) as usize)
            .map(|chunk| chunk.iter().map(|sample| sample * sample).sum::<f32>())
            .collect();
        let quietest = detail_energies
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min);
        let strongest = detail_energies.iter().copied().fold(0.0_f32, f32::max);
        assert!(
            strongest > quietest * 2.0,
            "digital micro-details lack short-timescale contrast"
        );
    }

    #[test]
    fn every_effect_is_finite_audible_and_finishes_at_supported_sample_rates() {
        for sample_rate in [8_000, SAMPLE_RATE, 192_000] {
            for kind in SfxKind::iter() {
                let mut bank = SfxBank::new(sample_rate, 1, 71, SpatialMode::Haas);
                let generated = bank.trigger(SfxEvent::new(kind)).unwrap();
                let frames = (generated.duration_seconds * sample_rate as f32).ceil() as usize + 1;
                let mut peak = 0.0_f32;
                for _ in 0..frames {
                    let sample = bank.tick();
                    assert!(sample.iter().all(|sample| sample.is_finite()), "{kind}");
                    peak = peak.max(sample[0].abs()).max(sample[1].abs());
                }
                assert!((0.00005..0.5).contains(&peak), "{kind}: peak {peak}");
                assert_eq!(bank.active_voices(), 0, "{kind}");
                assert!(bank.tick().iter().all(|sample| sample.abs() < 0.00001));
            }
        }
    }

    #[test]
    fn a_seed_reproduces_samples_while_repeated_events_vary() {
        let mut a = bank(19, 4, SpatialMode::Haas);
        let mut b = bank(19, 4, SpatialMode::Haas);
        let first = a.trigger(SfxEvent::new(SfxKind::Click)).unwrap();
        assert_eq!(Some(first), b.trigger(SfxEvent::new(SfxKind::Click)));
        for _ in 0..5_000 {
            assert_eq!(a.tick(), b.tick());
        }
        let second = a.trigger(SfxEvent::new(SfxKind::Click)).unwrap();
        assert_eq!(Some(second), b.trigger(SfxEvent::new(SfxKind::Click)));
        assert_ne!(first.seed, second.seed);
        assert_ne!(first.pitch_hz, second.pitch_hz);
        for _ in 0..5_000 {
            assert_eq!(a.tick(), b.tick());
        }
    }

    #[test]
    fn screen_position_controls_early_arrival_and_energy() {
        for pan in [-1.0_f32, 1.0] {
            let mut bank = bank(81, 1, SpatialMode::Haas);
            bank.trigger(SfxEvent {
                kind: SfxKind::Click,
                pan: Pan::new(pan).unwrap(),
                intensity: Unit::ONE,
                layers: SfxLayerSelection::All,
            })
            .unwrap();
            let near = usize::from(pan > 0.0);
            let far = 1 - near;
            let mut energy = [0.0; 2];
            for frame in 0..6_000 {
                let sample = bank.tick();
                if frame < 380 {
                    assert!(sample[far].abs() < 0.0000001);
                }
                energy[0] += sample[0] * sample[0];
                energy[1] += sample[1] * sample[1];
            }
            assert!(energy[near] > energy[far] * 10.0);
            assert!(energy[far] > 0.0);
        }
        let mut center = bank(81, 1, SpatialMode::Haas);
        center.trigger(SfxEvent::new(SfxKind::Click)).unwrap();
        for _ in 0..4_000 {
            let [left, right] = center.tick();
            assert!((left - right).abs() < 0.000001);
        }
    }

    #[test]
    fn event_storms_are_bounded_and_transaction_voices_are_protected() {
        let mut bank = bank(91, 2, SpatialMode::Pan);
        for _ in 0..1_000 {
            let _ = bank.trigger(SfxEvent::new(SfxKind::HoverEnter));
        }
        assert_eq!(bank.generated_events(), 1);
        assert_eq!(bank.coalesced_events(), 999);
        bank.trigger(SfxEvent::new(SfxKind::OrderFill)).unwrap();
        bank.trigger(SfxEvent::new(SfxKind::OrderCreate)).unwrap();
        assert_eq!(bank.active_voices(), 2);
        assert_eq!(bank.stolen_voices(), 1);
        assert!(bank.trigger(SfxEvent::new(SfxKind::Click)).is_none());
        assert!(bank.trigger(SfxEvent::new(SfxKind::Error)).is_some());
        assert_eq!(bank.active_voices(), 2);
        assert_eq!(bank.stolen_voices(), 2);
    }

    #[test]
    fn long_actions_have_a_signature_beyond_the_initial_click() {
        let mut click = bank(28, 1, SpatialMode::Pan);
        let mut fill = bank(28, 1, SpatialMode::Pan);
        let click_event = click.trigger(SfxEvent::new(SfxKind::Click)).unwrap();
        let fill_event = fill.trigger(SfxEvent::new(SfxKind::OrderFill)).unwrap();
        assert!(fill_event.duration_seconds > click_event.duration_seconds * 5.0);
        let mut late_click_energy = 0.0;
        let mut late_fill_energy = 0.0;
        for frame in 0..SAMPLE_RATE {
            let click_sample = click.tick();
            let fill_sample = fill.tick();
            if frame > SAMPLE_RATE / 3 {
                late_click_energy += click_sample[0] * click_sample[0];
                late_fill_energy += fill_sample[0] * fill_sample[0];
            }
        }
        assert_eq!(late_click_energy, 0.0);
        assert!(late_fill_energy > 0.001);
    }

    #[test]
    fn stealing_starts_from_the_current_output_and_fades_the_residual() {
        let mut bank = bank(44, 1, SpatialMode::Pan);
        bank.trigger(SfxEvent::new(SfxKind::Click)).unwrap();
        let mut last = [0.0; 2];
        for _ in 0..220 {
            last = bank.tick();
        }
        bank.trigger(SfxEvent::new(SfxKind::OrderFill)).unwrap();
        let first = bank.tick();
        assert_eq!(first, last);
        assert_eq!(bank.stolen_voices(), 1);
        for _ in 0..100_000 {
            assert!(bank.tick().iter().all(|sample| sample.is_finite()));
        }
        assert_eq!(bank.active_voices(), 0);
        assert_eq!(bank.tick(), [0.0; 2]);
    }
}
