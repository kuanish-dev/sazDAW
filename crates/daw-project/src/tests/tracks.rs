//! The `TrackControl` contract: tracks, their instruments and source nodes.

use super::*;

#[test]
fn set_instrument_swaps_the_kind_keeps_clips_and_drops_instrument_automation_in_one_undo_step() {
    let (rig, track, clip) = rig_with_clip();
    rig.notes.add_note(clip, note(60, 0, 100)).unwrap();
    rig.tracks.set_instrument_param(track, "gain", 0.9).unwrap();
    rig.automation.add_lane(ParamTarget::InstrumentParam { track, param: "gain".into() }).unwrap();
    let before = rig.dsp_log().len();

    rig.tracks.set_instrument(track, "sampler", Params::new(), None).unwrap();
    let instrument = rig.tracks.tracks()[0].instrument.clone().unwrap();
    assert_eq!(instrument.kind, "sampler");
    assert_eq!(instrument.params, FakeInstruments.defaults("sampler").unwrap(), "the new kind's params are the defaults");
    assert_eq!(rig.tracks.tracks()[0].name, "Synth", "the name stays");
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 1, "the notes stay");
    assert_eq!(rig.automation.lanes(), [], "automation of the old kind's params is removed");
    assert_eq!(rig.dsp_log().len(), before + 1);
    assert_eq!(rig.dsp_log().last().unwrap(), &format!("add_instrument:{track}"), "the engine got the new instrument");

    assert!(rig.service.undo(), "the change is one undo step");
    assert_eq!(rig.tracks.tracks()[0].instrument.as_ref().unwrap().kind, "synth");
    assert_eq!(rig.automation.lanes().len(), 1);
}

#[test]
fn set_instrument_to_the_same_kind_changes_nothing_and_bad_requests_fail() {
    let (rig, track, _) = rig_with_clip();
    rig.tracks.set_instrument_param(track, "gain", 0.9).unwrap();
    let revision = rig.service.revision();
    rig.tracks.set_instrument(track, "synth", Params::new(), None).unwrap();
    assert_eq!(rig.service.revision(), revision, "the same kind makes no edit");
    assert_eq!(rig.tracks.tracks()[0].instrument.as_ref().unwrap().params["gain"], 0.9, "the params are not reset");

    let unknown = rig.tracks.set_instrument(track, "nope", Params::new(), None);
    assert!(matches!(unknown, Err(e) if e.is(codes::tracks::UNKNOWN_INSTRUMENT)));
    let audio = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let audio_track = rig.tracks.set_instrument(audio, "synth", Params::new(), None);
    assert!(matches!(audio_track, Err(e) if e.is(codes::tracks::NO_INSTRUMENT)), "an audio track has no instrument");
    assert!(matches!(rig.tracks.set_instrument(9999, "synth", Params::new(), None), Err(e) if e.is(codes::tracks::NOT_FOUND)));
}

#[test]
fn set_instrument_with_a_name_renames_the_track_and_its_node_in_the_same_undo_step() {
    let (rig, track, _) = rig_with_clip();
    rig.tracks.set_instrument(track, "sampler", Params::new(), Some("Kick")).unwrap();
    assert_eq!(rig.tracks.tracks()[0].name, "Kick");
    assert_eq!(rig.graph.graph().node(source(&rig, track)).unwrap().name, "Kick");
    assert!(rig.service.undo(), "the change and the name are one step");
    assert_eq!(rig.tracks.tracks()[0].name, "Synth");
    assert_eq!(rig.graph.graph().node(source(&rig, track)).unwrap().name, "Synth");

    let revision = rig.service.revision();
    rig.tracks.set_instrument(track, "synth", Params::new(), None).unwrap();
    assert_eq!(rig.service.revision(), revision, "the same kind without a name makes no edit");
    rig.tracks.set_instrument(track, "synth", Params::new(), Some("Bass")).unwrap();
    assert_eq!(rig.tracks.tracks()[0].name, "Bass", "the same kind with a name renames");
}

