use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

use crate::{ClockState, Error, MarketContext, Result, Symbol, Trend};

pub const GLOBAL_SCALE: &str = "A natural minor";
const SCALE: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];
// G4 leaves room for unison detuning and half-semitone drift below 420 Hz.
const VOICING_CEILING: u8 = 67;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Display,
    EnumString,
    EnumIter,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum HarmonyMode {
    Diatonic,
    #[default]
    Dissonant,
}

impl HarmonyMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Diatonic => GLOBAL_SCALE,
            Self::Dissonant => "Ticker chromatic",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PitchClass {
    C,
    #[serde(rename = "C#")]
    CSharp,
    D,
    #[serde(rename = "D#")]
    DSharp,
    E,
    F,
    #[serde(rename = "F#")]
    FSharp,
    G,
    #[serde(rename = "G#")]
    GSharp,
    A,
    #[serde(rename = "A#")]
    ASharp,
    B,
}

impl PitchClass {
    pub const ALL: [Self; 12] = [
        Self::C,
        Self::CSharp,
        Self::D,
        Self::DSharp,
        Self::E,
        Self::F,
        Self::FSharp,
        Self::G,
        Self::GSharp,
        Self::A,
        Self::ASharp,
        Self::B,
    ];

    #[must_use]
    pub const fn semitones(self) -> u8 {
        match self {
            Self::C => 0,
            Self::CSharp => 1,
            Self::D => 2,
            Self::DSharp => 3,
            Self::E => 4,
            Self::F => 5,
            Self::FSharp => 6,
            Self::G => 7,
            Self::GSharp => 8,
            Self::A => 9,
            Self::ASharp => 10,
            Self::B => 11,
        }
    }

    #[must_use]
    pub const fn note_name(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::CSharp => "C#",
            Self::D => "D",
            Self::DSharp => "D#",
            Self::E => "E",
            Self::F => "F",
            Self::FSharp => "F#",
            Self::G => "G",
            Self::GSharp => "G#",
            Self::A => "A",
            Self::ASharp => "A#",
            Self::B => "B",
        }
    }

    #[must_use]
    pub const fn pedal_note(self) -> u8 {
        33 + (self.semitones() + 3) % 12
    }
}

impl std::fmt::Display for PitchClass {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.note_name())
    }
}

/// One of the seven roots in the shared A-natural-minor pitch collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaleDegree {
    I,
    II,
    III,
    IV,
    V,
    VI,
    VII,
}

impl ScaleDegree {
    pub const ALL: [Self; 7] = [
        Self::I,
        Self::II,
        Self::III,
        Self::IV,
        Self::V,
        Self::VI,
        Self::VII,
    ];

    #[must_use]
    pub const fn note_name(self) -> &'static str {
        self.pitch_class().note_name()
    }

    #[must_use]
    pub const fn pitch_class(self) -> PitchClass {
        match self {
            Self::I => PitchClass::A,
            Self::II => PitchClass::B,
            Self::III => PitchClass::C,
            Self::IV => PitchClass::D,
            Self::V => PitchClass::E,
            Self::VI => PitchClass::F,
            Self::VII => PitchClass::G,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::I => 0,
            Self::II => 1,
            Self::III => 2,
            Self::IV => 3,
            Self::V => 4,
            Self::VI => 5,
            Self::VII => 6,
        }
    }

    fn shifted(self, steps: usize) -> Self {
        Self::ALL[(self.index() + steps) % 7]
    }

    fn minor_upper_root(self) -> Self {
        match self {
            Self::I | Self::II | Self::VII => Self::I,
            Self::III | Self::IV => Self::IV,
            Self::V | Self::VI => Self::V,
        }
    }

    #[must_use]
    pub fn pedal_note(self) -> u8 {
        self.note(0) - 12
    }

    fn note(self, steps: usize) -> u8 {
        let degree = self.index() + steps;
        45 + (degree / SCALE.len()) as u8 * 12 + SCALE[degree % SCALE.len()]
    }
}

impl std::fmt::Display for ScaleDegree {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.note_name())
    }
}

/// Diatonic voicings, including minor upper chords over the selected pedal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChordShape {
    Sus2,
    Add9,
    Seventh,
    Subdominant,
    Submediant,
    Sus4,
    Tension,
    /// A, D, or E minor upper chord, retaining its third and seventh.
    MinorNinth,
    /// Complementary minor upper chord with an added eleventh.
    MinorEleventh,
    /// B half-diminished over the selected pedal.
    HalfDiminished,
}

