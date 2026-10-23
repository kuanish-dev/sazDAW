//! Strumming: one pass of a chord progression as notes. Every stroke of the strum pattern plays the chord of its half bar
//! the way the instrument holds it (`voicing.rs`):
//! * strings: a down stroke runs from the first string to the last, an up stroke back over the top four, each string
//!   `spread` ticks after the previous one and a little softer. A string rings until a stroke hits it again, the chord
//!   changes, or a muted stroke (a "chuck") stops every string; the muted stroke's own notes are short, so the
//!   instrument damps them at once;
//! * the bass: the root on the down strokes, ringing until the next one;
//! * a block chord: like the strings, but all notes at once.

use daw_contracts::{BAR_TICKS, Bpm, Chord, NewNote, PPQ, STRUM_STEP_TICKS, STRUM_STEPS};

use crate::voicing::{Style, voice};

/// How far apart a strum hits two neighbouring strings: about what a real strum takes.
const SPREAD_SECONDS: f64 = 0.012;
/// At a very fast tempo the strum still fits well inside one step.
const MAX_SPREAD: u32 = STRUM_STEP_TICKS / 12;
/// A muted stroke's notes: short, so they sound as a percussive "chuck".
const MUTED_TICKS: u32 = STRUM_STEP_TICKS / 8;
/// An up stroke hits only the top strings.
const UP_STRINGS: usize = 4;
/// Velocities of the bar's first stroke, a down, an up and a muted stroke; a lower case symbol is `SOFT` of it.
const FIRST: u8 = 110;
const DOWN: u8 = 96;
const UP: u8 = 76;
const MUTED: u8 = 88;
const SOFT: f32 = 0.65;
/// Each next string of a stroke is this much softer: the pick loses force.
const STRING_FALL: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Down,
    Up,
    Muted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stroke {
    pub(crate) step: u32,
    pub(crate) direction: Direction,
    pub(crate) soft: bool,
}

/// The strokes of a strum grid (`daw_contracts::STRUMS`, whose symbols are checked there).
pub(crate) fn strokes(grid: &str) -> Vec<Stroke> {
    grid.chars()
        .zip(0..)
        .filter_map(|(symbol, step)| {
            let direction = match symbol.to_ascii_uppercase() {
                'D' => Direction::Down,
                'U' => Direction::Up,
                'X' => Direction::Muted,
                _ => return None,
            };
            Some(Stroke { step, direction, soft: symbol.is_ascii_lowercase() })
        })
        .collect()
}

/// Ticks between two strings of a strum at `tempo`.
pub(crate) fn spread_ticks(tempo: Bpm) -> u32 {
    let ticks_per_second = tempo.get() / 60.0 * f64::from(PPQ);
    ((SPREAD_SECONDS * ticks_per_second).round() as u32).clamp(1, MAX_SPREAD)
}

#[derive(Clone, Copy)]
struct Ringing {
    pitch: u8,
    start: u32,
    velocity: u8,
}

/// The notes written so far and what is still sounding, by string.
#[derive(Default)]
struct Strummer {
    notes: Vec<NewNote>,
    ringing: Vec<Option<Ringing>>,
}

impl Strummer {
    fn end(&mut self, string: usize, at: u32) {
        if let Some(note) = self.ringing.get_mut(string).and_then(Option::take) {
            let length_ticks = at.saturating_sub(note.start).max(1);
            self.notes.push(NewNote { pitch: note.pitch, start_tick: note.start, length_ticks, velocity: note.velocity });
        }
    }

    fn end_all(&mut self, at: u32) {
        for string in 0..self.ringing.len() {
            self.end(string, at);
        }
    }

    fn ring(&mut self, string: usize, note: Ringing) {
        self.end(string, note.start);
        if self.ringing.len() <= string {
            self.ringing.resize(string + 1, None);
        }
        self.ringing[string] = Some(note);
    }

