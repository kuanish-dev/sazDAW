//! The processing graph: buses, source faders, mute/solo, effects and sidechain, processing order, meters, mixing to the device.

use super::*;

#[test]
fn bus_volume_pan_mute_and_solo() {
    let mut rig = Rig::new();
    let bus2 = rig.add_bus(0.5);
    let bus3 = rig.add_bus(1.0);
    rig.add_track(10, bus2);
    rig.add_track(11, bus3);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.25), "bus volume 0.5");

    // Pan left: the right channel fades out, the left one stays.
    rig.set_strip(bus2, 1.0, -1.0, false, false);
    rig.publish();
    rig.render(2_000);
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.5));
    assert!(near(Rig::right(&out, 1_000), 0.0));

    // Mute.
    rig.set_strip(bus2, 1.0, -1.0, true, false);
    rig.publish();
    rig.render(2_000);
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.0));

    // Solo on another bus silences the others, even with mute off.
    rig.set_strip(bus2, 1.0, 0.0, false, false);
    rig.set_strip(bus3, 1.0, 0.0, false, true);
    rig.publish();
    rig.render(2_000);
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.0), "bus 2 is not solo → silence");
}

#[test]
fn solo_never_silences_master_and_solo_bus_with_solo_source_is_heard() {
    let mut rig = Rig::new();
    let bus = rig.add_bus(1.0);
    let source = rig.add_track(10, bus);
    rig.note(10, 60, 0, 960);
    rig.set_strip(bus, 1.0, 0.0, false, true);
    rig.set_strip(source, 1.0, 0.0, false, true);
    rig.publish();
    rig.transport.play();
    rig.render(1_000);
    assert!(near(Rig::left(&rig.render(1_000), 500), 0.5), "the solo bus, the solo source and the master sound");
}

#[test]
fn solo_on_a_group_bus_keeps_its_inputs_audible() {
    let mut rig = Rig::new();
    let drums = rig.add_bus(1.0);
    let other = rig.add_bus(1.0);
    rig.add_track(10, drums);
    rig.add_track(11, other);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 1.0), "both groups sound");

    // Solo on the "drums" bus: its tracks and the path to the master are audible, the other group is silent.
    rig.set_strip(drums, 1.0, 0.0, false, true);
    rig.publish();
    rig.render(100);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.5), "only the solo group with its track sounds");
}

#[test]
fn solo_on_a_source_keeps_the_bus_it_feeds_but_silences_siblings() {
    let mut rig = Rig::new();
    let bus = rig.add_bus(1.0);
    let kick = rig.add_track(10, bus);
    rig.add_track(11, bus);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 1.0), "both tracks into one bus");

    rig.set_strip(kick, 1.0, 0.0, false, true);
    rig.publish();
    rig.render(100);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.5), "the bus sounds, but only from the solo track");
}

#[test]
fn solo_on_one_source_silences_the_other_sources_but_not_the_master() {
    let mut rig = Rig::new();
    let a = rig.add_track(10, MASTER);
    rig.add_track(11, MASTER);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 1.0), "both tracks sound");

    rig.set_strip(a, 1.0, 0.0, false, true);
    rig.publish();
    rig.render(100);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.5), "only the solo track sounds");

    // A solo bus also silences non-solo sources.
    rig.set_strip(a, 1.0, 0.0, false, false);
    let bus = rig.add_bus(1.0);
    rig.set_strip(bus, 1.0, 0.0, false, true);
    rig.publish();
    rig.render(100);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.0), "the sources are not solo, the bus is empty");
}

#[test]
fn a_muted_source_is_silent_but_its_instrument_keeps_advancing() {
    let mut rig = Rig::new();
    let node = rig.add_track(10, MASTER);
    rig.note(10, 60, 100, 200);
    rig.set_strip(node, 1.0, 0.0, true, false);
    rig.publish();
    rig.transport.play();
    let out = rig.render(10_000);
    assert!(out.iter().all(|v| *v == 0.0), "the track is muted");
    let log = rig.log();
    assert!(log.contains(&format!("on:60@{}", 100 * FRAMES_PER_TICK)), "the instrument got the notes: {log:?}");
    assert!(log.contains(&format!("off:60@{}", 300 * FRAMES_PER_TICK)), "{log:?}");

    // Unmuting brings the sound back without a restart: the note of the next repeat is audible.
    rig.set_strip(node, 1.0, 0.0, false, false);
    rig.publish();
    rig.render(100);
    let out = rig.render(BAR_TICKS as usize * FRAMES_PER_TICK);
    assert!(out.iter().any(|v| near(*v, 0.5)));
}

