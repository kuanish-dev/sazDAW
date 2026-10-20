//! Melodic step lines: a melody written the way a step sequencer (a TB-303) holds it, one token per step of a grid,
//! like the drum grids of `patterns`. `ClipControl::add_step_clip` turns a line into a note clip.
//!
//! Tokens are separated by spaces; `|` marks a bar for readability and is ignored. Each token is one step:
//! * a note name: a letter `C`…`B` (either case), an optional `#` or `b`, the octave (C4 = 60, middle C): `F2`, `Eb3`;
//! * a scale degree of the project key (`Project::key`): `1` is the tonic in the octave [`NewStepClip::octave`] and the
//!   numbers count up the scale (in a seven-note scale `8` is the tonic an octave up); each `+` after the number
//!   raises it an octave, each `-` lowers it: `1 3 5 8`, `5- 1`;
//! * `-` or `.` a rest; `_` holds the note before it through this step.
//!
//! A note may end with marks, in any order: `!` an accent (velocity [`VELOCITY_STRONG`] instead of
//! [`VELOCITY_NORMAL`]) and `~` a slide: the note sounds for all its steps and into the next note. Into the same pitch
//! a slide is a tie (the two make one note, the second one's accent is lost); into another pitch the notes overlap by
//! [`SLIDE_OVERLAP_TICKS`] at most, so a mono synth with glide slides; before a rest or at the end of the line the
//! note just sounds for all its steps. A note without `~` sounds for `gate` of its last step.
//!
//! An F minor acid riff on sixteenths: `F2 F3! - F2 Eb3~ F3 - C3 | F2 _ Ab2! - C3~ Eb3 F2 -`.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::chord_part::letter_pitch_class;
use crate::codes::{notes, patterns as codes};
use crate::{BAR_TICKS, ContractError, NewNote, STEP_TICKS, ScaleKey, TrackId, VELOCITY_NORMAL, VELOCITY_STRONG};

/// The longest overlap of a slid note with the next one, a 64th: enough for a mono synth to glide, too short to be
/// heard as two notes on a polyphonic one.
pub const SLIDE_OVERLAP_TICKS: u32 = STEP_TICKS / 4;

fn sixteenth() -> u32 {
    STEP_TICKS
}

fn half() -> f64 {
    0.5
}

fn third() -> i8 {
    3
}

/// A note clip written as a step line (`ClipControl::add_step_clip`).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewStepClip {
    pub track: TrackId,
    pub start_tick: u32,
    /// Clip length on the timeline; `None` is one pass of the line (a longer clip repeats the line).
    pub length_ticks: Option<u32>,
    /// The melody, one token per step, separated by spaces (`|` marks a bar and is ignored): a note name (`F2`, `Eb3`,
    /// `C#4`; C4 = 60) or a scale degree of the project key (`1` the tonic, `8` the tonic an octave up, `+`/`-` after
    /// it an octave up/down), then `!` for an accent and `~` to slide or tie into the next step; `-` is a rest, `_`
    /// holds the note before it. Example: `"F2 F3! - F2 Eb3~ F3 - C3 | F2 _ Ab2! - C3~ Eb3 F2 -"`.
    pub steps: String,
    /// Length of one step in ticks: 240 (a sixteenth) by default, 480 an eighth, 120 a 32nd.
    #[serde(default = "sixteenth")]
    pub step_ticks: u32,
    /// The share of its last step a note without `~` sounds: above 0, at most 1; 0.5 by default (short, like a 303).
    #[serde(default = "half")]
    pub gate: f64,
    /// The octave of scale degree `1` (the tonic), 3 by default: in F minor `1` is F3.
    #[serde(default = "third")]
    pub octave: i8,
}

/// A step line as notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepNotes {
    /// In time order; times are from the clip start.
    pub notes: Vec<NewNote>,
    /// The line's length: its steps times `step_ticks`.
    pub loop_ticks: u32,
}

/// One token of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Rest,
    Hold,
    Note { pitch: u8, accent: bool, slide: bool },
}

/// A note whose steps are still being read.
struct Open {
    pitch: u8,
    first: u32,
    steps: u32,
    accent: bool,
    slide: bool,
}

