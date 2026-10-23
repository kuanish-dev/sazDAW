//! Sends: a "pre-fader" connection (independent of the node volume and pan, but obeying mute), "post-fader", connection
//! level automation and latency compensation for a pre-fader send.

use super::latency::Latent;
use super::*;
use daw_contracts::{AutomationClip, MAX_CLIP_LOOP_TICKS};

impl Rig {
    /// Makes the connection `from → to` a pre-fader send (or post-fader).
    fn set_pre_fader(&mut self, from: NodeId, to: NodeId, pre_fader: bool) {
        let edge = self.project.graph.edges.iter_mut().find(|e| e.from == from && e.to == to && !e.sidechain).expect("the connection");
        edge.pre_fader = pre_fader;
    }
}

#[test]
fn pre_fader_send_ignores_volume_and_pan_but_post_fader_follows_them() {
    let mut rig = Rig::new();
    let source = rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 96_000);
    rig.set_strip(source, 0.25, -1.0, false, false);
    rig.set_pre_fader(source, MASTER, true);
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.5) && near(Rig::right(&out, 1_000), 0.5), "pre-fader: 0.5 in both channels");

    rig.set_pre_fader(source, MASTER, false);
    rig.publish();
    rig.render(1_000);
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.125), "post-fader: 0.5 · 0.25");
    assert!(near(Rig::right(&out, 1_000), 0.0), "and panned left");
}

#[test]
fn mute_silences_a_pre_fader_send_too() {
    let mut rig = Rig::new();
    let bus = rig.add_bus(1.0);
    let source = rig.add_track(10, MASTER);
    rig.connect(source, bus, 1.0, false);
    rig.set_pre_fader(source, bus, true);
    rig.note(10, 60, 0, 96_000);
    rig.set_strip(source, 0.0, 0.0, false, false);
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.5), "fader at zero, the pre-fader send sounds through the bus");

    rig.set_strip(source, 0.0, 0.0, true, false);
    rig.publish();
    rig.render(1_000);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.0), "mute silences the whole track, sends included");
}

#[test]
fn edge_level_automation_changes_the_send_over_time() {
    let mut rig = Rig::new();
    let bus = rig.add_bus(1.0);
    let source = rig.add_track(10, bus);
    rig.note(10, 60, 0, 96_000);
    rig.project.automation.push(AutomationLane {
        id: 900,
        target: ParamTarget::EdgeLevel { from: source, to: bus, sidechain: false },
        clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 0.0), AutoPoint::new(960, 1.0)])],
    });
    rig.publish();
    rig.transport.play();
    let out = rig.render(960 * FRAMES_PER_TICK);
    let at = |tick: usize| Rig::left(&out, tick * FRAMES_PER_TICK);
    assert!(at(100) < at(480) && at(480) < at(900), "the send level rises");
    assert!((at(480) - 0.25).abs() < 0.03, "about 0.5 · 0.5 in the middle");

    rig.project.automation.clear();
    rig.publish();
    rig.render(1_000);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.5));
}

#[test]
fn sidechain_level_automation_reaches_the_key_input() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(false, Box::new(DuckOrDouble));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(10, fx);
    let key = rig.add_track(11, MASTER);
    rig.connect(key, fx, 1.0, true);
    rig.set_strip(key, 0.0, 0.0, false, false);
    rig.note(10, 60, 0, 96_000);
    rig.note(11, 62, 0, 96_000);
    rig.project.automation.push(AutomationLane {
        id: 900,
        target: ParamTarget::EdgeLevel { from: key, to: fx, sidechain: true },
        clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 0.0)])],
    });
    rig.publish();
    rig.transport.play();
    rig.render(1_000);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 1.0), "the key is removed by automation at zero: the effect doubles");
}

#[test]
fn a_pre_fader_send_is_delay_compensated_like_any_edge() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(false, Latent::boxed(100));
    rig.connect(fx, MASTER, 1.0, false);
    let source = rig.add_track(10, fx);
    rig.connect(source, MASTER, 1.0, false);
    rig.set_pre_fader(source, MASTER, true);
    rig.note(10, 60, 0, 96_000);
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    let first = out.chunks_exact(2).position(|frame| frame[0].abs() > 1e-6);
    assert_eq!(first, Some(100), "the pre-fader send waits for the path through the delaying effect");
    assert!(near(Rig::left(&out, 100), 1.0), "both paths from the same frame: 0.5 + 0.5");
}