impl ChordShape {
    pub const ALL: [Self; 10] = [
        Self::Sus2,
        Self::Add9,
        Self::Seventh,
        Self::Subdominant,
        Self::Submediant,
        Self::Sus4,
        Self::Tension,
        Self::MinorNinth,
        Self::MinorEleventh,
        Self::HalfDiminished,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiatonicChord {
    pub root: ScaleDegree,
    pub shape: ChordShape,
}

impl DiatonicChord {
    #[must_use]
    pub const fn new(root: ScaleDegree, shape: ChordShape) -> Self {
        Self { root, shape }
    }

    /// Sorted, distinct MIDI notes voiced below 420 Hz over the bass pedal.
    #[must_use]
    pub fn notes(self) -> [u8; 4] {
        let mut notes = self.tones();
        for index in 0..notes.len() {
            while notes[index] > VOICING_CEILING {
                notes[index] -= 12;
            }
            while notes[..index].contains(&notes[index]) {
                notes[index] -= 12;
            }
        }
        notes.sort_unstable();

        // Preserve open spacing when octave folding creates an upper semitone.
        for index in (1..notes.len()).rev() {
            if notes[index] - notes[index - 1] == 1 {
                let lowered = notes[index] - 12;
                if lowered > self.root.pedal_note()
                    && notes.iter().all(|note| note.abs_diff(lowered) > 1)
                {
                    notes[index] = lowered;
                }
            }
        }
        notes.sort_unstable();
        notes
    }

    /// Chord tones in their open register and harmonic role order.
    #[must_use]
    fn tones(self) -> [u8; 4] {
        let offsets = match self.shape {
            ChordShape::Sus2 => [0, 4, 8, 11],
            ChordShape::Add9 => [0, 4, 9, 15],
            ChordShape::Seventh => [0, 6, 9, 11],
            ChordShape::Subdominant => [0, 5, 10, 11],
            ChordShape::Submediant => [0, 5, 9, 11],
            ChordShape::Sus4 => [0, 4, 10, 11],
            // At A this is A/F/B/E: the F-B color belongs to the shared scale.
            ChordShape::Tension => [0, 5, 8, 11],
            ChordShape::MinorNinth => [0, 6, 9, 15],
            ChordShape::MinorEleventh => [0, 6, 9, 17],
            ChordShape::HalfDiminished => [0, 4, 6, 9],
        };
        offsets.map(|offset| self.upper_root().note(offset))
    }

    fn upper_root(self) -> ScaleDegree {
        match self.shape {
            ChordShape::MinorNinth => self.root.minor_upper_root(),
            ChordShape::MinorEleventh => match self.root {
                ScaleDegree::I | ScaleDegree::III | ScaleDegree::VI | ScaleDegree::VII => {
                    ScaleDegree::IV
                }
                ScaleDegree::II => ScaleDegree::V,
                ScaleDegree::IV | ScaleDegree::V => ScaleDegree::I,
            },
            ChordShape::HalfDiminished => ScaleDegree::II,
            _ => self.root,
        }
    }

    fn write_upper(
        self,
        formatter: &mut std::fmt::Formatter<'_>,
        quality: &str,
    ) -> std::fmt::Result {
        let upper = self.upper_root();
        write!(formatter, "{upper}{quality}")?;
        if upper != self.root {
            write!(formatter, "/{}", self.root)?;
        }
        Ok(())
    }
}

fn triad_quality(root: ScaleDegree) -> &'static str {
    let third = root.note(2) - root.note(0);
    let fifth = root.note(4) - root.note(0);
    match (third, fifth) {
        (3, 6) => "dim",
        (3, 7) => "m",
        (4, 7) => "",
        _ => unreachable!("natural-minor triads have known qualities"),
    }
}

fn seventh_quality(root: ScaleDegree) -> &'static str {
    match (triad_quality(root), root.note(6) - root.note(0)) {
        ("dim", 10) => "m7b5",
        ("m", 10) => "m7",
        ("", 11) => "maj7",
        ("", 10) => "7",
        _ => unreachable!("natural-minor seventh chords have known qualities"),
    }
}

