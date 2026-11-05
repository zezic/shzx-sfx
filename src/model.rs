use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

use crate::{Chord, Error, HarmonyMode, PitchClass, Result, ScaleDegree, SfxLayer};

/// Validated uppercase ticker stored inline so commands remain allocation-free.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct Symbol(arrayvec::ArrayString<24>);

impl Symbol {
    pub const MAX_LEN: usize = 24;

    pub fn new(value: &str) -> Result<Self> {
        if value.is_empty() || value.len() > Self::MAX_LEN {
            return Err(Error::InvalidSymbol("expected 1..=24 ASCII characters"));
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'^'))
            || !value.bytes().any(|byte| byte.is_ascii_alphanumeric())
        {
            return Err(Error::InvalidSymbol(
                "use letters, digits, '.', '-', or '^' with at least one letter or digit",
            ));
        }
        let mut symbol = arrayvec::ArrayString::new();
        for byte in value.bytes() {
            symbol.push(char::from(byte.to_ascii_uppercase()));
        }
        Ok(Self(symbol))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Diatonic anchor in the A-natural-minor palette.
    #[must_use]
    pub fn root(self) -> ScaleDegree {
        ScaleDegree::ALL[(self.ascii_hash() % ScaleDegree::ALL.len() as u64) as usize]
    }

    pub(crate) fn ascii_hash(self) -> u64 {
        self.0
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            })
    }
}

