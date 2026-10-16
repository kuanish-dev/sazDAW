//! Groove as pure functions (`NoteTransform::Groove` and `GrooveFromClip`): a table on the sixteenth grid — for each
//! step a start offset in ticks and a velocity factor in percent — from a template or taken from a source clip, and
//! applying it to notes. The frontend mock repeats this exactly (`app/src/domain/groove.ts`), so the rounding is simple.

use daw_contracts::codes::notes;
use daw_contracts::{ContractError, GrooveTemplate, MAX_GROOVE_SHIFT_TICKS, Note, NoteContent, STEP_TICKS, SwingGrid};

use crate::note_transforms::fit;

/// One table step: start offset (ticks) and velocity (percent, 100 means unchanged).
#[derive(Debug, Clone, Copy, PartialEq)]
struct GrooveStep {
    offset: i64,
    velocity: i64,
}

/// A groove table: sixteenth number `i` from the content start takes step `i % len`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GrooveTable {
    steps: Vec<GrooveStep>,
}

pub(crate) fn check_amount(amount: f64) -> Result<(), ContractError> {
    if (0.0..=1.0).contains(&amount) { Ok(()) } else { Err(notes::GROOVE_AMOUNT_OUT_OF_RANGE.with("amount", amount)) }
}

pub(crate) fn check_template(template: GrooveTemplate) -> Result<(), ContractError> {
    match template {
        GrooveTemplate::Swing { percent, .. } if !(50..=75).contains(&percent) => Err(notes::SWING_OUT_OF_RANGE.with("percent", percent)),
        GrooveTemplate::LaidBack { ticks } | GrooveTemplate::Pushed { ticks } if ticks > MAX_GROOVE_SHIFT_TICKS => {
            Err(notes::GROOVE_SHIFT_OUT_OF_RANGE.with("ticks", ticks).with("max", MAX_GROOVE_SHIFT_TICKS))
        }
        GrooveTemplate::Accent { percent } if !(100..=200).contains(&percent) => Err(notes::ACCENT_OUT_OF_RANGE.with("percent", percent)),
        _ => Ok(()),
    }
}

/// The template table for one beat (four sixteenths).
pub(crate) fn template_table(template: GrooveTemplate) -> GrooveTable {
    let timing = |offsets: [i64; 4]| offsets.map(|offset| GrooveStep { offset, velocity: 100 }).to_vec();
    let steps = match template {
        GrooveTemplate::Swing { grid: SwingGrid::Eighth, percent } => {
            // Sixteenths between eighths follow them: half the shift (like stretching the halves of a beat).
            let (eighth, sixteenth) = (swing_shift(960, percent), swing_shift(480, percent));
            timing([0, sixteenth, eighth, sixteenth])
        }
        GrooveTemplate::Swing { grid: SwingGrid::Sixteenth, percent } => {
            let shift = swing_shift(480, percent);
            timing([0, shift, 0, shift])
        }
        GrooveTemplate::LaidBack { ticks } => {
            let t = i64::from(ticks);
            timing([0, t, t, t])
        }
        GrooveTemplate::Pushed { ticks } => {
            let t = -i64::from(ticks);
            timing([0, t, t, t])
        }
        GrooveTemplate::Accent { percent } => {
            let plain = GrooveStep { offset: 0, velocity: 100 };
            vec![GrooveStep { offset: 0, velocity: i64::from(percent) }, plain, plain, plain]
        }
    };
    GrooveTable { steps }
}

/// How much later the second note of a `pair`-tick pair lands at swing `percent` (its share of the pair instead of
/// 50 %), to the tick.
fn swing_shift(pair: i64, percent: u8) -> i64 {
    (pair * (i64::from(percent) - 50) + 50) / 100
}

/// The sixteenth nearest to the tick; halfway goes forward.
fn step_index(tick: u32) -> i64 {
    (i64::from(tick) + i64::from(STEP_TICKS / 2)) / i64::from(STEP_TICKS)
}

/// The groove of a source clip over one bar `bar` (the signature bar at its start): the average offset of note starts
/// from the nearest sixteenth and the average velocity at that step relative to the clip's average velocity; a step
/// without notes stays unchanged. `None` — the clip has no notes to take a groove from.
pub(crate) fn extract_groove(source: &NoteContent, bar: u32) -> Option<GrooveTable> {
    if source.notes.is_empty() {
        return None;
    }
    let len = (bar / STEP_TICKS).max(1) as usize;
    // Per step: sum of offsets, sum of velocities, note count.
    let mut sums = vec![(0_i64, 0_i64, 0_i64); len];
    for note in &source.notes {
        let index = step_index(note.start_tick);
        let sum = &mut sums[(index as usize) % len];
        sum.0 += i64::from(note.start_tick) - index * i64::from(STEP_TICKS);
        sum.1 += i64::from(note.velocity);
        sum.2 += 1;
    }
    let total: i64 = source.notes.iter().map(|n| i64::from(n.velocity)).sum();
    let average = total as f64 / source.notes.len() as f64;
    let steps = sums
        .iter()
        .map(|&(offsets, velocities, count)| {
            if count == 0 {
                return GrooveStep { offset: 0, velocity: 100 };
            }
            let offset = (offsets as f64 / count as f64).round() as i64;
            let velocity = (100.0 * (velocities as f64 / count as f64) / average).round() as i64;
            GrooveStep { offset, velocity }
        })
        .collect();
    Some(GrooveTable { steps })
}

/// Applies the table to notes: the offset and the velocity deviation from 100 % scaled by `amount`; lengths stay.
pub(crate) fn apply_groove(group: &[Note], table: &GrooveTable, amount: f64, limit: u32) -> Vec<Note> {
    group
        .iter()
        .map(|note| {
            let step = table.steps[(step_index(note.start_tick) as usize) % table.steps.len()];
            let shift = (step.offset as f64 * amount).round() as i64;
            let scale = 100.0 + (step.velocity - 100) as f64 * amount;
            let velocity = (f64::from(note.velocity) * scale / 100.0).round().clamp(1.0, 127.0) as u8;
            fit(Note { velocity, ..*note }, i64::from(note.start_tick) + shift, i64::from(note.length_ticks), limit)
        })
        .collect()
}
