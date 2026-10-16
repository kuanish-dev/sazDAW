//! DSP object sync and publishing snapshots to the engine.

use super::*;

#[test]
fn initial_snapshot_is_published() {
    let rig = Rig::new();
    let snapshot = rig.last_snapshot();
    assert!(snapshot.project.graph.node(MASTER).is_some());
    assert!(snapshot.project.tracks.is_empty(), "an empty project has only the master");
    assert_eq!(snapshot.order.last(), Some(&MASTER), "the master is processed last");
    assert_eq!(snapshot.order.len(), snapshot.project.graph.nodes.len());
}

#[test]
fn adding_and_removing_tracks_hands_dsp_objects_to_the_engine() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    assert_eq!(rig.dsp_log(), [format!("add_instrument:{track}")], "the instrument is keyed by the track");
    assert!(rig.last_snapshot().project.tracks.iter().any(|t| t.id == track));

    let audio = rig.tracks.add_track("Vocals", None, 60).unwrap();
    assert_eq!(rig.dsp_log().len(), 1, "an audio track has no instrument");
    rig.tracks.remove_track(audio).unwrap();
    assert_eq!(rig.dsp_log().len(), 1);

    rig.tracks.remove_track(track).unwrap();
    assert_eq!(rig.dsp_log().last().unwrap(), &format!("remove_instrument:{track}"));
    assert_eq!(rig.last_snapshot().project.tracks, [] as [daw_contracts::Track; 0]);
    assert!(rig.tracks.add_track("x", Some("nope"), 60).is_err());
    assert!(rig.tracks.add_track("x", Some("synth"), 200).is_err());
}

#[test]
fn undoing_track_creation_removes_the_dsp_object() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    assert!(rig.service.undo());
    assert_eq!(rig.dsp_log().last().unwrap(), &format!("remove_instrument:{track}"));
    assert!(rig.service.redo());
    assert_eq!(rig.dsp_log().last().unwrap(), &format!("add_instrument:{track}"));
}

#[test]
fn changing_the_sample_param_rebuilds_the_instrument() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Sampler", Some("sampler"), 60).unwrap();
    let before = rig.dsp_log().len();
    rig.tracks.set_instrument_param(track, "gain", 0.3).unwrap();
    assert_eq!(rig.dsp_log().len(), before, "a regular parameter does not recreate the instrument");
    rig.tracks.set_instrument_param(track, "sample", 5.0).unwrap();
    assert_eq!(rig.dsp_log().len(), before + 1);
    assert_eq!(rig.dsp_log().last().unwrap(), &format!("add_instrument:{track}"));
    assert!(rig.tracks.set_instrument_param(track, "nope", 1.0).is_err());
    rig.tracks.set_instrument_param(track, "gain", 99.0).unwrap();
    assert_eq!(rig.tracks.tracks()[0].instrument.as_ref().unwrap().params["gain"], 1.0, "the value is clamped");

    let audio = rig.tracks.add_track("Vocals", None, 60).unwrap();
    assert!(rig.tracks.set_instrument_param(audio, "gain", 1.0).is_err(), "an audio track has no instrument");
    assert!(rig.tracks.set_instrument_param(9999, "gain", 1.0).is_err());
}

#[test]
fn effect_dsp_objects_follow_effect_nodes() {
    let rig = Rig::new();
    let a = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    let b = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    let log = rig.dsp_log();
    assert_eq!(log, [format!("add_effect:{a}"), format!("add_effect:{b}")], "buses have no DSP objects");

    rig.graph.set_effect_param(a, "gain", 3.0).unwrap();
    assert_eq!(rig.dsp_log().len(), 2, "a parameter change does not recreate the effect");

    rig.graph.remove_node(a).unwrap();
    assert_eq!(rig.dsp_log().last().unwrap(), &format!("remove_effect:{a}"));
    assert!(rig.service.undo());
    assert_eq!(rig.dsp_log().last().unwrap(), &format!("add_effect:{a}"));
}

#[test]
fn snapshot_order_covers_every_node_and_ends_with_master() {
    let (rig, track, _) = rig_with_clip();
    let fx = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    rig.graph.connect(fx, bus, 1.0, false).unwrap();
    let snapshot = rig.last_snapshot();
    assert_eq!(snapshot.order, snapshot.project.graph.order());
    assert_eq!(snapshot.order.len(), snapshot.project.graph.nodes.len());
    assert_eq!(snapshot.order.last(), Some(&MASTER));
    let at = |id| snapshot.order.iter().position(|n| *n == id).unwrap();
    assert!(at(fx) < at(bus));
    assert!(at(source(&rig, track)) < at(MASTER));
    assert_eq!(snapshot.project.clips.len(), 1, "tracks and clips travel in the snapshot");
}

#[test]
fn snapshot_versions_increase_with_every_change() {
    let rig = Rig::new();
    let first = rig.last_snapshot().version;
    rig.tracks.add_track("Track", None, 60).unwrap();
    assert!(rig.last_snapshot().version > first);
}

#[test]
fn an_instrument_that_cannot_be_created_is_logged_once_and_kept_as_a_warning() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Plugin", Some("broken"), 60).unwrap();
    rig.tracks.rename_track(track, "Plugin 2").unwrap();
    let entries = rig.log.0.lock().unwrap().clone();
    assert_eq!(entries.len(), 1, "another edit does not repeat the failed attempt: {entries:?}");
    assert_eq!((entries[0].source.as_str(), entries[0].level), ("dsp", LogLevel::Error));
    assert!(entries[0].message.contains("Could not load the plugin: broken"), "{}", entries[0].message);
    let warnings = rig.files.take_warnings();
    assert_eq!(warnings.len(), 1, "and in the project warnings");
    assert!(warnings[0].is(codes::tracks::INSTRUMENT_NOT_CREATED));
    assert!(!rig.dsp_log().contains(&format!("add_instrument:{track}")));
}
