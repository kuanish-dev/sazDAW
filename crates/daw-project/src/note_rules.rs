//! Rules of a note clip's content as pure functions over `NoteContent`: checking notes and giving them ids, automatic
//! length, manual length. `ProjectNotes` (editing a clip's notes) and `ProjectClips` (a new clip with notes) share
//! them, so "a clip with notes in one call" behaves exactly like "an empty clip + notes + length".

use daw_contracts::codes::notes;
use daw_contracts::{
    ContractError, ControllerKind, ControllerLane, ControllerPoint, MAX_CLIP_LOOP_TICKS, MAX_CONTROLLER_POINTS, MAX_NOTES_PER_CLIP,
    NewNote, Note, NoteContent, STEP_TICKS,
};

/// Fits an automatic content length to the notes: up to the end of the last used bar of length `bar`, but no shorter
/// than one bar. The content bar is the bar of the time signature at the clip start (`TempoMap::bar_length_at`): the
/// clip has its own grid, and signature changes in the middle of the clip do not cut it.
pub(crate) fn fit_length(content: &mut NoteContent, bar: u32) {
    if !content.auto_length {
        return;
    }
    let bar = bar.max(1);
    let last_note = content.notes.iter().map(Note::end_tick).max().unwrap_or(0);
    // A controller point at tick t occupies the content up to t + 1: otherwise the automatic length would cut it off.
    let last_point = content.controllers.iter().filter_map(|lane| lane.points.last()).map(|p| p.tick + 1).max().unwrap_or(0);
    let last_end = last_note.max(last_point);
    // A non-4/4 bar may not divide the limit evenly: then the content stops at the limit instead of going past it.
    content.loop_ticks = (last_end.div_ceil(bar).max(1) * bar).min(MAX_CLIP_LOOP_TICKS);
}

/// The limit up to which a note may stretch the content.
pub(crate) fn note_limit(content: &NoteContent) -> u32 {
    if content.auto_length { MAX_CLIP_LOOP_TICKS } else { content.loop_ticks }
}

/// Checks the notes and gives them content ids; does not put them into the content.
pub(crate) fn create_notes(content: &mut NoteContent, notes: Vec<NewNote>) -> Result<Vec<Note>, ContractError> {
    let limit = note_limit(content);
    notes
        .into_iter()
        .map(|note| {
            note.validate(limit)?;
            let created = Note::from_new(content.next_note_id, note);
            content.next_note_id += 1;
            Ok(created)
        })
        .collect()
}

/// Replaces all notes of the content; returns their count.
pub(crate) fn replace_notes(content: &mut NoteContent, notes: Vec<NewNote>) -> Result<usize, ContractError> {
    if notes.len() > MAX_NOTES_PER_CLIP {
        return Err(notes::TOO_MANY_NOTES.with("max", MAX_NOTES_PER_CLIP));
    }
    content.notes = create_notes(content, notes)?;
    Ok(content.notes.len())
}

/// A manual content length: a multiple of the step, from one step to `MAX_CLIP_LOOP_TICKS`.
pub(crate) fn check_loop_ticks(loop_ticks: u32) -> Result<(), ContractError> {
    if !(STEP_TICKS..=MAX_CLIP_LOOP_TICKS).contains(&loop_ticks) || !loop_ticks.is_multiple_of(STEP_TICKS) {
        return Err(notes::LOOP_LENGTH_OUT_OF_RANGE.with("ticks", loop_ticks).with("step", STEP_TICKS).with("max", MAX_CLIP_LOOP_TICKS));
    }
    Ok(())
}

/// Sets a checked manual length: the automatic length turns off, notes beyond it are dropped, long ones are trimmed.
pub(crate) fn set_loop_ticks(content: &mut NoteContent, loop_ticks: u32) {
    content.auto_length = false;
    content.loop_ticks = loop_ticks;
    content.notes.retain(|n| n.start_tick < loop_ticks);
    for note in &mut content.notes {
        note.length_ticks = note.length_ticks.min(loop_ticks - note.start_tick);
    }
    for lane in &mut content.controllers {
        lane.points.retain(|p| p.tick < loop_ticks);
    }
    content.controllers.retain(|lane| !lane.points.is_empty());
}

/// Replaces the points of controller lane `kind` (no points removes the lane): checks the kind, the point limit, the
/// ticks (not beyond the content, or beyond `MAX_CLIP_LOOP_TICKS` with the automatic length) and the values
/// (`ControllerKind::range`), sorts by tick; of points with the same tick the last one stays.
pub(crate) fn set_controller_lane(
    content: &mut NoteContent,
    kind: ControllerKind,
    mut points: Vec<ControllerPoint>,
) -> Result<(), ContractError> {
    kind.validate()?;
    if points.len() > MAX_CONTROLLER_POINTS {
        return Err(notes::TOO_MANY_CONTROLLER_POINTS.with("max", MAX_CONTROLLER_POINTS));
    }
    let limit = note_limit(content);
    let (min, max) = kind.range();
    for point in &points {
        if point.tick >= limit {
            return Err(notes::CONTROLLER_TICK_OUT_OF_RANGE.with("tick", point.tick).with("max", limit - 1));
        }
        if !point.value.is_finite() || !(min..=max).contains(&point.value) {
            return Err(notes::CONTROLLER_VALUE_OUT_OF_RANGE.with("value", point.value).with("min", min).with("max", max));
        }
    }
    points.sort_by_key(|p| p.tick);
    points.reverse();
    points.dedup_by_key(|p| p.tick);
    points.reverse();
    content.controllers.retain(|lane| lane.kind != kind);
    if !points.is_empty() {
        content.controllers.push(ControllerLane { kind, points });
    }
    Ok(())
}
