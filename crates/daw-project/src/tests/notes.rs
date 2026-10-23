//! The `NoteControl` contract: a clip's notes and its content length.

use super::*;

#[test]
fn notes_are_validated_and_edited() {
    let (rig, _, clip) = rig_with_clip();
    let created = rig.notes.add_note(clip, note(60, 0, 240)).unwrap();
    assert_eq!(rig.notes.notes(clip).unwrap(), [created]);

    let moved = rig.notes.update_note(clip, created.id, note(62, 480, 120)).unwrap();
    assert_eq!((moved.pitch, moved.start_tick), (62, 480));

    assert!(rig.notes.add_note(clip, note(60, MAX_CLIP_LOOP_TICKS - 10, 100)).is_err());
    assert!(matches!(rig.notes.add_note(clip, note(200, 0, 10)), Err(e) if e.is(codes::notes::PITCH_OUT_OF_RANGE)), "pitch");
    let silent = rig.notes.add_note(clip, NewNote { velocity: 0, ..note(60, 0, 10) });
    assert!(matches!(silent, Err(e) if e.is(codes::notes::VELOCITY_OUT_OF_RANGE)), "velocity");
    assert!(rig.notes.add_note(clip, note(60, 0, 0)).is_err(), "zero length");
    assert!(matches!(rig.notes.add_note(9999, note(60, 0, 10)), Err(e) if e.is(codes::clips::NOT_FOUND)), "no such clip");
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 1, "errors do not change the project");

    rig.notes.remove_note(clip, created.id).unwrap();
    assert_eq!(rig.notes.notes(clip).unwrap(), [] as [daw_contracts::Note; 0]);
    assert!(matches!(rig.notes.remove_note(clip, created.id), Err(e) if e.is(codes::notes::NOT_FOUND)));
}

#[test]
fn set_loop_ticks_turns_auto_length_off_and_trims_notes() {
    let (rig, _, clip) = rig_with_clip();
    assert!(content(&rig, clip).auto_length);
    rig.notes.add_note(clip, note(60, 100, 200)).unwrap();
    rig.notes.add_note(clip, note(62, BAR_TICKS * 3, 480)).unwrap();
    assert_eq!(loop_ticks(&rig, clip), BAR_TICKS * 4);

    rig.notes.set_loop_ticks(clip, BAR_TICKS * 2).unwrap();
    let manual = content(&rig, clip);
    assert!(!manual.auto_length);
    assert_eq!(manual.loop_ticks, BAR_TICKS * 2);
    assert_eq!(manual.notes.len(), 1, "the note beyond the bound is removed");

    assert!(rig.notes.add_note(clip, note(60, BAR_TICKS * 2, 100)).is_err());
    rig.notes.add_note(clip, note(64, 0, 100)).unwrap();
    assert_eq!(loop_ticks(&rig, clip), BAR_TICKS * 2);

    assert!(matches!(rig.notes.set_loop_ticks(clip, 100), Err(e) if e.is(codes::notes::LOOP_LENGTH_OUT_OF_RANGE)));
    assert!(rig.notes.set_loop_ticks(clip, STEP_TICKS + 1).is_err());
    assert!(rig.notes.set_loop_ticks(clip, MAX_CLIP_LOOP_TICKS + STEP_TICKS).is_err());
    rig.notes.set_loop_ticks(clip, MAX_CLIP_LOOP_TICKS).unwrap();
    assert!(rig.notes.set_loop_ticks(9999, BAR_TICKS).is_err());

    assert!(rig.service.undo());
    assert_eq!(loop_ticks(&rig, clip), BAR_TICKS * 2, "shrinking and growing are separate undo steps");
}

#[test]
fn replace_notes_is_one_undo_step_and_all_or_nothing() {
    let (rig, _, clip) = rig_with_clip();
    rig.notes.add_note(clip, note(40, 0, 100)).unwrap();
    let new_notes = vec![note(60, 0, 240), note(62, 240, 240), note(64, 480, 240)];
    assert_eq!(rig.notes.replace_notes(clip, new_notes).unwrap(), 3);
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 3);

    let bad = vec![note(60, 0, 240), note(62, MAX_CLIP_LOOP_TICKS, 240)];
    assert!(rig.notes.replace_notes(clip, bad).is_err());
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 3, "an error changes nothing");

    assert!(rig.service.undo());
    let restored = rig.notes.notes(clip).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].pitch, 40);
}

#[test]
fn every_note_operation_is_a_single_undo_step() {
    let (rig, _, clip) = rig_with_clip();
    let base = rig.service.undo_label();

    let added = rig.notes.add_notes(clip, vec![note(60, 0, 100), note(62, BAR_TICKS * 2, 100)]).unwrap();
    assert_eq!(loop_ticks(&rig, clip), BAR_TICKS * 3);
    assert!(rig.service.undo());
    assert!(rig.notes.notes(clip).unwrap().is_empty(), "both notes and the length in one step");
    assert_eq!(loop_ticks(&rig, clip), BAR_TICKS);
    assert_eq!(rig.service.undo_label(), base);
    assert!(rig.service.redo());

    rig.notes.update_notes(clip, vec![(added[0].id, note(70, 10, 100)), (added[1].id, note(71, 20, 100))]).unwrap();
    assert_eq!(loop_ticks(&rig, clip), BAR_TICKS);
    assert!(rig.service.undo());
    assert_eq!(rig.notes.notes(clip).unwrap()[1].pitch, 62);
    assert_eq!(loop_ticks(&rig, clip), BAR_TICKS * 3);

    rig.notes.remove_notes(clip, vec![added[0].id, added[1].id]).unwrap();
    assert!(rig.service.undo());
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 2);

    rig.notes.replace_notes(clip, vec![note(50, 0, 10)]).unwrap();
    assert!(rig.service.undo());
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 2);

    rig.notes.add_note(clip, note(80, 0, 10)).unwrap();
    assert!(rig.service.undo());
    rig.notes.update_note(clip, added[0].id, note(81, 0, 10)).unwrap();
    assert!(rig.service.undo());
    rig.notes.remove_note(clip, added[0].id).unwrap();
    assert!(rig.service.undo());
    rig.notes.clear_notes(clip).unwrap();
    assert!(rig.service.undo());
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 2, "each operation rolled back in one step");
}

