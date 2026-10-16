//! Take lifecycle: track arming and inputs, checks before start, transport and input, status while recording, the
//! length limit, closing.

use super::*;

#[test]
fn only_audio_tracks_can_be_armed_and_the_engine_learns_every_change() {
    let w = world();
    let error = w.recorder.set_armed(SYNTH_TRACK, true).unwrap_err();
    assert!(error.is(audio::NOT_AUDIO_TRACK), "{error}");
    assert!(matches!(w.recorder.set_armed(99, true), Err(e) if e.is(tracks::NOT_FOUND)));
    assert!(matches!(w.recorder.set_input(99, InputChannels::Left), Err(e) if e.is(tracks::NOT_FOUND)));

    w.recorder.set_armed(AUDIO_TRACK, true).unwrap();
    w.recorder.set_input(BASS_TRACK, InputChannels::Right).unwrap();
    assert_eq!(
        *w.monitor.inputs.lock().unwrap(),
        [
            TrackInput { track: AUDIO_TRACK, armed: true, channels: InputChannels::Stereo },
            TrackInput { track: BASS_TRACK, armed: false, channels: InputChannels::Right },
        ]
    );
    let tracks = w.recorder.status().tracks;
    assert_eq!(tracks.len(), 2);
    assert!(tracks[0].armed && tracks[1].input == InputChannels::Right);
    assert!(w.events().is_empty(), "arming touches neither input nor transport: the engine opens the input");
}

#[test]
fn nothing_armed_or_no_project_file_is_refused_before_anything_starts() {
    let w = world();
    let error = w.recorder.start().unwrap_err();
    assert!(error.is(audio::NO_ARMED_TRACKS), "{error}");
    w.recorder.set_armed(AUDIO_TRACK, true).unwrap();
    *w.project.path.lock().unwrap() = None;
    let error = w.recorder.start().unwrap_err();
    assert!(error.is(audio::PROJECT_NOT_SAVED), "{error}");
    assert!(w.events().is_empty(), "neither input nor transport is touched");
    assert!(!w.recorder.status().recording);
}

#[test]
fn a_second_start_and_arming_during_a_take_are_refused_and_the_first_take_survives() {
    let w = world();
    w.record(AUDIO_TRACK);
    let error = w.recorder.start().unwrap_err();
    assert!(error.is(audio::RECORDING_ALREADY_RUNNING), "{error}");
    let error = w.recorder.set_armed(BASS_TRACK, true).unwrap_err();
    assert!(error.is(audio::ARMING_WHILE_RECORDING), "{error}");
    assert!(w.recorder.set_input(AUDIO_TRACK, InputChannels::Left).is_err());
    w.capture.push(&ramp(4_800));
    assert!(w.recorder.stop().is_ok());
    assert!(matches!(w.recorder.stop(), Err(e) if e.is(audio::NOT_RECORDING)), "nothing to stop");
}

#[test]
fn a_failing_input_leaves_the_transport_alone() {
    let w = world();
    *w.capture.fail_start.lock().unwrap() = true;
    w.recorder.set_armed(AUDIO_TRACK, true).unwrap();
    let error = w.recorder.start().unwrap_err();
    assert!(error.is(audio::NO_INPUT_DEVICE), "{error}");
    assert!(w.events().is_empty());
    assert!(!w.recorder.status().recording);
}

#[test]
fn a_transport_that_was_already_playing_is_left_playing() {
    let w = world();
    *w.transport.state.lock().unwrap() = TransportState::Playing;
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(4_800));
    w.recorder.stop().unwrap();
    assert!(!w.events().iter().any(|e| e.starts_with("transport")), "{:?}", w.events());
}

#[test]
fn status_shows_time_peaks_per_track_lost_frames_and_monitor_problems() {
    let w = world();
    w.recorder.set_input(AUDIO_TRACK, InputChannels::Right).unwrap();
    w.record(AUDIO_TRACK);
    assert!(w.recorder.status().recording);
    w.capture.dropped.store(5, Ordering::Relaxed);
    *w.devices.level.lock().unwrap() = InputLevel { left: 0.5, right: 0.25 }; // the audio host measures the level
    *w.monitor.status.lock().unwrap() = MonitorStatus { listening: 1, input_rate: 44_100, output_rate: 48_000 };
    w.capture.push(&[0.5, -0.25].repeat(24_000)); // 0.5 s
    wait_until("status with time", || w.recorder.status().seconds >= 0.49);
    let status = w.recorder.status();
    assert_eq!((status.left, status.right), (0.5, 0.25), "{status:?}");
    assert_eq!((status.tracks[0].left, status.tracks[0].right), (0.25, 0.25), "the track sees its own channel");
    assert_eq!(status.dropped_frames, 5);
    assert!(status.monitor_problem.unwrap().contains("44100"));
    w.recorder.stop().unwrap();
    let after = w.recorder.status();
    assert!(!after.recording && after.seconds == 0.0);
    assert!(after.tracks[0].armed, "arming survives the take");
    assert!(w.log.recent(10).iter().any(|e| e.level == LogLevel::Warn && e.message.contains("lost 5 frames")));
}

#[test]
fn hitting_the_length_limit_releases_input_and_transport_but_keeps_the_sound() {
    let w = world_with(2_000); // 1000 stereo frames
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(1_500));
    wait_until("stop at the limit", || w.recorder.status().full);
    assert!(w.recorder.status().recording, "the take waits for stop");
    assert!(w.events().contains(&"capture.stop".to_string()) && w.events().contains(&"transport.stop".to_string()));
    assert!(w.log.recent(10).iter().any(|e| e.level == LogLevel::Error && e.message.contains("length limit")));

    let before = w.events().len();
    let clip = w.stop_one();
    assert_eq!(clip.start_tick, 960);
    assert_eq!(w.writer.written.lock().unwrap()[0].1.frame_count(), 1_500 - 480, "the sound is not lost");
    assert_eq!(w.events().len(), before + 5, "after stop only the file write, group, import and clip: nothing is stopped twice");
}

#[test]
fn dropping_the_recorder_mid_take_closes_the_input() {
    let w = world();
    w.record(AUDIO_TRACK);
    let events = w.events.clone();
    drop(w.recorder);
    let log = events.lock().unwrap();
    assert!(log.contains(&"capture.stop".to_string()) && log.contains(&"transport.stop".to_string()));
    assert_eq!(*w.transport.holds.lock().unwrap(), 0, "the song loop is released too");
}

#[test]
fn a_take_holds_the_song_loop_from_start_to_stop() {
    let w = world();
    assert_eq!(*w.transport.holds.lock().unwrap(), 0);
    w.record(AUDIO_TRACK);
    assert_eq!(*w.transport.holds.lock().unwrap(), 1, "the take is one straight run: playback passes the song end");
    w.capture.push(&ramp(4_800));
    w.stop_one();
    assert_eq!(*w.transport.holds.lock().unwrap(), 0, "after stop the song loops again");

    w.record(AUDIO_TRACK);
    assert!(w.recorder.stop().is_err(), "an empty take");
    assert_eq!(*w.transport.holds.lock().unwrap(), 0, "a failed take releases the hold as well");
}
