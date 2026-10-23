//! Musical edits of a group of notes (`NoteTransform`) as pure functions over `NoteContent`: quantize, humanize,
//! transpose, fit to scale, legato; groove lives in `groove.rs`, pattern variation in `variation.rs`.
//! `ProjectNotes::transform_notes` calls them; the frontend mock repeats them exactly
//! (`app/src/pianoroll/noteTransforms.ts`), so the integer rounding and the random generator here are simple and portable.

use daw_contracts::codes::notes;
use daw_contracts::{
    ContractError, MAX_CLIP_LOOP_TICKS, MAX_HUMANIZE_TICKS, MAX_NOTES_PER_CLIP, NewNote, Note, NoteContent, NoteId, NoteTransform, Scale,
    Variation,
};

use crate::groove::{GrooveTable, apply_groove, check_amount, check_template, template_table};
use crate::note_rules::{create_notes, note_limit};
use crate::variation;

/// The largest humanize velocity spread: a note at 1 or 127 can still move across the whole velocity range.
const MAX_HUMANIZE_VELOCITY: u8 = 126;

/// What an edit needs besides the clip's notes: the bar of the time signature at the clip start (the anchor of a
/// variation) and the groove of the source clip (`GrooveFromClip`; `ProjectNotes` extracts it, since another clip is
/// not the business of a pure function over one content).
pub(crate) struct Surroundings {
    pub bar: u32,
    pub groove: Option<GrooveTable>,
}

/// Applies the edit to notes `ids` (`None` — all) and returns the group's notes after it (in content order; for a
/// variation, followed by the added ones).
pub(crate) fn transform_notes(
    content: &mut NoteContent,
    ids: Option<&[NoteId]>,
    transform: NoteTransform,
    around: &Surroundings,
) -> Result<Vec<Note>, ContractError> {
    check(transform)?;
    let positions = targets(content, ids)?;
    let group: Vec<Note> = positions.iter().map(|&i| content.notes[i]).collect();
    let limit = note_limit(content);
    let changed = match transform {
        NoteTransform::Quantize { grid, strength, ends } => group.iter().map(|n| quantize(*n, grid, strength, ends, limit)).collect(),
        NoteTransform::Humanize { timing_ticks, velocity, seed } => humanize(&group, timing_ticks, velocity, seed.unwrap_or(0), limit),
        NoteTransform::Transpose { semitones } => group.iter().map(|n| transpose(*n, semitones)).collect::<Result<_, _>>()?,
        NoteTransform::FitToScale { root, scale } => {
            group.iter().map(|n| Note { pitch: nearest_in_scale(n.pitch, root, scale), ..*n }).collect()
        }
        NoteTransform::Legato => legato(&group),
        NoteTransform::Groove { template, amount } => apply_groove(&group, &template_table(template), amount, limit),
        NoteTransform::GrooveFromClip { amount, .. } => {
            let table = around.groove.as_ref().ok_or_else(|| notes::GROOVE_NOT_EXTRACTED.err())?;
            apply_groove(&group, table, amount, limit)
        }
        NoteTransform::Vary(variation) => return vary_notes(content, &positions, &variation, around.bar),
    };
    for (&i, note) in positions.iter().zip(&changed) {
        to_new(*note).validate(limit)?;
        content.notes[i] = *note;
    }
    Ok(changed)
}

fn check(transform: NoteTransform) -> Result<(), ContractError> {
    match transform {
        NoteTransform::Quantize { grid, strength, .. } => {
            if grid == 0 || grid > MAX_CLIP_LOOP_TICKS {
                return Err(notes::QUANTIZE_GRID_OUT_OF_RANGE.with("grid", grid).with("max", MAX_CLIP_LOOP_TICKS));
            }
            if !(0.0..=1.0).contains(&strength) {
                return Err(notes::QUANTIZE_STRENGTH_OUT_OF_RANGE.with("strength", strength));
            }
        }
        NoteTransform::Humanize { timing_ticks, velocity, .. } => {
            if timing_ticks > MAX_HUMANIZE_TICKS {
                return Err(notes::HUMANIZE_TIMING_OUT_OF_RANGE.with("ticks", timing_ticks).with("max", MAX_HUMANIZE_TICKS));
            }
            if velocity > MAX_HUMANIZE_VELOCITY {
                return Err(notes::HUMANIZE_VELOCITY_OUT_OF_RANGE.with("velocity", velocity).with("max", MAX_HUMANIZE_VELOCITY));
            }
        }
        NoteTransform::Transpose { semitones } if semitones.unsigned_abs() > 127 => {
            return Err(notes::TRANSPOSE_OUT_OF_RANGE.with("semitones", semitones));
        }
        NoteTransform::FitToScale { root, .. } if root > 11 => return Err(notes::SCALE_ROOT_OUT_OF_RANGE.with("root", root)),
        NoteTransform::Groove { template, amount } => {
            check_template(template)?;
            check_amount(amount)?;
        }
        NoteTransform::GrooveFromClip { amount, .. } => check_amount(amount)?,
        NoteTransform::Vary(variation) => variation::check(&variation)?,
        _ => {}
    }
    Ok(())
}

/// Positions of the group's notes in the content, in order; an unknown id is `NT-001`, an empty list is `NT-111`.
fn targets(content: &NoteContent, ids: Option<&[NoteId]>) -> Result<Vec<usize>, ContractError> {
    let Some(ids) = ids else {
        return Ok((0..content.notes.len()).collect());
    };
    if ids.is_empty() {
        return Err(notes::NO_NOTES_SELECTED.err());
    }
    if let Some(missing) = ids.iter().find(|id| !content.notes.iter().any(|n| n.id == **id)) {
        return Err(notes::NOT_FOUND.with("id", missing));
    }
    Ok(content.notes.iter().enumerate().filter(|(_, n)| ids.contains(&n.id)).map(|(i, _)| i).collect())
}

