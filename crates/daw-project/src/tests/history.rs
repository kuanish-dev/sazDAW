//! The `HistoryControl` contract: undo, redo, merging knob edits, groups, and checkpoints.

use super::*;

#[test]
fn undo_and_redo_restore_state_without_reusing_ids() {
    let (rig, track, clip) = rig_with_clip();
    let first = rig.notes.add_note(clip, note(60, 0, 240)).unwrap();
    assert!(rig.service.can_undo());
    assert_eq!(rig.service.undo_label().as_deref(), Some("Add note"));

    assert!(rig.service.undo());
    assert_eq!(rig.notes.notes(clip).unwrap(), [] as [daw_contracts::Note; 0]);
    assert!(rig.service.can_redo());
    assert!(rig.service.redo());
    assert_eq!(rig.notes.notes(clip).unwrap(), [first]);

    // Undoing the creation of a clip and a track and creating again does not reuse ids.
    assert!(rig.service.undo());
    assert!(rig.service.undo());
    assert!(rig.service.undo());
    assert_eq!(rig.tracks.tracks(), [] as [daw_contracts::Track; 0]);
    let again = rig.tracks.add_track("Another", Some("synth"), 60).unwrap();
    assert!(again > clip && again > track);
    assert!(!rig.service.can_redo(), "a new edit clears redo");
}

#[test]
fn slider_movements_merge_into_one_undo_step() {
    let rig = Rig::new();
    for step in 1..=10 {
        rig.graph.set_volume(MASTER, 0.1 * step as f32).unwrap();
    }
    assert!(rig.service.undo());
    assert!((master_volume(&rig) - 0.8).abs() < 1e-6, "back to the original");
    assert!(!rig.service.can_undo());
}

#[test]
fn a_group_of_edits_is_a_single_undo_step() {
    let (rig, _, clip) = rig_with_clip();
    rig.service.begin_group("Recording");
    for start in [0, 240, 480] {
        rig.notes.add_note(clip, note(60, start, 120)).unwrap();
    }
    rig.service.end_group();
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 3);
    assert_eq!(rig.service.undo_label().as_deref(), Some("Recording"));

    assert!(rig.service.undo());
    assert!(rig.notes.notes(clip).unwrap().is_empty(), "all three notes went away with one undo");
    assert_eq!(rig.service.undo_label().as_deref(), Some("Add clip"));

    // An empty group records nothing.
    rig.service.begin_group("Nothing");
    rig.service.end_group();
    assert_eq!(rig.service.undo_label().as_deref(), Some("Add clip"));
}

#[test]
fn end_group_reports_whether_a_step_was_recorded() {
    let (rig, _, clip) = rig_with_clip();
    assert!(!rig.service.end_group(), "there is no group");
    rig.service.begin_group("Empty");
    assert!(!rig.service.end_group(), "the project did not change");
    rig.service.begin_group("Note");
    rig.notes.add_note(clip, note(60, 0, 120)).unwrap();
    assert!(rig.service.end_group());
}

#[test]
fn nested_groups_join_the_outer_one_and_record_a_single_step() {
    let (rig, _, clip) = rig_with_clip();
    rig.service.begin_group("Outside");
    rig.notes.add_note(clip, note(60, 0, 120)).unwrap();
    rig.service.begin_group("Inside");
    rig.notes.add_note(clip, note(62, 240, 120)).unwrap();
    assert!(!rig.service.end_group(), "a nested level records no step");
    rig.notes.add_note(clip, note(64, 480, 120)).unwrap();
    assert!(rig.service.end_group());
    assert_eq!(rig.service.undo_label().as_deref(), Some("Outside"));
    assert!(rig.service.undo());
    assert!(rig.notes.notes(clip).unwrap().is_empty(), "all three notes are one step");
}

#[test]
fn cancel_group_restores_the_state_records_nothing_and_closes_every_level() {
    let (rig, track, clip) = rig_with_clip();
    let before = rig.service.snapshot();
    rig.service.begin_group("Part");
    rig.notes.add_note(clip, note(60, 0, 120)).unwrap();
    rig.service.begin_group("Inside");
    let created = rig.tracks.add_track("Extra", Some("synth"), 60).unwrap();
    rig.service.cancel_group();
    assert_eq!(rig.service.snapshot().tracks, before.tracks, "the track is gone");
    assert!(rig.notes.notes(clip).unwrap().is_empty(), "the note is gone");
    assert_eq!(rig.last_snapshot().project.tracks.len(), 1, "the engine got the previous project");
    assert!(rig.dsp_log().contains(&format!("remove_instrument:{created}")), "the instrument of the cancelled track is removed");
    assert_eq!(rig.service.undo_label().as_deref(), Some("Add clip"), "no step was recorded");
    assert!(!rig.service.end_group() && !rig.service.end_group(), "the remaining end_group calls do nothing");

    // Ids from a cancelled group are not given out again.
    let next = rig.tracks.add_track("New", Some("synth"), 60).unwrap();
    assert!(next > created && track < created);
}