#[test]
fn source_volume_and_pan_are_applied() {
    let mut rig = Rig::new();
    let node = rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960);
    rig.set_strip(node, 0.5, -1.0, false, false);
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.25), "0.5 · source volume");
    assert!(near(Rig::right(&out, 1_000), 0.0), "pan left");
}

#[test]
fn gain_changes_are_smoothed_without_clicks() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 3000);
    rig.publish();
    rig.transport.play();
    rig.render(1_000);
    *rig.node_kind(MASTER) = NodeKind::Master { volume: 0.0 };
    rig.publish();
    let out = rig.render(1_000);
    let steps: Vec<f32> = (1..1_000).map(|f| (Rig::left(&out, f) - Rig::left(&out, f - 1)).abs()).collect();
    assert!(steps.iter().all(|s| *s < 0.01), "no sharp jumps");
    assert!(near(Rig::left(&out, 999), 0.0), "the volume reached the target");
}

#[test]
fn parallel_paths_from_one_instrument_sum_at_the_bus() {
    let mut rig = Rig::new();
    let bus = rig.add_bus(1.0);
    let fx = rig.add_fx(false, Box::new(Double));
    let inst = rig.add_track(10, bus);
    rig.connect(inst, fx, 0.5, false);
    rig.connect(fx, bus, 1.0, false);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.5 + 0.5 * 0.5 * 2.0), "dry 0.5 + 0.5 through the effect");
}

#[test]
fn instrument_effect_bus_master_chain_applies_effect_and_bus_volume() {
    let mut rig = Rig::new();
    let bus = rig.add_bus(0.5);
    let fx = rig.add_fx(false, Box::new(Double));
    rig.connect(fx, bus, 1.0, false);
    rig.add_track(10, fx);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.5), "0.5 · effect ×2 · bus 0.5");
}

#[test]
fn edge_level_scales_the_signal() {
    let mut rig = Rig::new();
    let inst = rig.add_track(10, MASTER);
    rig.project.graph.edges.iter_mut().find(|e| e.from == inst).unwrap().level = 0.5;
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.25));
}

#[test]
fn a_node_without_a_path_to_master_is_silent() {
    let mut rig = Rig::new();
    let bus = rig.add_node(NodeKind::Bus { strip: Strip { volume: 1.0, pan: 0.0, mute: false, solo: false, ..Strip::default() } });
    rig.add_track(10, bus); // the bus leads nowhere
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    assert!(rig.render(2_000).iter().all(|v| *v == 0.0));
}

#[test]
fn sidechain_edge_feeds_the_key_input_not_the_audio() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(false, Box::new(DuckOrDouble));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(11, fx); // the pad sounds all the time
    let kick = rig.add_track(10, MASTER);
    rig.connect(kick, fx, 1.0, true); // the kick drives the effect but does not enter its sound
    rig.note(10, 60, 960, 480);
    rig.note(11, 64, 0, 3000);
    rig.publish();
    rig.transport.play();
    let out = rig.render(960 * FRAMES_PER_TICK + 2_000);

    // Before the kick the pad is doubled by the effect (0.5 → 1.0).
    assert!(near(Rig::left(&out, 5_000), 1.0));
    // During the kick: the pad is ducked, only the kick itself remains (0.5).
    assert!(near(Rig::left(&out, 960 * FRAMES_PER_TICK + 1_000), 0.5));
}

#[test]
fn bypassed_effects_are_skipped() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(true, Box::new(Double));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(10, fx);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.5));

    *rig.node_kind(fx) = NodeKind::Effect { effect: "fx".into(), bypass: false, params: Params::new(), state: None };
    rig.publish();
    rig.render(100);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 1.0), "the effect doubles 0.5 → 1.0");
}

#[test]
fn removed_effect_stops_processing_but_keeps_the_path() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(false, Box::new(Double));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(10, fx);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 1.0));
    rig.link.send(DspCommand::RemoveEffect { node: fx }).unwrap();
    rig.render(100);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.5), "without a DSP object the node passes the sound as is");
}

