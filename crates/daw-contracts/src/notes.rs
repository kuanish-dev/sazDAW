//! Notes inside a note clip and the length of its content (`NoteControl`); musical edits of a group of notes
//! (`NoteTransform`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Clip, ClipId, ContractError, ControllerKind, ControllerPoint, NewNote, Note, NoteId};

/// Maximum number of notes in one clip.
pub const MAX_NOTES_PER_CLIP: usize = 2048;
/// Maximum number of points on one controller lane of a clip.
pub const MAX_CONTROLLER_POINTS: usize = 4096;

/// The notes of a note clip. Job: add, change and remove notes inside a clip; the content length (`loop_ticks`)
/// follows the notes while auto length is on (`NoteContent::auto_length`).
///
/// Rules for every method: an audio clip is an `Invalid`-kind error; an unknown clip or note is `clips::NOT_FOUND` /
/// `notes::NOT_FOUND`. A note is checked by `NewNote::validate`: pitch up to 127, velocity 1..=127, non-zero length,
/// end within the content (with auto length, within `MAX_CLIP_LOOP_TICKS`), otherwise a `Limit`-kind error. A clip
/// holds at most [`MAX_NOTES_PER_CLIP`] notes (`Limit`). Every changing method is one undo step; batch methods are all
/// or nothing.
pub trait NoteControl: Send + Sync {
    fn notes(&self, clip: ClipId) -> Result<Vec<Note>, ContractError>;
    fn add_note(&self, clip: ClipId, note: NewNote) -> Result<Note, ContractError>;
    fn update_note(&self, clip: ClipId, id: NoteId, note: NewNote) -> Result<Note, ContractError>;
    fn remove_note(&self, clip: ClipId, id: NoteId) -> Result<(), ContractError>;
    fn clear_notes(&self, clip: ClipId) -> Result<(), ContractError>;
    fn add_notes(&self, clip: ClipId, notes: Vec<NewNote>) -> Result<Vec<Note>, ContractError>;
    fn update_notes(&self, clip: ClipId, changes: Vec<(NoteId, NewNote)>) -> Result<Vec<Note>, ContractError>;
    fn remove_notes(&self, clip: ClipId, ids: Vec<NoteId>) -> Result<(), ContractError>;
    /// Replaces all notes of the clip; returns their count.
    fn replace_notes(&self, clip: ClipId, notes: Vec<NewNote>) -> Result<usize, ContractError>;
    /// Sets the content length by hand (turns auto length off): a multiple of `STEP_TICKS`, from one step to
    /// `MAX_CLIP_LOOP_TICKS` (otherwise `Limit`). Notes starting past the new length are removed, longer ones are cut
    /// at it; the clip phase (`offset_ticks`) wraps into the new length.
    fn set_loop_ticks(&self, clip: ClipId, loop_ticks: u32) -> Result<(), ContractError>;
    /// Replaces the points of the `kind` controller lane (creates the lane if missing; empty `points` removes it).
    /// Points are sorted by tick; of points with the same tick the last in the list wins. A CC number above
    /// `MAX_CC_NUMBER` (`notes::CC_NUMBER_OUT_OF_RANGE`) or a tick not below the content length (with auto length,
    /// `MAX_CLIP_LOOP_TICKS`) is a `Limit`-kind error; a non-finite value or one outside `ControllerKind::range` is
    /// `Invalid`; more than [`MAX_CONTROLLER_POINTS`] points is `Limit`. One undo step. Auto length counts points like
    /// notes; a manual length (`set_loop_ticks`) drops points beyond it.
    fn set_controller_lane(&self, clip: ClipId, kind: ControllerKind, points: Vec<ControllerPoint>) -> Result<(), ContractError>;
    /// A musical edit of a group of notes as one undo step (see [`NoteTransform`]): `ids` picks the notes (an unknown
    /// id is `notes::NOT_FOUND`, an empty list is `Invalid`), `None` means all notes of the clip. Returns the group's
    /// notes after the edit (in content order); a clip with no notes gives an empty list and no undo step. On a
    /// parameter error nothing changes.
    fn transform_notes(&self, clip: ClipId, ids: Option<Vec<NoteId>>, transform: NoteTransform) -> Result<Vec<Note>, ContractError>;
    /// "Make something similar but different": a copy of the clip right after it (like `ClipControl::duplicate_clip`,
    /// with the same placement and clip-limit errors) whose notes get a [`Variation`] (`ids` are notes of the source
    /// clip; the copy has the same ones). Copy and variation are one undo step, all or nothing; the copy is created
    /// even if the variation changed nothing. Returns the new clip.
    fn vary_to_new_clip(&self, clip: ClipId, ids: Option<Vec<NoteId>>, variation: Variation) -> Result<Clip, ContractError>;
}