impl NewStepClip {
    /// The line as notes; scale degrees are counted in `key` (the project key). Errors: a step length outside
    /// `1..=BAR_TICKS` is `patterns::STEP_LENGTH_OUT_OF_RANGE`, a gate outside `(0, 1]` `GATE_OUT_OF_RANGE`, a line
    /// without steps `NO_STEPS`, an unreadable token `BAD_STEP`, `_` after a rest or at the start `HOLD_WITHOUT_NOTE`,
    /// a degree without a key `DEGREE_WITHOUT_KEY`, a pitch outside 0..=127 `notes::PITCH_OUT_OF_RANGE`. Lengths are
    /// not checked here: the clip checks the notes and `loop_ticks` as for any note clip.
    pub fn notes(&self, key: Option<ScaleKey>) -> Result<StepNotes, ContractError> {
        let step_ticks = self.step_ticks;
        if !(1..=BAR_TICKS).contains(&step_ticks) {
            return Err(codes::STEP_LENGTH_OUT_OF_RANGE.with("ticks", step_ticks).with("max", BAR_TICKS));
        }
        if self.gate.is_nan() || self.gate <= 0.0 || self.gate > 1.0 {
            return Err(codes::GATE_OUT_OF_RANGE.with("gate", self.gate));
        }
        let tokens: Vec<&str> = self.steps.split(|c: char| c.is_whitespace() || c == '|').filter(|token| !token.is_empty()).collect();
        if tokens.is_empty() {
            return Err(codes::NO_STEPS.err());
        }
        let gate_ticks = ((self.gate * f64::from(step_ticks)).round() as u32).max(1);
        let mut line = Line { step_ticks, gate_ticks, notes: Vec::new() };
        let mut open: Option<Open> = None;
        for (index, token) in tokens.iter().enumerate() {
            let number = index + 1;
            match read_step(token, number, key, self.octave)? {
                Step::Rest => line.close(open.take(), false),
                Step::Hold => match open.as_mut() {
                    Some(note) => note.steps += 1,
                    None => return Err(codes::HOLD_WITHOUT_NOTE.with("step", number)),
                },
                Step::Note { pitch, accent, slide } => match open.as_mut() {
                    Some(note) if note.slide && note.pitch == pitch => {
                        note.steps += 1;
                        note.slide = slide;
                    }
                    _ => {
                        line.close(open.take(), true);
                        let first = u32::try_from(index).unwrap_or(u32::MAX);
                        open = Some(Open { pitch, first, steps: 1, accent, slide });
                    }
                },
            }
        }
        line.close(open, false);
        let steps = u32::try_from(tokens.len()).unwrap_or(u32::MAX);
        Ok(StepNotes { notes: line.notes, loop_ticks: steps.saturating_mul(step_ticks) })
    }
}

/// The notes written so far. Overflowing times saturate: the clip then refuses them as too long.
struct Line {
    step_ticks: u32,
    gate_ticks: u32,
    notes: Vec<NewNote>,
}

impl Line {
    /// Writes the finished `note`; `next_note` says whether a note starts right after it (a slide overlaps it).
    fn close(&mut self, note: Option<Open>, next_note: bool) {
        let Some(note) = note else { return };
        let all_steps = note.steps.saturating_mul(self.step_ticks);
        let length = match (note.slide, next_note) {
            (false, _) => all_steps - self.step_ticks + self.gate_ticks,
            // Every note sounds at least `gate_ticks`, so half of it always ends inside the next note.
            (true, true) => all_steps.saturating_add(SLIDE_OVERLAP_TICKS.min(self.gate_ticks / 2)),
            (true, false) => all_steps,
        };
        let velocity = if note.accent { VELOCITY_STRONG } else { VELOCITY_NORMAL };
        self.notes.push(NewNote {
            pitch: note.pitch,
            start_tick: note.first.saturating_mul(self.step_ticks),
            length_ticks: length,
            velocity,
        });
    }
}

