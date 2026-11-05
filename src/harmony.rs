use serde::{Deserialize, Serialize};

use crate::{MarketContext, Trend};

pub const GLOBAL_SCALE: &str = "A natural minor";
const SCALE: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];
// G4 leaves room for unison detuning and half-semitone drift below 420 Hz.
const VOICING_CEILING: u8 = 67;

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
        match self {
            Self::I => "A",
            Self::II => "B",
            Self::III => "C",
            Self::IV => "D",
            Self::V => "E",
            Self::VI => "F",
            Self::VII => "G",
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
pub struct Chord {
    pub root: ScaleDegree,
    pub shape: ChordShape,
}

impl Chord {
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

impl std::fmt::Display for Chord {
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

pub(crate) struct Harmony {
    pub chord: Chord,
    pub next_change: u64,
    step: usize,
}

impl Harmony {
    pub fn new(rate: u32, root: ScaleDegree) -> Self {
        Self {
            chord: Chord::new(root, ChordShape::MinorNinth),
            next_change: rate as u64 * 10,
            step: 0,
        }
    }

    pub fn set_root(&mut self, root: ScaleDegree) -> bool {
        let changed = self.chord.root != root;
        self.chord.root = root;
        changed
    }

    pub fn update(&mut self, sample: u64, rate: u32, market: MarketContext, focused: bool) -> bool {
        if sample < self.next_change {
            return false;
        }
        self.next_change = sample + (rate as f32 * (15.0 - 5.0 * market.activity.get())) as u64;
        if focused {
            return false;
        }
        let next = self.next_choice(market);
        self.step += 1;
        let changed = next != self.chord;
        self.chord = next;
        changed
    }

    pub fn next_choice(&self, market: MarketContext) -> Chord {
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
        let mut next = progression[self.step % progression.len()];
        if market.volatility.get() > 0.8 && self.step % 3 == 2 {
            next = HalfDiminished;
        }
        // B half-diminished over G completes a dominant ninth.
        if next == HalfDiminished
            && (market.volume.get() < 0.15 || self.chord.root == ScaleDegree::VII)
        {
            next = MinorEleventh;
        }
        Chord::new(self.chord.root, next)
    }
}

pub(crate) fn midi_hz(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Unit;

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
            let chord = Chord::new(root, shape);
            assert_eq!(chord.tones(), notes);
            assert_eq!(chord.to_string(), symbol);
        }
    }

    #[test]
    fn octave_inversions_preserve_each_chord_tone_and_keep_extensions_open() {
        for root in ScaleDegree::ALL {
            for shape in ChordShape::ALL {
                let chord = Chord::new(root, shape);
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
                        let mut harmony = Harmony::new(8_000, root);
                        for _ in 0..8 {
                            let chord = harmony.chord;
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
                            harmony.update(harmony.next_change, 8_000, market, false);
                        }
                    }
                }
            }
        }
    }
}
