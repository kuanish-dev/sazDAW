//! Chord parts: a chord progression played with a strum pattern on one instrument track (`ChordPartControl`), and
//! listening to it before building (`ChordPartPreview`). How the instrument plays depends on its kind: the strings
//! (`GUITAR_KIND`, `ELECTRIC_GUITAR_KIND`, `UKULELE_KIND`) strum chords voiced for their tuning, the bass (`BASS_GUITAR_KIND`) plays the roots
//! on the down strokes, any other instrument plays block chords.
//!
//! A strum pattern (`STRUMS`) is one 4/4 bar of eighth notes: `D` a down stroke, `U` an up stroke, `X` a muted down
//! stroke (a "chuck"), lower case softer, `.` nothing. Like the drum patterns it is not reshaped for another meter: in
//! 3/4 a pattern bar simply lasts longer than a bar of the song. Chords are symbols (`Chord::parse`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::codes::patterns;
use crate::{BAR_TICKS, ClipId, ContractError, L10n, MAX_CLIP_LOOP_TICKS, TrackId};

/// Strum steps in a bar (eighth notes).
pub const STRUM_STEPS: u32 = 8;
pub const STRUM_STEP_TICKS: u32 = BAR_TICKS / STRUM_STEPS;
/// The longest progression: it loops inside one clip.
pub const MAX_CHORD_BARS: u32 = MAX_CLIP_LOOP_TICKS / BAR_TICKS;

/// A strum pattern: the id for requests, the name for people and a grid of `STRUM_STEPS` symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Strum {
    pub id: &'static str,
    pub name: L10n,
    pub grid: &'static str,
}

/// The built-in strums, named by the strokes in a bar as in the Russian guitar tradition ("бой четвёрка, шестёрка,
/// восьмёрка").
pub const STRUMS: [Strum; 3] = [
    Strum { id: "four", name: L10n { en: "Four", ru: "Четвёрка" }, grid: "D.U.X.U." },
    Strum { id: "six", name: L10n { en: "Six", ru: "Шестёрка" }, grid: "D.DU.UDU" },
    Strum { id: "eight", name: L10n { en: "Eight", ru: "Восьмёрка" }, grid: "DUXUDUXU" },
];

/// The strum with this id; an unknown one is a refusal that lists the known ones.
pub fn strum(id: &str) -> Result<Strum, ContractError> {
    STRUMS.iter().copied().find(|strum| strum.id == id).ok_or_else(|| {
        let ids: Vec<&str> = STRUMS.iter().map(|strum| strum.id).collect();
        patterns::STRUM_NOT_FOUND.with("id", id).with("ids", ids.join(", "))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordQuality {
    Major,
    Minor,
    Seventh,
    MajorSeventh,
    MinorSeventh,
    Sus2,
    Sus4,
    Diminished,
    Augmented,
}

impl ChordQuality {
    pub const ALL: [ChordQuality; 9] = [
        ChordQuality::Major,
        ChordQuality::Minor,
        ChordQuality::Seventh,
        ChordQuality::MajorSeventh,
        ChordQuality::MinorSeventh,
        ChordQuality::Sus2,
        ChordQuality::Sus4,
        ChordQuality::Diminished,
        ChordQuality::Augmented,
    ];

    /// What follows the root in a symbol.
    pub fn suffix(self) -> &'static str {
        match self {
            ChordQuality::Major => "",
            ChordQuality::Minor => "m",
            ChordQuality::Seventh => "7",
            ChordQuality::MajorSeventh => "maj7",
            ChordQuality::MinorSeventh => "m7",
            ChordQuality::Sus2 => "sus2",
            ChordQuality::Sus4 => "sus4",
            ChordQuality::Diminished => "dim",
            ChordQuality::Augmented => "aug",
        }
    }

    /// Semitones above the root.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            ChordQuality::Major => &[0, 4, 7],
            ChordQuality::Minor => &[0, 3, 7],
            ChordQuality::Seventh => &[0, 4, 7, 10],
            ChordQuality::MajorSeventh => &[0, 4, 7, 11],
            ChordQuality::MinorSeventh => &[0, 3, 7, 10],
            ChordQuality::Sus2 => &[0, 2, 7],
            ChordQuality::Sus4 => &[0, 5, 7],
            ChordQuality::Diminished => &[0, 3, 6],
            ChordQuality::Augmented => &[0, 4, 8],
        }
    }
}

/// Root names for symbols, by pitch class: sharps or flats as they are usually written.
const ROOT_NAMES: [&str; 12] = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];

/// A chord: the root's pitch class (0 = C … 11 = B) and the quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub root: u8,
    pub quality: ChordQuality,
}

impl Chord {
    /// Reads a symbol: a root C…B with an optional `#` or `b`, then the quality's suffix ("C", "Am", "F#m7", "Bbmaj7").
    pub fn parse(symbol: &str) -> Result<Self, ContractError> {
        let bad = || patterns::BAD_CHORD.with("chord", symbol);
        let mut chars = symbol.trim().chars();
        let natural = match chars.next() {
            Some('C') => 0,
            Some('D') => 2,
            Some('E') => 4,
            Some('F') => 5,
            Some('G') => 7,
            Some('A') => 9,
            Some('B') => 11,
            _ => return Err(bad()),
        };
        let rest = chars.as_str();
        let (root, suffix) = match rest.chars().next() {
            Some('#') => (natural + 1, &rest[1..]),
            Some('b') => (natural + 11, &rest[1..]),
            _ => (natural, rest),
        };
        let quality = ChordQuality::ALL.into_iter().find(|quality| quality.suffix() == suffix).ok_or_else(bad)?;
        Ok(Self { root: root % 12, quality })
    }

