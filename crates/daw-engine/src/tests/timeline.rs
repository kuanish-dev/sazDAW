//! The timeline: note clips play at the exact frame, content repeats, clip bounds, the loop, seeking.

use super::*;

#[test]
fn silent_without_snapshot_or_while_stopped() {
    let mut rig = Rig::new();
    assert!(rig.render(512).iter().all(|v| *v == 0.0));
    rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 240);
    rig.publish();
    assert!(rig.render(512).iter().all(|v| *v == 0.0), "the transport is stopped");
}

#[test]
fn clip_notes_start_and_end_on_the_exact_frame() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.note(10, 60, 100, 200);
    rig.publish();
    rig.transport.play();
    let out = rig.render(10_000);

    let log = rig.log();
    assert!(log.contains(&format!("on:60@{}", 100 * FRAMES_PER_TICK)), "{log:?}");
    assert!(log.contains(&format!("off:60@{}", 300 * FRAMES_PER_TICK)), "{log:?}");
    assert!(near(Rig::left(&out, 100 * FRAMES_PER_TICK + 10), 0.5));
    assert_eq!(Rig::left(&out, 100 * FRAMES_PER_TICK - 1), 0.0);
    assert_eq!(Rig::left(&out, 300 * FRAMES_PER_TICK + 10), 0.0);
}

#[test]
fn block_size_does_not_change_the_result() {
    let build = || {
        let mut rig = Rig::new();
        rig.add_track(10, MASTER);
        rig.note(10, 60, 0, 300);
        rig.note(10, 62, 480, 120);
        rig.publish();
        rig.transport.play();
        rig
    };
    let mut big = build();
    let all_at_once = big.render(20_000);
    let mut small = build();
    let mut pieces = Vec::new();
    for _ in 0..80 {
        pieces.extend(small.render(250));
    }
    let first_diff = all_at_once.iter().zip(&pieces).position(|(a, b)| a != b);
    assert_eq!(first_diff, None, "the first mismatching sample");
    assert_eq!(all_at_once.len(), pieces.len());
}

#[test]
fn edits_during_playback_are_picked_up_and_hanging_notes_are_released() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 900);
    rig.publish();
    rig.transport.play();
    rig.render(25 * 100); // the note already sounds
    assert!(rig.log().iter().any(|e| e.starts_with("on:60")));

    // The user erased the clip notes while they sounded.
    rig.clear_notes();
    rig.publish();
    rig.render(25 * 1000);
    assert!(rig.log().contains(&format!("off:60@{}", 900 * FRAMES_PER_TICK)), "{:?}", rig.log());
}

#[test]
fn a_note_clip_plays_its_notes_at_the_right_ticks() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let clip = rig.note_clip(10, 960, 960, 960);
    rig.note_in(clip, 60, 100, 200);
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000 * FRAMES_PER_TICK);

    let log = rig.log();
    assert!(log.contains(&format!("on:60@{}", 1060 * FRAMES_PER_TICK)), "{log:?}");
    assert!(log.contains(&format!("off:60@{}", 1260 * FRAMES_PER_TICK)), "{log:?}");
    assert_eq!(Rig::left(&out, 500 * FRAMES_PER_TICK), 0.0, "quiet before the clip");
    assert!(near(Rig::left(&out, 1_100 * FRAMES_PER_TICK), 0.5));
    assert_eq!(Rig::left(&out, 1_500 * FRAMES_PER_TICK), 0.0, "quiet after the note");
}

#[test]
fn content_repeats_over_a_longer_clip() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    // A 4-bar clip with one bar of content: four repeats.
    let clip = rig.note_clip(10, 0, 4 * BAR_TICKS, BAR_TICKS);
    rig.note_in(clip, 60, 0, 100);
    rig.publish();
    rig.transport.play();
    rig.render((4 * BAR_TICKS) as usize * FRAMES_PER_TICK + 100);
    assert_eq!(rig.count("on:60"), 4);
    for bar in 0..4 {
        let frame = (bar * BAR_TICKS) as usize * FRAMES_PER_TICK;
        assert!(rig.log().contains(&format!("on:60@{frame}")), "bar {bar}: {:?}", rig.log());
    }
}

