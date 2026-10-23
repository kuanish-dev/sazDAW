//! Chord: one input note sounds as a chord built on it. The chord type comes from a table (major, minor, sevenths...)
//! or is "In scale": every other scale degree from the degree of the pressed note (a note outside the scale falls to
//! the degree below). Inversion moves the lower notes an octave up, "Wide" moves the second lowest note an octave up.

use daw_contracts::{BlockTiming, L10n, NoteBuffer, NoteEffect, NoteEvent, ParamInfo, ParamKind, Scale};

use crate::fit;

pub const KIND: &str = "chord";
pub const LABEL: L10n = L10n { en: "Chord", ru: "Аккорд" };

/// The largest chord: a seventh chord.
const MAX_VOICES: usize = 4;

/// Table chords: semitones from the pressed note (in the order of the `type` parameter's `choices`).
const CHORDS: [&[u8]; 9] =
    [&[0, 4, 7], &[0, 3, 7], &[0, 2, 7], &[0, 5, 7], &[0, 4, 7, 11], &[0, 3, 7, 10], &[0, 4, 7, 10], &[0, 7], &[0, 12]];
/// The "In scale" choices follow the table: the scale's triad and seventh chord.
const IN_SCALE_TRIAD: usize = CHORDS.len();

pub const PARAMS: [ParamInfo; 5] = [
    ParamInfo {
        name: "type",
        label: L10n { en: "Chord", ru: "Аккорд" },
        min: 0.0,
        max: 10.0,
        default: 0.0,
        kind: ParamKind::Choice,
        choices: &[
            L10n { en: "Major", ru: "Мажор" },
            L10n { en: "Minor", ru: "Минор" },
            L10n::same("Sus2"),
            L10n::same("Sus4"),
            L10n { en: "Major 7", ru: "Мажор 7" },
            L10n { en: "Minor 7", ru: "Минор 7" },
            L10n { en: "Dominant 7", ru: "Доминант 7" },
            L10n { en: "Fifth", ru: "Квинта" },
            L10n { en: "Octave", ru: "Октава" },
            L10n { en: "In scale", ru: "В ладу" },
            L10n { en: "In scale 7", ru: "В ладу 7" },
        ],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "inversion",
        label: L10n { en: "Inversion", ru: "Обращение" },
        min: 0.0,
        max: 3.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "spread",
        label: L10n { en: "Wide", ru: "Широко" },
        min: 0.0,
        max: 1.0,
        default: 0.0,
        kind: ParamKind::Switch,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "scale",
        label: L10n { en: "Scale (for «In scale»)", ru: "Лад (для «В ладу»)" },
        min: 0.0,
        max: 9.0,
        default: 0.0,
        kind: ParamKind::Choice,
        choices: &[
            L10n { en: "Major", ru: "Мажор" },
            L10n { en: "Minor", ru: "Минор" },
            L10n { en: "Dorian", ru: "Дорийский" },
            L10n { en: "Phrygian", ru: "Фригийский" },
            L10n { en: "Lydian", ru: "Лидийский" },
            L10n { en: "Mixolydian", ru: "Миксолидийский" },
            L10n { en: "Harmonic minor", ru: "Гармонический минор" },
            L10n { en: "Major pentatonic", ru: "Пентатоника мажор" },
            L10n { en: "Minor pentatonic", ru: "Пентатоника минор" },
            L10n { en: "Chromatic", ru: "Хроматический" },
        ],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "root",
        label: L10n { en: "Root (for «In scale»)", ru: "Тоника (для «В ладу»)" },
        min: 0.0,
        max: 11.0,
        default: 0.0,
        kind: ParamKind::Choice,
        choices: &[
            L10n::same("C"),
            L10n::same("C#"),
            L10n::same("D"),
            L10n::same("D#"),
            L10n::same("E"),
            L10n::same("F"),
            L10n::same("F#"),
            L10n::same("G"),
            L10n::same("G#"),
            L10n::same("A"),
            L10n::same("A#"),
            L10n::same("B"),
        ],
        about: L10n::EMPTY,
    },
];

/// The chord notes: pitches in ascending order and their count.
#[derive(Clone, Copy, Default)]
struct Voicing {
    pitches: [u8; MAX_VOICES],
    count: usize,
}