fn to_new(note: Note) -> NewNote {
    NewNote { pitch: note.pitch, start_tick: note.start_tick, length_ticks: note.length_ticks, velocity: note.velocity }
}

/// A note starting at `start` with length `length` inside content of length `limit`: the length is at least one tick
/// and at most the content, and whatever sticks out past the end is moved back.
pub(crate) fn fit(note: Note, start: i64, length: i64, limit: u32) -> Note {
    let length = length.clamp(1, i64::from(limit));
    let start = start.clamp(0, i64::from(limit) - length);
    Note { start_tick: start as u32, length_ticks: length as u32, ..note }
}

/// The nearest grid line; exactly halfway goes forward.
fn nearest_line(tick: i64, grid: i64) -> i64 {
    (tick + grid / 2) / grid * grid
}

/// The way from `from` to `to` by the share `strength`, rounded to a tick (halves away from zero, as in the mock).
fn toward(from: i64, to: i64, strength: f64) -> i64 {
    from + ((to - from) as f64 * strength).round() as i64
}

fn quantize(note: Note, grid: u32, strength: f64, ends: bool, limit: u32) -> Note {
    let grid = i64::from(grid);
    let start = i64::from(note.start_tick);
    let end = i64::from(note.end_tick());
    let line = nearest_line(start, grid);
    let new_start = toward(start, line, strength);
    let length = if ends {
        let end_line = nearest_line(end, grid).max(line + grid);
        toward(end, end_line, strength) - new_start
    } else {
        end - start
    };
    fit(note, new_start, length, limit)
}

/// The mulberry32 generator: 32-bit, deterministic, easy to repeat in TypeScript (`Math.imul`).
pub(crate) struct Random(pub(crate) u32);

impl Random {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 0x3D));
        t ^ (t >> 14)
    }

    /// A number from 0 (inclusive) to 1 (exclusive): 32 bits of the generator divided by 2³².
    pub(crate) fn unit(&mut self) -> f64 {
        f64::from(self.next()) / 4_294_967_296.0
    }

    /// An integer from -`range` to `range` inclusive.
    fn offset(&mut self, range: u32) -> i64 {
        i64::from(self.next() % (2 * range + 1)) - i64::from(range)
    }
}

fn humanize(group: &[Note], timing: u32, velocity: u8, seed: u32, limit: u32) -> Vec<Note> {
    let mut random = Random(seed);
    group
        .iter()
        .map(|note| {
            let dt = random.offset(timing);
            let dv = random.offset(u32::from(velocity));
            let velocity = (i64::from(note.velocity) + dv).clamp(1, 127) as u8;
            fit(Note { velocity, ..*note }, i64::from(note.start_tick) + dt, i64::from(note.length_ticks), limit)
        })
        .collect()
}

fn transpose(note: Note, semitones: i32) -> Result<Note, ContractError> {
    let pitch = i32::from(note.pitch) + semitones;
    let pitch = u8::try_from(pitch)
        .ok()
        .filter(|p| *p <= 127)
        .ok_or_else(|| notes::TRANSPOSED_PITCH_OUT_OF_RANGE.with("semitones", semitones).with("id", note.id).with("pitch", note.pitch))?;
    Ok(Note { pitch, ..note })
}

/// The scale pitch nearest to `pitch` within 0..=127; at equal distance the lower one.
fn nearest_in_scale(pitch: u8, root: u8, scale: Scale) -> u8 {
    for distance in 0..12 {
        let below = pitch.checked_sub(distance).filter(|p| scale.contains(root, *p));
        let above = pitch.checked_add(distance).filter(|p| *p <= 127 && scale.contains(root, *p));
        if let Some(found) = below.or(above) {
            return found;
        }
    }
    pitch
}

fn legato(group: &[Note]) -> Vec<Note> {
    let mut starts: Vec<u32> = group.iter().map(|n| n.start_tick).collect();
    starts.sort_unstable();
    starts.dedup();
    group
        .iter()
        .map(|note| match starts.iter().find(|s| **s > note.start_tick) {
            Some(next) => Note { length_ticks: next - note.start_tick, ..*note },
            None => *note,
        })
        .collect()
}

/// Variation: the group's notes change or go away, new ones get content ids and go to the end; all or nothing.
fn vary_notes(content: &mut NoteContent, positions: &[usize], variation: &Variation, bar: u32) -> Result<Vec<Note>, ContractError> {
    let group: Vec<Note> = positions.iter().map(|&i| content.notes[i]).collect();
    let others: Vec<Note> = content.notes.iter().enumerate().filter(|(i, _)| !positions.contains(i)).map(|(_, n)| *n).collect();
    let varied = variation::vary(&group, &others, variation, variation.seed.unwrap_or(0), bar, content.loop_ticks);
    let kept: Vec<Note> = varied.kept.iter().flatten().copied().collect();
    if others.len() + kept.len() + varied.added.len() > MAX_NOTES_PER_CLIP {
        return Err(notes::TOO_MANY_NOTES.with("max", MAX_NOTES_PER_CLIP));
    }
    let limit = note_limit(content);
    for note in &kept {
        to_new(*note).validate(limit)?;
    }
    let created = create_notes(content, varied.added)?;
    let mut changed = varied.kept.into_iter();
    let mut notes = Vec::with_capacity(others.len() + kept.len() + created.len());
    for (i, note) in content.notes.iter().enumerate() {
        if !positions.contains(&i) {
            notes.push(*note);
        } else if let Some(Some(after)) = changed.next() {
            notes.push(after);
        }
    }
    notes.extend(created.iter().copied());
    content.notes = notes;
    Ok(kept.into_iter().chain(created).collect())
}
