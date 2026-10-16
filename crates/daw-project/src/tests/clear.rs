//! The `ProjectClear` contract: all content is removed; the master, the file and (optionally) tempo and samples stay;
//! one undo step.

use super::*;
use crate::ProjectCleaner;

/// A project with everything clearing removes: a track with a clip and automation, an effect on the master, a bus,
/// a marker, a folder, a loop, tempo 90 and a sample.
fn full_rig() -> (Rig, ProjectCleaner) {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    rig.clips.add_note_clip(NewNoteClip::empty(track, 0, None)).unwrap();
    rig.graph.insert_effect_before(MASTER, "eq", 0.0, 0.0).unwrap();
    rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    rig.automation.add_lane(ParamTarget::InstrumentParam { track, param: "gain".into() }).unwrap();
    rig.markers.add_marker(0, "Intro").unwrap();
    rig.folders.create_folder("Folder", &[track]).unwrap();
    rig.clips.set_loop_region(Some((0, BAR_TICKS))).unwrap();
    rig.tempo.set_tempo(Bpm::new(90.0).unwrap()).unwrap();
    rig.graph.set_volume(MASTER, 0.5).unwrap();
    rig.catalog.import(Path::new("/music/kick.wav")).unwrap();
    let cleaner = ProjectCleaner::new(rig.service.clone());
    (rig, cleaner)
}

#[test]
fn clear_leaves_only_master_and_is_one_undo_step() {
    let (rig, cleaner) = full_rig();
    let before = rig.service.snapshot();
    cleaner.clear(ClearOptions::default()).unwrap();

    let after = rig.service.snapshot();
    assert!(after.tracks.is_empty() && after.clips.is_empty() && after.automation.is_empty());
    assert!(after.markers.is_empty() && after.folders.is_empty() && after.loop_region.is_none());
    assert_eq!(after.graph.nodes.len(), 1);
    assert_eq!(after.graph.nodes[0].kind, NodeKind::Master { volume: 0.5 }, "the master and its volume stay");
    assert!(after.graph.edges.is_empty());
    assert_eq!(after.tempo_map, TempoMap::default(), "the tempo is reset");
    assert_eq!(after.samples.len(), 1, "samples stay by default");
    assert!(rig.last_snapshot().project.tracks.is_empty(), "the engine got an empty project");
    assert_eq!(rig.service.undo_label().as_deref(), Some("Clear project"));

    assert!(rig.service.undo());
    assert_eq!(rig.service.snapshot(), before, "undo brings everything back in one step");
}

#[test]
fn clear_can_keep_tempo_and_remove_samples() {
    let (rig, cleaner) = full_rig();
    let tempo = rig.service.snapshot().tempo_map;
    cleaner.clear(ClearOptions { keep_tempo: true, remove_samples: true }).unwrap();
    let after = rig.service.snapshot();
    assert_eq!(after.tempo_map, tempo);
    assert!(after.samples.is_empty());
    assert!(rig.service.undo());
    assert_eq!(rig.service.snapshot().samples.len(), 1);
}

#[test]
fn clearing_an_empty_project_makes_no_undo_step() {
    let rig = Rig::new();
    let cleaner = ProjectCleaner::new(rig.service.clone());
    cleaner.clear(ClearOptions::default()).unwrap();
    assert!(!rig.service.can_undo());
}
