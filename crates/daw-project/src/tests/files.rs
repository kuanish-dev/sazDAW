//! The `ProjectIo` and `ProjectView` contracts: create, open, save, restore, read the whole project.

use super::*;

#[test]
fn save_and_load_round_trip_with_dirty_tracking() {
    let (rig, _, clip) = rig_with_clip();
    rig.notes.add_note(clip, note(64, 0, 240)).unwrap();
    rig.tempo.set_tempo(Bpm::new(97.0).unwrap()).unwrap();
    assert!(rig.files.is_dirty());

    rig.files.save(Path::new("/p/song.daw")).unwrap();
    assert!(!rig.files.is_dirty());
    assert_eq!(rig.storage.load(Path::new("/p/song.daw")).unwrap().tempo_map.base_tempo().get(), 97.0);

    rig.files.new_project();
    assert_eq!(rig.tracks.tracks(), [] as [daw_contracts::Track; 0]);
    assert_eq!(rig.clips.clips(), [] as [daw_contracts::Clip; 0]);
    assert_eq!(rig.graph.graph().nodes.len(), 1, "only the master");
    assert!(!rig.service.can_undo());
    rig.tempo.set_tempo(Bpm::new(140.0).unwrap()).unwrap();

    rig.files.load(Path::new("/p/song.daw")).unwrap();
    assert_eq!(rig.tracks.tracks().len(), 1);
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 1);
    assert_eq!(rig.tempo.tempo_map().base_tempo().get(), 97.0);
    assert!(!rig.files.is_dirty());
    assert!(matches!(rig.files.load(Path::new("/p/none.daw")), Err(e) if e.is(codes::project::FILE_NOT_READ)));
    assert!(rig.dsp_log().iter().filter(|l| l.starts_with("add_instrument")).count() >= 2, "after loading the instrument is created again");
}

#[test]
fn loading_reports_missing_sample_files_as_warnings() {
    let rig = Rig::new();
    rig.catalog.import(Path::new("/music/ok.wav")).unwrap();
    let mut project = rig.service.snapshot();
    project.samples.push(SampleRef { id: 77, name: "lost".into(), path: "/music/missing.wav".into(), markers: Vec::new() });
    rig.storage.save(Path::new("/p/a.daw"), &project).unwrap();
    rig.files.load(Path::new("/p/a.daw")).unwrap();
    let warnings = rig.files.take_warnings();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].is(codes::project::SAMPLE_NOT_LOADED));
    assert!(warnings[0].message().contains("\"lost\""), "{}", warnings[0]);
    assert!(rig.files.take_warnings().is_empty());
}

#[test]
fn creating_a_project_names_it_after_the_file_and_remembers_it() {
    let rig = Rig::new();
    let file = Path::new("/music/Бит/Бит.dawproj");
    rig.files.create(file).unwrap();
    assert_eq!(rig.files.project_name(), "Бит");
    assert_eq!(rig.files.path().as_deref(), Some(file));
    assert_eq!(rig.files.folder().as_deref(), Some(Path::new("/music/Бит")));
    assert!(!rig.files.is_dirty());
    assert_eq!(rig.recent.last().as_deref(), Some(file));
    assert!(rig.storage.load(file).is_ok(), "the project file is created at once");
    assert!(matches!(rig.files.create(file), Err(e) if e.is(codes::project::ALREADY_EXISTS)), "an existing project is not overwritten");
}

#[test]
fn save_as_renames_the_project_and_updates_recent() {
    let rig = Rig::new();
    rig.files.create(Path::new("/m/A/A.dawproj")).unwrap();
    rig.files.save(Path::new("/m/B/B.dawproj")).unwrap();
    assert_eq!(rig.files.project_name(), "B");
    assert_eq!(rig.recent.list(), [Path::new("/m/B/B.dawproj"), Path::new("/m/A/A.dawproj")]);
    rig.files.load(Path::new("/m/A/A.dawproj")).unwrap();
    assert_eq!(rig.recent.last().as_deref(), Some(Path::new("/m/A/A.dawproj")));
}

#[test]
fn a_failed_save_as_keeps_the_name_and_the_file_and_a_good_one_publishes_the_new_name() {
    let rig = Rig::new();
    rig.files.create(Path::new("/m/A/A.dawproj")).unwrap();
    let refused = |_: &Project| Err(codes::project::FILE_NOT_WRITTEN.with("path", "/m/B/B.dawproj").with("details", "disk full"));
    assert!(rig.service.save_with(Path::new("/m/B/B.dawproj"), refused).is_err());
    assert_eq!(rig.files.project_name(), "A", "the project is not renamed after a file it was not written to");
    assert_eq!(rig.files.path().as_deref(), Some(Path::new("/m/A/A.dawproj")));

    let before = rig.service.revision();
    rig.files.save(Path::new("/m/B/B.dawproj")).unwrap();
    assert_eq!(rig.service.snapshot().name, "B");
    assert!(rig.service.revision() > before, "readers learn the new name");
}

#[test]
fn restoring_an_autosave_keeps_the_path_and_marks_the_project_dirty() {
    let rig = Rig::new();
    let main = Path::new("/m/P/P.dawproj");
    rig.files.create(main).unwrap();
    let mut recovered = rig.service.snapshot();
    recovered.name = "From autosave".into();
    rig.storage.save(Path::new("/m/P/autosave/autosave.dawproj"), &recovered).unwrap();

    rig.files.restore_from(Path::new("/m/P/autosave/autosave.dawproj")).unwrap();
    assert_eq!(rig.files.project_name(), "From autosave");
    assert_eq!(rig.files.path().as_deref(), Some(main), "the project file stays the same");
    assert!(rig.files.is_dirty(), "the restored project needs saving");
    assert!(rig.files.restore_from(Path::new("/nope")).is_err());
}
