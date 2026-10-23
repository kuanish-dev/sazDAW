//! The contract of batch operations: clips (`edit_clips`, `remove_clips`, `copy_clips`) and track order (`move_track`).

use std::path::Path;

use daw_contracts::*;

use crate::testing::Rig;

fn shift(start: u32) -> ClipEdit {
    ClipEdit { start_tick: Some(start), ..ClipEdit::default() }
}

/// A track with two adjacent clips: [0, bar) and [bar, 2 bars).
fn two_clips() -> (Rig, TrackId, ClipId, ClipId) {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let a = rig.clips.add_note_clip(NewNoteClip::empty(track, 0, None)).unwrap().id;
    let b = rig.clips.add_note_clip(NewNoteClip::empty(track, BAR_TICKS, None)).unwrap().id;
    (rig, track, a, b)
}

fn starts(rig: &Rig) -> Vec<(ClipId, u32)> {
    rig.clips.clips().iter().map(|c| (c.id, c.start_tick)).collect()
}

#[test]
fn edit_clips_checks_the_final_state_so_clips_can_swap_and_shift_together() {
    let (rig, _, a, b) = two_clips();
    let swapped = rig.clips.edit_clips(vec![(a, shift(BAR_TICKS)), (b, shift(0))]).unwrap();
    assert_eq!(swapped.iter().map(|c| (c.id, c.start_tick)).collect::<Vec<_>>(), [(a, BAR_TICKS), (b, 0)]);

    let moved = rig.clips.edit_clips(vec![(a, shift(BAR_TICKS * 2)), (b, shift(BAR_TICKS))]).unwrap();
    assert_eq!(moved.len(), 2);
    assert_eq!(starts(&rig), [(a, BAR_TICKS * 2), (b, BAR_TICKS)], "the group moves together");
}

#[test]
fn edit_clips_is_atomic_and_one_undo_step() {
    let (rig, _, a, b) = two_clips();
    let before = rig.clips.clips();
    let undo_label = rig.service.undo_label();
    let overlap = rig.clips.edit_clips(vec![(a, shift(BAR_TICKS / 2)), (b, shift(BAR_TICKS))]);
    assert!(matches!(overlap, Err(e) if e.is(codes::clips::NOTE_CLIPS_OVERLAP)));
    assert!(matches!(rig.clips.edit_clips(vec![(a, shift(1)), (9999, shift(0))]), Err(e) if e.is(codes::clips::NOT_FOUND)));
    let nan = rig.clips.edit_clips(vec![(a, ClipEdit { gain: Some(f32::NAN), ..ClipEdit::default() })]);
    assert!(matches!(nan, Err(e) if e.is(codes::clips::GAIN_NOT_A_NUMBER)));
    assert_eq!(rig.clips.clips(), before);
    assert_eq!(rig.service.undo_label(), undo_label);

    rig.clips.edit_clips(vec![(a, shift(BAR_TICKS * 4)), (b, shift(BAR_TICKS * 6))]).unwrap();
    assert!(rig.service.undo());
    assert_eq!(rig.clips.clips(), before, "one undo step for the whole group");
    assert!(rig.service.redo());
    assert_eq!(starts(&rig), [(a, BAR_TICKS * 4), (b, BAR_TICKS * 6)]);
    assert_eq!(rig.clips.edit_clips(vec![]).unwrap(), [] as [daw_contracts::Clip; 0]);
}

#[test]
fn edit_clips_checks_track_kinds() {
    let (rig, _, a, _) = two_clips();
    let audio = rig.tracks.add_track("Sound", None, 60).unwrap();
    let to_audio = ClipEdit { track: Some(audio), ..ClipEdit::default() };
    assert!(
        matches!(rig.clips.edit_clips(vec![(a, to_audio)]), Err(e) if e.is(codes::clips::NOTE_CLIP_NEEDS_INSTRUMENT_TRACK)),
        "a note clip does not go on an audio track"
    );
}

#[test]
fn remove_clips_is_all_or_nothing_and_one_undo_step() {
    let (rig, _, a, b) = two_clips();
    assert!(matches!(rig.clips.remove_clips(vec![a, 9999]), Err(e) if e.is(codes::clips::NOT_FOUND)));
    assert_eq!(rig.clips.clips().len(), 2);

    rig.clips.remove_clips(vec![a, b]).unwrap();
    assert_eq!(rig.clips.clips(), [] as [daw_contracts::Clip; 0]);
    assert!(rig.service.undo());
    assert_eq!(rig.clips.clips().len(), 2, "both clips came back in one step");
}

#[test]
fn copy_clips_keeps_distances_content_and_loop_and_is_one_undo_step() {
    let (rig, track, a, b) = two_clips();
    let note = rig.notes.add_note(b, NewNote { pitch: 60, start_tick: 0, length_ticks: 240, velocity: 100 }).unwrap();
    rig.notes.set_loop_ticks(b, 480).unwrap();
    let before = rig.clips.clips();

    let copies = rig.clips.copy_clips(vec![b, a], BAR_TICKS * 4).unwrap();
    assert_eq!(copies.len(), 2);
    assert_eq!((copies[0].track, copies[0].start_tick), (track, BAR_TICKS * 5), "the order follows ids");
    assert_eq!(copies[1].start_tick, BAR_TICKS * 4, "the leftmost one went to at_tick");
    assert!(copies.iter().all(|c| c.id != a && c.id != b));
    assert_eq!(rig.notes.notes(copies[0].id).unwrap(), [note], "the notes are copied");
    let original = rig.clips.clips().into_iter().find(|c| c.id == b).unwrap();
    let ClipContent::Notes(src) = &original.content else { panic!() };
    let ClipContent::Notes(dst) = &copies[0].content else { panic!() };
    assert_eq!(src.loop_ticks, dst.loop_ticks);
    assert_eq!(rig.clips.clips().len(), 4);

    assert!(rig.service.undo());
    assert_eq!(rig.clips.clips(), before);
}

