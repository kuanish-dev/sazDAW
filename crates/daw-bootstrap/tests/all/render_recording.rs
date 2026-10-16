//! Recording notes played live with the real engine: a new clip, stopping with a held note, loop passes.
//! The take rules (grid snapping, the clip under the cursor, cutting at the loop end) are checked by the `daw-recorder` tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn live_playing_is_recorded_into_a_new_clip_as_one_undo_step() {
    let mut session = start(false);
    let track = add_synth_track(&session.controls, "Synth");
    session.controls.record.set_quantize(0).unwrap();
    play_one_note(&mut session, track);

    assert!(wait_for(|| session.controls.record.status().take_notes == 1), "the note is recorded");
    let notes = recorded_notes(&session, track);
    assert_eq!(notes.len(), 1);
    // A press is taken at the start of the block where it happened: accuracy within a block (512 frames ≈ 20 ticks).
    assert!((i64::from(notes[0].start_tick) - 100).abs() <= 21, "start = {}", notes[0].start_tick);
    assert!((i64::from(notes[0].length_ticks) - 480).abs() <= 21, "length = {}", notes[0].length_ticks);
    assert_eq!((notes[0].pitch, notes[0].velocity), (60, 127));
    let clips = track_clips(&session.controls, track);
    assert_eq!((clips.len(), clips[0].start_tick, clips[0].length_ticks), (1, 0, BAR_TICKS), "a one-bar clip is created");

    assert_eq!(session.controls.history.undo_label().as_deref(), Some("Recording"));
    assert!(session.controls.history.undo());
    assert!(recorded_notes(&session, track).is_empty(), "the take is undone as a whole");
    assert!(track_clips(&session.controls, track).is_empty(), "the clip created by the take is removed too");
    session.controls.record.set_armed(false);
}

#[test]
fn nothing_is_recorded_when_not_armed() {
    let mut session = start(false);
    let track = add_synth_track(&session.controls, "Synth");

    // Not armed.
    session.controls.transport.play();
    render(&mut session, 2_000);
    session.controls.live_notes.note_on(track, 60, 1.0);
    render(&mut session, 4_000);
    session.controls.live_notes.note_off(track, 60);
    render(&mut session, 1_000);
    session.controls.transport.stop();
    render(&mut session, 512);
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert_eq!(recorded_notes(&session, track), [] as [daw_contracts::Note; 0]);

    assert!(track_clips(&session.controls, track).is_empty(), "no clip is created without recording");
    assert!(!session.controls.record.status().capturing);
}

#[test]
fn a_note_still_held_when_playback_stops_is_closed_at_that_moment() {
    let mut session = start(false);
    let track = add_synth_track(&session.controls, "Synth");
    session.controls.record.set_quantize(0).unwrap();
    session.controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    session.controls.record.set_armed(true);
    session.controls.transport.play();
    render(&mut session, 2_500);
    session.controls.live_notes.note_on(track, 64, 0.5);
    render(&mut session, 12_000);
    session.controls.transport.pause();
    render(&mut session, 512);

    assert!(wait_for(|| session.controls.record.status().take_notes == 1));
    let notes = recorded_notes(&session, track);
    assert_eq!(notes.len(), 1);
    assert!(notes[0].length_ticks > 300, "the length up to the stop: {}", notes[0].length_ticks);
    assert_eq!(notes[0].velocity, 64);
    session.controls.record.set_armed(false);
}

#[test]
fn a_note_in_the_second_loop_pass_replaces_the_first_pass() {
    let mut session = start(false);
    let track = start_recording(&mut session);
    // A note placed before recording does not belong to the take and is not touched; the clip is one bar.
    let clip = note_clip(&session.controls, track, 0, 1);
    let before =
        session.controls.notes.add_note(clip.id, NewNote { pitch: 48, start_tick: 2_000, length_ticks: 240, velocity: 100 }).unwrap();

    render_ticks(&mut session, 100);
    tap(&mut session, track, 60, 240); // the first pass
    render_ticks(&mut session, 3_840 - 344 + 400); // play on to tick 400 of the second pass
    tap(&mut session, track, 62, 240); // the second pass
    finish_take(&mut session, 1);

    let notes = recorded_notes(&session, track);
    let pitches: Vec<_> = notes.iter().map(|n| n.pitch).collect();
    assert!(pitches.contains(&62), "the second pass note stayed: {pitches:?}");
    assert!(!pitches.contains(&60), "the first pass note is replaced: {pitches:?}");
    assert!(notes.iter().any(|n| n.id == before.id), "notes outside the take are not touched");
    let second = notes.iter().find(|n| n.pitch == 62).unwrap();
    assert!((i64::from(second.start_tick) - 400).abs() <= 25, "start = {}", second.start_tick);

    let clips = track_clips(&session.controls, track);
    assert_eq!(clips.len(), 1, "recording creates no new clips");
    assert_eq!(clips[0].length_ticks, BAR_TICKS, "recording does not lengthen the clip");
    let ClipContent::Notes(content) = &clips[0].content else { panic!("a note clip") };
    assert_eq!(content.loop_ticks, BAR_TICKS, "nor its content");

    // One undo step brings back the initial state: neither 60 nor 62.
    assert!(session.controls.history.undo());
    assert_eq!(recorded_notes(&session, track), vec![before]);
}

#[test]
fn notes_in_the_same_pass_accumulate() {
    let mut session = start(false);
    let track = start_recording(&mut session);
    render_ticks(&mut session, 100);
    tap(&mut session, track, 60, 200);
    render_ticks(&mut session, 300);
    tap(&mut session, track, 64, 200);
    render_ticks(&mut session, 300);
    tap(&mut session, track, 67, 200);
    finish_take(&mut session, 3);
    assert_eq!(recorded_notes(&session, track).len(), 3);
}

#[test]
fn a_silent_second_pass_keeps_the_first() {
    let mut session = start(false);
    let track = start_recording(&mut session);
    render_ticks(&mut session, 100);
    tap(&mut session, track, 60, 200);
    render_ticks(&mut session, 4_500); // a whole second pass without a single note
    finish_take(&mut session, 1);
    let notes = recorded_notes(&session, track);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].pitch, 60);
}
