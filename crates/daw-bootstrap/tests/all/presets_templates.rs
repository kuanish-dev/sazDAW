//! Full session: instrument and effect presets (save, apply in one undo step, remove) and project templates (the
//! structure without clips, sampler sounds travel with the template, a new project from a template).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

fn instrument_param(controls: &Controls, track: TrackId, name: &str) -> f32 {
    controls.tracks.tracks().into_iter().find(|t| t.id == track).unwrap().instrument.unwrap().params[name]
}

fn effect_param(controls: &Controls, node: NodeId, name: &str) -> f32 {
    match &controls.graph.graph().node(node).unwrap().kind {
        NodeKind::Effect { params, .. } => params[name],
        _ => panic!("the node is not an effect"),
    }
}

#[test]
fn instrument_preset_is_saved_applied_in_one_step_and_removed() {
    let session = start(false);
    let controls = &session.controls;
    let track = controls.tracks.add_track("Sampler", Some("sampler"), 60).unwrap();
    let other = controls.tracks.add_track("Second", Some("sampler"), 60).unwrap();
    let target = EditorTarget::Instrument { track };
    controls.tracks.set_instrument_param(track, "gain", 0.3).unwrap();
    controls.tracks.set_instrument_param(track, "attack", 0.05).unwrap();
    controls.presets.save(target, "  Soft ").unwrap();
    assert_eq!(controls.presets.list("sampler").unwrap(), ["Soft"]);
    assert!(controls.presets.list("eq").unwrap().is_empty(), "a preset belongs to its kind");

    // Onto another track of the same kind: all params at once, in one undo step.
    let before = controls.view.snapshot();
    controls.presets.apply(EditorTarget::Instrument { track: other }, "Soft").unwrap();
    assert_eq!((instrument_param(controls, other, "gain"), instrument_param(controls, other, "attack")), (0.3, 0.05));
    assert_eq!(controls.history.undo_label().as_deref(), Some("Preset \"Soft\""));
    assert!(controls.history.undo());
    assert_eq!(controls.view.snapshot(), before);

    controls.presets.remove("sampler", "Soft").unwrap();
    assert!(controls.presets.list("sampler").unwrap().is_empty());
    assert!(matches!(controls.presets.apply(target, "Soft"), Err(e) if e.is(codes::settings::PRESET_NOT_FOUND)), "removed");
    assert!(matches!(controls.presets.remove("sampler", "Soft"), Err(e) if e.is(codes::settings::PRESET_NOT_FOUND)));
}

#[test]
fn effect_preset_round_trip_and_bad_targets() {
    let session = start(false);
    let controls = &session.controls;
    let track = controls.tracks.add_track("Voice", None, 60).unwrap();
    let eq = controls.graph.insert_effect(source_node(controls, track), "eq", &Params::new(), 0.0, 0.0).unwrap();
    let target = EditorTarget::Effect { node: eq };
    controls.graph.set_effect_param(eq, "low_gain", 6.0).unwrap();
    controls.presets.save(target, "Warm").unwrap();
    controls.graph.set_effect_param(eq, "low_gain", -3.0).unwrap();
    controls.presets.apply(target, "Warm").unwrap();
    assert_eq!(effect_param(controls, eq, "low_gain"), 6.0);
    assert!(controls.history.undo());
    assert_eq!(effect_param(controls, eq, "low_gain"), -3.0);

    let audio = EditorTarget::Instrument { track };
    assert!(matches!(controls.presets.save(audio, "x"), Err(e) if e.is(codes::tracks::NO_INSTRUMENT)), "an audio track has no instrument");
    assert!(matches!(controls.presets.save(EditorTarget::Effect { node: MASTER }, "x"), Err(e) if e.is(codes::graph::NODE_NOT_EFFECT)));
    assert!(matches!(controls.presets.save(EditorTarget::Effect { node: 9_999 }, "x"), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
    for (bad, code) in
        [("", codes::settings::NAME_EMPTY), ("a/b", codes::settings::NAME_BAD_CHARS), (".hidden", codes::settings::NAME_BAD_CHARS)]
    {
        assert!(matches!(controls.presets.save(target, bad), Err(e) if e.is(code)), "{bad:?}");
    }
}

#[test]
fn template_keeps_the_structure_and_sampler_sounds_but_not_clips() {
    let dir = temp_dir("templates");
    write_wav(&dir.join("kick.wav"), 0.3, 60.0);
    write_wav(&dir.join("vocal.wav"), 1.0, 300.0);
    let session = start(false);
    let controls = &session.controls;
    controls.project.create(&dir.join("a/a.dawproj")).unwrap();
    let kick = controls.samples.import(&dir.join("kick.wav")).unwrap();
    let vocal = controls.samples.import(&dir.join("vocal.wav")).unwrap();
    let drums = controls.tracks.add_tracks(vec![sampler_track("Kick", kick.id, 0.0, 1.0)]).unwrap()[0];
    note_clip(controls, drums, 0, 1);
    let voice = controls.tracks.add_track("Voice", None, 60).unwrap();
    controls.clips.add_audio_clip(voice, 0, vocal.id).unwrap();
    let bus = controls.graph.add_bus("Group", 0.0, 0.0).unwrap();
    controls.graph.insert_effect(bus, "eq", &Params::new(), 0.0, 0.0).unwrap();
    let graph_before = controls.graph.graph();

    controls.templates.save("Beat").unwrap();
    assert_eq!(controls.templates.list().unwrap(), ["Beat"]);
    controls.templates.create_project("Beat", &dir.join("b/b.dawproj")).unwrap();

    let project = controls.view.snapshot();
    assert_eq!(project.name, "b");
    assert_eq!(controls.project.path(), Some(dir.join("b/b.dawproj")), "the new project is open");
    assert_eq!(project.tracks.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Kick", "Voice"]);
    assert_eq!(project.graph, graph_before, "the graph, effects and buses are as they were");
    assert!(project.clips.is_empty() && project.automation.is_empty(), "no clips and no automation");
    assert_eq!(project.samples.len(), 1, "only the sampler sound is left");
    assert!(project.samples[0].path.starts_with(dir.join("b/samples")), "the sound is copied into the new project");
    assert!(project.samples[0].path.is_file());
    assert_eq!(project.tracks[0].instrument.as_ref().unwrap().params["sample"], project.samples[0].id as f32);

    assert!(
        matches!(controls.templates.create_project("Beat", &dir.join("b/b.dawproj")), Err(e) if e.is(codes::project::ALREADY_EXISTS)),
        "the file exists"
    );
    assert!(
        matches!(controls.templates.create_project("No such", &dir.join("c/c.dawproj")), Err(e) if e.is(codes::settings::TEMPLATE_NOT_FOUND))
    );
    controls.templates.save("Beat").unwrap(); // overwrite under the same name
    controls.templates.remove("Beat").unwrap();
    assert!(controls.templates.list().unwrap().is_empty());
    assert!(matches!(controls.templates.remove("Beat"), Err(e) if e.is(codes::settings::TEMPLATE_NOT_FOUND)));
    std::fs::remove_dir_all(dir).ok();
}