#[test]
fn set_instrument_applies_params_over_defaults_clamped_in_the_same_undo_step() {
    let (rig, track, _) = rig_with_clip();
    let params = Params::from([("sample".to_string(), 7.0), ("gain".to_string(), 99.0)]);
    rig.tracks.set_instrument(track, "sampler", params, None).unwrap();
    let instrument = rig.tracks.tracks()[0].instrument.clone().unwrap();
    assert_eq!((instrument.kind.as_str(), instrument.params["sample"], instrument.params["gain"]), ("sampler", 7.0, 1.0));
    assert!(rig.service.undo(), "the change with params is one step");
    assert_eq!(rig.tracks.tracks()[0].instrument.as_ref().unwrap().kind, "synth");

    let same = Params::from([("sample".to_string(), 3.0)]);
    rig.tracks.set_instrument(track, "sampler", Params::new(), None).unwrap();
    rig.tracks.set_instrument(track, "sampler", same, None).unwrap();
    assert_eq!(rig.tracks.tracks()[0].instrument.as_ref().unwrap().params["sample"], 3.0, "the same kind with params applies them");
    assert!(rig.tracks.set_instrument(track, "sampler", Params::from([("nope".to_string(), 1.0)]), None).is_err());
}

#[test]
fn add_track_creates_the_track_a_source_node_and_an_edge_to_master() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 64).unwrap();
    let tracks = rig.tracks.tracks();
    assert_eq!((tracks[0].name.as_str(), tracks[0].default_pitch), ("Synth", 64));
    assert_eq!(tracks[0].instrument.as_ref().unwrap().kind, "synth");

    let graph = rig.graph.graph();
    let node = graph.source_of(track).expect("the source node");
    assert_eq!(node.name, "Synth");
    assert_eq!(node.kind, NodeKind::Instrument { track, strip: Strip::default() });
    assert_eq!(graph.edges, [Edge { from: node.id, to: MASTER, level: 1.0, sidechain: false, pre_fader: false }]);
    assert_eq!(graph.chain(node.id), TrackChain { effects: vec![], output: Some(MASTER) });
    assert_ne!(node.id, track, "the track and its node have different ids");

    let audio = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let node = rig.graph.graph().source_of(audio).unwrap().clone();
    assert!(matches!(node.kind, NodeKind::Audio { track: t, .. } if t == audio));
    assert!(rig.tracks.tracks()[1].is_audio());
    assert_eq!(edge_count(&rig, node.id, MASTER), 1);
}

#[test]
fn removing_a_track_cleans_clips_node_edges_and_automation() {
    let (rig, track, clip) = rig_with_clip();
    rig.notes.add_note(clip, note(60, 0, 240)).unwrap();
    let node = source(&rig, track);
    let fx = rig.graph.add_effect("eq", 0.0, 0.0).unwrap();
    rig.graph.connect(node, fx, 1.0, false).unwrap();
    rig.automation.add_lane(ParamTarget::InstrumentParam { track, param: "gain".into() }).unwrap();
    rig.automation.add_lane(ParamTarget::NodeVolume { node }).unwrap();
    let other = rig.tracks.add_track("Other", Some("synth"), 60).unwrap();
    let other_clip = rig.clips.add_note_clip(NewNoteClip::empty(other, 0, None)).unwrap();

    rig.tracks.remove_track(track).unwrap();
    let graph = rig.graph.graph();
    assert!(graph.node(node).is_none());
    assert!(graph.edges.iter().all(|e| e.from != node && e.to != node));
    assert!(graph.node(fx).is_some(), "the effect stays");
    assert_eq!(rig.automation.lanes(), [] as [daw_contracts::AutomationLane; 0]);
    assert_eq!(rig.clips.clips(), [other_clip], "the removed track's clips are gone, others stay");
    assert!(rig.tracks.remove_track(track).is_err());

    assert!(rig.service.undo(), "removing a track is one undo step");
    assert!(rig.graph.graph().node(node).is_some());
    assert_eq!(rig.clips.clips().len(), 2);
    assert_eq!(rig.automation.lanes().len(), 2);
}

#[test]
fn renaming_a_track_renames_its_node() {
    let (rig, track, _) = rig_with_clip();
    rig.tracks.rename_track(track, "Bass").unwrap();
    assert_eq!(rig.tracks.tracks()[0].name, "Bass");
    assert_eq!(rig.graph.graph().source_of(track).unwrap().name, "Bass");
    assert!(rig.tracks.rename_track(9999, "x").is_err());
}