/// Reads one token (`number` counts from 1 for the errors).
fn read_step(token: &str, number: usize, key: Option<ScaleKey>, octave: i8) -> Result<Step, ContractError> {
    match token {
        "-" | "." => return Ok(Step::Rest),
        "_" => return Ok(Step::Hold),
        _ => {}
    }
    let bad = || codes::BAD_STEP.with("step", number).with("token", token);
    let pitch_text = token.trim_end_matches(['!', '~']);
    let marks = &token[pitch_text.len()..];
    let pitch = if pitch_text.starts_with(|c: char| c.is_ascii_digit()) {
        let key = key.ok_or_else(|| codes::DEGREE_WITHOUT_KEY.with("step", number).with("token", token))?;
        degree_pitch(pitch_text, key, octave).ok_or_else(bad)?
    } else {
        named_pitch(pitch_text).ok_or_else(bad)?
    };
    let pitch = u8::try_from(pitch).ok().filter(|pitch| *pitch <= 127).ok_or_else(|| notes::PITCH_OUT_OF_RANGE.with("pitch", pitch))?;
    Ok(Step::Note { pitch, accent: marks.contains('!'), slide: marks.contains('~') })
}

/// `F2`, `Eb3`, `c#4` as a MIDI pitch (C4 = 60), not yet checked against 0..=127.
fn named_pitch(text: &str) -> Option<i64> {
    let mut chars = text.chars();
    let natural = letter_pitch_class(chars.next()?.to_ascii_uppercase())?;
    let rest = chars.as_str();
    let (shift, octave) = match rest.strip_prefix('#') {
        Some(octave) => (1, octave),
        None => rest.strip_prefix('b').map_or((0, rest), |octave| (-1, octave)),
    };
    let octave: i64 = octave.parse::<i8>().ok()?.into();
    Some(12 * (octave + 1) + i64::from(natural) + shift)
}

