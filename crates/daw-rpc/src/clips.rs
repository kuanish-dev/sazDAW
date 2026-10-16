//! `clips.*` methods: thin adapters over `ClipControl`, `TakeControl` and `ClipProcessing`. Full clips come inside
//! `project.get`.

use daw_contracts::{ClipEdit, ClipId, CompSegment, DEFAULT_NORMALIZE_PEAK_DB, NewNoteClip, SampleId, TrackId};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;
use crate::params::Id;

#[derive(Deserialize, JsonSchema)]
struct AddNoteMany {
    clips: Vec<NewNoteClip>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AddAudio {
    track: TrackId,
    start_tick: u32,
    sample: SampleId,
}

#[derive(Deserialize, JsonSchema)]
struct Take {
    id: ClipId,
    /// The take's sample (from the clip's `content.takes`).
    take: SampleId,
}

#[derive(Deserialize, JsonSchema)]
struct Comp {
    id: ClipId,
    /// Comp segments: `startTicks` from the clip start (the first is 0, ascending), `take` is a sample from `content.takes`.
    segments: Vec<CompSegment>,
}

#[derive(Deserialize, JsonSchema)]
struct TakeRange {
    id: ClipId,
    /// Start and end of the piece in ticks from the clip start (clamped to the clip).
    from: u32,
    to: u32,
    /// The take's sample (from the clip's `content.takes`).
    take: SampleId,
}

#[derive(Deserialize, JsonSchema)]
struct EditClip {
    id: ClipId,
    edit: ClipEdit,
}

#[derive(Deserialize, JsonSchema)]
struct EditMany {
    changes: Vec<EditClip>,
}

#[derive(Deserialize, JsonSchema)]
struct Ids {
    ids: Vec<ClipId>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Copy {
    ids: Vec<ClipId>,
    at_tick: u32,
}

#[derive(Deserialize, JsonSchema)]
struct Split {
    id: ClipId,
    tick: u32,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct TrimStart {
    id: ClipId,
    start_tick: u32,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Normalize {
    ids: Vec<ClipId>,
    /// Target peak in dBFS (−60…0); −0.3 when omitted.
    peak_db: Option<f32>,
}

#[derive(Deserialize, JsonSchema)]
struct Loop {
    region: Option<(u32, u32)>,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "clips.addNote",
        "Creates a note clip on an instrument track in one undo step: start and length in ticks (one bar without a length), \
         with notes `notes` (time from the clip start) and the length of the repeated content `loopTicks` (a multiple of 240; \
         fitted to the notes when omitted). Without notes the clip is empty. If any note is invalid, the clip is not created.",
        |c, p: NewNoteClip| c.clips.add_note_clip(p),
    );
    table.add(
        "clips.addNoteMany",
        "Creates several note clips in one call and one undo step (handy for building song sections): each one is as in \
         clips.addNote ({ track, startTick, lengthTicks?, notes?, loopTicks? }). All or nothing: an error in any clip \
         (the place is taken, including by another clip from the list, or an invalid note) creates none. Returns the clip ids \
         in order.",
        |c, p: AddNoteMany| Ok(c.clips.add_note_clips(p.clips)?.into_iter().map(|clip| clip.id).collect::<Vec<_>>()),
    );
    table.add(
        "clips.addAudio",
        "Creates an audio clip from a project sample on an audio track (its length is the whole sound at the current tempo).",
        |c, p: AddAudio| c.clips.add_audio_clip(p.track, p.start_tick, p.sample),
    );
    table.add(
        "clips.edit",
        "Edits a clip: track, start, length, gain (linear, 0…4), fades in ticks and their shapes `fadeInShape`/`fadeOutShape` \
         (linear, equalPower, sCurve, exponential) (pass only what changes). Audio clip: \
         `pitchSemitones` changes pitch without changing length (−24…24, fractional); `warp: { sourceBpm }` fits it to the project tempo \
         (the sound was recorded at sourceBpm and stretches with the tempo; the clip length is in beats of that tempo), \
         `warp: {}` or `sourceBpm: null` guesses the tempo from the file name or length, `warp: null` turns it off. \
         Audio clips on one track may overlap at the edges — the overlap plays as a crossfade.",
        |c, p: EditClip| c.clips.edit_clip(p.id, p.edit),
    );
    table.add(
        "clips.editMany",
        "Edits several clips in one undo step (same fields as clips.edit; placement is checked on the final result).",
        |c, p: EditMany| c.clips.edit_clips(p.changes.into_iter().map(|e| (e.id, e.edit)).collect()),
    );
    table.add("clips.removeMany", "Removes several clips in one step.", |c, p: Ids| c.clips.remove_clips(p.ids));
    table.add("clips.copy", "Copies a group of clips to a position, keeping the distances between them.", |c, p: Copy| {
        c.clips.copy_clips(p.ids, p.at_tick)
    });
    table.add("clips.split", "Splits a clip at a point (a tick inside the clip); returns the right part.", |c, p: Split| {
        c.clips.split_clip(p.id, p.tick)
    });
    table.add(
        "clips.trimStart",
        "Moves a clip's left edge to `startTick` keeping its end; the content stays in place on the timeline (an audio clip \
         starts later or earlier in its file, a note clip's loop phase turns). An audio clip cannot start before its file \
         start: the edge stops there. Returns the clip.",
        |c, p: TrimStart| c.clips.trim_start(p.id, p.start_tick),
    );
    table.add(
        "clips.normalize",
        "Normalizes audio clips without touching the audio: sets each clip's gain so the loudest sample it plays reaches \
         `peakDb` dBFS (default −0.3, at most ×4 gain; silent clips keep their gain). One undo step; returns the clips.",
        |c, p: Normalize| c.clip_processing.normalize(p.ids, p.peak_db.unwrap_or(DEFAULT_NORMALIZE_PEAK_DB)),
    );
    table.add(
        "clips.reverse",
        "Reverses an audio clip: writes a reversed copy of its sample into the project folder (the project must be saved) \
         and makes the clip play the same piece backwards in the same place. One undo step; returns the clip.",
        |c, p: Id| c.clip_processing.reverse(p.id),
    );
    table.add("clips.duplicate", "Copies a clip right after itself.", |c, p: Id| c.clips.duplicate_clip(p.id));
    table.add("clips.remove", "Removes a clip.", |c, p: Id| c.clips.remove_clip(p.id));
    table.add("clips.setLoopRegion", "Sets the playback loop region in ticks (null removes it).", |c, p: Loop| {
        c.clips.set_loop_region(p.region)
    });
    table.add(
        "clips.setActiveTake",
        "Makes the whole audio clip play another of its takes (a sample from content.takes; the comp is cleared); the clip's \
         place, length, warp and fades stay the same.",
        |c, p: Take| c.takes.set_active_take(p.id, p.take),
    );
    table.add(
        "clips.removeTake",
        "Removes a take from an audio clip (the sample stays in the project); if it was active, the clip plays the last remaining one.",
        |c, p: Take| c.takes.remove_take(p.id, p.take),
    );
    table.add(
        "clips.setComp",
        "Sets the whole take comp of an audio clip (comping): from startTicks to the next segment the take `take` plays. The first \
         segment starts at 0, ascending, inside the clip; adjacent segments of the same take merge; an empty list makes the whole \
         clip play the active take. Joins play a short crossfade (10 ms). Example: { id, segments: [{ startTicks: 0, take: 5 }, { startTicks: 1920, take: 6 }] }.",
        |c, p: Comp| c.takes.set_comp(p.id, p.segments),
    );
    table.add(
        "clips.chooseTakeRange",
        "Makes the piece [from, to) of an audio clip (ticks from the clip start) play take `take`, the rest stays as it was; returns \
         the clip with its comp in content.comp. To make the whole clip play one take again, use clips.setActiveTake.",
        |c, p: TakeRange| c.takes.choose_take_range(p.id, p.from, p.to, p.take),
    );
}