#[test]
fn add_tracks_creates_a_batch_with_params_in_one_undo_step() {
    let rig = Rig::new();
    let make = |name: &str, gain: f32| NewTrack {
        name: name.into(),
        kind: Some("sampler".into()),
        default_pitch: 60,
        params: Params::from([("gain".to_string(), gain), ("sample".to_string(), 7.0)]),
    };
    let audio = NewTrack { name: "Vocals".into(), kind: None, default_pitch: 60, params: Params::new() };
    let ids = rig.tracks.add_tracks(vec![make("Kick", 0.5), make("Snare", 5.0), audio]).unwrap();
    assert_eq!(ids.len(), 3);
    let tracks = rig.tracks.tracks();
    let params = |i: usize| &tracks[i].instrument.as_ref().unwrap().params;
    assert_eq!(params(0)["sample"], 7.0);
    assert_eq!(params(0)["gain"], 0.5);
    assert_eq!(params(1)["gain"], 1.0, "the value is clamped to the range");
    assert!(tracks[2].is_audio());
    assert_eq!(rig.dsp_log().iter().filter(|l| l.starts_with("add_instrument")).count(), 2);
    assert_eq!(rig.graph.graph().nodes.len(), 4, "the master and three sources");

    assert!(rig.service.undo());
    assert!(rig.tracks.tracks().is_empty(), "all tracks went away with one undo");
    assert_eq!(rig.graph.graph().nodes.len(), 1);
}

#[test]
fn add_tracks_is_all_or_nothing() {
    let rig = Rig::new();
    let good = NewTrack { name: "ok".into(), kind: Some("synth".into()), default_pitch: 60, params: Params::new() };
    let bad_param = NewTrack { params: Params::from([("nope".to_string(), 1.0)]), ..good.clone() };
    let bad_kind = NewTrack { kind: Some("ghost".into()), ..good.clone() };
    let audio_with_params = NewTrack { kind: None, params: Params::from([("gain".to_string(), 1.0)]), ..good.clone() };
    let fails = |tracks, code| matches!(rig.tracks.add_tracks(tracks), Err(e) if e.is(code));
    assert!(fails(vec![good.clone(), bad_param], codes::tracks::UNKNOWN_INSTRUMENT_PARAM));
    assert!(fails(vec![good.clone(), bad_kind], codes::tracks::UNKNOWN_INSTRUMENT));
    assert!(fails(vec![good.clone(), audio_with_params], codes::tracks::AUDIO_TRACK_TAKES_NO_PARAMS));
    assert!(fails(vec![NewTrack { default_pitch: 200, ..good }], codes::tracks::DEFAULT_PITCH_OUT_OF_RANGE));
    assert_eq!(rig.tracks.tracks(), [] as [daw_contracts::Track; 0]);
    assert_eq!(rig.tracks.add_tracks(Vec::new()).unwrap(), [] as [u32; 0]);
}

#[test]
fn a_track_has_its_own_height_until_reset_and_a_drag_is_one_undo_step() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let height = || rig.tracks.tracks()[0].height;
    assert_eq!(height(), None, "a new track follows the interface setting");

    // A drag sends several heights in a row: they merge into one undo step.
    for px in [80, 64, 48] {
        rig.tracks.set_height(track, Some(px)).unwrap();
    }
    assert_eq!(height(), Some(48));
    assert_eq!(rig.service.undo_label().as_deref(), Some("Track height"));
    assert!(rig.service.undo());
    assert_eq!(height(), None, "one undo takes the whole drag back");

    rig.tracks.set_height(track, Some(MAX_TRACK_HEIGHT)).unwrap();
    let revision = rig.service.revision();
    rig.tracks.set_height(track, Some(MAX_TRACK_HEIGHT)).unwrap();
    assert_eq!(rig.service.revision(), revision, "the same height records no step");
    rig.tracks.set_height(track, None).unwrap();
    assert_eq!(height(), None, "back to the interface setting");

    for wrong in [MIN_TRACK_HEIGHT - 1, MAX_TRACK_HEIGHT + 1] {
        assert!(matches!(rig.tracks.set_height(track, Some(wrong)), Err(e) if e.is(codes::tracks::HEIGHT_OUT_OF_RANGE)));
    }
    assert!(matches!(rig.tracks.set_height(9_999, Some(60)), Err(e) if e.is(codes::tracks::NOT_FOUND)));
}
