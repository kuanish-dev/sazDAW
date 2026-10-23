//! Automation: source and bus volume, instrument params, return to the project values.

use daw_contracts::{AutomationClip, AutomationHold, MAX_CLIP_LOOP_TICKS};

use super::*;

#[test]
fn volume_automation_can_target_a_source() {
    let mut rig = Rig::new();
    let node = rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960);
    rig.project.automation.push(AutomationLane {
        id: 900,
        target: ParamTarget::NodeVolume { node },
        clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 0.5)])],
    });
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(2_000), 1_500), 0.25));
}

#[test]
fn instrument_params_come_from_the_track_and_from_automation() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.project.tracks[0].instrument.as_mut().unwrap().params.insert("cutoff".into(), 0.7);
    rig.project.automation.push(AutomationLane {
        id: 900,
        target: ParamTarget::InstrumentParam { track: 10, param: "res".into() },
        clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 0.25)])],
    });
    rig.publish();
    rig.transport.play();
    rig.render(512);
    let log = rig.log();
    assert!(log.contains(&"param:cutoff=0.7".to_string()), "{log:?}");
    assert!(log.contains(&"param:res=0.25".to_string()), "{log:?}");
}

#[test]
fn volume_automation_follows_the_timeline_position() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960);
    rig.project.automation.push(AutomationLane {
        id: 900,
        target: ParamTarget::NodeVolume { node: MASTER },
        clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 0.0), AutoPoint::new(960, 1.0)])],
    });
    rig.publish();
    rig.transport.play();
    let out = rig.render(960 * FRAMES_PER_TICK);
    let at = |tick: usize| Rig::left(&out, tick * FRAMES_PER_TICK);
    assert!(at(100) < at(480) && at(480) < at(900), "the volume rises");
    assert!((at(480) - 0.25).abs() < 0.03, "about 0.5 · 0.5 in the middle");

    rig.transport.stop();
    rig.render(2_000);
    rig.transport.play();
    rig.project.automation.clear();
    rig.publish();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.5));
}

#[test]
fn bus_volume_automation_is_applied() {
    let mut rig = Rig::new();
    let bus = rig.add_bus(1.0);
    rig.add_track(10, bus);
    rig.note(10, 60, 0, 960);
    rig.project.automation.push(AutomationLane {
        id: 900,
        target: ParamTarget::NodeVolume { node: bus },
        clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 0.5)])],
    });
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_500), 0.25), "automation sets the bus volume to 0.5");
}

#[test]
fn a_held_lane_is_not_played_and_the_project_value_sounds() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960 * 4);
    rig.project.automation.push(AutomationLane {
        id: 900,
        target: ParamTarget::NodeVolume { node: MASTER },
        clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 0.0)])],
    });
    rig.publish();
    rig.link.hold(&[900]);
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.5), "the lane (silence) is held: the project volume plays");

    rig.link.hold(&[]);
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_000), 0.0), "released: the lane plays again");
}

#[test]
fn a_lane_overrides_its_param_only_inside_its_clips() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 2_880);
    let silent = AutomationClip { id: 901, start_tick: 960, length_ticks: 960, loop_ticks: 480, points: vec![AutoPoint::new(0, 0.0)] };
    rig.project.automation.push(AutomationLane { id: 900, target: ParamTarget::NodeVolume { node: MASTER }, clips: vec![silent] });
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_880 * FRAMES_PER_TICK);
    let at = |tick: usize| Rig::left(&out, tick * FRAMES_PER_TICK);
    assert!(at(480) > 0.1, "before the clip the project volume plays");
    assert!(at(1_200).abs() < 1e-3 && at(1_700).abs() < 1e-3, "both repeats of the clip mute");
    assert!(at(2_400) > 0.1, "after the clip the project volume plays again");
}
