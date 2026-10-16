//! The channel utility controls of `GraphControl`: polarity, width, mono and fine time alignment on sources and buses.

use super::*;

fn strip(rig: &Rig, node: NodeId) -> Strip {
    *rig.graph.graph().node(node).unwrap().kind.strip().unwrap()
}

#[test]
fn a_new_channel_starts_neutral() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Kick in", None, 36).unwrap();
    let bus = rig.graph.add_bus("Drums", 0.0, 0.0).unwrap();
    for node in [source(&rig, track), bus] {
        let strip = strip(&rig, node);
        assert_eq!((strip.polarity, strip.width, strip.mono, strip.delay_ms), (Polarity::default(), 1.0, false, 0.0));
    }
}

#[test]
fn sources_and_buses_take_every_utility_control_and_the_values_are_clamped() {
    let rig = Rig::new();
    let synth = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let audio = rig.tracks.add_track("Snare bottom", None, 38).unwrap();
    let bus = rig.graph.add_bus("Drums", 0.0, 0.0).unwrap();
    for node in [source(&rig, synth), source(&rig, audio), bus] {
        let inverted = Polarity { invert_left: true, invert_right: false };
        rig.graph.set_polarity(node, inverted).unwrap();
        rig.graph.set_mono(node, true).unwrap();
        rig.graph.set_width(node, 5.0).unwrap();
        rig.graph.set_delay(node, 50.0).unwrap();
        let set = strip(&rig, node);
        assert_eq!((set.polarity, set.mono, set.width, set.delay_ms), (inverted, true, MAX_WIDTH, MAX_CHANNEL_DELAY_MS));

        rig.graph.set_width(node, -1.0).unwrap();
        rig.graph.set_delay(node, -50.0).unwrap();
        let set = strip(&rig, node);
        assert_eq!((set.width, set.delay_ms), (0.0, -MAX_CHANNEL_DELAY_MS), "both ends are clamped");
        assert!(set.mono, "the width does not touch the mono switch");

        rig.graph.set_delay(node, 0.021).unwrap();
        assert_eq!(strip(&rig, node).delay_ms, 0.021, "a fraction of a millisecond is kept: one sample at 48 kHz");
    }
}

#[test]
fn values_that_are_not_numbers_are_refused_and_change_nothing() {
    let rig = Rig::new();
    let bus = rig.graph.add_bus("Room", 0.0, 0.0).unwrap();
    use codes::graph::{DELAY_NOT_A_NUMBER, WIDTH_NOT_A_NUMBER};
    assert!(matches!(rig.graph.set_width(bus, f32::NAN), Err(e) if e.is(WIDTH_NOT_A_NUMBER)));
    assert!(matches!(rig.graph.set_delay(bus, f32::INFINITY), Err(e) if e.is(DELAY_NOT_A_NUMBER)));
    let strip = strip(&rig, bus);
    assert_eq!((strip.width, strip.delay_ms), (1.0, 0.0));
}

#[test]
fn the_master_effects_and_unknown_nodes_have_no_utility() {
    let rig = Rig::new();
    let fx = rig.graph.add_effect("eq", 0.0, 0.0).unwrap();
    use codes::graph::{NO_UTILITY, NODE_NOT_FOUND};
    for node in [MASTER, fx] {
        assert!(matches!(rig.graph.set_polarity(node, Polarity::default()), Err(e) if e.is(NO_UTILITY)));
        assert!(matches!(rig.graph.set_width(node, 1.0), Err(e) if e.is(NO_UTILITY)));
        assert!(matches!(rig.graph.set_mono(node, true), Err(e) if e.is(NO_UTILITY)));
        assert!(matches!(rig.graph.set_delay(node, 1.0), Err(e) if e.is(NO_UTILITY)));
        assert!(matches!(rig.automation.add_lane(ParamTarget::NodeWidth { node }), Err(e) if e.is(NO_UTILITY)));
    }
    assert!(matches!(rig.graph.set_width(9999, 1.0), Err(e) if e.is(NODE_NOT_FOUND)));
}

#[test]
fn a_knob_gesture_is_one_undo_step_and_each_switch_its_own() {
    let rig = Rig::new();
    let bus = rig.graph.add_bus("Overheads", 0.0, 0.0).unwrap();
    for step in 1..=10 {
        rig.graph.set_width(bus, 0.15 * step as f32).unwrap();
    }
    for step in 1..=10 {
        rig.graph.set_delay(bus, 0.1 * step as f32).unwrap();
    }
    rig.graph.set_polarity(bus, Polarity { invert_left: true, invert_right: true }).unwrap();
    rig.graph.set_mono(bus, true).unwrap();
    assert_eq!(rig.service.undo_label().as_deref(), Some("Mono"));

    assert!(rig.service.undo());
    assert!(!strip(&rig, bus).mono);
    assert!(rig.service.undo());
    assert_eq!(strip(&rig, bus).polarity, Polarity::default());
    assert!(rig.service.undo());
    assert_eq!(strip(&rig, bus).delay_ms, 0.0, "the whole delay gesture is one step");
    assert!((strip(&rig, bus).width - 1.5).abs() < 1e-6, "the width gesture is a step of its own");
    assert!(rig.service.undo());
    assert_eq!(strip(&rig, bus).width, 1.0);
    assert!(rig.graph.graph().node(bus).is_some(), "the next step back is the bus itself");
}

#[test]
fn width_automation_lives_on_sources_and_buses_and_goes_with_the_node() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Pad", Some("synth"), 60).unwrap();
    let bus = rig.graph.add_bus("Pads", 0.0, 0.0).unwrap();
    rig.automation.add_lane(ParamTarget::NodeWidth { node: source(&rig, track) }).unwrap();
    rig.automation.add_lane(ParamTarget::NodeWidth { node: bus }).unwrap();
    assert_eq!(rig.automation.lanes().len(), 2);
    rig.graph.remove_node(bus).unwrap();
    let lanes = rig.automation.lanes();
    assert_eq!(lanes.len(), 1, "the bus took its width lane with it");
    assert_eq!(lanes[0].target, ParamTarget::NodeWidth { node: source(&rig, track) });
}