#[test]
fn copy_clips_fails_atomically_on_overlap_unknown_id_or_empty_list() {
    let (rig, _, a, b) = two_clips();
    let before = rig.clips.clips();
    let overlap = |r: Result<Vec<Clip>, ContractError>| matches!(r, Err(e) if e.is(codes::clips::NOTE_CLIPS_OVERLAP));
    assert!(overlap(rig.clips.copy_clips(vec![a, b], BAR_TICKS)));
    assert!(overlap(rig.clips.copy_clips(vec![a], 0)), "the place is taken by the original");
    assert!(matches!(rig.clips.copy_clips(vec![a, 9999], BAR_TICKS * 8), Err(e) if e.is(codes::clips::NOT_FOUND)));
    assert!(matches!(rig.clips.copy_clips(vec![], 0), Err(e) if e.is(codes::clips::NOTHING_TO_COPY)), "an empty list is an error");
    assert!(overlap(rig.clips.copy_clips(vec![a, a], BAR_TICKS * 8)), "two copies at one place overlap");
    assert_eq!(rig.clips.clips(), before);
}

#[test]
fn move_track_reorders_clamps_and_is_one_undo_step() {
    let rig = Rig::new();
    let ids: Vec<TrackId> = ["A", "B", "C"].iter().map(|n| rig.tracks.add_track(n, Some("synth"), 60).unwrap()).collect();
    let order = |rig: &Rig| rig.tracks.tracks().iter().map(|t| t.id).collect::<Vec<_>>();

    rig.tracks.move_track(ids[2], 0).unwrap();
    assert_eq!(order(&rig), [ids[2], ids[0], ids[1]]);
    rig.tracks.move_track(ids[2], 1).unwrap();
    assert_eq!(order(&rig), [ids[0], ids[2], ids[1]]);
    rig.tracks.move_track(ids[0], 100).unwrap();
    assert_eq!(order(&rig), [ids[2], ids[1], ids[0]], "an index past the end goes to the end");
    assert!(matches!(rig.tracks.move_track(9999, 0), Err(e) if e.is(codes::tracks::NOT_FOUND)));

    assert!(rig.service.undo());
    assert_eq!(order(&rig), [ids[0], ids[2], ids[1]]);
    assert!(rig.service.redo());
    assert_eq!(order(&rig), [ids[2], ids[1], ids[0]]);
    assert!(rig.graph.graph().source_of(ids[0]).is_some());
}

#[test]
fn track_order_survives_save_and_load() {
    let rig = Rig::new();
    let a = rig.tracks.add_track("A", Some("synth"), 60).unwrap();
    let b = rig.tracks.add_track("B", Some("synth"), 60).unwrap();
    let clip = rig.clips.add_note_clip(NewNoteClip::empty(a, 0, None)).unwrap().id;
    rig.tracks.move_track(b, 0).unwrap();

    rig.files.save(Path::new("/p/order.daw")).unwrap();
    rig.files.new_project();
    rig.files.load(Path::new("/p/order.daw")).unwrap();
    assert_eq!(rig.tracks.tracks().iter().map(|t| t.id).collect::<Vec<_>>(), [b, a]);
    assert_eq!(rig.clips.clips()[0].id, clip);
}

#[test]
fn add_note_clips_creates_all_in_one_undo_step() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let with_note = NewNoteClip { notes: vec![super::note(60, 0, 240)], ..NewNoteClip::empty(track, BAR_TICKS * 4, None) };
    let clips = rig.clips.add_note_clips(vec![NewNoteClip::empty(track, 0, Some(BAR_TICKS * 2)), with_note]).unwrap();
    assert_eq!(clips.iter().map(|c| (c.start_tick, c.length_ticks)).collect::<Vec<_>>(), [(0, BAR_TICKS * 2), (BAR_TICKS * 4, BAR_TICKS)]);
    assert_eq!(rig.clips.clips(), clips);
    assert_eq!(rig.service.undo_label().as_deref(), Some("Add clips"));
    assert!(rig.service.undo());
    assert!(rig.clips.clips().is_empty(), "both clips went away in one step");
    assert!(rig.clips.add_note_clips(Vec::new()).unwrap().is_empty());
}

#[test]
fn add_note_clips_is_all_or_nothing() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let undo_label = rig.service.undo_label();
    let overlapping = vec![NewNoteClip::empty(track, 0, Some(BAR_TICKS * 2)), NewNoteClip::empty(track, BAR_TICKS, None)];
    assert!(matches!(rig.clips.add_note_clips(overlapping), Err(e) if e.is(codes::clips::NOTE_CLIPS_OVERLAP)));
    let bad_note = NewNoteClip { notes: vec![NewNote { velocity: 0, ..super::note(60, 0, 240) }], ..NewNoteClip::empty(track, 0, None) };
    let bad = rig.clips.add_note_clips(vec![NewNoteClip::empty(track, BAR_TICKS * 8, None), bad_note]);
    assert!(matches!(bad, Err(e) if e.is(codes::notes::VELOCITY_OUT_OF_RANGE)));
    assert!(matches!(rig.clips.add_note_clips(vec![NewNoteClip::empty(999, 0, None)]), Err(e) if e.is(codes::tracks::NOT_FOUND)));
    assert!(rig.clips.clips().is_empty());
    assert_eq!(rig.service.undo_label(), undo_label);
}