impl Default for Symbol {
    fn default() -> Self {
        Self::new("AAPL").expect("default ticker is valid")
    }
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for Symbol {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<String> for Symbol {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        Self::new(&value)
    }
}

impl Serialize for Symbol {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// A finite value in the inclusive range 0..=1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct Unit(f32);

impl Unit {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    pub fn new(value: f32) -> Result<Self> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidParameter {
                name: "normalized value",
                value: value as f64,
                expected: "a finite value in 0..=1",
            })
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    pub(crate) fn clamped(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }
}

impl TryFrom<f32> for Unit {
    type Error = Error;
    fn try_from(value: f32) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Unit> for f32 {
    fn from(value: Unit) -> Self {
        value.get()
    }
}

/// Horizontal screen position: -1 is left, 0 is center, +1 is right.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct Pan(f32);

impl Pan {
    pub const CENTER: Self = Self(0.0);
    pub fn new(value: f32) -> Result<Self> {
        if value.is_finite() && (-1.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidParameter {
                name: "pan",
                value: value as f64,
                expected: "a finite value in -1..=1",
            })
        }
    }
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl TryFrom<f32> for Pan {
    type Error = Error;
    fn try_from(value: f32) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Pan> for f32 {
    fn from(value: Pan) -> Self {
        value.get()
    }
}

/// Output trim in decibels, separate from the master's 0..1 attenuation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct OutputGain(f32);

impl OutputGain {
    pub const MIN_DB: f32 = -60.0;
    pub const MAX_DB: f32 = 24.0;

    pub fn new(db: f32) -> Result<Self> {
        if db.is_finite() && (Self::MIN_DB..=Self::MAX_DB).contains(&db) {
            Ok(Self(db))
        } else {
            Err(Error::InvalidParameter {
                name: "output gain",
                value: db as f64,
                expected: "finite decibels in -60..=24",
            })
        }
    }

    #[must_use]
    pub const fn db(self) -> f32 {
        self.0
    }

    #[must_use]
    pub fn amplitude(self) -> f32 {
        10.0_f32.powf(self.0 / 20.0)
    }
}

impl Default for OutputGain {
    fn default() -> Self {
        Self(12.0)
    }
}

impl TryFrom<f32> for OutputGain {
    type Error = Error;
    fn try_from(db: f32) -> Result<Self> {
        Self::new(db)
    }
}

impl From<OutputGain> for f32 {
    fn from(gain: OutputGain) -> Self {
        gain.db()
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Display, EnumString,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Trend {
    Bearish,
    #[default]
    Sideways,
    Bullish,
}

/// A summary of the candles visible on screen, independent of a market feed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketContext {
    pub trend: Trend,
    pub trend_strength: Unit,
    /// Mean visible volume divided by a caller-selected reference, capped at one.
    pub volume: Unit,
    pub volatility: Unit,
    /// 0 is the broad overview; 1 is the deepest zoom.
    pub zoom: Unit,
    /// Recent interaction activity, decayed by the host application.
    pub activity: Unit,
}

impl Default for MarketContext {
    fn default() -> Self {
        Self {
            trend: Trend::Sideways,
            trend_strength: Unit(0.2),
            volume: Unit(0.35),
            volatility: Unit(0.2),
            zoom: Unit::ZERO,
            activity: Unit(0.1),
        }
    }
}

/// Validated OHLCV input for deriving a visible-market summary.
#[derive(Clone, Copy, Debug)]
pub struct Candle {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

impl Candle {
    pub fn new(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Result<Self> {
        for (name, value) in [
            ("open", open),
            ("high", high),
            ("low", low),
            ("close", close),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(Error::InvalidParameter {
                    name,
                    value,
                    expected: "a finite positive price",
                });
            }
        }
        if high < open.max(close) || low > open.min(close) || high < low {
            return Err(Error::InvalidParameter {
                name: "OHLC range",
                value: high - low,
                expected: "low <= open/close <= high",
            });
        }
        if !volume.is_finite() || volume < 0.0 {
            return Err(Error::InvalidParameter {
                name: "volume",
                value: volume,
                expected: "finite and nonnegative",
            });
        }
        Ok(Self {
            open,
            high,
            low,
            close,
            volume,
        })
    }
}

impl MarketContext {
    /// Derive a scale-independent summary. `reference_volume` is mean volume
    /// corresponding to maximum texture density, typically a rolling percentile.
    pub fn from_candles(
        candles: &[Candle],
        reference_volume: f64,
        zoom: Unit,
        activity: Unit,
    ) -> Result<Self> {
        let Some(first) = candles.first() else {
            return Err(Error::InvalidParameter {
                name: "candles",
                value: 0.0,
                expected: "at least one visible candle",
            });
        };
        if !reference_volume.is_finite() || reference_volume <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "reference volume",
                value: reference_volume,
                expected: "finite and positive",
            });
        }
        let last = candles[candles.len() - 1];
        let high = candles
            .iter()
            .map(|c| c.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let low = candles.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
        let displacement = last.close - first.open;
        let strength = if high > low {
            displacement / (high - low)
        } else {
            0.0
        };
        let trend = if strength > 0.08 {
            Trend::Bullish
        } else if strength < -0.08 {
            Trend::Bearish
        } else {
            Trend::Sideways
        };
        // Scale before summation to avoid overflow for finite, large inputs.
        let count = candles.len() as f64;
        let volume = candles
            .iter()
            .map(|c| c.volume / reference_volume / count)
            .fold(0.0_f64, |sum, contribution| (sum + contribution).min(1.0));
        let range = candles
            .iter()
            .map(|c| ((c.high - c.low) / c.close).min(1.0) / count)
            .sum::<f64>();
        Ok(Self {
            trend,
            trend_strength: Unit::clamped(strength.abs() as f32),
            volume: Unit::clamped(volume as f32),
            volatility: Unit::clamped((range * 20.0) as f32),
            zoom,
            activity,
        })
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Display, EnumString,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Mode {
    #[default]
    Browse,
    Dialog,
    Focus,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Display, EnumString,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum SpatialMode {
    Pan,
    #[default]
    Haas,
}

/// A DSP parameter's current value and the requested destination.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ParameterSnapshot {
    pub current: f32,
    pub target: f32,
}

/// An autonomous decision can be scheduled, held, or fixed by its current mode.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum ClockState {
    Scheduled { in_seconds: f32 },
    Held { recheck_in_seconds: f32 },
    Fixed,
}

impl ClockState {
    pub(crate) fn at(next: u64, sample: u64, rate: f32, held: bool) -> Self {
        let seconds = next.saturating_sub(sample) as f32 / rate;
        if held {
            Self::Held {
                recheck_in_seconds: seconds,
            }
        } else {
            Self::Scheduled {
                in_seconds: seconds,
            }
        }
    }
}

/// A chord change uses two voiced banks rather than sliding oscillator pitch.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum ChordTransition {
    Steady {
        chord: Chord,
    },
    Crossfading {
        from: Chord,
        to: Chord,
        progress: Unit,
        remaining_seconds: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct HarmonySnapshot {
    pub mode: HarmonyMode,
    pub root: PitchClass,
    pub root_transition: RootTransition,
    pub transition: ChordTransition,
    /// Predicted from the current smoothed market controls. Further smoothing
    /// or a later view change can replace this choice before the decision.
    pub next: Chord,
    pub clock: ClockState,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum RootTransition {
    Steady {
        root: PitchClass,
    },
    Crossfading {
        from: PitchClass,
        to: PitchClass,
        progress: Unit,
        remaining_seconds: f32,
    },
}

/// Source activity excludes the delay and reverb tails that follow a voice.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum BackgroundVoice {
    Silent,
    Playing {
        frequency_hz: f32,
        remaining_seconds: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct DetailSnapshot {
    pub generated: u64,
    pub next_in_seconds: f32,
    pub voice: BackgroundVoice,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct MelodySnapshot {
    pub generated: u64,
    pub clock: ClockState,
    pub voice: BackgroundVoice,
}

/// Current and requested parameters for one autonomous ping-pong send.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct DelaySnapshot {
    pub time_ms: ParameterSnapshot,
    pub feedback: ParameterSnapshot,
    pub send: ParameterSnapshot,
}

/// Activity and the next entrance of an autonomous environment source.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct AmbientSnapshot {
    pub generated: u64,
    pub active: usize,
    pub clock: ClockState,
}

/// Current environment state, destinations, and autonomous arrangement clocks.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BackgroundSnapshot {
    /// Smoothed controls actually driving synthesis. `Snapshot::market`
    /// separately records the requested view.
    pub market: MarketContext,
    pub focus: ParameterSnapshot,
    pub lowpass_hz: ParameterSnapshot,
    pub highpass_hz: ParameterSnapshot,
    pub reverb_room: ParameterSnapshot,
    pub reverb_width: ParameterSnapshot,
    pub harmony: HarmonySnapshot,
    pub harmonic_swell: Unit,
    pub details: DetailSnapshot,
    pub melody: MelodySnapshot,
    pub demonic_voices: AmbientSnapshot,
    pub demonic_stutters: AmbientSnapshot,
    pub flying_coins: AmbientSnapshot,
    pub laser_beams: AmbientSnapshot,
    pub detail_delay: DelaySnapshot,
    pub melody_delay: DelaySnapshot,
}

/// Layer levels are linear amplitude multipliers. Master defaults conservatively.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mix {
    pub master: Unit,
    pub output_gain: OutputGain,
    pub drone: Unit,
    pub sub: Unit,
    pub texture: Unit,
    pub harmony: Unit,
    pub melody: Unit,
    pub demonic_voices: Unit,
    pub demonic_stutters: Unit,
    pub flying_coins: Unit,
    pub laser_beams: Unit,
    /// UI/event master attenuation shared by the dry sound and wet return.
    pub ui: Unit,
    pub transmission: Unit,
    pub reverb: Unit,
}

impl Default for Mix {
    fn default() -> Self {
        Self {
            master: Unit(0.55),
            output_gain: OutputGain::default(),
            drone: Unit(0.35),
            sub: Unit(0.35),
            texture: Unit(0.45),
            harmony: Unit(0.55),
            melody: Unit(0.08),
            demonic_voices: Unit(0.3),
            demonic_stutters: Unit(0.2),
            flying_coins: Unit(0.05),
            laser_beams: Unit(0.05),
            ui: Unit(0.316_227_76),
            transmission: Unit(0.10),
            reverb: Unit(0.5),
        }
    }
}

/// Native reverb controls. Room size controls feedback and tail length; it is
/// not a calibrated decay time in seconds. Wet is the per-bus return level.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReverbParameters {
    pub send: Unit,
    pub room_size: Unit,
    pub damping: Unit,
    pub width: Unit,
    pub wet: Unit,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReverbSettings {
    pub environment: ReverbParameters,
    pub events: ReverbParameters,
    pub transmission: ReverbParameters,
    pub melody: ReverbParameters,
    pub accents: ReverbParameters,
}

impl Default for ReverbSettings {
    fn default() -> Self {
        Self {
            environment: ReverbParameters {
                send: Unit(0.85),
                room_size: Unit(0.92),
                damping: Unit(0.72),
                width: Unit(0.92),
                wet: Unit(0.32),
            },
            events: ReverbParameters {
                send: Unit(0.12),
                room_size: Unit(0.90),
                damping: Unit(0.55),
                width: Unit(0.95),
                wet: Unit(0.32),
            },
            transmission: ReverbParameters {
                send: Unit(0.67),
                room_size: Unit(0.78),
                damping: Unit(0.83),
                width: Unit(0.75),
                wet: Unit(0.25),
            },
            melody: ReverbParameters {
                send: Unit(0.95),
                room_size: Unit(0.88),
                damping: Unit(0.68),
                width: Unit(0.92),
                wet: Unit(0.85),
            },
            accents: ReverbParameters {
                send: Unit(0.85),
                room_size: Unit(0.88),
                damping: Unit(0.08),
                width: Unit(0.92),
                wet: Unit(0.32),
            },
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    Display,
    EnumString,
    EnumIter,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum SfxKind {
    Click,
    SecondaryClick,
    Confirm,
    Cancel,
    DestructiveClick,
    ToggleOn,
    ToggleOff,
    HoverEnter,
    HoverLeave,
    Focus,
    Blur,
    Scroll,
    ScrollBoundary,
    ScrollbarGrab,
    ScrollbarMove,
    ScrollbarRelease,
    DragStart,
    DragOver,
    Drop,
    DropRejected,
    Resize,
    ZoomIn,
    ZoomOut,
    TabOpen,
    TabClose,
    DialogOpen,
    DialogClose,
    Search,
    DataRefresh,
    Notification,
    Warning,
    Error,
    OrderCreate,
    OrderFill,
    OrderCancel,
    Connect,
    Disconnect,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SfxEvent {
    pub kind: SfxKind,
    pub pan: Pan,
    pub intensity: Unit,
    #[serde(default)]
    pub layers: SfxLayerSelection,
    #[serde(default)]
    pub edits: SfxEdits,
}

/// A bounded multiplier for an event's pitch or duration.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct SfxScale(f32);

impl SfxScale {
    pub const MIN: f32 = 0.25;
    pub const MAX: f32 = 4.0;
    pub const ONE: Self = Self(1.0);

    pub fn new(value: f32) -> Result<Self> {
        if value.is_finite() && (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidParameter {
                name: "SFX scale",
                value: value as f64,
                expected: "a finite multiplier in 0.25..=4",
            })
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Default for SfxScale {
    fn default() -> Self {
        Self::ONE
    }
}

impl TryFrom<f32> for SfxScale {
    type Error = Error;

    fn try_from(value: f32) -> Result<Self> {
        Self::new(value)
    }
}

impl From<SfxScale> for f32 {
    fn from(value: SfxScale) -> Self {
        value.get()
    }
}

/// An ordered filter band, validated before it reaches synthesis.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SfxFilterBandInput")]
pub struct SfxFilterBand {
    highpass_hz: f32,
    lowpass_hz: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SfxFilterBandInput {
    highpass_hz: f32,
    lowpass_hz: f32,
}

impl SfxFilterBand {
    pub const MIN_HZ: f32 = 10.0;
    pub const MAX_HZ: f32 = 24_000.0;

    pub fn new(highpass_hz: f32, lowpass_hz: f32) -> Result<Self> {
        for (name, value) in [("SFX high-pass", highpass_hz), ("SFX low-pass", lowpass_hz)] {
            if !value.is_finite() || !(Self::MIN_HZ..=Self::MAX_HZ).contains(&value) {
                return Err(Error::InvalidParameter {
                    name,
                    value: value as f64,
                    expected: "a finite frequency in 10..=24000 Hz",
                });
            }
        }
        if highpass_hz >= lowpass_hz {
            return Err(Error::InvalidParameter {
                name: "SFX high-pass",
                value: highpass_hz as f64,
                expected: "a frequency below the low-pass cutoff",
            });
        }
        Ok(Self {
            highpass_hz,
            lowpass_hz,
        })
    }

    #[must_use]
    pub const fn highpass_hz(self) -> f32 {
        self.highpass_hz
    }

    #[must_use]
    pub const fn lowpass_hz(self) -> f32 {
        self.lowpass_hz
    }
}

impl TryFrom<SfxFilterBandInput> for SfxFilterBand {
    type Error = Error;

    fn try_from(value: SfxFilterBandInput) -> Result<Self> {
        Self::new(value.highpass_hz, value.lowpass_hz)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SfxLayerState {
    #[default]
    Audible,
    Muted,
}

/// Layer overrides keep muting independent of the stored level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SfxLayerEdits {
    pub state: SfxLayerState,
    pub level: Option<Unit>,
    pub filter: Option<SfxFilterBand>,
}

impl SfxLayerEdits {
    pub const DEFAULT: Self = Self {
        state: SfxLayerState::Audible,
        level: None,
        filter: None,
    };
}

/// Tonal sequence clock rate, independent of the texture's grain clock.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct SfxStepRate(f32);

impl SfxStepRate {
    pub const MIN_HZ: f32 = 20.0;
    pub const MAX_HZ: f32 = 2_000.0;

    pub fn new(value: f32) -> Result<Self> {
        if value.is_finite() && (Self::MIN_HZ..=Self::MAX_HZ).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidParameter {
                name: "SFX tonal step rate",
                value: value as f64,
                expected: "a finite frequency in 20..=2000 Hz",
            })
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl TryFrom<f32> for SfxStepRate {
    type Error = Error;

    fn try_from(value: f32) -> Result<Self> {
        Self::new(value)
    }
}

impl From<SfxStepRate> for f32 {
    fn from(value: SfxStepRate) -> Self {
        value.get()
    }
}

/// A fixed rate retains the numeric JSON form; a sweep integrates a linear
/// rate envelope across each macro phrase part.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum SfxTonalRate {
    Fixed(SfxStepRate),
    Sweep {
        start: SfxStepRate,
        end: SfxStepRate,
    },
}

impl SfxTonalRate {
    #[must_use]
    pub const fn start(self) -> SfxStepRate {
        match self {
            Self::Fixed(rate) | Self::Sweep { start: rate, .. } => rate,
        }
    }

    #[must_use]
    pub const fn end(self) -> SfxStepRate {
        match self {
            Self::Fixed(rate) | Self::Sweep { end: rate, .. } => rate,
        }
    }

    pub(crate) fn at_time(self, time: f32, duration: f32) -> f32 {
        match self {
            Self::Fixed(rate) => rate.get(),
            Self::Sweep { start, end } => {
                start.get() + (end.get() - start.get()) * (time / duration)
            }
        }
    }

    pub(crate) fn cycles(self, time: f32, duration: f32) -> f64 {
        let time = f64::from(time);
        match self {
            Self::Fixed(rate) => f64::from(rate.get()) * time,
            Self::Sweep { start, end } => {
                (f64::from(start.get())
                    + 0.5 * f64::from(end.get() - start.get()) * (time / f64::from(duration)))
                    * time
            }
        }
    }
}

impl std::fmt::Display for SfxTonalRate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(rate) => write!(formatter, "{:.0} Hz fixed", rate.get()),
            Self::Sweep { start, end } => {
                write!(formatter, "{:.0}->{:.0} Hz linear", start.get(), end.get())
            }
        }
    }
}

/// FM modulation depth in multiples of the modulator frequency.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct SfxFmIndex(f32);

impl SfxFmIndex {
    pub const MIN: f32 = 0.0;
    pub const MAX: f32 = 8.0;

    pub fn new(value: f32) -> Result<Self> {
        if value.is_finite() && (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidParameter {
                name: "SFX FM index",
                value: value as f64,
                expected: "a finite modulation index in 0..=8",
            })
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl TryFrom<f32> for SfxFmIndex {
    type Error = Error;

    fn try_from(value: f32) -> Result<Self> {
        Self::new(value)
    }
}

impl From<SfxFmIndex> for f32 {
    fn from(value: SfxFmIndex) -> Self {
        value.get()
    }
}

/// Ratio of the FM modulator frequency to its carrier frequency.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct SfxFmRatio(f32);

impl SfxFmRatio {
    pub const MIN: f32 = 0.25;
    pub const MAX: f32 = 12.0;

    pub fn new(value: f32) -> Result<Self> {
        if value.is_finite() && (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidParameter {
                name: "SFX FM ratio",
                value: value as f64,
                expected: "a finite frequency ratio in 0.25..=12",
            })
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl TryFrom<f32> for SfxFmRatio {
    type Error = Error;

    fn try_from(value: f32) -> Result<Self> {
        Self::new(value)
    }
}

impl From<SfxFmRatio> for f32 {
    fn from(value: SfxFmRatio) -> Self {
        value.get()
    }
}

/// Positive envelope timing in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct SfxMilliseconds(f32);

impl SfxMilliseconds {
    pub const MIN: f32 = 0.01;
    pub const MAX: f32 = 20.0;

    pub fn new(value: f32) -> Result<Self> {
        if value.is_finite() && (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidParameter {
                name: "SFX envelope time",
                value: value as f64,
                expected: "a finite duration in 0.01..=20 milliseconds",
            })
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl TryFrom<f32> for SfxMilliseconds {
    type Error = Error;

    fn try_from(value: f32) -> Result<Self> {
        Self::new(value)
    }
}

impl From<SfxMilliseconds> for f32 {
    fn from(value: SfxMilliseconds) -> Self {
        value.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SfxTonalStep {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
}

impl SfxTonalStep {
    pub const ALL: [Self; 6] = [
        Self::One,
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
        Self::Six,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::One => "Step 1",
            Self::Two => "Step 2",
            Self::Three => "Step 3",
            Self::Four => "Step 4",
            Self::Five => "Step 5",
            Self::Six => "Step 6",
        }
    }

    pub(crate) fn at_clock(clock: f64) -> Self {
        match clock as usize % 6 {
            0 => Self::One,
            1 => Self::Two,
            2 => Self::Three,
            3 => Self::Four,
            4 => Self::Five,
            5 => Self::Six,
            _ => unreachable!("six-step clock stays within the sequence"),
        }
    }
}

/// Owned pitch ratios retain their slot names in serialized event edits.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SfxTonalSequence {
    one: SfxScale,
    two: SfxScale,
    three: SfxScale,
    four: SfxScale,
    five: SfxScale,
    six: SfxScale,
}

impl SfxTonalSequence {
    #[must_use]
    pub const fn new(ratios: [SfxScale; 6]) -> Self {
        let [one, two, three, four, five, six] = ratios;
        Self {
            one,
            two,
            three,
            four,
            five,
            six,
        }
    }

    #[must_use]
    pub const fn get(self, step: SfxTonalStep) -> SfxScale {
        match step {
            SfxTonalStep::One => self.one,
            SfxTonalStep::Two => self.two,
            SfxTonalStep::Three => self.three,
            SfxTonalStep::Four => self.four,
            SfxTonalStep::Five => self.five,
            SfxTonalStep::Six => self.six,
        }
    }

    pub fn set(&mut self, step: SfxTonalStep, ratio: SfxScale) {
        *match step {
            SfxTonalStep::One => &mut self.one,
            SfxTonalStep::Two => &mut self.two,
            SfxTonalStep::Three => &mut self.three,
            SfxTonalStep::Four => &mut self.four,
            SfxTonalStep::Five => &mut self.five,
            SfxTonalStep::Six => &mut self.six,
        } = ratio;
    }
}

impl Default for SfxTonalSequence {
    fn default() -> Self {
        Self::new([SfxScale::ONE; 6])
    }
}

/// Controls for the tonal FM generator and its six-step pitch sequence.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SfxTonalEdits {
    pub carrier: SfxScale,
    pub fm_index: SfxFmIndex,
    pub fm_ratio: SfxFmRatio,
    pub step_rate: Option<SfxTonalRate>,
    pub attack: SfxMilliseconds,
    pub release: SfxMilliseconds,
    pub pitch_ratios: Option<SfxTonalSequence>,
}

impl SfxTonalEdits {
    pub const DEFAULT: Self = Self {
        carrier: SfxScale(0.52),
        fm_index: SfxFmIndex(1.6),
        fm_ratio: SfxFmRatio(4.7),
        step_rate: None,
        attack: SfxMilliseconds(0.12),
        release: SfxMilliseconds(0.35),
        pitch_ratios: None,
    };
}

impl Default for SfxTonalEdits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Inline edits travel with an event through the existing audio command queue.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SfxEdits {
    pub pitch: SfxScale,
    pub duration: SfxScale,
    pub tonal: SfxTonalEdits,
    layers: SfxLayers,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SfxLayers {
    body: SfxLayerEdits,
    texture: SfxLayerEdits,
    tonal: SfxLayerEdits,
    flecks: SfxLayerEdits,
    shimmer: SfxLayerEdits,
    crisp: SfxLayerEdits,
}

impl SfxEdits {
    pub const DEFAULT: Self = Self {
        pitch: SfxScale::ONE,
        duration: SfxScale::ONE,
        tonal: SfxTonalEdits::DEFAULT,
        layers: SfxLayers {
            body: SfxLayerEdits::DEFAULT,
            texture: SfxLayerEdits::DEFAULT,
            tonal: SfxLayerEdits::DEFAULT,
            flecks: SfxLayerEdits::DEFAULT,
            shimmer: SfxLayerEdits::DEFAULT,
            crisp: SfxLayerEdits::DEFAULT,
        },
    };

    #[must_use]
    pub const fn layer(self, layer: SfxLayer) -> SfxLayerEdits {
        match layer {
            SfxLayer::Body => self.layers.body,
            SfxLayer::Texture => self.layers.texture,
            SfxLayer::Tonal => self.layers.tonal,
            SfxLayer::Flecks => self.layers.flecks,
            SfxLayer::Shimmer => self.layers.shimmer,
            SfxLayer::Crisp => self.layers.crisp,
        }
    }

    pub fn set_layer(&mut self, layer: SfxLayer, edits: SfxLayerEdits) {
        *match layer {
            SfxLayer::Body => &mut self.layers.body,
            SfxLayer::Texture => &mut self.layers.texture,
            SfxLayer::Tonal => &mut self.layers.tonal,
            SfxLayer::Flecks => &mut self.layers.flecks,
            SfxLayer::Shimmer => &mut self.layers.shimmer,
            SfxLayer::Crisp => &mut self.layers.crisp,
        } = edits;
    }
}

impl Default for SfxEdits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Components rendered for an event. Isolating a component preserves its
/// original synthesis choices so it can be compared with the full sound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "layer", rename_all = "kebab-case")]
pub enum SfxLayerSelection {
    #[default]
    All,
    Only(SfxLayer),
}

impl SfxLayerSelection {
    #[must_use]
    pub fn contains(self, layer: SfxLayer) -> bool {
        matches!(self, Self::All) || self == Self::Only(layer)
    }
}

impl SfxEvent {
    #[must_use]
    pub const fn new(kind: SfxKind) -> Self {
        Self {
            kind,
            pan: Pan::CENTER,
            intensity: Unit(0.6),
            layers: SfxLayerSelection::All,
            edits: SfxEdits::DEFAULT,
        }
    }
}

/// Exact random choices for the most recently admitted procedural voice.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeneratedEvent {
    pub kind: SfxKind,
    pub pan: Pan,
    pub intensity: Unit,
    pub seed: u64,
    pub pitch_hz: f32,
    pub duration_seconds: f32,
    pub cutoff_hz: f32,
    pub gain: f32,
    pub layers: SfxLayerSelection,
    #[serde(default)]
    pub edits: SfxEdits,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Command {
    Symbol(Symbol),
    Harmony(HarmonyMode),
    Market(MarketContext),
    Mode(Mode),
    Mix(Mix),
    Reverb(ReverbSettings),
    Spatial(SpatialMode),
    Trigger(SfxEvent),
    DataTransmission(DataTransmissionCommand),
}

/// Maximum number of short pulses between the opening and closing signatures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct TransmissionCount(u16);

impl TransmissionCount {
    pub const MAX: u16 = 256;

    pub fn new(value: u16) -> Result<Self> {
        if (1..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidParameter {
                name: "transmission trigger count",
                value: f64::from(value),
                expected: "1..=256 triggers",
            })
        }
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl Default for TransmissionCount {
    fn default() -> Self {
        Self(24)
    }
}

impl TryFrom<u16> for TransmissionCount {
    type Error = Error;

    fn try_from(value: u16) -> Result<Self> {
        Self::new(value)
    }
}

impl From<TransmissionCount> for u16 {
    fn from(value: TransmissionCount) -> Self {
        value.get()
    }
}

/// Nominal pulse rate for the transmission's fixed-rate sections.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct TransmissionRate(f32);

impl TransmissionRate {
    pub const MIN_HZ: f32 = 2.0;
    pub const MAX_HZ: f32 = 80.0;

    pub fn new(value: f32) -> Result<Self> {
        if value.is_finite() && (Self::MIN_HZ..=Self::MAX_HZ).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidParameter {
                name: "transmission rate",
                value: f64::from(value),
                expected: "a finite rate in 2..=80 Hz",
            })
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Default for TransmissionRate {
    fn default() -> Self {
        Self(10.0)
    }
}

impl TryFrom<f32> for TransmissionRate {
    type Error = Error;

    fn try_from(value: f32) -> Result<Self> {
        Self::new(value)
    }
}

impl From<TransmissionRate> for f32 {
    fn from(value: TransmissionRate) -> Self {
        value.get()
    }
}

/// One externally delimited transmission, with a bounded pulse budget.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DataTransmission {
    pub pan: Pan,
    pub intensity: Unit,
    pub max_triggers: TransmissionCount,
    pub rate: TransmissionRate,
}

impl Default for DataTransmission {
    fn default() -> Self {
        Self {
            pan: Pan::CENTER,
            intensity: Unit(0.5),
            max_triggers: TransmissionCount::default(),
            rate: TransmissionRate::default(),
        }
    }
}

/// Start replaces the current sequence; End closes it once and retains FX tails.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum DataTransmissionCommand {
    Start(DataTransmission),
    Pan(Pan),
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransmissionPhase {
    Idle,
    Opening,
    Repeating,
    AwaitingEnd,
    Closing,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct TransmissionSnapshot {
    pub phase: TransmissionPhase,
    pub repeated: u16,
    pub max_triggers: TransmissionCount,
    pub pan: Pan,
    pub rate_hz: f32,
    pub active_voices: usize,
    pub generated_events: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct RenderConfig {
    pub symbol: Symbol,
    pub harmony_mode: HarmonyMode,
    pub sample_rate: u32,
    pub seed: u64,
    pub max_voices: usize,
    pub mix: Mix,
    pub reverb: ReverbSettings,
    pub spatial: SpatialMode,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            symbol: Symbol::default(),
            harmony_mode: HarmonyMode::default(),
            sample_rate: 48_000,
            seed: 0x5348_495a_4558,
            max_voices: 32,
            mix: Mix::default(),
            reverb: ReverbSettings::default(),
            spatial: SpatialMode::Haas,
        }
    }
}

impl RenderConfig {
    pub(crate) fn validate(self) -> Result<Self> {
        for (name, value, valid, expected) in [
            (
                "sample rate",
                self.sample_rate as f64,
                (8_000..=192_000).contains(&self.sample_rate),
                "8000..=192000 Hz",
            ),
            (
                "max voices",
                self.max_voices as f64,
                (1..=128).contains(&self.max_voices),
                "1..=128",
            ),
        ] {
            if !valid {
                return Err(Error::InvalidParameter {
                    name,
                    value,
                    expected,
                });
            }
        }
        Ok(self)
    }
}