/// Largest note start spread for humanize (`Humanize::timing_ticks`): a quarter note.
pub const MAX_HUMANIZE_TICKS: u32 = crate::PPQ;

/// Musical edits of a group of notes (`NoteControl::transform_notes`). Times are relative to the clip content start,
/// so the quantize grid counts from there. Timing edits never push notes out of the content: with a manual length, a
/// note that would run past the end is shortened (to one tick at least) and moved back; a start never goes below 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteTransform {
    /// Pulls note starts towards a grid of `grid` ticks (1..=`MAX_CLIP_LOOP_TICKS`) by `strength` (0..=1: 1 is exactly
    /// on the line, 0.5 halfway). With `ends`, note ends move the same way (no closer than one grid step to the start);
    /// otherwise the length is kept. Out of range is a `Limit`- or `Invalid`-kind error.
    Quantize { grid: u32, strength: f64, ends: bool },
    /// Randomly shifts the start by ±`timing_ticks` (up to [`MAX_HUMANIZE_TICKS`]) and velocity by ±`velocity` (up to
    /// 126; result 1..=127); the length is kept. The same `seed` on the same notes gives the same result; `None` means
    /// a random seed.
    Humanize { timing_ticks: u32, velocity: u8, seed: Option<u32> },
    /// Shifts pitch by `semitones` (−127..=127). If any note would leave 0..=127, it is an `Invalid`-kind error with
    /// that note's id and nothing changes (no clamping to the edge: a chord would silently collapse).
    Transpose { semitones: i32 },
    /// Moves each note to the nearest degree of `scale` with tonic `root` (0..=11, 0 is C); ties go down. The degree is
    /// searched within 0..=127.
    FitToScale { root: u8, scale: Scale },
    /// Legato: each note is stretched (or shortened) to the start of the next note of the group at any pitch; notes
    /// with the same start (a chord) all stretch to the same next start; the last ones stay unchanged.
    Legato,
    /// Groove from a template (see [`GrooveTemplate`]): by its place on the sixteenth grid (the nearest line from the
    /// content start), each note is shifted by the template's offset for that step and its velocity is scaled by the
    /// step's multiplier; `amount` (0..=1) is how much of the offset and multiplier applies. The offset is added to the
    /// current start: straight notes get exactly the template, and applying the groove again adds it again (this edits
    /// notes, it is not a playback setting; undo restores the previous state). The length is kept; velocity stays
    /// within 1..=127. Out-of-range parameters are an `Invalid`- or `Limit`-kind error.
    Groove { template: GrooveTemplate, amount: f64 },
    /// Groove extracted from another note clip `source` (it may be this one): for each sixteenth of the bar (the bar of
    /// the time signature at the source clip's start), the average offset of its note starts from the line and the
    /// average velocity relative to the clip's average; applied like [`NoteTransform::Groove`]. A source offset larger
    /// than half a sixteenth belongs to the neighbouring line (so heavy eighth swing, above 62 %, is not captured). A
    /// missing clip is `clips::NOT_FOUND`; an audio clip or a clip without notes is `Invalid`.
    GrooveFromClip { source: ClipId, amount: f64 },
    /// A variation of the pattern (see [`Variation`]): the only edit that removes and adds notes. Returns the remaining
    /// notes of the group (in content order), then the added ones.
    Vary(Variation),
}

