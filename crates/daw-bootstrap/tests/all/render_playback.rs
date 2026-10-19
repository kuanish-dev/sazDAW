//! A full session without an audio device: the demo sound, edits and undo by ear, effects, the sampler, the loop, the
//! song loop, live notes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn demo_project_is_silent_when_stopped_and_loud_but_clean_when_playing() {
    let mut session = start(true);
    assert_eq!(peak(&render(&mut session, 4_800)), 0.0);

    session.controls.transport.play();
    let out = render(&mut session, 96_000);
    assert!(rms(&out) > 0.02, "the beat must sound, rms = {}", rms(&out));
    assert!(peak(&out) <= 1.0);
    assert!(out.iter().all(|v| v.is_finite()));

    let meters = session.controls.stats.meters();
    let graph = session.controls.graph.graph();
    let buses: Vec<_> = graph.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Bus { .. })).map(|n| n.name.clone()).collect();
    assert_eq!(buses, ["Drums", "Bass", "Reverb"]);
    let master = meters.iter().find(|m| m.node == MASTER).unwrap();
    assert!(master.peak_left > 0.05);
    let reverb_id = bus_named(&session.controls, "Reverb");
    assert!(meters.iter().find(|m| m.node == reverb_id).unwrap().peak_left > 0.001, "the parallel path to the reverb gets through");
}

#[test]
fn edits_are_heard_and_undo_silences_them() {
    let mut session = start(false);
    let controls = session.controls.clone();
    synth_track_with_note(&controls, note(60, 0, 960));
    controls.transport.play();
    assert!(rms(&render(&mut session, 24_000)) > 0.01);

    controls.transport.stop();
    render(&mut session, 48_000);
    assert!(controls.history.undo(), "undo of adding the note");
    controls.transport.play();
    let silent = render(&mut session, 24_000);
    assert!(peak(&silent) < 1e-4, "no note, so silence");
    assert!(controls.history.redo());
    controls.transport.stop();
    render(&mut session, 100);
    controls.transport.play();
    assert!(rms(&render(&mut session, 24_000)) > 0.01);
}

#[test]
fn graph_effects_change_the_sound() {
    let mut session = start(false);
    let controls = session.controls.clone();
    let (track, _) = synth_track_with_note(&controls, note(60, 0, 480));
    controls.transport.play();
    let dry = render(&mut session, 48_000);

    controls.transport.stop();
    render(&mut session, 100);
    // A reverb between the instrument and the master: instrument → effect → master.
    let source = source_node(&controls, track);
    let fx = controls.graph.add_effect("reverb", &Params::new(), 220.0, 40.0).unwrap();
    controls.graph.disconnect(source, MASTER, false).unwrap();
    controls.graph.connect(source, fx, 1.0, false).unwrap();
    controls.graph.connect(fx, MASTER, 1.0, false).unwrap();
    controls.graph.set_effect_param(fx, "mix", 1.0).unwrap();
    controls.graph.set_effect_param(fx, "room", 0.9).unwrap();
    controls.transport.play();
    let wet = render(&mut session, 48_000);
    // The note ends at 0.23 s; by 0.8 s the dry sound is silent, the reverb has a tail.
    assert!(rms(&dry[38_400..]) < 0.001);
    assert!(rms(&wet[38_400..]) > 0.002, "the reverb tail, rms = {}", rms(&wet[38_400..]));

    controls.graph.set_effect_bypass(fx, true).unwrap();
    controls.transport.stop();
    render(&mut session, 48_000);
    controls.transport.play();
    let bypassed = render(&mut session, 48_000);
    assert!(rms(&bypassed[38_400..]) < 0.001, "bypass brings back the dry sound");
}