    pub fn symbol(self) -> String {
        format!("{}{}", ROOT_NAMES[usize::from(self.root % 12)], self.quality.suffix())
    }

    /// The chord's pitch classes, the root first.
    pub fn pitch_classes(self) -> Vec<u8> {
        self.quality.intervals().iter().map(|interval| (self.root + interval) % 12).collect()
    }
}

/// Where a chord part plays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ChordTarget {
    /// An existing instrument track.
    Track { id: TrackId },
    /// A new track of this instrument kind ("guitar", "electric_guitar", "ukulele", "bass_guitar", "synth"…), named
    /// after the kind.
    New { kind: String },
}

fn once() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChordPartRequest {
    pub target: ChordTarget,
    /// The bar (from zero) where the part starts.
    #[serde(default)]
    pub start_bar: u32,
    /// Strum pattern id: "four", "six" or "eight" (`chords.strums`).
    pub strum: String,
    /// The progression, one entry per bar: one chord symbol, or two (the second starts in the middle of the bar).
    /// A symbol is a root C…B with an optional # or b, then "", "m", "7", "maj7", "m7", "sus2", "sus4", "dim" or "aug".
    pub bars: Vec<Vec<String>>,
    /// How many times the progression plays; the repeats loop inside one clip.
    #[serde(default = "once")]
    pub repeat: u32,
    /// Replace the track's clips where the part goes instead of refusing.
    #[serde(default)]
    pub replace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChordPartReport {
    pub track: TrackId,
    /// The new track's name when the target was `new`.
    pub created_track: Option<String>,
    pub clip: ClipId,
    /// Bars the part takes: the progression times its repeats.
    pub bars: u32,
}

/// "Turn a chord progression and a strum into a note clip". Everything is checked up front (the strum, every chord
/// symbol, one or two chords per bar, the limits, the track, occupied space) and the part is built in ONE undo step:
/// a new track if asked and one note clip whose content is the progression, looping for the repeats. Occupied space:
/// `replace: false` is a refusal, `true` removes the track's clips in the part's range.
pub trait ChordPartControl: Send + Sync {
    fn build(&self, request: ChordPartRequest) -> Result<ChordPartReport, ContractError>;
}

/// "Play a chord part without changing the project": the same notes a build would write, played once into a buffer
/// with the target track's instrument (a new kind with its defaults) and looped on the engine's preview voice until
/// `stop`; a new `play` replaces the previous one. `start_bar` and `replace` are ignored; a part over 16 bars is
/// refused.
pub trait ChordPartPreview: Send + Sync {
    fn play(&self, request: ChordPartRequest) -> Result<(), ContractError>;
    fn stop(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_read_and_write_back() {
        for symbol in ["C", "Am", "F#m7", "Bbmaj7", "Gsus4", "Ebdim", "Caug", "D7", "Esus2"] {
            assert_eq!(Chord::parse(symbol).map(Chord::symbol).as_deref(), Ok(symbol));
        }
        assert_eq!(Chord::parse("A#").map(Chord::symbol).as_deref(), Ok("Bb"), "a sharp is written as usual");
        assert_eq!(Chord::parse("Cb").map(|chord| chord.root), Ok(11));
        assert_eq!(Chord::parse("Am").map(Chord::pitch_classes), Ok(vec![9, 0, 4]));
    }

    #[test]
    fn a_bad_symbol_is_a_readable_refusal() {
        for symbol in ["", "H", "am", "Cmin", "C/E", "F##"] {
            let error = Chord::parse(symbol).unwrap_err();
            assert!(error.is(patterns::BAD_CHORD), "{symbol}: {error}");
        }
    }

    #[test]
    fn strums_are_one_bar_of_known_symbols() {
        for strum in STRUMS {
            assert_eq!(strum.grid.len(), STRUM_STEPS as usize, "{}", strum.id);
            assert!(strum.grid.chars().all(|c| "DdUuXx.".contains(c)), "{}", strum.id);
        }
        assert_eq!(strum("six").map(|strum| strum.grid), Ok("D.DU.UDU"));
        let error = strum("seven").unwrap_err();
        assert!(error.is(patterns::STRUM_NOT_FOUND) && error.to_string().contains("four, six, eight"), "{error}");
    }

    #[test]
    fn requests_read_with_defaults() {
        let request: ChordPartRequest = serde_json::from_value(serde_json::json!({
            "target": { "type": "new", "kind": "guitar" }, "strum": "six", "bars": [["C"], ["Am", "F"]]
        }))
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((request.start_bar, request.repeat, request.replace), (0, 1, false));
        assert_eq!(request.target, ChordTarget::New { kind: "guitar".into() });
    }
}
