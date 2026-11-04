use serde::{Deserialize, Serialize};

use crate::{MarketContext, Trend};

pub const GLOBAL_SCALE: &str = "A natural minor";
const SCALE: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];

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

/// Diatonic voicing profiles; intervals follow the global scale at every root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChordShape {
    Sus2,
    Add9,
    Seventh,
    Subdominant,
    Submediant,
    Sus4,
    Tension,
}

impl ChordShape {
    pub const ALL: [Self; 7] = [
        Self::Sus2,
        Self::Add9,
        Self::Seventh,
        Self::Subdominant,
        Self::Submediant,
        Self::Sus4,
        Self::Tension,
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

    /// Upper voicing in MIDI notes. The bass pedal follows the same root.
    #[must_use]
    pub fn notes(self) -> [u8; 4] {
        let offsets = match self.shape {
            ChordShape::Sus2 => [0, 4, 8, 11],
            ChordShape::Add9 => [0, 4, 9, 15],
            ChordShape::Seventh => [0, 6, 9, 11],
            ChordShape::Subdominant => [0, 5, 10, 11],
            ChordShape::Submediant => [0, 5, 9, 11],
            ChordShape::Sus4 => [0, 4, 10, 11],
            // At A this is A/F/B/E: the F-B color belongs to the shared scale.
            ChordShape::Tension => [0, 5, 8, 11],
        };
        offsets.map(|offset| self.root.note(offset))
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
            chord: Chord::new(root, ChordShape::Sus2),
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
                [Add9, Submediant, Subdominant, Sus2]
            }
            Trend::Bearish if market.trend_strength.get() > 0.3 => {
                [Add9, Subdominant, Tension, Sus4]
            }
            _ => [Sus2, Add9, Sus4, Seventh],
        };
        let mut next = progression[self.step % progression.len()];
        if market.volatility.get() > 0.8 && self.step % 3 == 2 {
            next = Tension;
        }
        if market.volume.get() < 0.15 && next == Tension {
            next = Sus4;
        }
        Chord::new(self.chord.root, next)
    }
}

pub(crate) fn midi_hz(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}