fn write_ninth(formatter: &mut std::fmt::Formatter<'_>, root: ScaleDegree) -> std::fmt::Result {
    let ninth = if root.note(1) - root.note(0) == 1 {
        "addb9"
    } else {
        "add9"
    };
    write!(
        formatter,
        "{}{}({ninth})",
        root.note_name(),
        triad_quality(root)
    )
}

impl std::fmt::Display for DiatonicChord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.shape {
            ChordShape::Add9 => write_ninth(formatter, self.root),
            ChordShape::Seventh => write!(formatter, "{}{}", self.root, seventh_quality(self.root)),
            ChordShape::Subdominant => {
                write_ninth(formatter, self.root.shifted(3))?;
                write!(formatter, "/{}", self.root)
            }
            ChordShape::Submediant => {
                let upper = self.root.shifted(5);
                write!(
                    formatter,
                    "{}{}/{}",
                    upper,
                    seventh_quality(upper),
                    self.root
                )
            }
            ChordShape::Sus2 | ChordShape::Sus4 => {
                let interval = if self.shape == ChordShape::Sus2 {
                    if self.root.note(1) - self.root.note(0) == 1 {
                        "b2"
                    } else {
                        "2"
                    }
                } else if self.root.note(3) - self.root.note(0) == 6 {
                    "#4"
                } else {
                    "4"
                };
                let fifth = if self.root.note(4) - self.root.note(0) == 6 {
                    "(b5)"
                } else {
                    ""
                };
                write!(formatter, "{}sus{interval}{fifth}", self.root)
            }
            ChordShape::Tension => write!(formatter, "{}(color)", self.root),
            ChordShape::MinorNinth => self.write_upper(
                formatter,
                if self.upper_root() == ScaleDegree::V {
                    "m7(b9,no5)"
                } else {
                    "m9(no5)"
                },
            ),
            ChordShape::MinorEleventh => self.write_upper(formatter, "m7(add11,no5)"),
            ChordShape::HalfDiminished => self.write_upper(formatter, "m7b5"),
        }
    }
}

/// Four distinct pitch classes; the first selects the independent bass pedal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ChromaticInput", into = "ChromaticInput")]
pub struct ChromaticVoicing([PitchClass; 4]);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChromaticInput {
    chromatic: [PitchClass; 4],
}

impl TryFrom<ChromaticInput> for ChromaticVoicing {
    type Error = Error;

    fn try_from(input: ChromaticInput) -> Result<Self> {
        Self::new(input.chromatic)
    }
}

impl From<ChromaticVoicing> for ChromaticInput {
    fn from(voicing: ChromaticVoicing) -> Self {
        Self {
            chromatic: voicing.0,
        }
    }
}

impl ChromaticVoicing {
    /// Preserve the first pitch as the pedal and sort the other distinct pitches
    /// so equivalent voicings have the same identity and serialization.
    pub fn new(mut pitches: [PitchClass; 4]) -> Result<Self> {
        for (index, pitch) in pitches.iter().enumerate() {
            if pitches[..index].contains(pitch) {
                return Err(Error::InvalidChromaticChord(
                    "expected four distinct pitch classes",
                ));
            }
        }
        pitches[1..].sort_unstable();
        Ok(Self(pitches))
    }

    fn from_symbol(symbol: Symbol) -> Self {
        let mut pool = PitchClass::ALL;
        let mut state = symbol.ascii_hash();
        // Four partial Fisher-Yates swaps consume the whole normalized ticker
        // hash, with a fixed integer mixer independent of runtime randomness.
        for round in 0..4 {
            state = state
                .rotate_left(17)
                .wrapping_mul(0x0000_0100_0000_01b3)
                .wrapping_add(round as u64 + 1);
            let selected = round + (state % (pool.len() - round) as u64) as usize;
            pool.swap(round, selected);
        }
        Self::new(std::array::from_fn(|i| pool[i]))
            .expect("partial shuffle produces four distinct pitch classes")
    }

    fn root(self) -> PitchClass {
        self.0[0]
    }

    fn notes(self) -> [u8; 4] {
        let mut notes = self.0.map(|pitch| 48 + pitch.semitones());
        notes.sort_unstable();
        notes
    }
}