/// A groove template: a table for one beat of four sixteenths, with a start offset in ticks and a velocity multiplier
/// in percent for each. JSON: `{ "type": "swing", "grid": "eighth", "percent": 58 }`,
/// `{ "type": "laidBack", "ticks": 20 }`, `{ "type": "pushed", "ticks": 20 }`, `{ "type": "accent", "percent": 120 }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GrooveTemplate {
    /// Swing: the second eighth of a beat (`grid: eighth`) or every second sixteenth (`sixteenth`, MPC swing) lands at
    /// `percent` % of the pair instead of 50 (50..=75); with eighth swing the sixteenths in between move half as much.
    Swing { grid: SwingGrid, percent: u8 },
    /// Laid back: everything off the beat is `ticks` later (up to `MAX_GROOVE_SHIFT_TICKS`).
    LaidBack { ticks: u32 },
    /// Pushed: everything off the beat is `ticks` earlier (up to `MAX_GROOVE_SHIFT_TICKS`).
    Pushed { ticks: u32 },
    /// Beat accent: notes on the beat get `percent` % velocity (100..=200); the rest stay unchanged.
    Accent { percent: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SwingGrid {
    Eighth,
    Sixteenth,
}

/// Largest laid-back / pushed shift: half a sixteenth; any further and the note would belong to the next line.
pub const MAX_GROOVE_SHIFT_TICKS: u32 = crate::STEP_TICKS / 2;

/// A pattern variation (`NoteTransform::Vary`, `NoteControl::vary_to_new_clip`). All fractions are per-note
/// probabilities. Rules: notes on the first beat of a bar (the time signature's bar at the clip start) never move or
/// change pitch and are removed only when `density` < −0.5; new notes take pitch, velocity and length from the group's
/// notes and land on free sixteenths within the content length; pitches move to a neighbouring degree of `scale` (with
/// no scale, to a neighbouring pitch among the group's); notes of the same pitch never overlap (a change that would
/// overlap is skipped). The same seed on the same notes gives the same result; no `seed` means random. All zeros
/// change nothing. Out of range is an `Invalid`-kind error; more than `MAX_NOTES_PER_CLIP` notes is `Limit`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Variation {
    pub seed: Option<u32>,
    /// −1..=1: below zero, the fraction of notes to remove; above zero, how many to add (1 doubles the notes).
    pub density: f64,
    /// 0..=1: fraction of notes moving to a neighbouring degree.
    pub pitch: f64,
    /// 0..=1: fraction of notes that shift by a sixteenth, split in half or merge with the next note of the same pitch.
    pub rhythm: f64,
    /// 0..=1: velocity spread (1 means up to ±32).
    pub velocity: f64,
    pub scale: Option<ScaleKey>,
}

/// A scale with its tonic: `root` 0..=11 (0 is C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScaleKey {
    pub root: u8,
    pub scale: Scale,
}

/// A scale for fit-to-scale (`NoteTransform::FitToScale`) and piano roll row highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Scale {
    Major,
    Minor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    HarmonicMinor,
    PentatonicMajor,
    PentatonicMinor,
    Chromatic,
}

impl Scale {
    /// All scales in order: this order gives the option numbers wherever a scale is picked from a list (the Chord
    /// parameter).
    pub const ALL: [Scale; 10] = [
        Scale::Major,
        Scale::Minor,
        Scale::Dorian,
        Scale::Phrygian,
        Scale::Lydian,
        Scale::Mixolydian,
        Scale::HarmonicMinor,
        Scale::PentatonicMajor,
        Scale::PentatonicMinor,
        Scale::Chromatic,
    ];

    /// Scale degrees in semitones from the tonic, ascending.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Scale::PentatonicMajor => &[0, 2, 4, 7, 9],
            Scale::PentatonicMinor => &[0, 3, 5, 7, 10],
            Scale::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        }
    }

    /// Whether `pitch` belongs to the scale with tonic `root`.
    pub fn contains(self, root: u8, pitch: u8) -> bool {
        let degree = (i32::from(pitch) - i32::from(root)).rem_euclid(12) as u8;
        self.intervals().contains(&degree)
    }
}