fn length_of(rig: &Rig, clip: ClipId) -> u32 {
    loop_ticks(rig, clip)
}

#[test]
fn auto_length_grows_with_notes_and_shrinks_back_atomically() {
    let (rig, _, clip) = rig_with_clip();
    assert_eq!(length_of(&rig, clip), BAR_TICKS);

    let far = rig.notes.add_note(clip, note(60, BAR_TICKS * 2 + 100, 200)).unwrap();
    assert_eq!(length_of(&rig, clip), BAR_TICKS * 3);
    assert_eq!(rig.clips.clips()[0].length_ticks, BAR_TICKS);
    let near = rig.notes.add_note(clip, note(62, 0, 240)).unwrap();
    assert_eq!(length_of(&rig, clip), BAR_TICKS * 3);

    let crossing = rig.notes.add_note(clip, note(64, BAR_TICKS * 3 - 100, 400)).unwrap();
    assert_eq!(length_of(&rig, clip), BAR_TICKS * 4);
    rig.notes.remove_note(clip, crossing.id).unwrap();
    assert_eq!(length_of(&rig, clip), BAR_TICKS * 3);

    rig.notes.update_note(clip, far.id, note(60, BAR_TICKS + 10, 100)).unwrap();
    assert_eq!(length_of(&rig, clip), BAR_TICKS * 2);
    rig.notes.remove_note(clip, far.id).unwrap();
    rig.notes.remove_note(clip, near.id).unwrap();
    assert_eq!(length_of(&rig, clip), BAR_TICKS);

    assert!(rig.service.undo());
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 1);
    assert_eq!(length_of(&rig, clip), BAR_TICKS);
    assert!(rig.service.undo());
    assert_eq!(length_of(&rig, clip), BAR_TICKS * 2);
}

#[test]
fn clearing_and_replacing_notes_refit_the_length() {
    let (rig, _, clip) = rig_with_clip();
    rig.notes.replace_notes(clip, vec![note(60, 0, 100), note(62, BAR_TICKS * 5, 100)]).unwrap();
    assert_eq!(length_of(&rig, clip), BAR_TICKS * 6);
    rig.notes.clear_notes(clip).unwrap();
    assert_eq!(length_of(&rig, clip), BAR_TICKS);
}

#[test]
fn batch_note_operations_are_all_or_nothing() {
    let (rig, _, clip) = rig_with_clip();
    let added = rig.notes.add_notes(clip, vec![note(60, 0, 240), note(62, 240, 240), note(64, 480, 240)]).unwrap();
    assert_eq!(added.len(), 3);
    assert!(added.windows(2).all(|w| w[0].id < w[1].id), "ids are given out in a row");

    let moved = rig
        .notes
        .update_notes(clip, added.iter().map(|n| (n.id, note(n.pitch + 1, n.start_tick + BAR_TICKS, n.length_ticks))).collect())
        .unwrap();
    assert_eq!(moved.iter().map(|n| n.pitch).collect::<Vec<_>>(), [61, 63, 65]);
    assert_eq!(length_of(&rig, clip), BAR_TICKS * 2);

    let bad = vec![(added[0].id, note(70, 0, 100)), (999, note(70, 0, 100))];
    assert!(matches!(rig.notes.update_notes(clip, bad), Err(e) if e.is(codes::notes::NOT_FOUND)));
    assert_eq!(rig.notes.notes(clip).unwrap()[0].pitch, 61, "the first edit was not applied");
    assert!(rig.notes.remove_notes(clip, vec![added[0].id, 999]).is_err());
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 3);

    rig.notes.remove_notes(clip, vec![added[0].id, added[1].id]).unwrap();
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 1);

    assert!(rig.notes.add_notes(9999, vec![note(60, 0, 10)]).is_err());
    assert!(rig.notes.add_notes(clip, vec![note(60, 0, 10), note(200, 0, 10)]).is_err());
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 1, "the error added nothing");
}

#[test]
fn notes_per_clip_are_limited() {
    let (rig, _, clip) = rig_with_clip();
    let many: Vec<NewNote> = (0..2048).map(|i| note(60, i, 1)).collect();
    rig.notes.add_notes(clip, many).unwrap();
    use codes::notes::TOO_MANY_NOTES;
    assert!(matches!(rig.notes.add_note(clip, note(61, 0, 1)), Err(e) if e.is(TOO_MANY_NOTES)));
    assert!(matches!(rig.notes.add_notes(clip, vec![note(61, 0, 1)]), Err(e) if e.is(TOO_MANY_NOTES)));
    assert!(matches!(rig.notes.replace_notes(clip, vec![note(61, 0, 1); 2049]), Err(e) if e.is(TOO_MANY_NOTES)));
}