#[test]
fn undo_and_redo_wait_until_the_group_is_closed() {
    let (rig, _, clip) = rig_with_clip();
    rig.service.begin_group("Gesture");
    rig.notes.add_note(clip, note(60, 0, 120)).unwrap();
    assert!(!rig.service.undo(), "a history step in the middle of a group would break it");
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 1);
    assert!(rig.service.end_group());
    assert!(rig.service.undo());
}

#[test]
fn mark_baseline_clears_history_and_dirty_flag() {
    let (rig, _, clip) = rig_with_clip();
    rig.service.begin_group("Unclosed");
    rig.notes.add_note(clip, note(60, 0, 120)).unwrap();
    rig.service.mark_baseline();
    assert!(!rig.service.can_undo() && !rig.service.can_redo());
    assert!(!rig.files.is_dirty());
    assert!(!rig.service.end_group(), "the group is closed without a step");
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 1, "the state stays");
}

#[test]
fn steps_list_the_history_on_both_sides_of_the_current_point() {
    let (rig, _, clip) = rig_with_clip();
    for (label, pitch) in [("One", 60), ("Two", 62), ("Three", 64)] {
        rig.service.begin_group(label);
        rig.notes.add_note(clip, note(pitch, 0, 120)).unwrap();
        rig.service.end_group();
    }
    assert!(rig.service.undo() && rig.service.undo());
    let revision = rig.service.revision();

    let steps = rig.service.steps();
    assert_eq!(steps.done, ["Add track", "Add clip", "One"], "oldest first, the next undo last");
    assert_eq!(steps.undone, ["Two", "Three"], "the next redo first");
    assert_eq!(rig.service.revision(), revision, "reading publishes nothing");
    assert_eq!(rig.service.steps(), steps, "and changes nothing");
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 1);

    // A new edit drops the undone steps; a fresh baseline drops everything.
    rig.notes.add_note(clip, note(67, 240, 120)).unwrap();
    assert_eq!(rig.service.steps().done, ["Add track", "Add clip", "One", "Add note"]);
    assert!(rig.service.steps().undone.is_empty());
    rig.service.mark_baseline();
    assert_eq!(rig.service.steps(), HistorySteps::default());
}

#[test]
fn steps_show_a_squashed_change_as_one_step_and_an_open_group_only_when_it_closes() {
    let (rig, _, clip) = rig_with_clip();
    let mark = rig.service.checkpoint();
    rig.tracks.add_track("Bass", Some("synth"), 36).unwrap();
    rig.notes.add_note(clip, note(60, 0, 120)).unwrap();
    assert_eq!(rig.service.steps().done, ["Add track", "Add clip", "Add track", "Add note"], "ordinary steps until the squash");
    assert_eq!(rig.service.squash(mark, "AI: bass line"), Ok(true));
    assert_eq!(rig.service.steps().done, ["Add track", "Add clip", "AI: bass line"]);

    rig.service.begin_group("Recording");
    rig.notes.add_note(clip, note(62, 240, 120)).unwrap();
    assert_eq!(rig.service.steps().done.last().map(String::as_str), Some("AI: bass line"), "an open group is not a step yet");
    assert!(rig.service.end_group());
    assert_eq!(rig.service.steps().done.last().map(String::as_str), Some("Recording"));
}

#[test]
fn squash_turns_the_steps_after_a_checkpoint_into_one_undo_step() {
    let (rig, _, clip) = rig_with_clip();
    let mark = rig.service.checkpoint();
    let bass = rig.tracks.add_track("Bass", Some("synth"), 36).unwrap();
    for start in [0, 240, 480] {
        rig.notes.add_note(clip, note(60, start, 120)).unwrap();
    }
    let revision = rig.service.revision();
    assert_eq!(rig.service.squash(mark, " AI: melody "), Ok(true));
    assert_eq!(rig.service.undo_label().as_deref(), Some("AI: melody"), "the label is trimmed");
    assert!(rig.service.revision() > revision, "the interface learns that the undo label changed");

    assert!(rig.service.undo());
    assert!(rig.notes.notes(clip).unwrap().is_empty(), "one undo took the notes back");
    assert_eq!(rig.tracks.tracks().len(), 1, "and the track");
    assert_eq!(rig.service.undo_label().as_deref(), Some("Add clip"), "steps before the mark stay as they were");
    assert!(rig.service.redo());
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 3);
    assert!(rig.tracks.tracks().iter().any(|track| track.id == bass));
}