/// A scale degree (`5`, `3+`, `7--`) in `key` as a MIDI pitch, degree `1` in octave `octave`.
fn degree_pitch(text: &str, key: ScaleKey, octave: i8) -> Option<i64> {
    let digits = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
    let (number, marks) = text.split_at(digits);
    let index = number.parse::<i64>().ok().filter(|degree| *degree >= 1)? - 1;
    if !marks.chars().all(|c| matches!(c, '+' | '-')) {
        return None;
    }
    let shift = marks.matches('+').count() as i64 - marks.matches('-').count() as i64;
    let intervals = key.scale.intervals();
    let size = intervals.len() as i64;
    let octaves = i64::from(octave) + index / size + shift;
    let interval = intervals[(index % size) as usize];
    Some(12 * (octaves + 1) + i64::from(key.root) + i64::from(interval))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scale;

    fn line(steps: &str) -> NewStepClip {
        NewStepClip { track: 1, start_tick: 0, length_ticks: None, steps: steps.into(), step_ticks: STEP_TICKS, gate: 0.5, octave: 3 }
    }

    fn notes(steps: &str, key: Option<ScaleKey>) -> Vec<(u8, u32, u32, u8)> {
        line(steps).notes(key).unwrap().notes.iter().map(|n| (n.pitch, n.start_tick, n.length_ticks, n.velocity)).collect()
    }

    const F_MINOR: Option<ScaleKey> = Some(ScaleKey { root: 5, scale: Scale::Minor });

    #[test]
    fn note_names_rests_and_accents_make_notes_on_the_grid() {
        assert_eq!(
            notes("F2 F3! - | Eb3 c#4", None),
            [
                (41, 0, 120, VELOCITY_NORMAL),
                (53, 240, 120, VELOCITY_STRONG),
                (51, 720, 120, VELOCITY_NORMAL),
                (61, 960, 120, VELOCITY_NORMAL)
            ]
        );
        let line = line("F2 - . -").notes(None).unwrap();
        assert_eq!(line.loop_ticks, 4 * STEP_TICKS, "the length is the steps, rests at the end included");
        assert_eq!(notes("Cb4 B#3 C-1 G9", None).iter().map(|n| n.0).collect::<Vec<_>>(), [59, 60, 0, 127]);
    }

    #[test]
    fn holds_lengthen_a_note_and_the_gate_cuts_its_last_step() {
        assert_eq!(notes("C3 _ _ -", None), [(48, 0, 2 * 240 + 120, VELOCITY_NORMAL)]);
        let full = NewStepClip { gate: 1.0, ..line("C3 D3") };
        assert_eq!(full.notes(None).unwrap().notes.iter().map(|n| n.length_ticks).collect::<Vec<_>>(), [240, 240]);
    }

    #[test]
    fn a_slide_overlaps_the_next_note_and_ties_the_same_pitch() {
        // Into another pitch: the whole step plus a short overlap (at most half of the next note's gate).
        assert_eq!(notes("F2~ Ab2", None), [(41, 0, 240 + 60, VELOCITY_NORMAL), (44, 240, 120, VELOCITY_NORMAL)]);
        // Into the same pitch: one note (a tie) whose end follows the last tied step: here it slides on into G2.
        assert_eq!(notes("F2~ F2~ _ G2 -", None), [(41, 0, 3 * 240 + 60, VELOCITY_NORMAL), (43, 720, 120, VELOCITY_NORMAL)]);
        assert_eq!(notes("F2~ F2!", None), [(41, 0, 240 + 120, VELOCITY_NORMAL)], "the tied step neither retriggers nor accents");
        // Before a rest or at the end of the line: all its steps, no overlap.
        assert_eq!(
            notes("F2~ - F2 F2~", None),
            [(41, 0, 240, VELOCITY_NORMAL), (41, 480, 120, VELOCITY_NORMAL), (41, 720, 240, VELOCITY_NORMAL)]
        );
    }

    #[test]
    fn scale_degrees_count_in_the_project_key() {
        assert_eq!(notes("1 3 5 8", F_MINOR).iter().map(|n| n.0).collect::<Vec<_>>(), [53, 56, 60, 65]);
        assert_eq!(notes("5- 1+ 2++ 7", F_MINOR).iter().map(|n| n.0).collect::<Vec<_>>(), [48, 65, 79, 63]);
        let low = NewStepClip { octave: 1, ..line("1") };
        assert_eq!(low.notes(F_MINOR).unwrap().notes[0].pitch, 29, "degree 1 sits in the given octave");
        let pentatonic = Some(ScaleKey { root: 0, scale: Scale::PentatonicMinor });
        assert_eq!(notes("6", pentatonic)[0].0, 60, "past the scale's degrees means the next octave");
    }

    #[test]
    fn mistakes_are_registered_codes() {
        let error = |steps: &str, key: Option<ScaleKey>| line(steps).notes(key).unwrap_err();
        assert!(error("F2 H2", None).is(codes::BAD_STEP));
        assert!(error("F2 5x", F_MINOR).is(codes::BAD_STEP));
        assert!(error("0", F_MINOR).is(codes::BAD_STEP));
        assert!(error("F2,", None).is(codes::BAD_STEP));
        assert!(error("_ F2", None).is(codes::HOLD_WITHOUT_NOTE));
        assert!(error("F2 - _", None).is(codes::HOLD_WITHOUT_NOTE));
        assert!(error(" | ", None).is(codes::NO_STEPS));
        assert!(error("1 3", None).is(codes::DEGREE_WITHOUT_KEY));
        assert!(error("G#9", None).is(notes::PITCH_OUT_OF_RANGE));
        assert!(error("1-----", F_MINOR).is(notes::PITCH_OUT_OF_RANGE));
        for gate in [0.0, 1.5, f64::NAN] {
            assert!(NewStepClip { gate, ..line("F2") }.notes(None).unwrap_err().is(codes::GATE_OUT_OF_RANGE));
        }
        for step_ticks in [0, BAR_TICKS + 1] {
            assert!(NewStepClip { step_ticks, ..line("F2") }.notes(None).unwrap_err().is(codes::STEP_LENGTH_OUT_OF_RANGE));
        }
        let message = error("F2 Q", None).to_string();
        assert!(message.contains("Step 2") && message.contains("\"Q\""), "{message}");
    }

    #[test]
    fn json_takes_only_the_line_and_place_by_default() {
        let clip: NewStepClip = serde_json::from_str(r#"{"track":3,"startTick":3840,"steps":"F2 F3!"}"#).unwrap();
        assert_eq!(
            clip,
            NewStepClip { track: 3, start_tick: 3840, length_ticks: None, steps: "F2 F3!".into(), step_ticks: 240, gate: 0.5, octave: 3 }
        );
    }
}
