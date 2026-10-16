//! Full session and files: export to WAV and stems, saving with samples, MIDI there and back.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn song_with_audio_clip_exports_to_wav_with_the_right_length() {
    let dir = temp_dir("export");
    write_wav(&dir.join("loop.wav"), 1.0, 220.0);

    let mut session = start(false);
    let controls = session.controls.clone();
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let info = controls.samples.import(&dir.join("loop.wav")).unwrap();
    let track = controls.tracks.add_track("Sound", None, 60).unwrap();
    controls.clips.add_audio_clip(track, 1920, info.id).unwrap(); // from second 1

    let out_path = dir.join("mix.wav");
    let report = controls
        .exporter
        .export(
            &ExportRequest {
                sample_rate: 44_100,
                bit_depth: BitDepth::Int16,
                stems: false,
                tail_seconds: 0.5,
                ..ExportRequest::new(out_path.clone())
            },
            &|_| {},
        )
        .unwrap();
    // The clip ends at second 2, plus a 0.5 s tail.
    assert_eq!(report.frames, (2.5 * 44_100.0) as u64);
    let (spec, samples) = read_wav(&out_path);
    assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (2, 44_100, 16));
    assert_eq!(samples.len() as u64, report.frames * 2);
    assert!(rms(&samples[..44_100 * 2 / 2]) < 1e-4, "silence before the clip");
    assert!(rms(&samples[(1.2 * 44_100.0) as usize * 2..(1.8 * 44_100.0) as usize * 2]) > 0.1);

    // Export does not touch live playback.
    assert_eq!(controls.transport.state(), TransportState::Stopped);
    render(&mut session, 1_000);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn stems_are_written_per_bus() {
    let dir = temp_dir("stems");
    let session = start(true);
    let controls = session.controls.clone();
    let path = dir.join("song.wav");
    let report = controls
        .exporter
        .export(
            &ExportRequest {
                sample_rate: 44_100,
                bit_depth: BitDepth::Float32,
                stems: true,
                tail_seconds: 0.2,
                ..ExportRequest::new(path.clone())
            },
            &|_| {},
        )
        .unwrap();
    assert_eq!(report.files.len(), 4, "mix + 3 buses: {:?}", report.files);

    let mix = read_wav(&report.files[0]).1;
    // The stem name contains the bus name (the exact name format is the export's business).
    let stem = |bus: &str| report.files.iter().find(|f| f.to_string_lossy().contains(bus)).cloned().unwrap();
    let drums = read_wav(&stem("Drums")).1;
    let bass = read_wav(&stem("Bass")).1;
    assert!(rms(&mix) > 0.02 && rms(&drums) > 0.01 && rms(&bass) > 0.005);
    assert_eq!(mix.len(), drums.len());
    // Stems are not louder than the mix.
    assert!(peak(&drums) <= peak(&mix) + 1e-3 && peak(&bass) <= peak(&mix) + 1e-3);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn project_survives_save_and_load_including_samples() {
    let dir = temp_dir("saveload");
    write_wav(&dir.join("kick.wav"), 0.3, 80.0);

    let session = start(true);
    let controls = session.controls.clone();
    let info = controls.samples.import(&dir.join("kick.wav")).unwrap();
    controls.tempo.set_tempo(Bpm::new(99.0).unwrap()).unwrap();
    assert!(controls.project.is_dirty());

    let project_path = dir.join("proj").join("song.dawproj");
    controls.project.save(&project_path).unwrap();
    assert!(!controls.project.is_dirty());
    assert!(dir.join("proj/samples/kick.wav").exists(), "the sample is copied next to the project");

    let mut other = start(false);
    other.controls.project.load(&project_path).unwrap();
    assert!(other.controls.project.take_warnings().is_empty());
    assert_eq!(other.controls.tracks.tracks().len(), 5);
    assert_eq!(other.controls.tempo.tempo_map().base_tempo().get(), 99.0);
    assert_eq!(other.controls.samples.samples()[0].id, info.id);
    assert_eq!(other.controls.graph.graph(), controls.graph.graph(), "the graph is saved and loaded whole");

    // The loaded project plays the same.
    other.controls.transport.play();
    assert!(rms(&render(&mut other, 48_000)) > 0.02);

    // Loading works without the source file, and the loss goes to the warnings.
    std::fs::remove_file(dir.join("proj/samples/kick.wav")).unwrap();
    let third = start(false);
    third.controls.project.load(&project_path).unwrap();
    assert_eq!(third.controls.project.take_warnings().len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn midi_export_and_import_round_trip() {
    let dir = temp_dir("midi");
    let session = start(true);
    let controls = session.controls.clone();
    let synth = controls.tracks.tracks().into_iter().find(|t| t.instrument.as_ref().is_some_and(|i| i.kind == SYNTH_KIND)).unwrap().id;
    let clip = track_clips(&controls, synth)[0].id;
    let original = controls.notes.notes(clip).unwrap();
    assert_eq!(original.len(), 5);

    let file = dir.join("bass.mid");
    controls.midi_files.export_clip(clip, &file).unwrap();
    // The whole project is exported too (one file track per project track).
    controls.midi_files.export_project(&dir.join("all.mid")).unwrap();
    assert!(dir.join("all.mid").is_file());

    controls.notes.clear_notes(clip).unwrap();
    assert_eq!(controls.midi_files.import_into(clip, &file).unwrap(), 5);
    let back = controls.notes.notes(clip).unwrap();
    let shape = |notes: &[Note]| notes.iter().map(|n| (n.pitch, n.start_tick, n.length_ticks)).collect::<Vec<_>>();
    let mut a = shape(&original);
    let mut b = shape(&back);
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b, "the notes came back unchanged");

    assert!(controls.midi_files.import_into(clip, &dir.join("nope.mid")).is_err());
    std::fs::write(dir.join("junk.mid"), b"not midi").unwrap();
    assert!(controls.midi_files.import_into(clip, &dir.join("junk.mid")).is_err());
    std::fs::remove_dir_all(dir).ok();
}
