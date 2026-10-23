//! Computer keys as a piano — the same layout as in the app (`app/src/keyboard/keyboard.ts`), but by macOS key codes
//! (they are tied to the key position, so the keyboard language does not matter). A plugin window without a web page does not see
//! the main window's hotkeys, so `KeyPlayer` plays them itself.
//!
//!   lower octave:  S D   G H J   L ;      black
//!                  Z X C V B N M , . /     white
//!   upper octave:  2 3   5 6 7   9 0      black
//!                  Q W E R T Y U I O P     white
//!   `[` and `]` — octave down and up.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use daw_contracts::sync::lock;
use daw_contracts::{LiveNoteSink, TrackId};

/// macOS key codes (`kVK_ANSI_*` from `Events.h`): the physical key number, not the layout letter.
mod code {
    pub const S: u16 = 1;
    pub const D: u16 = 2;
    pub const H: u16 = 4;
    pub const G: u16 = 5;
    pub const Z: u16 = 6;
    pub const X: u16 = 7;
    pub const C: u16 = 8;
    pub const V: u16 = 9;
    pub const B: u16 = 11;
    pub const Q: u16 = 12;
    pub const W: u16 = 13;
    pub const E: u16 = 14;
    pub const R: u16 = 15;
    pub const Y: u16 = 16;
    pub const T: u16 = 17;
    pub const TWO: u16 = 19;
    pub const THREE: u16 = 20;
    pub const SIX: u16 = 22;
    pub const FIVE: u16 = 23;
    pub const NINE: u16 = 25;
    pub const SEVEN: u16 = 26;
    pub const ZERO: u16 = 29;
    pub const RIGHT_BRACKET: u16 = 30;
    pub const O: u16 = 31;
    pub const U: u16 = 32;
    pub const LEFT_BRACKET: u16 = 33;
    pub const I: u16 = 34;
    pub const P: u16 = 35;
    pub const L: u16 = 37;
    pub const J: u16 = 38;
    pub const SEMICOLON: u16 = 41;
    pub const COMMA: u16 = 43;
    pub const SLASH: u16 = 44;
    pub const N: u16 = 45;
    pub const M: u16 = 46;
    pub const PERIOD: u16 = 47;
}

const WHITE_OFFSETS: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const LOWER_WHITE: [u16; 10] = [code::Z, code::X, code::C, code::V, code::B, code::N, code::M, code::COMMA, code::PERIOD, code::SLASH];
const UPPER_WHITE: [u16; 10] = [code::Q, code::W, code::E, code::R, code::T, code::Y, code::U, code::I, code::O, code::P];
/// (key code, semitones from the row start): C#, D#, F#, G#, A#, C#', D#'.
const LOWER_BLACK: [(u16, u8); 7] =
    [(code::S, 1), (code::D, 3), (code::G, 6), (code::H, 8), (code::J, 10), (code::L, 13), (code::SEMICOLON, 15)];
const UPPER_BLACK: [(u16, u8); 7] =
    [(code::TWO, 1), (code::THREE, 3), (code::FIVE, 6), (code::SIX, 8), (code::SEVEN, 10), (code::NINE, 13), (code::ZERO, 15)];

pub const OCTAVE_DOWN: u16 = code::LEFT_BRACKET;
pub const OCTAVE_UP: u16 = code::RIGHT_BRACKET;
const MIN_OCTAVE: i32 = 0;
const MAX_OCTAVE: i32 = 8;
const DEFAULT_OCTAVE: i32 = 4;
const VELOCITY: f32 = 0.8;

fn white_semitone(index: usize) -> u8 {
    WHITE_OFFSETS[index % 7] + 12 * (index / 7) as u8
}

/// The note pitch (MIDI) for a key at base octave `octave` (C4 = 60), or `None` if the key is not a note key.
pub fn pitch_for(code: u16, octave: i32) -> Option<u8> {
    let lower = (octave + 1) * 12;
    let upper = (octave + 2) * 12;
    let semitones = |base: i32, offset: u8| base + i32::from(offset);
    let pitch = if let Some(i) = LOWER_WHITE.iter().position(|&c| c == code) {
        semitones(lower, white_semitone(i))
    } else if let Some(i) = UPPER_WHITE.iter().position(|&c| c == code) {
        semitones(upper, white_semitone(i))
    } else if let Some(&(_, offset)) = LOWER_BLACK.iter().find(|(c, _)| *c == code) {
        semitones(lower, offset)
    } else if let Some(&(_, offset)) = UPPER_BLACK.iter().find(|(c, _)| *c == code) {
        semitones(upper, offset)
    } else {
        return None;
    };
    u8::try_from(pitch).ok().filter(|p| *p <= 127)
}

/// A key press or release in a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    pub code: u16,
    pub down: bool,
    /// Auto-repeat while held: ignored.
    pub repeat: bool,
    /// Cmd/Ctrl/Alt are held: this is a shortcut, not a note.
    pub chord: bool,
}

/// Job: turn a plugin window's keys into live notes on its instrument's track.
pub struct KeyPlayer {
    live: Arc<dyn LiveNoteSink>,
    state: Mutex<PlayerState>,
}

struct PlayerState {
    octave: i32,
    /// Plugin windows (by window id) and the instrument track the window plays on.
    windows: HashMap<usize, TrackId>,
    /// Held keys: on which track and which note to release.
    held: HashMap<u16, (TrackId, u8)>,
}

impl KeyPlayer {
    pub fn new(live: Arc<dyn LiveNoteSink>) -> Self {
        Self { live, state: Mutex::new(PlayerState { octave: DEFAULT_OCTAVE, windows: HashMap::new(), held: HashMap::new() }) }
    }