impl std::fmt::Display for ChromaticVoicing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut classes = self.0;
        classes.sort_unstable();
        write!(
            formatter,
            "[{},{},{},{}]/{}",
            classes[0],
            classes[1],
            classes[2],
            classes[3],
            self.root()
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Chord {
    Diatonic(DiatonicChord),
    Chromatic(ChromaticVoicing),
}

impl Chord {
    #[must_use]
    pub const fn new(root: ScaleDegree, shape: ChordShape) -> Self {
        Self::Diatonic(DiatonicChord::new(root, shape))
    }

    #[must_use]
    pub fn from_symbol(symbol: Symbol, mode: HarmonyMode) -> Self {
        match mode {
            HarmonyMode::Diatonic => Self::new(symbol.root(), ChordShape::MinorNinth),
            HarmonyMode::Dissonant => Self::Chromatic(ChromaticVoicing::from_symbol(symbol)),
        }
    }

    #[must_use]
    pub fn root(self) -> PitchClass {
        match self {
            Self::Diatonic(chord) => chord.root.pitch_class(),
            Self::Chromatic(voicing) => voicing.root(),
        }
    }

    #[must_use]
    pub fn pedal_note(self) -> u8 {
        self.root().pedal_note()
    }

    #[must_use]
    pub fn notes(self) -> [u8; 4] {
        match self {
            Self::Diatonic(chord) => chord.notes(),
            Self::Chromatic(voicing) => voicing.notes(),
        }
    }

    #[must_use]
    pub const fn mode(self) -> HarmonyMode {
        match self {
            Self::Diatonic(_) => HarmonyMode::Diatonic,
            Self::Chromatic(_) => HarmonyMode::Dissonant,
        }
    }
}

impl std::fmt::Display for Chord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diatonic(chord) => std::fmt::Display::fmt(chord, formatter),
            Self::Chromatic(voicing) => std::fmt::Display::fmt(voicing, formatter),
        }
    }
}

pub(crate) enum Harmony {
    Diatonic {
        chord: DiatonicChord,
        next_change: u64,
        step: usize,
    },
    Dissonant(ChromaticVoicing),
}

impl Harmony {
    pub fn new(rate: u32, chord: Chord) -> Self {
        match chord {
            Chord::Diatonic(chord) => Self::Diatonic {
                chord,
                next_change: u64::from(rate) * 10,
                step: 0,
            },
            Chord::Chromatic(voicing) => Self::Dissonant(voicing),
        }
    }

    pub fn chord(&self) -> Chord {
        match *self {
            Self::Diatonic { chord, .. } => Chord::Diatonic(chord),
            Self::Dissonant(voicing) => Chord::Chromatic(voicing),
        }
    }

    pub fn set_symbol(&mut self, symbol: Symbol) -> bool {
        match self {
            Self::Diatonic { chord, .. } => {
                let root = symbol.root();
                let changed = chord.root != root;
                chord.root = root;
                changed
            }
            Self::Dissonant(voicing) => {
                let next = ChromaticVoicing::from_symbol(symbol);
                let changed = *voicing != next;
                *voicing = next;
                changed
            }
        }
    }

    pub fn set_mode(&mut self, mode: HarmonyMode, symbol: Symbol, rate: u32, sample: u64) -> bool {
        if self.chord().mode() == mode {
            return false;
        }
        *self = match mode {
            HarmonyMode::Diatonic => Self::Diatonic {
                chord: DiatonicChord::new(symbol.root(), ChordShape::MinorNinth),
                next_change: sample + u64::from(rate) * 10,
                step: 0,
            },
            HarmonyMode::Dissonant => Self::Dissonant(ChromaticVoicing::from_symbol(symbol)),
        };
        true
    }

    pub fn update(&mut self, sample: u64, rate: u32, market: MarketContext, focused: bool) -> bool {
        let Self::Diatonic {
            chord,
            next_change,
            step,
        } = self
        else {
            return false;
        };
        if sample < *next_change {
            return false;
        }
        *next_change = sample + (rate as f32 * (15.0 - 5.0 * market.activity.get())) as u64;
        if focused {
            return false;
        }
        let next = next_diatonic_chord(*chord, *step, market);
        *step += 1;
        let changed = next != *chord;
        *chord = next;
        changed
    }

    pub fn next_choice(&self, market: MarketContext) -> Chord {
        match *self {
            Self::Diatonic { chord, step, .. } => {
                Chord::Diatonic(next_diatonic_chord(chord, step, market))
            }
            Self::Dissonant(voicing) => Chord::Chromatic(voicing),
        }
    }

    pub fn clock(&self, sample: u64, rate: u32, focused: bool) -> ClockState {
        match *self {
            Self::Diatonic { next_change, .. } => {
                ClockState::at(next_change, sample, rate as f32, focused)
            }
            Self::Dissonant(_) => ClockState::Fixed,
        }
    }
}