    fn play(&mut self, stroke: Stroke, notes: &[(usize, u8)], style: Style, tick: u32, spread: u32) {
        let base = match stroke.direction {
            Direction::Down if stroke.step == 0 => FIRST,
            Direction::Down => DOWN,
            Direction::Up => UP,
            Direction::Muted => MUTED,
        };
        let velocity = if stroke.soft { (f32::from(base) * SOFT) as u8 } else { base };
        let spread = if matches!(style, Style::Strings { .. }) { spread } else { 0 };
        let hit: Vec<(usize, u8)> = match (style, stroke.direction) {
            (Style::Bass { .. }, Direction::Up | Direction::Muted) => return,
            (_, Direction::Up) => notes.iter().rev().take(UP_STRINGS).copied().collect(),
            (_, Direction::Down | Direction::Muted) => notes.to_vec(),
        };
        if stroke.direction == Direction::Muted {
            self.end_all(tick);
        }
        for (k, (string, pitch)) in (0u32..).zip(hit) {
            let start = tick + k * spread;
            let velocity = velocity.saturating_sub(u8::try_from(k).unwrap_or(u8::MAX).saturating_mul(STRING_FALL)).max(1);
            if stroke.direction == Direction::Muted {
                self.notes.push(NewNote { pitch, start_tick: start, length_ticks: MUTED_TICKS, velocity });
            } else {
                self.ring(string, Ringing { pitch, start, velocity });
            }
        }
    }
}

/// One pass of the progression: in every bar (one chord, or two — the second from the middle of the bar) the strum's
/// strokes. Times are from the pass start; everything stops at its end, `bars.len()` bars later.
pub(crate) fn progression_notes(bars: &[Vec<Chord>], strokes: &[Stroke], style: Style, spread: u32) -> Vec<NewNote> {
    let mut strummer = Strummer::default();
    let mut previous: Option<Chord> = None;
    for (bar, chords) in (0u32..).zip(bars) {
        for stroke in strokes {
            let tick = bar * BAR_TICKS + stroke.step * STRUM_STEP_TICKS;
            let chord = if stroke.step >= STRUM_STEPS / 2 { chords.last() } else { chords.first() };
            let Some(&chord) = chord else { continue };
            if previous.is_some_and(|previous| previous != chord) {
                strummer.end_all(tick);
            }
            previous = Some(chord);
            strummer.play(*stroke, &voice(style, chord), style, tick, spread);
        }
    }
    strummer.end_all(u32::try_from(bars.len()).unwrap_or(u32::MAX).saturating_mul(BAR_TICKS));
    strummer.notes.sort_by_key(|note| (note.start_tick, note.pitch));
    strummer.notes
}

#[cfg(test)]
mod tests {
    use daw_contracts::{STRUMS, strum};

    use super::*;
    use crate::voicing::style;

    fn chords(symbols: &[&[&str]]) -> Vec<Vec<Chord>> {
        symbols.iter().map(|bar| bar.iter().map(|s| Chord::parse(s).unwrap_or_else(|e| panic!("{e}"))).collect()).collect()
    }

    fn notes(kind: &str, strum_id: &str, symbols: &[&[&str]]) -> Vec<NewNote> {
        let grid = strum(strum_id).unwrap_or_else(|e| panic!("{e}")).grid;
        progression_notes(&chords(symbols), &strokes(grid), style(kind), 20)
    }

    /// The ticks where notes start, each once.
    fn onsets(notes: &[NewNote]) -> Vec<u32> {
        let mut ticks: Vec<u32> = notes.iter().map(|n| n.start_tick).collect();
        ticks.dedup();
        ticks
    }

    #[test]
    fn the_six_strum_is_down_down_up_up_down_up() {
        let strokes = strokes(strum("six").unwrap_or_else(|e| panic!("{e}")).grid);
        let directions: Vec<_> = strokes.iter().map(|s| (s.step, s.direction)).collect();
        use Direction::{Down, Up};
        assert_eq!(directions, [(0, Down), (2, Down), (3, Up), (5, Up), (6, Down), (7, Up)]);
    }

