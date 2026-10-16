//! Full session: sound levels (`MixAnalysis`) match the export bar by bar, a silent built-in synth track (volume 0) and
//! master overload are found by the mix check (`MixCheck`), and "listening" does not change the project.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

/// 120 BPM, 48 kHz: a 4/4 bar is 2 s = 96 000 frames.
const BAR_FRAMES: usize = 96_000;

/// Mono WAV at 48 kHz: a 440 Hz sine with peak `amplitude`, `seconds` long.
fn write_tone(path: &Path, amplitude: f32, seconds: f32) {
    let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..(48_000.0 * seconds) as usize {
        writer.write_sample(amplitude * (std::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin()).unwrap();
    }
    writer.finalize().unwrap();
}

/// An audio track with a tone at bar `bar` (from zero) and node volume `volume`.
fn tone_track(controls: &Controls, sample: SampleId, bar: u32, volume: f32) -> TrackId {
    let track = controls.tracks.add_track("Tone", None, 60).unwrap();
    controls.clips.add_audio_clip(track, bar * BAR_TICKS, sample).unwrap();
    controls.graph.set_volume(source_node(controls, track), volume).unwrap();
    track
}

fn db(value: f32) -> f32 {
    20.0 * value.log10()
}

fn levels(controls: &Controls, per: LevelTargets, to_tick: u32) -> LevelReport {
    let request = LevelRequest { from_tick: None, to_tick: Some(to_tick), per, resolution_bars: Some(1) };
    controls.analysis.levels(&controls.view.snapshot(), &request).unwrap()
}

#[test]
fn levels_per_bar_match_the_exported_mix_for_a_known_tone() {
    let dir = temp_dir("analysis-levels");
    let session = start(false);
    let controls = &session.controls;
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    write_tone(&dir.join("tone.wav"), 0.5, 1.0);
    let sample = controls.samples.import(&dir.join("tone.wav")).unwrap();
    // A tone in bar 2 (the first second of two), track and master volume at one.
    tone_track(controls, sample.id, 1, 1.0);
    controls.graph.set_volume(MASTER, 1.0).unwrap();
    let before = controls.view.snapshot();

    let report = levels(controls, LevelTargets::Master, BAR_TICKS * 3);
    assert_eq!(
        report.sections.iter().map(|s| (s.bar, s.from_tick, s.to_tick)).collect::<Vec<_>>(),
        [(1, 0, BAR_TICKS), (2, BAR_TICKS, BAR_TICKS * 2), (3, BAR_TICKS * 2, BAR_TICKS * 3)]
    );
    let master = &report.targets[0];
    assert_eq!(master.sections[0], None, "silence before the clip");
    assert_eq!(master.sections[2], None, "silence after the clip");
    let tone = master.sections[1].expect("the bar with the tone sounds");
    // The RMS of a sine is 3 dB below the peak, and 3 dB more because the tone sounds for half the bar.
    assert!((tone.peak_db - tone.rms_db - 6.0).abs() < 0.2, "{tone:?}");
    assert!((tone.peak_db - db(0.5)).abs() < 0.1, "tone peak 0.5: {tone:?}");

    // The same span by export: the peak and RMS of the second bar from the file.
    let path = dir.join("mix.wav");
    let request = ExportRequest {
        sample_rate: 48_000,
        bit_depth: BitDepth::Float32,
        stems: false,
        tail_seconds: 0.0,
        ..ExportRequest::new(path.clone())
    };
    controls.exporter.export(&request, &|_| {}).unwrap();
    let (_, mix) = read_wav(&path);
    let bar = &mix[BAR_FRAMES * 2..(BAR_FRAMES * 2 * 2).min(mix.len())];
    let silent_rest = BAR_FRAMES as f32 / (bar.len() / 2) as f32;
    assert!((tone.peak_db - db(peak(bar))).abs() < 0.11, "peak: analysis {} and export {}", tone.peak_db, db(peak(bar)));
    // In the export file the bar is cut at the clip end: compute the RMS over the whole bar (silence after that).
    let expected_rms = db(rms(bar)) - 10.0 * silent_rest.log10();
    assert!((tone.rms_db - expected_rms).abs() < 0.11, "RMS: analysis {} and export {expected_rms}", tone.rms_db);
    assert!(master.loudness_lufs.is_some());

    // A track sounds the same at its own output (master at one); tracks without clips are absent.
    let tracks = levels(controls, LevelTargets::Track, BAR_TICKS * 3);
    assert_eq!(tracks.targets.len(), 1);
    assert_eq!(tracks.targets[0].sections, master.sections);
    assert_eq!(controls.view.snapshot(), before, "analysis does not change the project");
    std::fs::remove_dir_all(dir).ok();
}

/// A built-in synth param by its name.
fn synth_param(controls: &Controls, name: &str) -> ParamInfo {
    let kinds = controls.instruments.kinds();
    let synth = kinds.into_iter().find(|k| k.kind == SYNTH_KIND).expect("the built-in synth is in the catalog");
    synth.params.into_iter().find(|p| p.name == name).unwrap_or_else(|| panic!("the synth has no param \"{name}\""))
}

#[test]
fn check_finds_a_silent_synth_track_with_gain_at_zero() {
    let session = start(false);
    let controls = &session.controls;
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let mut tracks = Vec::new();
    for name in ["Bass", "Lead"] {
        let track = add_synth_track(controls, name);
        let clip =
            NewNoteClip { notes: vec![note(48, 0, 1920), note(55, 1920, 1920)], ..NewNoteClip::empty(track, 0, Some(BAR_TICKS * 2)) };
        controls.clips.add_note_clip(clip).unwrap();
        tracks.push(track);
    }
    let gain = synth_param(controls, "gain");
    controls.tracks.set_instrument_param(tracks[0], gain.name, gain.min).unwrap();

    // The synth smooths the volume from the default to zero in the first milliseconds: a click remains, not a sound.
    let per_track = levels(controls, LevelTargets::Track, BAR_TICKS * 2);
    let bass = &per_track.targets[0];
    assert!(bass.sections.iter().flatten().all(|l| l.rms_db < -45.0), "the bass without volume is inaudible: {bass:?}");
    assert!(
        per_track.targets[1].sections.iter().all(|l| l.is_some_and(|l| l.rms_db > -30.0)),
        "the lead sounds: {:?}",
        per_track.targets[1]
    );

    let findings = controls.mix_check.check(&controls.view.snapshot(), None, None);
    let silent = findings.iter().find(|f| matches!(f.code, "silentTrack" | "quietTrack")).expect("the inaudible track is found");
    assert_eq!(silent.track, Some(tracks[0]));
    assert!(silent.message.contains("\"Bass\""), "{}", silent.message);
    assert!(findings.iter().any(|f| f.code == "suspiciousParam" && f.track == Some(tracks[0])), "{findings:?}");
    assert!(!findings.iter().any(|f| f.track == Some(tracks[1])), "the lead is fine: {findings:?}");
    assert!(findings.windows(2).all(|pair| pair[0].severity <= pair[1].severity), "serious ones first: {findings:?}");
}

#[test]
fn check_finds_clipping_on_master_with_its_bars() {
    let dir = temp_dir("analysis-clip");
    let session = start(false);
    let controls = &session.controls;
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    write_tone(&dir.join("hot.wav"), 0.9, 1.0);
    let sample = controls.samples.import(&dir.join("hot.wav")).unwrap();
    // A quiet tone in bar 1, an overloaded one in bar 3 (three loud tracks and the master above one).
    tone_track(controls, sample.id, 0, 0.1);
    for _ in 0..3 {
        tone_track(controls, sample.id, 2, MAX_LEVEL);
    }
    controls.graph.set_volume(MASTER, MAX_LEVEL).unwrap();

    let findings = controls.mix_check.check(&controls.view.snapshot(), None, Some(BAR_TICKS * 4));
    let clipping = findings.iter().find(|f| f.code == "masterClipping").expect("the overload is found");
    assert_eq!(clipping.ranges, [(BAR_TICKS * 2, BAR_TICKS * 3)], "only bar 3: {}", clipping.message);
    assert!(clipping.message.contains("bar 3"), "{}", clipping.message);
    std::fs::remove_dir_all(dir).ok();
}
