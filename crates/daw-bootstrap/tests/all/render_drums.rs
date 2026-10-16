//! Full session and drums: a built part sounds, a drum on its own sample, previewing a part.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn a_built_drum_part_sounds_and_a_repeated_pattern_keeps_looping_inside_its_clip() {
    let mut session = start(false);
    session.controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let slots = [("rock_basic", 4), ("fill_tom_cascade", 1), ("break_stop", 1)]
        .map(|(pattern, repeat)| PatternSlot { pattern: pattern.into(), repeat });
    session.controls.drum_part.build(DrumPartRequest { start_bar: 0, slots: slots.to_vec(), replace: false, lanes: vec![] }).unwrap();
    // The silence after the part is checked: the song must not loop back to its start.
    session.controls.transport.set_song_loop(false);
    session.controls.transport.play();

    let bar = 96_000; // 120 BPM, 48 kHz: a bar is 2 s
    let bars: Vec<f32> = (0..6).map(|_| peak(&render(&mut session, bar))).collect();
    assert!(bars[0] > 0.1, "the first bar plays");
    assert!(bars[2] > 0.1, "the third bar too: the one-bar pattern loops inside the four-bar clip");
    assert!(bars[4] > 0.1, "the fill sounds");
    assert!(bars[5] > 0.1, "the break: a kick and crash hit on the first beat");
    let after = peak(&render(&mut session, 3 * bar));
    assert!(after < 0.05, "silence after the part (except the crash tail), but the peak is {after}");
}

#[test]
fn a_drum_can_be_played_by_the_users_own_sample() {
    let dir = temp_dir("drum-sample");
    write_two_tones(&dir.join("tones.wav"));
    let mut session = start(false);
    session.controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let sample = session.controls.samples.import(&dir.join("tones.wav")).unwrap();

    // "Break: stop" is a kick and a crash on the first beat; the crash is skipped, the kick plays your sound (a 440 Hz tone).
    let request = DrumPartRequest {
        start_bar: 0,
        slots: vec![PatternSlot { pattern: "break_stop".into(), repeat: 1 }],
        replace: false,
        lanes: vec![
            LaneAssignment { lane: "kick".into(), target: LaneTarget::Sample { id: sample.id } },
            LaneAssignment { lane: "crash".into(), target: LaneTarget::Skip },
        ],
    };
    let report = session.controls.drum_part.build(request).unwrap();
    assert_eq!(report.created_tracks, ["Kick: tones"]);
    let track = session.controls.tracks.tracks().into_iter().find(|t| t.name == "Kick: tones").unwrap();
    let spec = track.instrument.unwrap();
    assert_eq!(spec.kind, "sampler");
    assert_eq!(spec.params.get("sample").copied(), Some(sample.id as f32));

    session.controls.transport.play();
    let first_beat = render(&mut session, 19_200); // 0.4 s: a 440 Hz tone, ≈176 zero crossings; a kick drum would be much lower
    let crossings = zero_crossings(&first_beat);
    assert!((150..=200).contains(&crossings), "your 440 Hz tone plays, {crossings} zero crossings");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_drum_part_preview_sounds_loops_stops_and_leaves_the_project_untouched() {
    let mut session = start(false);
    let c = session.controls.clone();
    c.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    // Tempo is a project edit (undoable); from here on we check that previewing changes nothing.
    c.history.mark_baseline();
    c.drum_preview.play(preview_request(&[("rock_basic", 1)], vec![])).unwrap();

    // Plays right away, without the transport; the pattern is one bar (2 s) and loops.
    let bar = 96_000;
    let first = peak(&render(&mut session, bar));
    let second = peak(&render(&mut session, bar));
    assert!(first > 0.1 && second > 0.1, "two loops in a row sound: {first}, {second}");

    // Nothing appeared in the project, and there is nothing to undo.
    assert!(c.tracks.tracks().is_empty() && c.clips.clips().is_empty());
    assert!(!c.history.can_undo());

    c.drum_preview.stop();
    render(&mut session, 4_800); // the command reaches the engine
    assert!(peak(&render(&mut session, bar)) < 0.01, "silence after stop");
}

#[test]
fn a_drum_part_preview_plays_the_users_sample_and_refuses_too_long_or_wrong_parts() {
    let dir = temp_dir("drum-preview");
    write_two_tones(&dir.join("tones.wav"));
    let mut session = start(false);
    let c = session.controls.clone();
    c.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let sample = c.samples.import(&dir.join("tones.wav")).unwrap();

    // Only the kick, and it plays your 440 Hz tone (the crash is skipped).
    let lanes = vec![
        LaneAssignment { lane: "kick".into(), target: LaneTarget::Sample { id: sample.id } },
        LaneAssignment { lane: "crash".into(), target: LaneTarget::Skip },
    ];
    c.drum_preview.play(preview_request(&[("break_stop", 1)], lanes)).unwrap();
    let crossings = zero_crossings(&render(&mut session, 19_200));
    assert!((150..=200).contains(&crossings), "your 440 Hz tone plays, {crossings} zero crossings");
    assert!(c.tracks.tracks().is_empty(), "no sampler track is created for the preview");
    let too_long = c.drum_preview.play(preview_request(&[("rock_basic", 17)], vec![])).unwrap_err();
    assert!(too_long.is(codes::patterns::PREVIEW_TOO_LONG) && param(&too_long, "max") == Some("16"), "{too_long}");
    assert!(c.drum_preview.play(preview_request(&[("nope", 1)], vec![])).is_err());
    std::fs::remove_dir_all(dir).ok();
}
