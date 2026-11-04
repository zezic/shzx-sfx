use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

use crate::{Chord, Error, Result, ScaleDegree, SfxLayer};

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

    #[must_use]
    pub fn root(self) -> ScaleDegree {
        let hash = self
            .0
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            });
        ScaleDegree::ALL[(hash % ScaleDegree::ALL.len() as u64) as usize]
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

/// The next musical decision is either scheduled or inhibited by focus.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum ClockState {
    Scheduled { in_seconds: f32 },
    Held { recheck_in_seconds: f32 },
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
    pub root: ScaleDegree,
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
        root: ScaleDegree,
    },
    Crossfading {
        from: ScaleDegree,
        to: ScaleDegree,
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
    pub texture: Unit,
    pub harmony: Unit,
    pub melody: Unit,
    pub ui: Unit,
    pub reverb: Unit,
}

impl Default for Mix {
    fn default() -> Self {
        Self {
            master: Unit(0.55),
            output_gain: OutputGain::default(),
            drone: Unit(0.65),
            texture: Unit(0.45),
            harmony: Unit(0.55),
            melody: Unit(0.28),
            ui: Unit(0.7),
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
        }
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Display, EnumString, EnumIter,
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
    Market(MarketContext),
    Mode(Mode),
    Mix(Mix),
    Reverb(ReverbSettings),
    Spatial(SpatialMode),
    Trigger(SfxEvent),
}

#[derive(Clone, Copy, Debug)]
pub struct RenderConfig {
    pub symbol: Symbol,
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