#[test]
fn a_clip_shorter_than_its_content_cuts_the_notes() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let clip = rig.note_clip(10, 960, 480, 960);
    rig.note_in(clip, 60, 0, 960); // a note over the whole content
    rig.note_in(clip, 62, 600, 100); // starts beyond the clip end: does not play
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000 * FRAMES_PER_TICK);

    let log = rig.log();
    assert!(log.contains(&format!("on:60@{}", 960 * FRAMES_PER_TICK)), "{log:?}");
    assert!(log.contains(&format!("off:60@{}", 1440 * FRAMES_PER_TICK)), "cut at the clip end: {log:?}");
    assert_eq!(rig.count("on:62"), 0, "a note beyond the clip end");
    assert!(near(Rig::left(&out, 1_200 * FRAMES_PER_TICK), 0.5));
    assert_eq!(Rig::left(&out, 1_600 * FRAMES_PER_TICK), 0.0, "quiet after the clip");
}

#[test]
fn notes_after_the_clip_end_do_not_play() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let clip = rig.note_clip(10, 0, 480, 960);
    rig.note_in(clip, 60, 100, 100);
    rig.publish();
    rig.transport.play();
    // The window is wider than the clip and the content: no repeats after tick 480.
    rig.render(5 * 960 * FRAMES_PER_TICK);
    assert_eq!(rig.count("on:60"), 1);
}

#[test]
fn clip_offset_ticks_shifts_the_content_phase() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let clip = rig.note_clip(10, 0, 960, 960);
    rig.note_in(clip, 60, 100, 50);
    rig.clip_mut(clip).offset_ticks = 100; // the clip starts at content tick 100
    rig.publish();
    rig.transport.play();
    rig.render(1_000 * FRAMES_PER_TICK);
    assert!(rig.log().contains(&"on:60@0".to_string()), "the note is now at the very start: {:?}", rig.log());
    assert_eq!(rig.count("on:60"), 1);
}

#[test]
fn two_clips_on_one_track_play_in_sequence() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let first = rig.note_clip(10, 0, 480, 480);
    rig.note_in(first, 60, 0, 100);
    let second = rig.note_clip(10, 480, 480, 480);
    rig.note_in(second, 62, 0, 100);
    rig.publish();
    rig.transport.play();
    rig.render(1_000 * FRAMES_PER_TICK);
    let log = rig.log();
    assert!(log.contains(&"on:60@0".to_string()), "{log:?}");
    assert!(log.contains(&format!("on:62@{}", 480 * FRAMES_PER_TICK)), "{log:?}");
    assert_eq!(rig.count("on:"), 2);
}

#[test]
fn loop_region_repeats_the_timeline_section() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let clip = rig.note_clip(10, 0, 960, 960);
    rig.note_in(clip, 60, 0, 100);
    rig.project.loop_region = Some((0, 960));
    rig.transport.set_loop(Some((0, 960))); // the transport applies the loop; the engine does not read it itself
    rig.publish();
    rig.transport.play();
    // Three passes of 960 ticks.
    rig.render(3 * 960 * FRAMES_PER_TICK + 10);
    assert_eq!(rig.count("on:60"), 4, "starts at ticks 0, 960, 1920, 2880 (the loop is joined without clicks)");
}

#[test]
fn seeking_releases_everything() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 900);
    rig.publish();
    rig.transport.play();
    rig.render(2_000);
    rig.clear_log();
    rig.transport.seek(5_000);
    rig.render(100);
    assert!(rig.log().contains(&"all_off".to_string()));
}

#[test]
fn engine_does_not_panic_on_tracks_without_nodes_or_clips_on_missing_tracks() {
    let mut rig = Rig::new();
    rig.add_track_only(10); // the track and instrument exist, the graph node does not
    rig.add_track(11, MASTER);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    let ghost = rig.note_clip(99, 0, 960, 960); // track 99 does not exist at all
    rig.note_in(ghost, 64, 0, 100);
    rig.samples.insert(300, constant_sample(5_000, 0.5));
    rig.add_clip(98, 0, 960, ClipContent::audio(300)); // no audio track
    rig.add_clip(10, 960, 960, ClipContent::audio(300)); // no Audio node
    rig.publish();
    rig.transport.play();
    rig.link.note_on(10, 60, 1.0);
    let out = rig.render(3_000);
    assert!(out.iter().all(|v| v.is_finite()));
    assert!(near(Rig::left(&out, 1_000), 0.5), "the track with a node sounds, the rest is silent");
    rig.render(3_000 * FRAMES_PER_TICK);
}