pub(crate) struct Chord {
    kind: usize,
    inversion: usize,
    spread: bool,
    scale: Scale,
    root: u8,
    /// What each pressed note played: releasing the input releases exactly that, even if the parameters changed since.
    played: [Voicing; 128],
    /// How many pressed notes hold each output pitch: a pitch is released when nobody holds it.
    holders: [u8; 128],
}

impl Chord {
    pub fn new() -> Self {
        Self { kind: 0, inversion: 0, spread: false, scale: Scale::Major, root: 0, played: [Voicing::default(); 128], holders: [0; 128] }
    }

    /// Chord semitones from the pressed note `pitch` (without inversion), ascending.
    fn intervals(&self, pitch: u8) -> ([i32; MAX_VOICES], usize) {
        let mut result = [0; MAX_VOICES];
        if let Some(chord) = CHORDS.get(self.kind) {
            for (slot, &step) in result.iter_mut().zip(chord.iter()) {
                *slot = i32::from(step);
            }
            return (result, chord.len());
        }
        let count = if self.kind == IN_SCALE_TRIAD { 3 } else { 4 };
        let steps = self.scale.intervals();
        let class = (i32::from(pitch) - i32::from(self.root)).rem_euclid(12);
        let degree = steps.iter().rposition(|&s| i32::from(s) <= class).unwrap_or(0);
        let below = class - i32::from(steps[degree]);
        for (voice, slot) in result.iter_mut().take(count).enumerate() {
            let at = degree + voice * 2;
            let octave = (at / steps.len()) as i32;
            *slot = i32::from(steps[at % steps.len()]) + 12 * octave - i32::from(steps[degree]) - below;
        }
        (result, count)
    }

    /// Chord pitches from `pitch` with inversion and spread; notes outside 0..=127 are dropped.
    fn voicing(&self, pitch: u8) -> Voicing {
        let (mut steps, count) = self.intervals(pitch);
        let steps = &mut steps[..count];
        for _ in 0..self.inversion.min(count - 1) {
            steps[0] += 12;
            steps.sort_unstable();
        }
        if self.spread && count >= 3 {
            steps[1] += 12;
            steps.sort_unstable();
        }
        let mut voicing = Voicing::default();
        for &step in steps.iter() {
            let note = i32::from(pitch) + step;
            if (0..=127).contains(&note) && !voicing.pitches[..voicing.count].contains(&(note as u8)) {
                voicing.pitches[voicing.count] = note as u8;
                voicing.count += 1;
            }
        }
        voicing
    }

    fn press(&mut self, event: NoteEvent, output: &mut NoteBuffer) {
        let slot = usize::from(event.pitch & 0x7f);
        if self.played[slot].count > 0 {
            self.release(event, output);
        }
        let voicing = self.voicing(event.pitch & 0x7f);
        for &note in &voicing.pitches[..voicing.count] {
            let holders = &mut self.holders[usize::from(note)];
            if *holders == 0 {
                output.push(NoteEvent::on(event.frame, note, event.velocity));
            }
            *holders = holders.saturating_add(1);
        }
        self.played[slot] = voicing;
    }

    fn release(&mut self, event: NoteEvent, output: &mut NoteBuffer) {
        let slot = usize::from(event.pitch & 0x7f);
        let voicing = std::mem::take(&mut self.played[slot]);
        for &note in &voicing.pitches[..voicing.count] {
            let holders = &mut self.holders[usize::from(note)];
            *holders = holders.saturating_sub(1);
            if *holders == 0 {
                output.push(NoteEvent::off(event.frame, note));
            }
        }
    }
}

impl NoteEffect for Chord {
    fn set_param(&mut self, name: &str, value: f32) {
        let Some(value) = fit(&PARAMS, name, value) else {
            return;
        };
        match name {
            "type" => self.kind = value as usize,
            "inversion" => self.inversion = value.round() as usize,
            "spread" => self.spread = value >= 0.5,
            "scale" => self.scale = Scale::ALL[value as usize],
            "root" => self.root = value as u8,
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.played = [Voicing::default(); 128];
        self.holders = [0; 128];
    }

    fn process(&mut self, input: &[NoteEvent], _timing: &BlockTiming, output: &mut NoteBuffer) {
        for &event in input {
            if event.is_on() {
                self.press(event, output);
            } else {
                self.release(event, output);
            }
        }
    }
}