#[test]
fn processing_follows_the_order_from_the_snapshot() {
    let build = |swap: bool| {
        let mut rig = Rig::new();
        let bus = rig.add_bus(1.0);
        let fx = rig.add_fx(false, Box::new(Double));
        rig.connect(fx, bus, 1.0, false);
        let inst = rig.add_track(10, fx);
        rig.note(10, 60, 0, 960);
        if swap {
            rig.order = Some(vec![inst, MASTER, bus, fx]);
        }
        rig.publish();
        rig.transport.play();
        Rig::left(&rig.render(2_000), 1_000)
    };
    let honest = build(false);
    assert!(near(honest, 1.0));
    // The bus comes before the effect: the effect's contribution arrives after the bus is processed and misses this block.
    let swapped = build(true);
    assert!(near(swapped, 0.0), "{swapped}");
}

#[test]
fn missing_or_garbled_order_does_not_panic() {
    let build = |order: fn(NodeId, NodeId) -> Vec<NodeId>| {
        let mut rig = Rig::new();
        let bus = rig.add_bus(1.0);
        let inst = rig.add_track(10, bus);
        rig.note(10, 60, 0, 960);
        rig.order = Some(order(inst, bus));
        rig.publish();
        rig.transport.play();
        rig.render(2_000)
    };
    // An empty order: nothing is processed, silence.
    assert!(build(|_, _| vec![]).iter().all(|v| *v == 0.0));
    // Unknown ids are skipped, repeats do not double the sound.
    let out = build(|inst, bus| vec![999, inst, inst, bus, bus, MASTER, MASTER, 12345]);
    assert!(near(Rig::left(&out, 1_000), 0.5), "{}", Rig::left(&out, 1_000));
}

#[test]
fn engine_survives_more_nodes_and_edges_than_the_limits() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960);
    // The engine ignores buses and connections beyond the limits.
    let mut previous = MASTER;
    for _ in 0..MAX_NODES + 40 {
        let id = rig.add_node(NodeKind::Bus { strip: Strip { volume: 1.0, pan: 0.0, mute: false, solo: false, ..Strip::default() } });
        rig.project.graph.edges.push(Edge { from: id, to: previous, level: 1.0, sidechain: false, pre_fader: false });
        previous = id;
    }
    for _ in 0..MAX_EDGES + 100 {
        rig.project.graph.edges.push(Edge { from: previous, to: MASTER, level: 0.1, sidechain: false, pre_fader: false });
    }
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(out.iter().all(|v| v.is_finite()));
    assert!(near(Rig::left(&out, 1_000), 0.5));
}

#[test]
fn meters_are_reported_per_track_bus_and_master_with_node_ids() {
    let mut rig = Rig::new();
    let bus = rig.add_bus(0.5);
    let quiet = rig.add_bus(1.0);
    let inst = rig.add_track(10, bus);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    rig.render(2_000);
    let meters = rig.link.meters();
    let of = |node: NodeId| meters.iter().find(|m| m.node == node).copied();
    assert!(near(of(bus).expect("bus meter").peak_left, 0.25));
    assert!(near(of(quiet).expect("second bus meter").peak_left, 0.0));
    assert!(near(of(MASTER).expect("master meter").peak_left, 0.25));
    assert!(near(of(inst).expect("track meter").peak_left, 0.5), "the track level after its fader, before the bus");
    assert_eq!(meters.len(), 4);
    assert!(rig.link.meters().iter().all(|m| m.peak_left == 0.0 && m.peak_right == 0.0), "reset after reading");
}

#[test]
fn removing_a_track_node_while_playing_keeps_the_engine_alive() {
    let mut rig = Rig::new();
    let node = rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    rig.render(2_000);
    rig.project.graph.nodes.retain(|n| n.id != node);
    rig.project.graph.edges.retain(|e| e.from != node);
    rig.publish();
    let out = rig.render(2_000);
    assert!(out.iter().all(|v| *v == 0.0), "no node, no sound, and no panic");
}

#[test]
fn a_mono_device_gets_a_downmix() {
    let mut rig = Rig::new();
    rig.renderer.prepare(StreamConfig { sample_rate: SR, channels: 1 });
    rig.add_track(10, MASTER);
    rig.publish();
    rig.link.note_on(10, 60, 1.0);
    let mut mono = vec![0.0; 256];
    rig.renderer.render(&mut mono);
    assert!(near(mono[200], 0.5), "the middle of the two channels: {}", mono[200]);
}
