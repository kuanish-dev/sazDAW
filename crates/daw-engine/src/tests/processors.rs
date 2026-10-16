//! Effects after buses and on the master: processing order by connections, solo through a bus effect, meters in the chain.

use super::*;

/// A bus with volume `volume` and no outputs: the test decides where it goes.
fn loose_bus(rig: &mut Rig, volume: f32) -> NodeId {
    rig.add_node(NodeKind::Bus { strip: Strip { volume, pan: 0.0, mute: false, solo: false, ..Strip::default() } })
}

#[test]
fn bus_compressor_processes_the_bus_output() {
    let mut rig = Rig::new();
    // The effect is created before the bus (a smaller id): the order must still follow the connections, not the kind or id.
    let fx = rig.add_fx(false, Box::new(Double));
    let bus = loose_bus(&mut rig, 0.5);
    rig.connect(bus, fx, 1.0, false);
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(10, bus);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.5), "0.5 · bus 0.5 · effect ×2");
}

#[test]
fn master_insert_chain_takes_all_sources() {
    let mut rig = Rig::new();
    let limiter = rig.add_fx(false, Box::new(Double));
    rig.connect(limiter, MASTER, 1.0, false);
    rig.add_track(10, limiter);
    rig.add_track(11, limiter);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    // Master 0.25: the result stays below 1, where the device output limits nothing.
    *rig.node_kind(MASTER) = NodeKind::Master { volume: 0.25 };
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.5), "(0.5 + 0.5) · ×2 on the master · 0.25");
}

#[test]
fn bus_effect_bus_chain_meters_each_bus() {
    let mut rig = Rig::new();
    let first = loose_bus(&mut rig, 1.0);
    let fx = rig.add_fx(false, Box::new(Double));
    let second = rig.add_bus(0.5);
    rig.connect(first, fx, 1.0, false);
    rig.connect(fx, second, 1.0, false);
    rig.add_track(10, first);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.5), "0.5 · ×2 · 0.5");
    let meters = rig.link.meters();
    let of = |node: NodeId| meters.iter().find(|m| m.node == node).map(|m| m.peak_left);
    assert!(near(of(first).expect("first bus meter"), 0.5));
    assert!(near(of(second).expect("second bus meter"), 0.5));
    assert!(near(of(MASTER).expect("master meter"), 0.5));
}

#[test]
fn solo_passes_through_a_bus_effect() {
    let mut rig = Rig::new();
    let bus = loose_bus(&mut rig, 1.0);
    let fx = rig.add_fx(false, Box::new(Double));
    rig.connect(bus, fx, 1.0, false);
    rig.connect(fx, MASTER, 1.0, false);
    let soloed = rig.add_track(10, bus);
    let other = rig.add_track(11, MASTER);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    // Tracks at 0.5: the result stays below 1, where the device output limits nothing.
    rig.set_strip(soloed, 0.5, 0.0, false, false);
    rig.set_strip(other, 0.5, 0.0, false, false);
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.75), "0.25 · ×2 + 0.25");

    // Solo on a track: its path through the bus and effect is audible, the neighbouring track is silent.
    rig.set_strip(soloed, 0.5, 0.0, false, true);
    rig.publish();
    rig.render(100);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.5), "the solo track through the bus effect");

    // Solo on the bus: the same, its input and output through the effect are audible.
    rig.set_strip(soloed, 0.5, 0.0, false, false);
    rig.set_strip(bus, 1.0, 0.0, false, true);
    rig.publish();
    rig.render(100);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.5), "the solo bus with the effect after it");
}

/// An effect that halves the signal and reports a 6 dB gain reduction.
struct Halve;

impl Effect for Halve {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, buffer: &mut [f32], _: Option<&[f32]>) {
        for sample in buffer {
            *sample *= 0.5;
        }
    }
    fn gain_reduction_db(&self) -> Option<f32> {
        Some(6.0)
    }
}

#[test]
fn effects_report_gain_reduction_and_reading_resets_it() {
    let mut rig = Rig::new();
    let comp = rig.add_fx(false, Box::new(Halve));
    let plain = rig.add_fx(false, Box::new(Double));
    let bypassed = rig.add_fx(true, Box::new(Halve));
    rig.connect(comp, plain, 1.0, false);
    rig.connect(plain, bypassed, 1.0, false);
    rig.connect(bypassed, MASTER, 1.0, false);
    rig.add_track(10, comp);
    rig.publish();
    rig.render(2_000);

    let reductions = rig.link.gain_reductions();
    assert_eq!(reductions.len(), 1, "only a processed effect that reports it: {reductions:?}");
    assert_eq!((reductions[0].node, reductions[0].db), (comp, 6.0));
    assert!(rig.link.gain_reductions().is_empty(), "reading resets the reports");
}
