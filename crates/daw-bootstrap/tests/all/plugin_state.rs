//! Plugin state in a full session with the dev-only fixture synth plugin (`test-plugins/synth`): the plugin window hands
//! its state to the project (the undo step "Plugin state", the instrument is recreated and sounds), the state survives
//! saving and reopening the project (base64 in the file), undo brings back the previous one. No fixture built — the test
//! is skipped (build it: `scripts/build-test-plugins.sh`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

fn synth_state(controls: &Controls, track: TrackId) -> Option<std::sync::Arc<PluginState>> {
    let project = controls.view.snapshot();
    project.track(track).and_then(|t| t.instrument.as_ref()).and_then(|spec| spec.state.clone())
}

fn cutoff_param(controls: &Controls) -> String {
    let kinds = controls.instruments.kinds();
    let synth = kinds.iter().find(|k| k.kind == FIXTURE_SYNTH).expect("the fixture synth is in the catalog");
    synth.params.iter().find(|p| p.label.get() == "Cutoff, Hz").expect("the cutoff param").name.to_string()
}

#[test]
fn window_state_is_an_undo_step_survives_save_and_load_and_keeps_the_synth_playing() {
    let Some(mut session) = start_with_fixture_plugins("plugin-state") else { return };
    let controls = session.controls.clone();
    let track = controls.tracks.add_track("Synth", Some(FIXTURE_SYNTH), 60).unwrap();
    let clip = note_clip(&controls, track, 0, 1);
    controls.notes.add_note(clip.id, note(60, 0, 1920)).unwrap();
    let target = EditorTarget::Instrument { track };

    let window = controls.plugin_editors.open(target).expect("the synth window is created (without showing)");
    controls.tracks.set_instrument_param(track, &cutoff_param(&controls), 300.0).unwrap();
    controls.plugin_editors.sync(window.id).unwrap();
    controls.plugin_editors.store_states();
    controls.plugin_editors.close(window.id);
    let state = synth_state(&controls, track).expect("the window state is in the project");
    assert_eq!(controls.history.undo_label().as_deref(), Some("Plugin state"));

    controls.transport.play();
    let sound = render(&mut session, 24_000);
    controls.transport.stop();
    assert!(peak(&sound) > 0.01, "the synth with the state is silent: {}", peak(&sound));

    let dir = temp_dir("plugin-state");
    let file = dir.join("state.daw");
    controls.project.save(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"state\""), "the state is written to the project file");
    controls.project.new_project();
    controls.project.load(&file).unwrap();
    assert_eq!(synth_state(&controls, track).as_deref(), Some(&*state), "after reopening the project the state is the same");

    assert!(controls.project.take_warnings().is_empty());
    let window = controls.plugin_editors.open(target).unwrap();
    controls.tracks.set_instrument_param(track, &cutoff_param(&controls), 2000.0).unwrap();
    controls.plugin_editors.sync(window.id).unwrap();
    controls.plugin_editors.close(window.id);
    let newer = synth_state(&controls, track).unwrap();
    assert_ne!(*newer, *state);
    assert!(controls.history.undo(), "undo of the \"Plugin state\" edit");
    assert_eq!(synth_state(&controls, track).as_deref(), Some(&*state), "undo brought back the previous state");
    assert!(controls.log.recent(20).iter().all(|e| e.source != "vst3" && e.source != "dsp"), "log: {:?}", controls.log.recent(20));
    let _ = std::fs::remove_dir_all(dir);
}
