//! Full session: "Bounce to audio" and "Freeze" sound the same as the track itself (comparing exports before and
//! after), the copy's WAV is the track sound alone, the copy goes under the track, all in one undo step; unfreezing
//! brings the instrument back.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

/// 120 BPM, 48 kHz: a 4/4 bar is 2 s = 96 000 frames.
const BAR_FRAMES: usize = 96_000;
const RATE: u32 = 48_000;

struct Song {
    dir: PathBuf,
    session: Session,
    bass: TrackId,
    other: TrackId,
}

/// A project with a file: "Bass" (synth → EQ → master, volume 0.6, panned left) with clips in bars 2 and 4;
/// "Other" is a synth with a note in bar 1 (muted so the export sounds like one track). The master is at one.
fn song(name: &str) -> Song {
    let dir = temp_dir(name);
    let session = start(false);
    let controls = session.controls.clone();
    controls.project.create(&dir.join("song/song.dawproj")).unwrap();
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let bass = add_synth_track(&controls, "Bass");
    for bar in [1, 3] {
        let clip = note_clip(&controls, bass, bar, 1);
        controls.notes.add_note(clip.id, note(48, 0, 1920)).unwrap();
        controls.notes.add_note(clip.id, note(55, 960, 1920)).unwrap();
    }
    let other = add_synth_track(&controls, "Other");
    let clip = note_clip(&controls, other, 0, 1);
    controls.notes.add_note(clip.id, note(72, 0, 960)).unwrap();
    let source = source_node(&controls, bass);
    let eq = controls.graph.insert_effect(source, "eq", &Params::new(), 0.0, 0.0).unwrap();
    controls.graph.set_effect_param(eq, "low_gain", 6.0).unwrap();
    controls.graph.set_volume(source, 0.6).unwrap();
    controls.graph.set_pan(source, -0.4).unwrap();
    controls.graph.set_mute(source_node(&controls, other), true).unwrap();
    controls.graph.set_volume(MASTER, 1.0).unwrap();
    Song { dir, session, bass, other }
}

/// The whole project to a 32-bit WAV (no tail beyond `tail`): stereo frames.
fn export(song: &Song, file: &str) -> Vec<f32> {
    let path = song.dir.join(file);
    let request = ExportRequest {
        sample_rate: RATE,
        bit_depth: BitDepth::Float32,
        stems: false,
        tail_seconds: 1.0,
        ..ExportRequest::new(path.clone())
    };
    song.session.controls.exporter.export(&request, &|_| {}).unwrap();
    read_wav(&path).1
}

/// Whether two mixes sound the same: the RMS difference is below 1 % of the sound (the 24-bit copy is slightly rounded).
/// The mix with the copy may be longer by the bounce tail (the copy clip ends after the last clip): there is silence there.
fn assert_same_sound(expected: &[f32], actual: &[f32], what: &str) {
    assert!(actual.len() >= expected.len(), "{what}: length {} < {}", actual.len(), expected.len());
    assert!(peak(&actual[expected.len()..]) < 1e-3, "{what}: silence after the end of the previous mix");
    let actual = &actual[..expected.len()];
    let signal = rms(expected);
    assert!(signal > 0.01, "{what}: the track sounds ({signal})");
    let diff: Vec<f32> = expected.iter().zip(actual).map(|(a, b)| a - b).collect();
    assert!(rms(&diff) < signal * 0.01, "{what}: difference {} with sound {signal}", rms(&diff));
}

#[test]
fn bounce_sounds_like_the_track_and_is_one_undo_step() {
    let song = song("bounce");
    let controls = &song.session.controls;
    let before_mix = export(&song, "before.wav");
    let before = controls.view.snapshot();

    let copy = controls.bounce.bounce(song.bass, 1.0).unwrap();

    let order: Vec<TrackId> = controls.tracks.tracks().iter().map(|t| t.id).collect();
    assert_eq!(order, [song.bass, copy, song.other], "the copy is under the track");
    let clips = track_clips(controls, copy);
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].start_tick, BAR_TICKS, "the clip starts at the track's first clip");
    let sample = controls.samples.samples().into_iter().find(|s| Some(s.id) == clips[0].content.sample()).unwrap();
    assert!(sample.path.starts_with(song.dir.join("song/samples")), "the WAV is in the project sounds folder: {}", sample.path.display());

    let (spec, bounced) = read_wav(&sample.path);
    assert_eq!((spec.channels, spec.sample_rate), (2, RATE));
    assert_eq!(bounced.len(), (3 * BAR_FRAMES + RATE as usize) * 2, "bars 2–4 and the tail");
    assert_same_sound(&before_mix[BAR_FRAMES * 2..], &bounced, "copy WAV");
    assert!(
        peak(&bounced[..BAR_FRAMES * 2]) > 0.01 && peak(&bounced[BAR_FRAMES * 2..2 * BAR_FRAMES * 2]) < 1e-3,
        "the pause between clips is silent"
    );

    assert!(controls.graph.graph().node(source_node(controls, song.bass)).unwrap().kind.strip().unwrap().mute);
    assert_same_sound(&before_mix, &export(&song, "after.wav"), "mix after the bounce");

    assert_eq!(controls.history.undo_label().as_deref(), Some("Bounce to audio «Bass»"));
    assert!(controls.history.undo());
    assert_eq!(Project { next_id: before.next_id, ..controls.view.snapshot() }, before, "one undo step brings the project back");
    std::fs::remove_dir_all(&song.dir).ok();
}

#[test]
fn freeze_keeps_the_sound_without_the_instrument_and_unfreeze_brings_it_back() {
    let song = song("freeze");
    let controls = &song.session.controls;
    let before_mix = export(&song, "before.wav");

    let copy = controls.bounce.freeze(song.bass, 1.0).unwrap();
    let spec = controls.tracks.tracks().into_iter().find(|t| t.id == song.bass).unwrap().instrument.unwrap();
    assert_eq!(spec.frozen, Some(copy));
    assert_same_sound(&before_mix, &export(&song, "frozen.wav"), "frozen project");

    controls.graph.set_mute(source_node(controls, copy), true).unwrap();
    assert!(peak(&export(&song, "silent.wav")) < 1e-4, "the instrument is frozen, so the track is silent");
    controls.history.undo();

    controls.bounce.unfreeze(song.bass).unwrap();
    assert!(controls.tracks.tracks().iter().all(|t| t.id != copy), "the copy is removed");
    assert!(controls.samples.samples().is_empty(), "and its sound too");
    assert_same_sound(&before_mix, &export(&song, "unfrozen.wav"), "unfrozen project");
    std::fs::remove_dir_all(&song.dir).ok();
}

#[test]
fn refusals_leave_the_project_alone() {
    let song = song("bounce-errors");
    let controls = &song.session.controls;
    let empty = add_synth_track(controls, "Empty");
    let before = controls.view.snapshot();
    assert!(matches!(controls.bounce.bounce(empty, 0.0), Err(e) if e.is(codes::export::TRACK_HAS_NO_CLIPS)), "no clips");
    assert!(matches!(controls.bounce.bounce(song.bass, 99.0), Err(e) if e.is(codes::export::TAIL_OUT_OF_RANGE)), "tail");
    assert!(matches!(controls.bounce.bounce(9_999, 0.0), Err(e) if e.is(codes::tracks::NOT_FOUND)));
    assert!(matches!(controls.bounce.unfreeze(song.bass), Err(e) if e.is(codes::export::NOT_FROZEN)), "not frozen");
    assert_eq!(controls.view.snapshot(), before);
    std::fs::remove_dir_all(&song.dir).ok();
}
