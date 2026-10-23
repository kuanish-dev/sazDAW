//! How an instrument holds a chord (`Style`, by its kind). The strings voice it for their tuning the way a player would:
//! one note per string, an open string where it is a chord tone, a fretted one within one hand position (four frets),
//! on the guitar the root as the lowest note (so C is x32010 and D is xx0232). The bass plays the root in its lowest
//! octave; any other instrument a block chord from C3.

use daw_contracts::{BASS_GUITAR_KIND, Chord, ELECTRIC_GUITAR_KIND, GUITAR_KIND, UKULELE_KIND};

/// Standard guitar tuning, the low string first: E2 A2 D3 G3 B3 E4.
const GUITAR: [u8; 6] = [40, 45, 50, 55, 59, 64];
/// The ukulele as most are strung: re-entrant, the G string high (G4 C4 E4 A4), in the order a down stroke hits them.
const UKULELE: [u8; 4] = [67, 60, 64, 69];
/// The bass's lowest string (E1): roots go from it up an octave.
const BASS_LOWEST: u8 = 28;
/// A block chord's root sits in the octave from C3.
const KEYS_ROOT_FROM: u8 = 48;
/// A hand position covers four frets: its first one and three more.
const SPAN: u8 = 3;
/// Higher positions are not tried: open-position shapes are what a strummed part wants.
const MAX_POSITION: u8 = 7;
/// Fewer sounding strings than this is not a strummed chord.
const MIN_STRINGS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Style {
    /// Strummed strings: the tuning in the order a down stroke hits them, and whether the lowest note must be the root.
    Strings { tuning: &'static [u8], root_in_bass: bool },
    /// The root alone, from this note up an octave.
    Bass { lowest: u8 },
    /// A block chord.
    Keys,
}

pub(crate) fn style(kind: &str) -> Style {
    match kind {
        GUITAR_KIND | ELECTRIC_GUITAR_KIND => Style::Strings { tuning: &GUITAR, root_in_bass: true },
        UKULELE_KIND => Style::Strings { tuning: &UKULELE, root_in_bass: false },
        BASS_GUITAR_KIND => Style::Bass { lowest: BASS_LOWEST },
        _ => Style::Keys,
    }
}

/// The chord's notes in stroke order: `(string, pitch)` for the strings (muted strings left out); for the bass and a
/// block chord the index is the note's place in the chord.
pub(crate) fn voice(style: Style, chord: Chord) -> Vec<(usize, u8)> {
    match style {
        Style::Strings { tuning, root_in_bass } => fretted(tuning, root_in_bass, chord).unwrap_or_else(|| block(chord)),
        Style::Bass { lowest } => vec![(0, lowest + (chord.root + 12 - lowest % 12) % 12)],
        Style::Keys => block(chord),
    }
}

fn block(chord: Chord) -> Vec<(usize, u8)> {
    chord.quality.intervals().iter().enumerate().map(|(index, interval)| (index, KEYS_ROOT_FROM + chord.root + interval)).collect()
}

/// The first fret of the hand position (the open string first) whose note is at least `lowest` and whose pitch class
/// is `wanted`.
fn fret_in(position: u8, open: u8, lowest: u8, wanted: impl Fn(u8) -> bool) -> Option<u8> {
    std::iter::once(0).chain(position.max(1)..=position + SPAN).find(|fret| open + fret >= lowest && wanted((open + fret) % 12))
}

/// The best way to hold the chord in one hand position. With `root_in_bass` the lowest string that can play the root
/// in the position plays it, as a player puts the root on the bass string (G7 is 320001, not an F on the low E), and the
/// strings below are muted; every other string plays its open note if that is a chord tone, otherwise the lowest chord
/// tone within the position's frets that is not below the bass, otherwise it is muted. A position must sound the
/// root; positions are scored: every chord tone first, then more different notes (a unison on two strings adds
/// nothing: D is xx0232, not x50232), more open strings, a lower position. `None` when no position holds the chord.
fn fretted(tuning: &[u8], root_in_bass: bool, chord: Chord) -> Option<Vec<(usize, u8)>> {
    let tones = chord.pitch_classes();
    let is_root = |class: u8| class == chord.root;
    let is_tone = |class: u8| tones.contains(&class);
    let mut best: Option<(i64, Vec<(usize, u8)>)> = None;
    for position in 0..=MAX_POSITION {
        let (first, lowest) = if root_in_bass {
            let bass =
                tuning.iter().enumerate().find_map(|(string, &open)| fret_in(position, open, 0, is_root).map(|fret| (string, open + fret)));
            let Some(bass) = bass else { continue };
            bass
        } else {
            (0, 0)
        };
        let notes: Vec<(usize, u8, bool)> = tuning
            .iter()
            .enumerate()
            .skip(first)
            .filter_map(|(string, &open)| {
                let fret = if root_in_bass && string == first {
                    fret_in(position, open, 0, is_root)
                } else {
                    fret_in(position, open, lowest, is_tone)
                };
                fret.map(|fret| (string, open + fret, fret == 0))
            })
            .collect();
        let has_root = notes.iter().any(|(_, pitch, _)| is_root(pitch % 12));
        if !has_root || notes.len() < MIN_STRINGS.min(tuning.len()) {
            continue;
        }
        let covered = tones.iter().filter(|tone| notes.iter().any(|(_, pitch, _)| pitch % 12 == **tone)).count();
        let open = notes.iter().filter(|(_, _, open)| *open).count();
        let mut different: Vec<u8> = notes.iter().map(|(_, pitch, _)| *pitch).collect();
        different.sort_unstable();
        different.dedup();
        let score = covered as i64 * 100 + different.len() as i64 * 10 + open as i64 * 3 - i64::from(position) * 4;
        if best.as_ref().is_none_or(|(best, _)| score > *best) {
            best = Some((score, notes.into_iter().map(|(string, pitch, _)| (string, pitch)).collect()));
        }
    }
    best.map(|(_, notes)| notes)
}

#[cfg(test)]
mod tests {
    use daw_contracts::ChordQuality;

    use super::*;

    fn pitches(kind: &str, symbol: &str) -> Vec<u8> {
        let chord = Chord::parse(symbol).unwrap_or_else(|e| panic!("{e}"));
        voice(style(kind), chord).into_iter().map(|(_, pitch)| pitch).collect()
    }

    #[test]
    fn the_guitar_holds_the_open_chords_a_player_would() {
        assert_eq!(pitches(GUITAR_KIND, "C"), [48, 52, 55, 60, 64], "x32010");
        assert_eq!(pitches(GUITAR_KIND, "G"), [43, 47, 50, 55, 59, 67], "320003");
        assert_eq!(pitches(GUITAR_KIND, "Am"), [45, 52, 57, 60, 64], "x02210");
        assert_eq!(pitches(GUITAR_KIND, "D"), [50, 57, 62, 66], "xx0232");
        assert_eq!(pitches(GUITAR_KIND, "E"), [40, 47, 52, 56, 59, 64], "022100");
        assert_eq!(pitches(GUITAR_KIND, "Em"), [40, 47, 52, 55, 59, 64], "022000");
        assert_eq!(pitches(GUITAR_KIND, "G7"), [43, 47, 50, 55, 59, 65], "320001");
        assert_eq!(pitches(ELECTRIC_GUITAR_KIND, "C"), pitches(GUITAR_KIND, "C"), "the electric guitar holds the same shapes");
    }

    #[test]
    fn the_ukulele_keeps_its_high_g_string_first() {
        assert_eq!(pitches(UKULELE_KIND, "C"), [67, 60, 64, 72], "0003");
        assert_eq!(pitches(UKULELE_KIND, "Am"), [69, 60, 64, 69], "2000");
        assert_eq!(pitches(UKULELE_KIND, "F"), [69, 60, 65, 69], "2010");
        assert_eq!(pitches(UKULELE_KIND, "G"), [67, 62, 67, 71], "0232");
    }

    #[test]
    fn the_bass_plays_the_root_low_and_other_instruments_a_block_chord() {
        assert_eq!(pitches(BASS_GUITAR_KIND, "E"), [28]);
        assert_eq!(pitches(BASS_GUITAR_KIND, "C"), [36]);
        assert_eq!(pitches(BASS_GUITAR_KIND, "Ebm"), [39]);
        assert_eq!(pitches("synth", "Am7"), [57, 60, 64, 67]);
    }

    #[test]
    fn every_chord_has_a_voicing_with_its_root_on_every_instrument() {
        for kind in [GUITAR_KIND, ELECTRIC_GUITAR_KIND, UKULELE_KIND, BASS_GUITAR_KIND, "synth"] {
            for root in 0..12 {
                for quality in ChordQuality::ALL {
                    let chord = Chord { root, quality };
                    let notes = voice(style(kind), chord);
                    assert!(!notes.is_empty() && notes.iter().all(|(_, pitch)| *pitch <= 127), "{kind} {}", chord.symbol());
                    assert!(notes.iter().any(|(_, pitch)| pitch % 12 == root), "{kind} {}: no root", chord.symbol());
                    if kind == GUITAR_KIND || kind == ELECTRIC_GUITAR_KIND {
                        let lowest = notes.iter().map(|(_, pitch)| *pitch).min().unwrap_or_default();
                        assert_eq!(lowest % 12, root, "{}: the root is the lowest note: {notes:?}", chord.symbol());
                    }
                }
            }
        }
    }
}
