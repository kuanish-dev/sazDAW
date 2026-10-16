//! `notes.*` methods: thin adapters over `NoteControl` (notes inside a note clip and musical edits of a group of notes).

use daw_contracts::{ClipId, ControllerKind, ControllerPoint, GrooveTemplate, NewNote, NoteId, NoteTransform, Scale, Variation};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct ControllerLaneParams {
    clip: ClipId,
    kind: ControllerKind,
    points: Vec<ControllerPoint>,
}

#[derive(Deserialize, JsonSchema)]
struct Add {
    clip: ClipId,
    note: NewNote,
}

#[derive(Deserialize, JsonSchema)]
struct Update {
    clip: ClipId,
    id: NoteId,
    note: NewNote,
}

#[derive(Deserialize, JsonSchema)]
struct Remove {
    clip: ClipId,
    id: NoteId,
}

#[derive(Deserialize, JsonSchema)]
struct Clear {
    clip: ClipId,
}

#[derive(Deserialize, JsonSchema)]
struct Many {
    clip: ClipId,
    notes: Vec<NewNote>,
}

#[derive(Deserialize, JsonSchema)]
struct Change {
    id: NoteId,
    note: NewNote,
}

#[derive(Deserialize, JsonSchema)]
struct UpdateMany {
    clip: ClipId,
    changes: Vec<Change>,
}

#[derive(Deserialize, JsonSchema)]
struct RemoveMany {
    clip: ClipId,
    ids: Vec<NoteId>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SetLoopTicks {
    clip: ClipId,
    loop_ticks: u32,
}

/// Params of musical edits: `ids` are the notes to edit; without it, all notes of the clip.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Quantize {
    clip: ClipId,
    ids: Option<Vec<NoteId>>,
    grid: u32,
    #[serde(default = "full_strength")]
    strength: f64,
    #[serde(default)]
    ends: bool,
}

