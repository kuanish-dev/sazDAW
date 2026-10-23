//! Pattern variation (`NoteTransform::Vary`) as a pure function over the group's notes: remove and add notes, move
//! pitches to neighbouring steps, shift, split or merge notes, spread velocities. The frontend mock repeats this
//! exactly (`app/src/pianoroll/variation.ts`): the same mulberry32 generator and the same order of random numbers.

use daw_contracts::codes::notes;
use daw_contracts::{ContractError, ErrorCode, NewNote, Note, STEP_TICKS, ScaleKey, Variation};

use crate::note_transforms::Random;

/// How many random numbers each group note draws — always, at any settings: one seed then gives each note the same
/// fate when only one slider changes.
const DRAWS_PER_NOTE: usize = 6;
/// The largest velocity spread at `velocity` = 1.
const VELOCITY_SPREAD: f64 = 32.0;

pub(crate) fn check(variation: &Variation) -> Result<(), ContractError> {
    let share = |value: f64, min: f64, out_of_range: ErrorCode| {
        if (min..=1.0).contains(&value) { Ok(()) } else { Err(out_of_range.with("value", value)) }
    };
    share(variation.density, -1.0, notes::VARIATION_DENSITY_OUT_OF_RANGE)?;
    share(variation.pitch, 0.0, notes::VARIATION_PITCH_OUT_OF_RANGE)?;
    share(variation.rhythm, 0.0, notes::VARIATION_RHYTHM_OUT_OF_RANGE)?;
    share(variation.velocity, 0.0, notes::VARIATION_VELOCITY_OUT_OF_RANGE)?;
    match variation.scale {
        Some(key) if key.root > 11 => Err(notes::SCALE_ROOT_OUT_OF_RANGE.with("root", key.root)),
        _ => Ok(()),
    }
}

/// The result of a variation: the group's notes in order (`None` — removed or merged into a neighbour) and new notes.
pub(crate) struct Varied {
    pub kept: Vec<Option<Note>>,
    pub added: Vec<NewNote>,
}

/// Where notes lie during a variation: the clip's other notes, the group's notes and the added ones. Its job is to
/// answer whether a place is free for a note (one pitch does not overlap, everything stays inside content `span`).
struct Board<'a> {
    others: &'a [Note],
    kept: Vec<Option<Note>>,
    added: Vec<NewNote>,
    span: u32,
    bar: u32,
}

impl Board<'_> {
    /// All notes (pitch, start, end) except group note `skip`.
    fn occupied(&self, skip: Option<usize>) -> Vec<(u8, u32, u32)> {
        let kept = self.kept.iter().enumerate().filter(|(i, _)| Some(*i) != skip).filter_map(|(_, n)| *n);
        let notes = self.others.iter().copied().chain(kept).map(|n| (n.pitch, n.start_tick, n.end_tick()));
        notes.chain(self.added.iter().map(|n| (n.pitch, n.start_tick, n.start_tick + n.length_ticks))).collect()
    }

    fn is_free(&self, note: &Note, skip: usize) -> bool {
        note.end_tick() <= self.span
            && !self
                .occupied(Some(skip))
                .iter()
                .any(|&(pitch, start, end)| pitch == note.pitch && start < note.end_tick() && note.start_tick < end)
    }
}

/// Varies notes `group` among the clip's other notes `others`; `bar` is the bar (notes on its start anchor the
/// pattern), `span` the content length (new and moved notes do not go beyond it).
pub(crate) fn vary(group: &[Note], others: &[Note], variation: &Variation, seed: u32, bar: u32, span: u32) -> Varied {
    let mut random = Random(seed);
    let bar = bar.max(1);
    let mut board = Board { others, kept: group.iter().copied().map(Some).collect(), added: Vec::new(), span, bar };
    let mut pool: Vec<u8> = group.iter().map(|n| n.pitch).collect();
    pool.sort_unstable();
    pool.dedup();

    for i in 0..group.len() {
        let draw: [f64; DRAWS_PER_NOTE] = std::array::from_fn(|_| random.unit());
        let Some(mut note) = board.kept[i] else { continue };
        let downbeat = note.start_tick.is_multiple_of(bar);
        let remove = if downbeat { -variation.density - 0.5 } else { -variation.density };
        if draw[0] < remove {
            board.kept[i] = None;
            continue;
        }
        if !downbeat {
            if draw[1] < variation.pitch
                && let Some(pitch) = neighbour(note.pitch, draw[2] < 0.5, variation.scale, &pool)
            {
                let moved = Note { pitch, ..note };
                if board.is_free(&moved, i) {
                    note = moved;
                }
            }
            if draw[3] < variation.rhythm {
                board.kept[i] = Some(note);
                note = change_rhythm(&mut board, i, note, draw[4]);
            }
        }
        let shift = ((draw[5] * 2.0 - 1.0) * variation.velocity * VELOCITY_SPREAD).round() as i64;
        note.velocity = (i64::from(note.velocity) + shift).clamp(1, 127) as u8;
        board.kept[i] = Some(note);
    }

    if variation.density > 0.0 {
        add_notes(&mut board, group, (variation.density * group.len() as f64).round() as usize, &mut random);
    }
    Varied { kept: board.kept, added: board.added }
}