fn next_diatonic_chord(chord: DiatonicChord, step: usize, market: MarketContext) -> DiatonicChord {
    use ChordShape::*;
    let progression = match market.trend {
        Trend::Bullish if market.trend_strength.get() > 0.3 => {
            [MinorEleventh, MinorNinth, HalfDiminished, MinorNinth]
        }
        Trend::Bearish if market.trend_strength.get() > 0.3 => {
            [MinorEleventh, HalfDiminished, MinorNinth, HalfDiminished]
        }
        _ => [MinorNinth, MinorEleventh, MinorNinth, MinorEleventh],
    };
    let mut next = progression[step % progression.len()];
    if market.volatility.get() > 0.8 && step % 3 == 2 {
        next = HalfDiminished;
    }
    // B half-diminished over G completes a dominant ninth.
    if next == HalfDiminished && (market.volume.get() < 0.15 || chord.root == ScaleDegree::VII) {
        next = MinorEleventh;
    }
    DiatonicChord::new(chord.root, next)
}

pub(crate) fn midi_hz(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Unit;

    #[test]
    fn ticker_chromatic_mapping_is_stable_uses_punctuation_and_reaches_every_root() {
        let examples = [
            (
                "AAPL",
                PitchClass::CSharp,
                [49, 52, 53, 54],
                "[C#,E,F,F#]/C#",
            ),
            ("TSLA", PitchClass::D, [48, 50, 55, 59], "[C,D,G,B]/D"),
            (
                "IBM",
                PitchClass::CSharp,
                [49, 55, 58, 59],
                "[C#,G,A#,B]/C#",
            ),
            (
                "BRK.B",
                PitchClass::ASharp,
                [49, 54, 57, 58],
                "[C#,F#,A,A#]/A#",
            ),
        ];
        for (ticker, root, notes, display) in examples {
            let chord = Chord::from_symbol(Symbol::new(ticker).unwrap(), HarmonyMode::Dissonant);
            assert_eq!(chord.root(), root);
            assert_eq!(chord.pedal_note(), root.pedal_note());
            assert_eq!(chord.notes(), notes);
            assert_eq!(chord.to_string(), display);
            assert_eq!(chord.mode(), HarmonyMode::Dissonant);
        }
        let mapped =
            |ticker: &str| Chord::from_symbol(Symbol::new(ticker).unwrap(), HarmonyMode::Dissonant);
        assert_eq!(mapped("aapl"), mapped("AAPL"));
        assert_ne!(mapped("BRK.B"), mapped("BRKB"));
        assert_ne!(mapped("^GSPC"), mapped("GSPC"));

        let mut roots = std::collections::BTreeSet::new();
        for ticker in 0..1024 {
            let chord = mapped(&format!("T{ticker:04}"));
            roots.insert(chord.root());
            let notes = chord.notes();
            assert!(notes.windows(2).all(|pair| pair[0] < pair[1]));
            assert!(notes.iter().all(|note| (48..=59).contains(note)));
            assert!(chord.pedal_note() < notes[0]);
        }
        assert_eq!(roots.into_iter().collect::<Vec<_>>(), PitchClass::ALL);
        for pitch in PitchClass::ALL {
            assert!((33..=44).contains(&pitch.pedal_note()));
            assert_eq!(pitch.pedal_note() % 12, pitch.semitones());
        }
        for root in ScaleDegree::ALL {
            assert_eq!(root.pedal_note(), root.pitch_class().pedal_note());
        }
    }

    #[test]
    fn chord_serialization_preserves_diatonic_shape_and_validates_chromatic_input() {
        let diatonic = Chord::new(ScaleDegree::III, ChordShape::MinorNinth);
        let old_json = r#"{"root":"III","shape":"MinorNinth"}"#;
        assert_eq!(serde_json::to_string(&diatonic).unwrap(), old_json);
        assert_eq!(serde_json::from_str::<Chord>(old_json).unwrap(), diatonic);
        let chromatic = Chord::from_symbol(Symbol::new("AAPL").unwrap(), HarmonyMode::Dissonant);
        let json = r#"{"chromatic":["C#","E","F","F#"]}"#;
        assert_eq!(serde_json::to_string(&chromatic).unwrap(), json);
        assert_eq!(serde_json::from_str::<Chord>(json).unwrap(), chromatic);
        let permuted = r#"{"chromatic":["C#","F#","F","E"]}"#;
        let normalized = serde_json::from_str::<Chord>(permuted).unwrap();
        assert_eq!(normalized, chromatic);
        assert_eq!(serde_json::to_string(&normalized).unwrap(), json);
        let constructed = ChromaticVoicing::new([
            PitchClass::CSharp,
            PitchClass::FSharp,
            PitchClass::F,
            PitchClass::E,
        ])
        .unwrap();
        assert_eq!(Chord::Chromatic(constructed), chromatic);
        let other_pedal = ChromaticVoicing::new([
            PitchClass::E,
            PitchClass::CSharp,
            PitchClass::FSharp,
            PitchClass::F,
        ])
        .unwrap();
        assert_ne!(constructed, other_pedal);
        assert_eq!(constructed.root(), PitchClass::CSharp);
        assert_eq!(constructed.notes(), other_pedal.notes());
        assert!(matches!(
            ChromaticVoicing::new([PitchClass::C; 4]),
            Err(Error::InvalidChromaticChord(_))
        ));
        for invalid in [
            r#"{"chromatic":["C","C","D","E"]}"#,
            r#"{"chromatic":["C","D","E"]}"#,
            r#"{"chromatic":["C","D","E","F","G"]}"#,
            r#"{"chromatic":["C","D","E","H"]}"#,
            r#"{"chromatic":["C","D","E","F"],"root":"I"}"#,
            r#"{"root":"I","shape":"MinorNinth","extra":0}"#,
        ] {
            assert!(serde_json::from_str::<Chord>(invalid).is_err(), "{invalid}");
        }
        for pitch in PitchClass::ALL {
            let serialized = serde_json::to_string(&pitch).unwrap();
            assert_eq!(serialized, format!("\"{}\"", pitch.note_name()));
            assert_eq!(
                serde_json::from_str::<PitchClass>(&serialized).unwrap(),
                pitch
            );
        }
        assert_eq!(HarmonyMode::default(), HarmonyMode::Dissonant);
        assert_eq!(
            serde_json::to_string(&HarmonyMode::Dissonant).unwrap(),
            "\"dissonant\""
        );
        assert_eq!(
            "diatonic".parse::<HarmonyMode>().unwrap(),
            HarmonyMode::Diatonic
        );
    }

    #[test]
    fn dissonant_harmony_is_fixed_and_switching_back_restarts_the_diatonic_clock() {
        let rate = 8_000;
        let symbol = Symbol::new("AAPL").unwrap();
        let chord = Chord::from_symbol(symbol, HarmonyMode::Dissonant);
        let mut harmony = Harmony::new(rate, chord);
        assert_eq!(harmony.clock(0, rate, false), ClockState::Fixed);
        for trend in [Trend::Bullish, Trend::Bearish, Trend::Sideways] {
            let market = MarketContext {
                trend,
                trend_strength: Unit::ONE,
                activity: Unit::ONE,
                volatility: Unit::ONE,
                volume: Unit::ZERO,
                ..MarketContext::default()
            };
            assert!(!harmony.update(1_000_000, rate, market, false));
            assert!(!harmony.update(2_000_000, rate, market, true));
            assert_eq!(harmony.chord(), chord);
            assert_eq!(harmony.next_choice(market), chord);
            assert_eq!(harmony.clock(2_000_000, rate, true), ClockState::Fixed);
        }
        assert!(!harmony.set_mode(HarmonyMode::Dissonant, symbol, rate, 2_000_000));
        let next_symbol = Symbol::new("TSLA").unwrap();
        assert!(harmony.set_symbol(next_symbol));
        assert!(!harmony.set_symbol(next_symbol));
        assert_eq!(
            harmony.chord(),
            Chord::from_symbol(next_symbol, HarmonyMode::Dissonant)
        );
        assert!(harmony.set_mode(HarmonyMode::Diatonic, next_symbol, rate, 2_000_000));
        assert_eq!(
            harmony.chord(),
            Chord::new(next_symbol.root(), ChordShape::MinorNinth)
        );
        assert_eq!(
            harmony.clock(2_000_000, rate, false),
            ClockState::Scheduled { in_seconds: 10.0 }
        );
        assert!(!harmony.set_mode(HarmonyMode::Diatonic, next_symbol, rate, 2_000_000));
        assert_eq!(
            harmony.clock(2_000_000, rate, false),
            ClockState::Scheduled { in_seconds: 10.0 }
        );
        let mut custom = Harmony::new(rate, Chord::new(symbol.root(), ChordShape::MinorEleventh));
        assert!(custom.set_symbol(next_symbol));
        assert_eq!(
            custom.chord(),
            Chord::new(next_symbol.root(), ChordShape::MinorEleventh)
        );
    }

    #[test]
    fn dark_upper_voicings_keep_their_actual_minor_quality_and_pedal_symbols() {
        let examples = [
            (
                ScaleDegree::I,
                ChordShape::MinorNinth,
                [45, 55, 60, 71],
                "Am9(no5)",
            ),
            (
                ScaleDegree::III,
                ChordShape::MinorNinth,
                [50, 60, 65, 76],
                "Dm9(no5)/C",
            ),
            (
                ScaleDegree::VI,
                ChordShape::MinorNinth,
                [52, 62, 67, 77],
                "Em7(b9,no5)/F",
            ),
            (
                ScaleDegree::I,
                ChordShape::MinorEleventh,
                [50, 60, 65, 79],
                "Dm7(add11,no5)/A",
            ),
            (
                ScaleDegree::VI,
                ChordShape::MinorEleventh,
                [50, 60, 65, 79],
                "Dm7(add11,no5)/F",
            ),
            (
                ScaleDegree::I,
                ChordShape::HalfDiminished,
                [47, 53, 57, 62],
                "Bm7b5/A",
            ),
        ];
        for (root, shape, notes, symbol) in examples {
            let chord = DiatonicChord::new(root, shape);
            assert_eq!(chord.tones(), notes);
            assert_eq!(chord.to_string(), symbol);
        }
    }

    #[test]
    fn octave_inversions_preserve_each_chord_tone_and_keep_extensions_open() {
        for root in ScaleDegree::ALL {
            for shape in ChordShape::ALL {
                let chord = DiatonicChord::new(root, shape);
                let mut original = chord.tones().map(|note| note % 12);
                let mut voiced = chord.notes().map(|note| note % 12);
                original.sort_unstable();
                voiced.sort_unstable();
                assert_eq!(voiced, original, "{chord}");
            }
        }
        assert_eq!(
            Chord::new(ScaleDegree::I, ChordShape::MinorNinth).notes(),
            [45, 48, 55, 59]
        );
    }

    #[test]
    fn generated_harmony_retains_minor_thirds_at_every_pedal_and_market_state() {
        for root in ScaleDegree::ALL {
            for trend in [Trend::Bullish, Trend::Bearish, Trend::Sideways] {
                for volume in [Unit::ZERO, Unit::ONE] {
                    for volatility in [Unit::ZERO, Unit::ONE] {
                        let market = MarketContext {
                            trend,
                            trend_strength: Unit::ONE,
                            volume,
                            volatility,
                            ..MarketContext::default()
                        };
                        let mut harmony =
                            Harmony::new(8_000, Chord::new(root, ChordShape::MinorNinth));
                        for _ in 0..8 {
                            let Chord::Diatonic(chord) = harmony.chord() else {
                                panic!("expected diatonic harmony")
                            };
                            assert_eq!(chord.root, root);
                            let notes = chord.notes();
                            assert!(chord.root.pedal_note() < notes[0]);
                            assert!(notes.windows(2).all(|pair| pair[0] < pair[1]));
                            let upper = chord.upper_root().note(0) % 12;
                            let mut intervals = notes.map(|note| (note % 12 + 12 - upper) % 12);
                            intervals.sort_unstable();
                            assert!(
                                matches!(intervals, [0, 1 | 2, 3, 10] | [0, 3, 5 | 6, 10]),
                                "{chord}: {notes:?}"
                            );
                            let pedal = chord.root.pedal_note() % 12;
                            let classes = notes.map(|note| note % 12);
                            assert!(
                                !(classes.contains(&((pedal + 4) % 12))
                                    && classes.contains(&((pedal + 7) % 12))),
                                "{chord} completes a major triad on its pedal"
                            );
                            let next_change = match &harmony {
                                Harmony::Diatonic { next_change, .. } => *next_change,
                                Harmony::Dissonant(_) => unreachable!(),
                            };
                            harmony.update(next_change, 8_000, market, false);
                        }
                    }
                }
            }
        }
    }
}
