//! DSP object params: on a new snapshot only the changed ones are sent, preview commands do not touch them, and after
//! `prepare` and the end of automation the objects get the project values in full.

use super::*;

/// A track with param `cutoff = 0.7` and a `Knobs` effect with param `mix = 0.5`, all played once.
fn rig_with_params() -> (Rig, NodeId) {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.project.tracks[0].instrument.as_mut().unwrap().params.insert("cutoff".into(), 0.7);
    let knobs = Box::new(Knobs { log: Arc::clone(&rig.log) });
    let fx = rig.add_fx(false, knobs);
    if let NodeKind::Effect { params, .. } = rig.node_kind(fx) {
        params.insert("mix".into(), 0.5);
    }
    rig.publish();
    rig.render(512);
    (rig, fx)
}

fn sent_params(rig: &Rig) -> Vec<String> {
    rig.log().into_iter().filter(|e| e.starts_with("param:") || e.starts_with("fx:")).collect()
}

#[test]
fn new_objects_get_all_their_params_once() {
    let (rig, _) = rig_with_params();
    assert_eq!(sent_params(&rig), ["fx:mix=0.5", "param:cutoff=0.7"]);
}

#[test]
fn an_unrelated_edit_resends_nothing_and_a_changed_value_goes_alone() {
    let (mut rig, fx) = rig_with_params();
    rig.clear_log();
    *rig.node_kind(MASTER) = NodeKind::Master { volume: 0.5 };
    rig.publish();
    rig.render(512);
    assert_eq!(sent_params(&rig), [] as [String; 0], "master volume does not concern object params");

    if let NodeKind::Effect { params, .. } = rig.node_kind(fx) {
        params.insert("mix".into(), 0.6);
    }
    rig.publish();
    rig.render(512);
    assert_eq!(sent_params(&rig), ["fx:mix=0.6"]);
}

#[test]
fn preview_commands_do_not_resend_params() {
    let (mut rig, _) = rig_with_params();
    rig.clear_log();
    rig.link.send(DspCommand::PreviewPlay { data: constant_sample(1_000, 0.1), from: 0, to: 1_000, looping: false }).unwrap();
    rig.link.send(DspCommand::PreviewStop).unwrap();
    rig.render(512);
    assert_eq!(sent_params(&rig), [] as [String; 0]);
}

#[test]
fn a_replaced_instrument_gets_all_params_even_if_the_project_did_not_change() {
    let (mut rig, _) = rig_with_params();
    rig.clear_log();
    let instrument = Box::new(Dc { log: Arc::clone(&rig.log), rendered: 0, sounding: false, velocity: 1.0 });
    rig.link.send(DspCommand::AddInstrument { track: 10, instrument }).unwrap();
    rig.render(512);
    assert_eq!(sent_params(&rig), ["param:cutoff=0.7"]);
}

/// `prepare` (a device or rate change) may reset objects to their defaults:
/// after it the engine sends them the project params again.
#[test]
fn prepare_resends_all_params() {
    let (mut rig, _) = rig_with_params();
    rig.clear_log();
    rig.renderer.prepare(StreamConfig { sample_rate: 44_100, channels: 2 });
    rig.render(512);
    assert_eq!(sent_params(&rig), ["fx:mix=0.5", "param:cutoff=0.7"]);
}
