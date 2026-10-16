//! The whole session: the startup project, importing and cutting sounds into the project folder, picking a sound from
//! the state folder, previewing files.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use std::fs;
use std::path::{Path, PathBuf};

use daw_bootstrap::{Session, SessionConfig, build_headless_session_in};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("daw-boot-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Creates a project the way the user does from the welcome window.
fn create(session: &Session, root: &Path, name: &str) -> PathBuf {
    let file = root.join(name).join(format!("{name}.dawproj"));
    session.controls.project.create(&file).unwrap();
    file
}

#[test]
fn nothing_is_opened_until_the_user_creates_a_project_then_the_last_one_reopens() {
    let root = temp("startup");
    let (state, projects) = (root.join("state"), root.join("projects"));

    let first = build_headless_session_in(&SessionConfig::new(state.clone()));
    assert_eq!(first.startup().open(), None, "first start: no project");
    assert_eq!(first.controls.project.path(), None);
    assert_eq!(first.controls.project.folder(), None);
    assert!(!projects.exists(), "nothing is created by itself");

    let file = create(&first, &projects, "Beat");
    first.controls.tracks.add_track("Kick", Some("kick"), 36).unwrap();
    first.controls.tempo.set_tempo(daw_contracts::Bpm::new(99.0).unwrap()).unwrap();
    first.controls.project.save(&file).unwrap();

    let second = build_headless_session_in(&SessionConfig::new(state.clone()));
    assert_eq!(second.startup().open(), Some(file.clone()), "the last project is open");
    assert_eq!(second.controls.tracks.tracks().len(), 1);
    assert_eq!(second.controls.tempo.tempo_map().base_tempo(), daw_contracts::Bpm::new(99.0).unwrap());

    fs::remove_dir_all(file.parent().unwrap()).unwrap();
    let third = build_headless_session_in(&SessionConfig::new(state.clone()));
    assert_eq!(third.startup().open(), None, "the project was deleted by hand: again nothing is open");
    assert_eq!(third.controls.project.path(), None);
    assert!(third.controls.recent.list().is_empty(), "the missing project is forgotten");
    fs::remove_dir_all(root).ok();
}

#[test]
fn the_last_project_is_not_opened_when_the_setting_is_off() {
    let root = temp("startup-off");
    let (state, projects) = (root.join("state"), root.join("projects"));
    let first = build_headless_session_in(&SessionConfig::new(state.clone()));
    let file = create(&first, &projects, "Beat");
    first.controls.project.save(&file).unwrap();
    let mut preferences = first.controls.preferences.get();
    preferences.profile.open_last_project = false;
    first.controls.preferences.set(preferences).unwrap();

    let second = build_headless_session_in(&SessionConfig::new(state));
    assert_eq!(second.startup().open(), None, "the setting is off: start without a project");
    assert_eq!(second.controls.recent.list(), vec![file], "the recent list is untouched");
    fs::remove_dir_all(root).ok();
}

#[test]
fn a_broken_latest_project_falls_back_to_the_previous_one() {
    let root = temp("fallback");
    let state = root.join("state");
    let first = build_headless_session_in(&SessionConfig::new(state.clone()));
    let (a, b) = (create(&first, &root, "A"), create(&first, &root, "B"));
    fs::write(&b, "{ not json").unwrap();

    let second = build_headless_session_in(&SessionConfig::new(state));
    assert_eq!(second.startup().open(), Some(a));
    fs::remove_dir_all(root).ok();
}

#[test]
fn importing_into_a_project_copies_the_sound_and_autosave_goes_to_its_folder() {
    let root = temp("assets");
    let first = build_headless_session_in(&SessionConfig::new(root.join("state")));
    let file = create(&first, &root.join("p"), "P");
    let wav = root.join("kick.wav");
    daw_contracts::SampleWriter::write(
        &daw_samples::WavWriter,
        &wav,
        &daw_contracts::SampleData { sample_rate: 48_000, channels: 1, frames: vec![0.1; 480] },
    )
    .unwrap();
    let info = first.controls.samples.import(&wav).unwrap();
    assert_eq!(info.path, file.parent().unwrap().join("samples/kick.wav"));
    assert!(info.path.is_file());
    assert!(first.controls.project.is_dirty());
    fs::remove_dir_all(root).ok();
}

#[test]
fn extracted_regions_become_independent_playable_files() {
    let root = temp("extract");
    let session = build_headless_session_in(&SessionConfig::new(root.join("state")));
    let file = create(&session, &root.join("p"), "P");
    let wav = root.join("song.wav");
    let frames: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
    daw_contracts::SampleWriter::write(
        &daw_samples::WavWriter,
        &wav,
        &daw_contracts::SampleData { sample_rate: 48_000, channels: 1, frames },
    )
    .unwrap();
    let song = session.controls.samples.import(&wav).unwrap();
    let regions = [daw_contracts::Region { from: 12_000, to: 24_000, name: "Chunk".into() }];
    let part = session.controls.sample_editor.extract_regions(song.id, &regions).unwrap().remove(0);

    assert_eq!(part.frame_count, 12_000, "the file holds only the cut part");
    assert_eq!(part.path, file.parent().unwrap().join("samples/Chunk.wav"));
    assert!(part.path.is_file());
    // The cut sound is a separate file: the source can be deleted.
    fs::remove_file(song.path).unwrap();
    assert!(part.path.is_file());
    fs::remove_dir_all(root).ok();
}

#[test]
fn audio_choice_is_read_from_the_state_dir_and_audio_does_not_start_by_itself() {
    let root = temp("audio");
    let chosen = daw_contracts::AudioSettings {
        output: Some("Speakers".into()),
        input: None,
        buffer_frames: Some(256),
        record_latency_ms: Some(15.0),
    };
    std::fs::create_dir_all(root.join("state")).unwrap();
    std::fs::write(root.join("state/audio.json"), r#"{ "output": "Speakers", "bufferFrames": 256, "recordLatencyMs": 15.0 }"#).unwrap();

    // A host without devices, as in `build_session`: the test needs no sound card.
    let session = build_headless_session_in(&SessionConfig::new(root.join("state")));
    assert_eq!(session.controls.audio.settings(), chosen);
    assert!(!session.controls.audio.status().running);
    fs::remove_dir_all(root).ok();
}

#[test]
fn preferences_live_in_the_state_dir_and_survive_a_restart() {
    let root = temp("preferences");
    let state = root.join("state");
    let session = build_headless_session_in(&SessionConfig::new(state.clone()));
    let mut prefs = session.controls.preferences.get();
    assert_eq!(prefs, daw_contracts::Preferences::default());
    prefs.profile.author = "Anya".into();
    prefs.autosave_seconds = 0;
    session.controls.preferences.set(prefs.clone()).unwrap();
    assert!(state.join("preferences.json").is_file());
    let _autosave = session.start_autosave();

    let again = build_headless_session_in(&SessionConfig::new(state));
    assert_eq!(again.controls.preferences.get(), prefs);
    fs::remove_dir_all(root).ok();
}

#[test]
fn library_files_can_be_auditioned_without_importing() {
    let root = temp("audition");
    let mut session = build_headless_session_in(&SessionConfig::new(root.join("state")));
    session.renderer.prepare(daw_contracts::StreamConfig { sample_rate: 48_000, channels: 2 });
    let wav = root.join("hit.wav");
    daw_contracts::SampleWriter::write(
        &daw_samples::WavWriter,
        &wav,
        &daw_contracts::SampleData { sample_rate: 48_000, channels: 1, frames: vec![0.5; 4_800] },
    )
    .unwrap();
    session.controls.preview.play_file(&wav).unwrap();
    let mut out = vec![0.0f32; 2 * 512];
    session.renderer.render(&mut out);
    assert!(out.iter().any(|v| v.abs() > 0.01), "the sound is audible");
    assert!(session.controls.samples.samples().is_empty(), "nothing is added to the project");
    assert!(session.controls.preview.play_file(&root.join("nope.wav")).is_err());
    fs::remove_dir_all(root).ok();
}