/// The neighbouring pitch up (`up`) or down: a scale step or the nearest of the group's pitches; at the edge, the
/// other way.
fn neighbour(pitch: u8, up: bool, scale: Option<ScaleKey>, pool: &[u8]) -> Option<u8> {
    let toward = |up: bool| match scale {
        Some(key) => (1..=12_i32)
            .map(|d| i32::from(pitch) + if up { d } else { -d })
            .take_while(|p| (0..=127).contains(p))
            .map(|p| p as u8)
            .find(|p| key.scale.contains(key.root, *p)),
        None if up => pool.iter().copied().find(|p| *p > pitch),
        None => pool.iter().rev().copied().find(|p| *p < pitch),
    };
    toward(up).or_else(|| toward(!up))
}

/// The rhythm of note `i`: `kind` < 0.25 — a sixteenth earlier, < 0.5 — later, < 0.75 — split in half on the grid
/// (if the note is at least two sixteenths long), otherwise merge with the next note of the same pitch starting no
/// more than a sixteenth after its end (a bar anchor is never swallowed). A change that does not fit is skipped.
fn change_rhythm(board: &mut Board, i: usize, note: Note, kind: f64) -> Note {
    let step = STEP_TICKS;
    if kind < 0.5 {
        let start = if kind < 0.25 { note.start_tick.checked_sub(step) } else { Some(note.start_tick + step) };
        let moved = start.map(|start_tick| Note { start_tick, ..note });
        return moved.filter(|m| board.is_free(m, i)).unwrap_or(note);
    }
    if kind < 0.75 {
        if note.length_ticks < 2 * step {
            return note;
        }
        let half = note.length_ticks / 2 / step * step;
        let second = NewNote {
            pitch: note.pitch,
            start_tick: note.start_tick + half,
            length_ticks: note.length_ticks - half,
            velocity: note.velocity,
        };
        board.added.push(second);
        return Note { length_ticks: half, ..note };
    }
    let next = board
        .kept
        .iter()
        .enumerate()
        .filter_map(|(j, n)| n.map(|n| (j, n)))
        .filter(|(j, n)| *j != i && n.pitch == note.pitch && !n.start_tick.is_multiple_of(board.bar))
        .filter(|(_, n)| n.start_tick >= note.end_tick() && n.start_tick - note.end_tick() <= step)
        .min_by_key(|(j, n)| (n.start_tick, *j));
    let Some((j, next)) = next else { return note };
    board.kept[j] = None;
    let merged = Note { length_ticks: next.end_tick() - note.start_tick, ..note };
    if board.is_free(&merged, i) {
        merged
    } else {
        board.kept[j] = Some(next);
        note
    }
}

/// Adds up to `count` notes on free sixteenths (where no group note starts): pitch, velocity and length come from a
/// random group note; the length is cut at the next note of the same pitch and at the content end.
fn add_notes(board: &mut Board, group: &[Note], count: usize, random: &mut Random) {
    let starts: Vec<u32> = board.kept.iter().flatten().map(|n| n.start_tick).chain(board.added.iter().map(|n| n.start_tick)).collect();
    let mut free: Vec<u32> = (0..board.span / STEP_TICKS).map(|s| s * STEP_TICKS).filter(|t| !starts.contains(t)).collect();
    for _ in 0..count {
        let (slot, pick) = (random.unit(), random.unit());
        if free.is_empty() || group.is_empty() {
            break;
        }
        let start = free.remove((slot * free.len() as f64) as usize);
        let model = group[(pick * group.len() as f64) as usize];
        let same: Vec<(u8, u32, u32)> = board.occupied(None).into_iter().filter(|n| n.0 == model.pitch).collect();
        if same.iter().any(|&(_, s, e)| s <= start && start < e) {
            continue;
        }
        let next = same.iter().map(|n| n.1).filter(|s| *s > start).min().unwrap_or(board.span);
        let length = model.length_ticks.min(next - start).min(board.span - start);
        board.added.push(NewNote { pitch: model.pitch, start_tick: start, length_ticks: length, velocity: model.velocity });
    }
}
