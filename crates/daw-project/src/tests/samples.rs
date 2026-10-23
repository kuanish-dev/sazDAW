//! The `SampleCatalog` and `SampleEditor` contracts: import, removal, waveform, markers, cutting regions.

use super::*;

#[test]
fn importing_samples_dedupes_and_feeds_the_engine() {
    let rig = Rig::new();
    let info = rig.catalog.import(Path::new("/music/kick.wav")).unwrap();
    assert_eq!(info.frame_count, 48_000);
    assert!(rig.last_snapshot().samples.contains_key(&info.id), "the data is in the engine snapshot");
    assert_eq!(rig.catalog.import(Path::new("/music/kick.wav")).unwrap().id, info.id);
    assert_eq!(rig.catalog.samples().len(), 1);
    assert!(matches!(rig.catalog.import(Path::new("/music/missing.wav")), Err(e) if e.is(codes::samples::FILE_READ_FAILED)));
    assert!(rig.service.undo());
    assert_eq!(rig.catalog.samples(), [] as [daw_contracts::SampleInfo; 0]);
}

#[test]
fn removing_a_sample_drops_its_clips_resets_samplers_and_undoes_in_one_step() {
    let rig = Rig::new();
    let kept = rig.catalog.import(Path::new("/music/keep.wav")).unwrap();
    let gone = rig.catalog.import(Path::new("/music/gone.wav")).unwrap();
    let audio = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let on_gone = rig.clips.add_audio_clip(audio, 0, gone.id).unwrap();
    let on_kept = rig.clips.add_audio_clip(audio, 4000, kept.id).unwrap();
    let sampler = rig.tracks.add_track("Sampler", Some("sampler"), 60).unwrap();
    rig.tracks.set_instrument_param(sampler, "sample", gone.id as f32).unwrap();

    rig.catalog.remove(gone.id).unwrap();
    assert_eq!(rig.catalog.samples().iter().map(|s| s.id).collect::<Vec<_>>(), vec![kept.id]);
    let clips = rig.clips.clips();
    assert!(clips.iter().all(|c| c.id != on_gone.id) && clips.iter().any(|c| c.id == on_kept.id));
    let chosen = rig.tracks.tracks().into_iter().find(|t| t.id == sampler).unwrap().instrument.unwrap().params["sample"];
    assert_eq!(chosen, 0.0);
    assert!(matches!(rig.catalog.remove(gone.id), Err(e) if e.is(codes::samples::NOT_FOUND)), "the second time is an error");

    assert!(rig.service.undo());
    assert_eq!(rig.catalog.samples().len(), 2);
    assert!(rig.clips.clips().iter().any(|c| c.id == on_gone.id));
    assert!(rig.last_snapshot().samples.contains_key(&gone.id), "the sound is back after undo");
}

#[test]
fn waveform_is_computed_for_the_requested_range_and_validated() {
    let rig = Rig::new();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();
    let wave = rig.editor.waveform(sample.id, 0, 48_000, 100).unwrap();
    assert_eq!((wave.frame_count, wave.sample_rate, wave.peaks.len()), (48_000, 48_000, 100));
    assert!(wave.peaks.iter().all(|(low, high)| *low <= *high && (*high - 0.1).abs() < 1e-6));

    let part = rig.editor.waveform(sample.id, 1_000, 999_999_999, 10).unwrap();
    assert_eq!((part.from, part.to), (1_000, 48_000), "the bounds are clamped to the file length");
    assert!(matches!(rig.editor.waveform(sample.id, 0, 10, 0), Err(e) if e.is(codes::samples::WAVEFORM_BUCKETS_OUT_OF_RANGE)));
    assert!(rig.editor.waveform(sample.id, 0, 10, 1_000_000).is_err());
    assert!(rig.editor.waveform(4242, 0, 10, 10).is_err());
}

#[test]
fn markers_are_sorted_deduped_kept_in_range_and_undoable() {
    let rig = Rig::new();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();
    rig.editor.set_markers(sample.id, vec![30_000, 10_000, 10_000, 0, 48_000, 99_999]).unwrap();
    assert_eq!(rig.catalog.samples()[0].markers, [10_000, 30_000], "no zeros, repeats or positions beyond the file");
    assert_eq!(rig.service.snapshot().samples[0].markers, [10_000, 30_000], "the markers live in the project model");

    assert!(rig.editor.set_markers(4242, vec![1]).is_err());
    assert!(rig.service.undo());
    assert_eq!(rig.catalog.samples()[0].markers, [] as [u64; 0]);
}