    #[test]
    fn a_down_stroke_runs_up_the_strings_and_an_up_stroke_back_over_the_top_four() {
        let notes = notes("guitar", "six", &[&["C"]]);
        let first: Vec<_> = notes.iter().filter(|n| n.start_tick < STRUM_STEP_TICKS).map(|n| (n.start_tick, n.pitch, n.velocity)).collect();
        assert_eq!(first, [(0, 48, 110), (20, 52, 107), (40, 55, 104), (60, 60, 101), (80, 64, 98)]);
        let up: Vec<_> = notes.iter().filter(|n| (1440..1440 + 100).contains(&n.start_tick)).map(|n| (n.start_tick, n.pitch)).collect();
        assert_eq!(up, [(1440, 64), (1460, 60), (1480, 55), (1500, 52)]);
    }

    #[test]
    fn a_string_rings_until_it_is_hit_again_and_the_part_ends_with_its_last_bar() {
        let notes = notes("guitar", "six", &[&["C"]]);
        let low: Vec<_> = notes.iter().filter(|n| n.pitch == 48).map(|n| (n.start_tick, n.length_ticks)).collect();
        assert_eq!(low, [(0, 960), (960, 1920), (2880, 960)]);
        assert!(notes.iter().all(|n| n.start_tick + n.length_ticks <= BAR_TICKS && n.validate(BAR_TICKS).is_ok()));
    }

    #[test]
    fn a_muted_stroke_stops_every_string_and_is_short_itself() {
        let notes = notes("guitar", "four", &[&["G"]]);
        let chuck_at = 4 * STRUM_STEP_TICKS;
        assert!(notes.iter().filter(|n| n.start_tick < chuck_at).all(|n| n.start_tick + n.length_ticks <= chuck_at), "all strings stop");
        let chuck: Vec<_> = notes.iter().filter(|n| (chuck_at..chuck_at + 200).contains(&n.start_tick)).collect();
        assert_eq!(chuck.len(), 6);
        assert!(chuck.iter().all(|n| n.length_ticks == MUTED_TICKS));
    }

    #[test]
    fn a_chord_change_in_the_middle_of_a_bar_lets_the_old_strings_go() {
        let notes = notes("guitar", "eight", &[&["C", "G"]]);
        let middle = BAR_TICKS / 2;
        assert!(notes.iter().filter(|n| n.start_tick < middle).all(|n| n.start_tick + n.length_ticks <= middle));
        let after: Vec<u8> = notes.iter().filter(|n| n.start_tick >= middle).map(|n| n.pitch).collect();
        assert!(after.contains(&43) && !after.contains(&48), "G from the middle on: {after:?}");
    }

    #[test]
    fn the_bass_plays_the_root_on_down_strokes_only() {
        let notes = notes("bass_guitar", "six", &[&["A"], &["D"]]);
        let played: Vec<_> = notes.iter().map(|n| (n.start_tick, n.pitch)).collect();
        assert_eq!(played, [(0, 33), (960, 33), (2880, 33), (3840, 38), (4800, 38), (6720, 38)]);
    }

    #[test]
    fn a_block_chord_sounds_at_once() {
        let notes = notes("synth", "four", &[&["Am"]]);
        assert_eq!(onsets(&notes), [0, 960, 1920, 2880]);
        assert_eq!(notes.iter().filter(|n| n.start_tick == 0).count(), 3);
    }

    #[test]
    fn every_strum_on_every_instrument_writes_valid_notes_inside_the_pass() {
        for strum in STRUMS {
            for kind in ["guitar", "ukulele", "bass_guitar", "synth"] {
                let notes = notes(kind, strum.id, &[&["C"], &["Am", "F#m7"], &["Bbmaj7"], &["Gsus4", "G7"]]);
                assert!(!notes.is_empty(), "{kind} {}", strum.id);
                assert!(notes.iter().all(|n| n.validate(4 * BAR_TICKS).is_ok()), "{kind} {}", strum.id);
            }
        }
    }

    #[test]
    fn the_strum_spread_follows_the_tempo_but_stays_inside_a_step() {
        let spread = |bpm| spread_ticks(Bpm::new(bpm).unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(spread(120.0), 23, "12 ms at 120 BPM");
        assert!(spread(60.0) < spread(120.0));
        assert_eq!(spread(Bpm::MAX), MAX_SPREAD);
    }
}