    fn state(&self) -> MutexGuard<'_, PlayerState> {
        lock(&self.state)
    }

    pub fn watch(&self, window: usize, track: TrackId) {
        self.state().windows.insert(window, track);
    }

    /// The window is closed: its notes are released.
    pub fn unwatch(&self, window: usize) {
        let mut state = self.state();
        let Some(track) = state.windows.remove(&window) else { return };
        let released: Vec<u16> = state.held.iter().filter(|(_, (t, _))| *t == track).map(|(code, _)| *code).collect();
        for code in released {
            if let Some((track, pitch)) = state.held.remove(&code) {
                self.live.note_off(track, pitch);
            }
        }
    }

    /// Handles a key from `window`; `true` when the key is eaten (a note or an octave) and need not be passed to the plugin window.
    pub fn press(&self, window: usize, key: KeyPress) -> bool {
        let mut state = self.state();
        if !key.down {
            return match state.held.remove(&key.code) {
                Some((track, pitch)) => {
                    self.live.note_off(track, pitch);
                    true
                }
                None => false,
            };
        }
        let Some(&track) = state.windows.get(&window) else { return false };
        if key.chord {
            return false;
        }
        if key.code == OCTAVE_DOWN || key.code == OCTAVE_UP {
            if !key.repeat {
                let step = if key.code == OCTAVE_UP { 1 } else { -1 };
                state.octave = (state.octave + step).clamp(MIN_OCTAVE, MAX_OCTAVE);
            }
            return true;
        }
        let Some(pitch) = pitch_for(key.code, state.octave) else { return false };
        if !key.repeat {
            state.held.insert(key.code, (track, pitch));
            self.live.note_on(track, pitch, VELOCITY);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Notes(Mutex<Vec<String>>);

    impl LiveNoteSink for Notes {
        fn note_on(&self, track: TrackId, pitch: u8, _velocity: f32) {
            self.0.lock().unwrap().push(format!("on {track} {pitch}"));
        }
        fn note_off(&self, track: TrackId, pitch: u8) {
            self.0.lock().unwrap().push(format!("off {track} {pitch}"));
        }
        fn controller(&self, _: TrackId, _: daw_contracts::ControllerKind, _: f32) {}
    }

    const WINDOW: usize = 7;
    const Z: u16 = code::Z;
    const Q: u16 = code::Q;
    /// Key A: it is not in the piano layout.
    const KVK_ANSI_A: u16 = 0;

    fn press(code: u16, down: bool) -> KeyPress {
        KeyPress { code, down, repeat: false, chord: false }
    }

    fn rig() -> (KeyPlayer, Arc<Notes>) {
        let notes = Arc::new(Notes::default());
        let player = KeyPlayer::new(notes.clone());
        player.watch(WINDOW, 3);
        (player, notes)
    }

    #[test]
    fn layout_matches_the_app_keyboard() {
        assert_eq!(pitch_for(Z, 4), Some(60), "Z is C of the keyboard's lower octave (C4)");
        assert_eq!(pitch_for(Q, 4), Some(72), "Q is C of the upper row (C5)");
        assert_eq!(pitch_for(code::S, 4), Some(61), "S — C#4");
        assert_eq!(pitch_for(code::SLASH, 4), Some(60 + 16), "/ is the tenth white key: E5");
        assert_eq!(pitch_for(code::P, 4), Some(72 + 16), "P is the tenth white key of the upper row");
        assert_eq!(pitch_for(KVK_ANSI_A, 4), None, "A is not a note key");
        assert_eq!(pitch_for(code::P, 8), None, "no notes beyond MIDI range");
    }

    #[test]
    fn a_key_in_a_watched_window_plays_on_its_track_and_release_stops_it() {
        let (player, notes) = rig();
        assert!(player.press(WINDOW, press(Z, true)));
        assert!(player.press(99, press(Z, false)), "a release works in any window");
        assert_eq!(*notes.0.lock().unwrap(), ["on 3 60", "off 3 60"]);
    }

    #[test]
    fn other_windows_repeats_and_chords_are_ignored() {
        let (player, notes) = rig();
        assert!(!player.press(99, press(Z, true)), "another window (the main one plays by itself)");
        assert!(player.press(WINDOW, press(Z, true)));
        assert!(player.press(WINDOW, KeyPress { repeat: true, ..press(Z, true) }), "auto-repeat is eaten but does not play again");
        assert!(!player.press(WINDOW, KeyPress { chord: true, ..press(Q, true) }), "Cmd+Q is not a note");
        assert_eq!(notes.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn octave_keys_shift_the_pitch_and_stay_in_range() {
        let (player, notes) = rig();
        assert!(player.press(WINDOW, press(OCTAVE_UP, true)));
        player.press(WINDOW, press(Z, true));
        assert_eq!(notes.0.lock().unwrap()[0], "on 3 72");
        for _ in 0..20 {
            player.press(WINDOW, press(OCTAVE_DOWN, true));
        }
        player.press(WINDOW, press(Q, true));
        assert_eq!(notes.0.lock().unwrap()[1], "on 3 24", "octave 0: Q is C1");
    }

    #[test]
    fn closing_the_window_releases_its_notes() {
        let (player, notes) = rig();
        player.press(WINDOW, press(Z, true));
        player.unwatch(WINDOW);
        assert_eq!(*notes.0.lock().unwrap(), ["on 3 60", "off 3 60"]);
        assert!(!player.press(WINDOW, press(Z, true)), "after closing, the window is not listened to");
    }
}