#[test]
fn squash_with_nothing_after_the_mark_changes_nothing() {
    let (rig, _, _) = rig_with_clip();
    let mark = rig.service.checkpoint();
    assert_eq!(rig.service.squash(mark, "AI: nothing"), Ok(false));
    assert_eq!(rig.service.undo_label().as_deref(), Some("Add clip"));
}

#[test]
fn a_checkpoint_on_an_empty_history_covers_every_step() {
    let rig = Rig::new();
    let mark = rig.service.checkpoint();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    rig.clips.add_note_clip(NewNoteClip::empty(track, 0, None)).unwrap();
    assert_eq!(rig.service.squash(mark, "AI: start"), Ok(true));
    assert!(rig.service.undo());
    assert!(!rig.service.can_undo() && rig.tracks.tracks().is_empty());
}

#[test]
fn knob_edits_merge_neither_across_a_checkpoint_nor_into_a_squashed_step() {
    let rig = Rig::new();
    rig.graph.set_volume(MASTER, 0.5).unwrap();
    let mark = rig.service.checkpoint();
    rig.graph.set_volume(MASTER, 0.3).unwrap();
    assert_eq!(rig.service.squash(mark, "AI: quieter"), Ok(true));
    rig.graph.set_volume(MASTER, 0.2).unwrap();

    assert!(rig.service.undo());
    assert!((master_volume(&rig) - 0.3).abs() < 1e-6, "the edit after the squash is its own step");
    assert_eq!(rig.service.undo_label().as_deref(), Some("AI: quieter"));
    assert!(rig.service.undo());
    assert!((master_volume(&rig) - 0.5).abs() < 1e-6, "the edit before the mark is its own step");
}

#[test]
fn squash_needs_a_label_and_waits_for_an_open_group() {
    let (rig, _, clip) = rig_with_clip();
    let mark = rig.service.checkpoint();
    rig.notes.add_note(clip, note(60, 0, 120)).unwrap();
    assert!(rig.service.squash(mark, "  ").unwrap_err().is(codes::history::STEP_LABEL_EMPTY));
    rig.service.begin_group("Recording");
    assert!(rig.service.squash(mark, "AI: note").unwrap_err().is(codes::history::GROUP_OPEN));
    rig.service.end_group();
    assert_eq!(rig.service.squash(mark, "AI: note"), Ok(true), "the failed calls changed nothing");
}

#[test]
fn squash_fails_when_the_mark_has_left_the_history() {
    let (rig, _, clip) = rig_with_clip();
    let mark = rig.service.checkpoint();
    rig.notes.add_note(clip, note(60, 0, 120)).unwrap();
    // Undone past the mark and then edited: the state at the mark is gone.
    assert!(rig.service.undo() && rig.service.undo(), "the note and the clip are undone");
    rig.tracks.add_track("Other", Some("synth"), 60).unwrap();
    assert!(rig.service.squash(mark, "AI: note").unwrap_err().is(codes::history::CHECKPOINT_GONE));

    // A cleared history (a project opened or created) loses every mark, even one taken on an empty history.
    rig.service.mark_baseline();
    let fresh = rig.service.checkpoint();
    rig.service.mark_baseline();
    rig.tracks.add_track("After reopening", Some("synth"), 60).unwrap();
    assert!(rig.service.squash(fresh, "AI: track").unwrap_err().is(codes::history::CHECKPOINT_GONE));
}

#[test]
fn a_mark_lives_while_its_state_is_still_in_the_step_limit() {
    // The history keeps 200 steps (`LIMIT` in `service/history.rs`): 200 edits after the mark push the mark's own step
    // out, and the state at the mark becomes the bottom of the history.
    let (rig, _, clip) = rig_with_clip();
    let edit = |i: u8| {
        rig.notes.add_note(clip, note(36 + i / 16, u32::from(i % 16) * 240, 120)).unwrap();
    };
    let mark = rig.service.checkpoint();
    (0..200).for_each(edit);
    assert_eq!(rig.service.squash(mark, "AI: many notes"), Ok(true));
    assert!(rig.service.undo());
    assert!(rig.notes.notes(clip).unwrap().is_empty());
    assert!(!rig.service.can_undo(), "the merged step was the whole history");

    // One edit more, and the state at the mark is forgotten too.
    let mark = rig.service.checkpoint();
    (0..201).for_each(edit);
    assert!(rig.service.squash(mark, "AI: too many").unwrap_err().is(codes::history::CHECKPOINT_GONE));
}