fn full_strength() -> f64 {
    1.0
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Humanize {
    clip: ClipId,
    ids: Option<Vec<NoteId>>,
    timing_ticks: u32,
    velocity: u8,
    seed: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
struct Transpose {
    clip: ClipId,
    ids: Option<Vec<NoteId>>,
    semitones: i32,
}

#[derive(Deserialize, JsonSchema)]
struct FitToScale {
    clip: ClipId,
    ids: Option<Vec<NoteId>>,
    root: u8,
    scale: Scale,
}

#[derive(Deserialize, JsonSchema)]
struct Legato {
    clip: ClipId,
    ids: Option<Vec<NoteId>>,
}

#[derive(Deserialize, JsonSchema)]
struct Groove {
    clip: ClipId,
    ids: Option<Vec<NoteId>>,
    template: GrooveTemplate,
    #[serde(default = "full_strength")]
    amount: f64,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GrooveFromClip {
    clip: ClipId,
    ids: Option<Vec<NoteId>>,
    source_clip: ClipId,
    #[serde(default = "full_strength")]
    amount: f64,
}

/// Variation: the `Variation` fields (`seed?`, `density`, `pitch`, `rhythm`, `velocity`, `scale?`) next to `clip` and `ids`.
#[derive(Deserialize, JsonSchema)]
struct Vary {
    clip: ClipId,
    ids: Option<Vec<NoteId>>,
    #[serde(flatten)]
    variation: Variation,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "notes.add",
        "Adds a note to a clip: MIDI pitch, start and length in ticks within the clip, velocity 1–127.",
        |c, p: Add| c.notes.add_note(p.clip, p.note),
    );
    table.add("notes.update", "Edits a note.", |c, p: Update| c.notes.update_note(p.clip, p.id, p.note));
    table.add("notes.remove", "Removes a note.", |c, p: Remove| c.notes.remove_note(p.clip, p.id));
    table.add("notes.clear", "Removes all notes of a clip.", |c, p: Clear| c.notes.clear_notes(p.clip));
    table.add("notes.addMany", "Adds many notes to a clip in one step.", |c, p: Many| c.notes.add_notes(p.clip, p.notes));
    table.add("notes.updateMany", "Edits several notes in one step.", |c, p: UpdateMany| {
        let changes = p.changes.into_iter().map(|c| (c.id, c.note)).collect();
        c.notes.update_notes(p.clip, changes)
    });
    table.add("notes.removeMany", "Removes several notes in one step.", |c, p: RemoveMany| c.notes.remove_notes(p.clip, p.ids));
    table.add("notes.replace", "Replaces all notes of a clip with a new list.", |c, p: Many| c.notes.replace_notes(p.clip, p.notes));
    table.add(
        "notes.setLoopTicks",
        "Sets the length of a clip's content in ticks (the pattern loops if the clip is longer).",
        |c, p: SetLoopTicks| c.notes.set_loop_ticks(p.clip, p.loop_ticks),
    );
    table.add(
        "notes.setControllerLane",
        "Sets a MIDI controller lane of a note clip (replaces or creates it; empty points removes it): kind is { type: \"cc\", number: \
         0–119 } (1 modulation, 7 volume, 64 sustain), { type: \"pitchBend\" } or { type: \"aftertouch\" }; points are \
         [{ tick, value }], tick from the clip start (as for notes), value 0–1 (pitch bend −1…1, 0 is the center). A value holds until \
         the next point (as a step); the lane loops together with the notes. One undo step.",
        |c, p: ControllerLaneParams| c.notes.set_controller_lane(p.clip, p.kind, p.points),
    );
    register_transforms(table);
}

/// Musical edits of a group of notes: each is one undo step and returns the group's notes after the edit.
fn register_transforms(table: &mut RpcTable) {
    table.add(
        "notes.quantize",
        "Quantizes a clip's notes (ids selects them, all without ids): the start is pulled toward the nearest grid line of grid ticks \
         (240 a sixteenth, 480 an eighth, 960 a quarter; the grid starts at the clip start) by the fraction strength 0–1 (1 by default, \
         exactly onto the line); ends: true does the same for note ends, otherwise length is kept. Notes stay inside the clip content.",
        |c, p: Quantize| {
            c.notes.transform_notes(p.clip, p.ids, NoteTransform::Quantize { grid: p.grid, strength: p.strength, ends: p.ends })
        },
    );
    table.add(
        "notes.humanize",
        "Humanizes notes (ids selects them, all without ids): a random start shift of ±timingTicks ticks (up to 960, length \
         is kept, start not below 0) and velocity of ±velocity (up to 126, result 1–127). The same seed gives the same result; \
         without seed it is new every time.",
        |c, p: Humanize| {
            let transform = NoteTransform::Humanize { timing_ticks: p.timing_ticks, velocity: p.velocity, seed: p.seed };
            c.notes.transform_notes(p.clip, p.ids, transform)
        },
    );
    table.add(
        "notes.transpose",
        "Transposes notes (ids selects them, all without ids) by semitones (12 an octave up, −12 down). \
         If any note would leave the pitch range 0–127, returns an error and nothing changes.",
        |c, p: Transpose| c.notes.transform_notes(p.clip, p.ids, NoteTransform::Transpose { semitones: p.semitones }),
    );
    table.add(
        "notes.fitToScale",
        "Fits notes to a scale (ids selects them, all without ids): each note moves to the nearest degree of the scale `scale` \
         from the tonic root (0 is C, 1 is C# … 11 is B); at equal distance, down. Scales: major, minor, dorian, phrygian, \
         lydian, mixolydian, harmonicMinor, pentatonicMajor, pentatonicMinor, chromatic.",
        |c, p: FitToScale| c.notes.transform_notes(p.clip, p.ids, NoteTransform::FitToScale { root: p.root, scale: p.scale }),
    );
    table.add(
        "notes.legato",
        "Makes notes legato (ids selects them, all without ids): each note is stretched or shortened to the start of the next note \
         of this group in time, of any pitch (a chord: all its notes to the one next start); the last notes don't change.",
        |c, p: Legato| c.notes.transform_notes(p.clip, p.ids, NoteTransform::Legato),
    );
    register_groove(table);
}

/// Groove and pattern variations: they edit the notes (not a playback setting); `history.undo` brings the old ones back.
fn register_groove(table: &mut RpcTable) {
    table.add(
        "notes.groove",
        "Applies a groove (ids selects notes, all without ids): each note, by its nearest sixteenth, shifts by the template's offset \
         and scales its velocity by the template's factor; amount 0–1 (1 by default) is how much. template: { type: \"swing\", \
         grid: \"eighth\"|\"sixteenth\", percent: 50–75 } (eighth or sixteenth swing, as on an MPC: 54, 58, 62, 66, 71), \
         { type: \"laidBack\", ticks: 0–120 } (everything off the beat later — laid back), { type: \"pushed\", ticks } (earlier — \
         pushed), { type: \"accent\", percent: 100–200 } (notes on beats louder). The offset adds to the current \
         position: straight notes get exactly the template (quantize uneven ones first). One undo step.",
        |c, p: Groove| c.notes.transform_notes(p.clip, p.ids, NoteTransform::Groove { template: p.template, amount: p.amount }),
    );
    table.add(
        "notes.grooveFromClip",
        "Extracts the groove from the note clip sourceClip (average deviation from the grid and average velocity on each sixteenth of a bar) \
         and applies it to the notes of clip (ids selects them, all without ids) with fraction amount 0–1 (1 by default), like notes.groove. \
         A deviation in the source beyond 120 ticks belongs to the neighbouring sixteenth. One undo step.",
        |c, p: GrooveFromClip| {
            c.notes.transform_notes(p.clip, p.ids, NoteTransform::GrooveFromClip { source: p.source_clip, amount: p.amount })
        },
    );
    table.add("notes.vary", VARY_HELP, |c, p: Vary| c.notes.transform_notes(p.clip, p.ids, NoteTransform::Vary(p.variation)));
    table.add(
        "notes.varyToNewClip",
        "\"Make a similar but different one\": copies the clip right after itself; the copy's notes are a variation with the same params as \
         notes.vary (ids are notes of the source clip). The copy and the variation are one undo step. Returns the new clip.",
        |c, p: Vary| c.notes.vary_to_new_clip(p.clip, p.ids, p.variation),
    );
}

const VARY_HELP: &str = "Varies a pattern (ids selects notes, all without ids); all params are optional (0 by default): \
    density −1…1 (below 0, the fraction of notes removed; above 0, how many to add: 1 doubles them; new notes take pitch, \
    velocity and length from the pattern's notes and land on free sixteenths), pitch 0–1 (the fraction of notes moved to a neighbouring degree of scale \
    { root: 0–11, scale } or, without a scale, to a neighbouring pitch already in use), rhythm 0–1 (the fraction of notes that shift by \
    a sixteenth, split in half or merge with the next one), velocity 0–1 (velocity spread up to ±32), seed: the same \
    seed repeats the result, without it the result is new every time. Notes on the first beat of a bar don't move (they are removed only when \
    density < −0.5); notes of the same pitch don't overlap. Returns the remaining notes of the group, then the added ones. One undo \
    step.";
