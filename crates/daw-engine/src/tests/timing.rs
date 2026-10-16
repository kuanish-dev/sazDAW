//! The segment time for DSP objects (`set_timing`): plugins that follow the beat learn the transport position, tempo,
//! meter and loop before they play the segment.

use daw_contracts::BlockTiming;

use super::*;

/// An instrument and an effect that log the time they got and the frames they played.
struct Clocked {
    log: Log,
}

fn timing_entry(prefix: &str, timing: &BlockTiming) -> String {
    format!(
        "{prefix}:{}@{}+{}{}",
        timing.playing,
        timing.start_tick,
        timing.frames,
        if timing.loop_region.is_some() { ":loop" } else { "" }
    )
}

impl Instrument for Clocked {
    fn prepare(&mut self, _: StreamConfig) {}
    fn note_on(&mut self, _: u8, _: f32) {}
    fn note_off(&mut self, _: u8) {}
    fn all_notes_off(&mut self) {}
    fn controller(&mut self, _: daw_contracts::ControllerKind, _: f32) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn render(&mut self, block: &mut AudioBlock) {
        self.log.lock().unwrap().push(format!("render:{}", block.frames()));
    }
    fn set_timing(&mut self, timing: &BlockTiming) {
        self.log.lock().unwrap().push(timing_entry("inst", timing));
    }
}

impl Effect for Clocked {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, _: &mut [f32], _: Option<&[f32]>) {}
    fn set_timing(&mut self, timing: &BlockTiming) {
        self.log.lock().unwrap().push(timing_entry("fx", timing));
    }
}

fn clocked_rig() -> Rig {
    let mut rig = Rig::new();
    rig.add_track(1, MASTER);
    let instrument = Box::new(Clocked { log: Arc::clone(&rig.log) });
    rig.link.send(DspCommand::AddInstrument { track: 1, instrument }).unwrap();
    let effect = Box::new(Clocked { log: Arc::clone(&rig.log) });
    rig.add_fx(false, effect);
    rig.publish();
    rig
}

fn timings(rig: &Rig) -> Vec<String> {
    rig.log().into_iter().filter(|e| e.starts_with("inst:") || e.starts_with("fx:") || e.starts_with("render:")).collect()
}

#[test]
fn every_segment_tells_its_time_before_the_sound() {
    let mut rig = clocked_rig();
    rig.render(256);
    assert_eq!(timings(&rig), ["inst:false@0+256", "fx:false@0+256", "render:256"], "stopped: the position stands still");
    rig.clear_log();
    rig.transport.play();
    rig.render(256);
    rig.render(256);
    assert_eq!(
        timings(&rig),
        ["inst:true@0+256", "fx:true@0+256", "render:256", "inst:true@10.24+256", "fx:true@10.24+256", "render:256"],
        "120 BPM, 48 kHz: 0.04 ticks per frame"
    );
}

#[test]
fn loop_region_reaches_plugins_while_playing() {
    let mut rig = clocked_rig();
    rig.transport.set_loop(Some((0, 3840)));
    rig.transport.play();
    rig.render(128);
    assert_eq!(timings(&rig)[0], "inst:true@0+128:loop");
}

#[test]
fn a_tempo_change_starts_a_new_segment_on_its_frame() {
    let mut rig = clocked_rig();
    // 120 BPM (0.04 ticks per frame) up to tick 20 is 500 frames; then 60 BPM.
    let changes = vec![
        daw_contracts::TempoChange { tick: 0, bpm: Bpm::new(120.0).unwrap(), ramp: false },
        daw_contracts::TempoChange { tick: 20, bpm: Bpm::new(60.0).unwrap(), ramp: false },
    ];
    rig.project.tempo_map = rig.project.tempo_map.with_tempos(changes).unwrap();
    rig.publish();
    rig.clear_log();
    rig.transport.play();
    rig.render(512);
    let segments: Vec<String> = timings(&rig).into_iter().filter(|e| e.starts_with("inst:")).collect();
    assert_eq!(segments, ["inst:true@0+500", "inst:true@20+12"], "the segment is cut exactly at the tempo change");
    rig.clear_log();
    rig.render(100);
    let next = timings(&rig).into_iter().find(|e| e.starts_with("inst:")).unwrap();
    let tick: f64 = next.trim_start_matches("inst:true@").split('+').next().unwrap().parse().unwrap();
    assert!((tick - 20.24).abs() < 1e-9, "after the change, 0.02 ticks per frame: {tick}");
}
