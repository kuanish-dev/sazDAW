//! Clip commands: list, create, remove, notes and steps.

use daw_contracts::{BAR_TICKS, Controls, DEFAULT_PITCH, DEFAULT_VELOCITY, NewNote, NewNoteClip, STEP_TICKS};

use super::{Words, bar_tick, checked, number, report, tracks};

/// A clip by its number in the `clips` list (MIDI file commands need it too).
pub(super) fn id_at(controls: &Controls, index: usize) -> Option<u32> {
    let id = controls.clips.clips().get(index).map(|c| c.id);
    if id.is_none() {
        eprintln!("no clip #{index} (list: clips)");
    }
    id
}

pub(super) fn run(controls: &Controls, command: &str, words: &Words) -> bool {
    match command {
        "clips" => {
            let tracks = controls.tracks.tracks();
            let map = controls.tempo.tempo_map();
            for (index, clip) in controls.clips.clips().iter().enumerate() {
                let track = tracks.iter().find(|t| t.id == clip.track).map_or("?", |t| t.name.as_str());
                println!(
                    "{index} {:<10} bar {} length {} bars  {}",
                    track,
                    map.bar_index(f64::from(clip.start_tick)) + 1,
                    map.bar_index(clip.end_tick() as f64) - map.bar_index(f64::from(clip.start_tick)),
                    match &clip.content {
                        daw_contracts::ClipContent::Notes(content) => format!("notes: {}", content.notes.len()),
                        daw_contracts::ClipContent::Audio { sample, .. } => format!("sound {sample}"),
                    }
                );
            }
        }
        "clip" => {
            if let (Some(track), Some(bar)) =
                (number(words.arg(1), "track").and_then(|i| tracks::id_at(controls, i)), number::<u32>(words.arg(2), "bar"))
            {
                let start = bar_tick(controls, bar.saturating_sub(1));
                let length = words
                    .arg(3)
                    .and_then(|w| w.parse::<u32>().ok())
                    .map(|bars| bar_tick(controls, bar.saturating_sub(1).saturating_add(bars)).saturating_sub(start));
                report(controls.clips.add_note_clip(NewNoteClip::empty(track, start, length)));
            }
        }
        "rmclip" => {
            if let Some(id) = number(words.arg(1), "clip").and_then(|i| id_at(controls, i)) {
                report(controls.clips.remove_clip(id));
            }
        }
        "notes" => {
            if let Some(id) = number(words.arg(1), "clip").and_then(|i| id_at(controls, i)) {
                print_clip(controls, id);
            }
        }
        "step" => toggle_step(controls, words.arg(1), words.arg(2)),
        "clear" => {
            if let Some(id) = number(words.arg(1), "clip").and_then(|i| id_at(controls, i)) {
                report(controls.notes.clear_notes(id));
            }
        }
        _ => return false,
    }
    true
}

/// The clip notes as a row of steps: `x` means a note on the step.
fn print_clip(controls: &Controls, clip: u32) {
    let Ok(notes) = controls.notes.notes(clip) else {
        eprintln!("the clip has no notes (an audio clip?)");
        return;
    };
    let length = controls.clips.clips().iter().find(|c| c.id == clip).map_or(0, |c| c.length_ticks.min(BAR_TICKS * 2));
    let row: String =
        (0..(length / STEP_TICKS)).map(|s| if notes.iter().any(|n| n.start_tick == s * STEP_TICKS) { 'x' } else { '.' }).collect();
    println!("{row}   (notes: {})", notes.len());
}

/// Toggles the note on a clip step (the pitch is the track default).
fn toggle_step(controls: &Controls, clip: Option<&str>, step: Option<&str>) {
    let (Some(id), Some(step)) = (number::<usize>(clip, "clip").and_then(|i| id_at(controls, i)), number::<u32>(step, "step")) else {
        return;
    };
    let Some(clip) = controls.clips.clips().into_iter().find(|c| c.id == id) else { return };
    let pitch = controls.tracks.tracks().iter().find(|t| t.id == clip.track).map_or(DEFAULT_PITCH, |t| t.default_pitch);
    let start = step * STEP_TICKS;
    let Some(notes) = checked(controls.notes.notes(id)) else { return };
    match notes.iter().find(|n| n.start_tick == start) {
        Some(existing) => report(controls.notes.remove_note(id, existing.id)),
        None => {
            report(controls.notes.add_note(id, NewNote { pitch, start_tick: start, length_ticks: STEP_TICKS, velocity: DEFAULT_VELOCITY }));
        }
    }
}
