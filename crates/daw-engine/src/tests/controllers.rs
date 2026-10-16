//! Note clip controllers: the exact frame, before a note at the same tick, the chase after a seek, the reset on stop.

use super::*;

const BEND: ControllerKind = ControllerKind::PitchBend;

#[test]
fn controller_points_arrive_at_their_exact_frame_before_notes_of_the_same_tick() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let clip = rig.note_clip(10, 0, 10_000, 960);
    rig.controller_in(clip, BEND, &[(100, 0.5), (480, -0.25)]);
    rig.note_in(clip, 60, 480, 100);
    rig.publish();
    rig.transport.play();
    rig.render(960 * FRAMES_PER_TICK + 10);

    let log = rig.log();
    let bend = log.iter().position(|e| *e == format!("bend=-0.25@{}", 480 * FRAMES_PER_TICK)).expect("a bend at tick 480");
    let note = log.iter().position(|e| *e == format!("on:60@{}", 480 * FRAMES_PER_TICK)).expect("a note at tick 480");
    assert!(bend < note, "the controller before the note: {log:?}");
    assert!(log.contains(&format!("bend=0.5@{}", 100 * FRAMES_PER_TICK)), "{log:?}");
}

#[test]
fn seeking_chases_the_held_value_and_stopping_resets_touched_controllers() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let clip = rig.note_clip(10, 0, 10_000, 3840);
    rig.controller_in(clip, BEND, &[(100, 0.75), (2000, 0.0)]);
    rig.controller_in(clip, ControllerKind::Cc { number: 7 }, &[(0, 0.4)]);
    rig.publish();

    rig.transport.seek(1000);
    rig.transport.play();
    rig.render(512);
    let log = rig.log();
    assert!(log.contains(&"bend=0.75@0".to_string()), "a jump into a bent place: {log:?}");
    assert!(log.contains(&"cc7=0.4@0".to_string()), "{log:?}");

    rig.clear_log();
    rig.transport.stop();
    rig.render(512);
    let log = rig.log();
    assert!(log.contains(&"bend=0@512".to_string()), "a stop resets the wheel: {log:?}");
    assert!(!log.iter().any(|e| e.starts_with("cc7")), "volume (CC7) is not reset: {log:?}");
    assert!(!log.iter().any(|e| e.starts_with("at=") || e.starts_with("cc64")), "untouched ones are left alone: {log:?}");
}

#[test]
fn loop_wrap_chases_the_value_at_the_loop_start() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let clip = rig.note_clip(10, 0, 10_000, 3840);
    rig.controller_in(clip, BEND, &[(0, -0.5), (200, 0.5)]);
    rig.publish();
    rig.transport.set_loop(Some((100, 400)));
    rig.transport.seek(100);
    rig.transport.play();
    rig.render(700 * FRAMES_PER_TICK);
    let bends: Vec<_> = rig.log().into_iter().filter(|e| e.starts_with("bend")).collect();
    // Start at 100: −0.5 is held; at 200, 0.5; on the loop, a reset, the chase −0.5, then 0.5 again.
    let values: Vec<_> = bends.iter().map(|e| e.split('@').next().unwrap().to_string()).collect();
    assert_eq!(values[..5], ["bend=-0.5", "bend=0.5", "bend=0", "bend=-0.5", "bend=0.5"], "{bends:?}");
}