#[test]
fn sampler_plays_an_imported_file_at_the_requested_pitch() {
    let dir = temp_dir("sampler");
    write_wav(&dir.join("tone.wav"), 1.0, 440.0);

    let mut session = start(false);
    let controls = session.controls.clone();
    let info = controls.samples.import(&dir.join("tone.wav")).unwrap();
    assert_eq!(info.sample_rate, 44_100);

    let track = controls.tracks.add_track("Sampler", Some("sampler"), 60).unwrap();
    controls.tracks.set_instrument_param(track, "sample", info.id as f32).unwrap();
    let clip = note_clip(&controls, track, 0, 1);
    controls.notes.add_note(clip.id, note(60, 0, 960)).unwrap();
    controls.transport.play();
    let out = render(&mut session, 24_000);
    assert!(rms(&out) > 0.05, "the file sounds, rms = {}", rms(&out));

    // An octave up the frequency doubles: count zero crossings.
    let crossings =
        |buffer: &[f32]| buffer.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
    let base = crossings(&out[2_000..20_000]);
    controls.tracks.remove_track(track).unwrap();
    let high = controls.tracks.add_track("Higher", Some("sampler"), 72).unwrap();
    controls.tracks.set_instrument_param(high, "sample", info.id as f32).unwrap();
    let high_clip = note_clip(&controls, high, 0, 1);
    controls.notes.add_note(high_clip.id, note(72, 0, 960)).unwrap();
    controls.transport.stop();
    render(&mut session, 100);
    controls.transport.play();
    let octave = render(&mut session, 24_000);
    let ratio = crossings(&octave[2_000..20_000]) as f32 / base as f32;
    assert!((ratio - 2.0).abs() < 0.15, "ratio = {ratio}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn loop_region_keeps_playing_past_the_end_of_clips_in_a_loop() {
    let mut session = start(true);
    let controls = session.controls.clone();
    controls.clips.set_loop_region(Some((0, 3840))).unwrap();
    assert_eq!(controls.transport.loop_region(), Some((0, 3840)));
    controls.transport.play();
    // Five seconds at 128 BPM is more than one bar; the position does not go past the loop end.
    render(&mut session, 5 * 48_000);
    assert!(controls.transport.position_ticks() < 3840);
    controls.transport.stop();
}

#[test]
fn playing_to_the_song_end_jumps_back_to_the_start_and_plays_on() {
    let mut session = start(false);
    let controls = session.controls.clone();
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    // One note at the start of a one-bar song: a bar at 120 BPM and 48 kHz is 96 000 frames.
    synth_track_with_note(&controls, note(60, 0, 960));
    controls.transport.play();
    let first = render(&mut session, 96_000);
    assert!(rms(&first[..24_000 * 2]) > 0.01, "the note plays");
    // Past the song end the playhead is back in bar 1 and the note sounds again.
    let second = render(&mut session, 24_000);
    assert!(controls.transport.position_ticks() < 3840, "back at the start: {}", controls.transport.position_ticks());
    assert!(rms(&second) > 0.01, "playback goes on from bar 1: rms = {}", rms(&second));

    controls.transport.set_song_loop(false);
    render(&mut session, 96_000);
    assert!(controls.transport.position_ticks() > 3840, "without the song loop it runs on past the end");
    controls.transport.stop();
}

#[test]
fn live_notes_play_even_when_stopped() {
    let mut session = start(false);
    let controls = session.controls.clone();
    let track = add_synth_track(&controls, "Synth");
    render(&mut session, 512); // the engine gets the instrument
    controls.live_notes.note_on(track, 60, 1.0);
    assert!(rms(&render(&mut session, 4_800)) > 0.005);
    controls.live_notes.note_off(track, 60);
}

#[test]
fn many_edits_and_blocks_stay_stable() {
    let mut session = start(true);
    let controls = session.controls.clone();
    controls.transport.play();
    let kick_clip = controls.clips.clips().into_iter().find(|c| c.track == controls.tracks.tracks()[0].id).unwrap().id;
    for round in 0..40u32 {
        let id = controls.notes.add_note(kick_clip, note(36, (round % 15) * 240, 120)).ok();
        controls.graph.set_pan(bus_named(&controls, "Drums"), (round as f32 / 20.0) - 1.0).unwrap();
        let out = render(&mut session, 3_001);
        assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        if let Some(note) = id {
            controls.notes.remove_note(kick_clip, note.id).unwrap();
        }
    }
    assert!(controls.stats.cpu_load().is_finite());
}

#[test]
fn a_hand_picked_region_becomes_a_track_that_plays_only_that_sound() {
    let dir = temp_dir("extract");
    write_two_tones(&dir.join("tones.wav"));
    let mut session = start(false);
    let controls = session.controls.clone();
    let sample = controls.samples.import(&dir.join("tones.wav")).unwrap();
    assert_eq!(sample.frame_count, 44_100);

    // The waveform shows the pause between the tones.
    let wave = controls.sample_editor.waveform(sample.id, 0, 44_100, 100).unwrap();
    assert_eq!(wave.peaks.len(), 100);
    assert!(wave.peaks[20].1 > 0.5, "the first tone has sound");
    assert!(wave.peaks[43].1 < 0.01, "the pause is quiet (bucket 43 ≈ 0.43 s)");
    assert!(wave.peaks[70].1 > 0.5, "the second tone has sound");

    // A hand-picked region 0.5–0.9 s → a sampler track (in one undo step).
    let ids = controls.tracks.add_tracks(vec![sampler_track("High", sample.id, 0.5, 0.9)]).unwrap();
    let clip = note_clip(&controls, ids[0], 0, 1);
    controls.notes.add_note(clip.id, NewNote { pitch: 60, start_tick: 0, length_ticks: 960, velocity: 127 }).unwrap();
    controls.transport.play();
    let out = render(&mut session, 24_000);
    // 0.4 s of sound = 18 000 frames at 48 kHz; an 880 Hz tone → ~350 zero crossings, not ~176 as for 440 Hz.
    let crossings = zero_crossings(&out[..18_000 * 2]);
    assert!((300..400).contains(&crossings), "the high tone: {crossings}");
    assert!(peak(&out[19_500 * 2..]) < 1e-3, "silence after the region (0.4 s of the file at 44.1 kHz = 19 200 frames at 48 kHz)");

    // Steps: the note, the clip, then creating the track; tracks from a batch go with one undo.
    assert!(controls.history.undo());
    assert!(controls.history.undo());
    assert!(controls.history.undo(), "creating the track");
    assert_eq!(controls.tracks.tracks(), [] as [daw_contracts::Track; 0]);
    std::fs::remove_dir_all(dir).ok();
}