#[test]
fn export_region_writes_only_the_selected_frames() {
    let rig = Rig::new();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();
    rig.editor.export_region(sample.id, 1_000, 3_500, Path::new("/out/slice.wav")).unwrap();
    assert_eq!(rig.writer.0.lock().unwrap().as_slice(), [(Path::new("/out/slice.wav").to_path_buf(), 2_500)]);
    let empty = rig.editor.export_region(sample.id, 500, 500, Path::new("/out/x.wav"));
    assert!(matches!(empty, Err(e) if e.is(codes::samples::REGION_EMPTY)));
    let readonly = rig.editor.export_region(sample.id, 0, 100, Path::new("/readonly/x.wav"));
    assert!(matches!(readonly, Err(e) if e.is(codes::samples::WAV_WRITE_FAILED)));
    assert!(rig.editor.export_region(999, 0, 100, Path::new("/out/x.wav")).is_err());
}

#[test]
fn imported_samples_are_adopted_into_the_project_folder() {
    let rig = Rig::new();
    rig.files.create(Path::new("/m/P/P.dawproj")).unwrap();
    let info = rig.catalog.import(Path::new("/music/kick.wav")).unwrap();
    assert_eq!(info.path, Path::new("/m/P/samples/kick.wav"));
    assert_eq!(rig.catalog.import(Path::new("/music/kick.wav")).unwrap().id, info.id, "importing again makes no duplicates");
    assert_eq!(rig.catalog.import(Path::new("/m/P/samples/kick.wav")).unwrap().id, info.id);
}

#[test]
fn a_failed_copy_into_the_project_is_an_error_not_a_link_to_the_original() {
    let rig = Rig::new();
    rig.files.create(Path::new("/m/P/P.dawproj")).unwrap();
    assert!(matches!(rig.catalog.import(Path::new("/packs/unreadable.wav")), Err(e) if e.is(codes::project::SAMPLE_NOT_COPIED)));
    assert!(rig.catalog.samples().is_empty(), "the project does not depend on an outside file");
}

#[test]
fn extracting_regions_writes_standalone_files_into_the_project_and_imports_them() {
    let rig = Rig::new();
    rig.files.create(Path::new("/m/P/P.dawproj")).unwrap();
    let source = rig.catalog.import(Path::new("/music/song.wav")).unwrap();
    let regions = [Region { from: 1_000, to: 3_500, name: "Kick".into() }, Region { from: 5_000, to: 6_000, name: "Snare".into() }];
    let infos = rig.editor.extract_regions(source.id, &regions).unwrap();
    assert_eq!(infos.len(), 2);
    assert_eq!(infos[0].path, Path::new("/m/P/samples/Kick.wav"));
    assert_eq!(infos[1].path, Path::new("/m/P/samples/Snare.wav"));
    assert_ne!(infos[0].id, source.id);
    let writes = rig.writer.0.lock().unwrap().clone();
    assert_eq!(writes, [(PathBuf::from("/m/P/samples/Kick.wav"), 2_500), (PathBuf::from("/m/P/samples/Snare.wav"), 1_000)]);
    assert_eq!(rig.catalog.samples().len(), 3, "the source stays");

    assert!(matches!(rig.editor.extract_regions(source.id, &[]), Err(e) if e.is(codes::samples::NO_REGIONS)));
    let empty = rig.editor.extract_regions(source.id, &[Region { from: 10, to: 10, name: "x".into() }]);
    assert!(matches!(empty, Err(e) if e.is(codes::samples::REGION_EMPTY)));
    assert!(rig.editor.extract_regions(999, &regions).is_err());
}

#[test]
fn extracting_regions_is_one_undo_step_and_a_failure_midway_changes_nothing() {
    let rig = Rig::new();
    rig.files.create(Path::new("/m/P/P.dawproj")).unwrap();
    let source = rig.catalog.import(Path::new("/music/song.wav")).unwrap();
    let ok = Region { from: 0, to: 100, name: "Kick".into() };
    rig.editor.extract_regions(source.id, &[ok.clone(), Region { name: "Snare".into(), ..ok.clone() }]).unwrap();
    assert_eq!(rig.service.undo_label().as_deref(), Some("Cut regions"));
    assert!(rig.service.undo());
    assert_eq!(rig.catalog.samples().len(), 1, "both regions went away with one undo");

    let broken = Region { name: "readonly".into(), ..ok.clone() };
    assert!(matches!(rig.editor.extract_regions(source.id, &[ok.clone(), broken]), Err(e) if e.is(codes::samples::WAV_WRITE_FAILED)));
    assert_eq!(rig.catalog.samples().len(), 1, "the project is as it was");
    assert!(rig.service.can_redo(), "the failure recorded no step and did not clear redo");

    let many = vec![ok; MAX_REGIONS + 1];
    let too_many = rig.editor.extract_regions(source.id, &many);
    assert_eq!(too_many, Err(codes::samples::TOO_MANY_REGIONS.with("max", MAX_REGIONS)));
}
