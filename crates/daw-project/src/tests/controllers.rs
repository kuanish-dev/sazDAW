//! The `NoteControl::set_controller_lane` contract: controller lanes of a note clip.

use super::*;

fn point(tick: u32, value: f32) -> ControllerPoint {
    ControllerPoint { tick, value }
}

const BEND: ControllerKind = ControllerKind::PitchBend;
const MOD: ControllerKind = ControllerKind::Cc { number: CC_MODULATION };

#[test]
fn lane_is_created_sorted_replaced_and_removed_by_empty_points() {
    let (rig, _, clip) = rig_with_clip();
    rig.notes.set_controller_lane(clip, BEND, vec![point(480, 0.5), point(0, -1.0), point(480, 0.25)]).unwrap();
    let lanes = content(&rig, clip).controllers;
    assert_eq!(lanes.len(), 1);
    assert_eq!(lanes[0].kind, BEND);
    assert_eq!(lanes[0].points, [point(0, -1.0), point(480, 0.25)], "by tick; of equal ticks the last one");

    rig.notes.set_controller_lane(clip, MOD, vec![point(10, 1.0)]).unwrap();
    rig.notes.set_controller_lane(clip, BEND, vec![point(5, 0.0)]).unwrap();
    let lanes = content(&rig, clip).controllers;
    assert_eq!(lanes.len(), 2, "one lane per kind");
    assert_eq!(lanes.iter().find(|l| l.kind == BEND).unwrap().points, [point(5, 0.0)]);

    rig.notes.set_controller_lane(clip, BEND, Vec::new()).unwrap();
    assert_eq!(content(&rig, clip).controllers.iter().map(|l| l.kind).collect::<Vec<_>>(), [MOD]);
}

#[test]
fn values_ticks_and_numbers_are_checked_and_errors_change_nothing() {
    let (rig, _, clip) = rig_with_clip();
    let out_of_range = |r: Result<(), ContractError>| matches!(r, Err(e) if e.is(codes::notes::CONTROLLER_VALUE_OUT_OF_RANGE));
    assert!(out_of_range(rig.notes.set_controller_lane(clip, MOD, vec![point(0, -0.1)])), "CC is only 0..1");
    assert!(out_of_range(rig.notes.set_controller_lane(clip, BEND, vec![point(0, 1.5)])), "bend is only -1..1");
    assert!(rig.notes.set_controller_lane(clip, BEND, vec![point(0, f32::NAN)]).is_err());
    assert!(rig.notes.set_controller_lane(clip, ControllerKind::Cc { number: 120 }, vec![point(0, 0.5)]).is_err());
    let late = rig.notes.set_controller_lane(clip, BEND, vec![point(MAX_CLIP_LOOP_TICKS, 0.0)]);
    assert!(matches!(late, Err(e) if e.is(codes::notes::CONTROLLER_TICK_OUT_OF_RANGE)));
    let many = (0..=MAX_CONTROLLER_POINTS as u32).map(|t| point(t, 0.0)).collect();
    assert!(matches!(rig.notes.set_controller_lane(clip, BEND, many), Err(e) if e.is(codes::notes::TOO_MANY_CONTROLLER_POINTS)));
    assert!(rig.notes.set_controller_lane(9999, BEND, vec![point(0, 0.0)]).is_err());
    assert!(content(&rig, clip).controllers.is_empty());
}

#[test]
fn auto_length_counts_points_and_manual_length_trims_them() {
    let (rig, _, clip) = rig_with_clip();
    rig.notes.set_controller_lane(clip, ControllerKind::Aftertouch, vec![point(0, 0.2), point(BAR_TICKS * 2, 0.8)]).unwrap();
    assert_eq!(loop_ticks(&rig, clip), BAR_TICKS * 3, "a point at the start of the third bar keeps three bars");

    rig.notes.set_loop_ticks(clip, BAR_TICKS).unwrap();
    assert_eq!(content(&rig, clip).controllers[0].points, [point(0, 0.2)]);
    assert!(rig.notes.set_controller_lane(clip, BEND, vec![point(BAR_TICKS, 0.0)]).is_err(), "a manual length is the bound");
}

#[test]
fn each_change_is_one_undo_step() {
    let (rig, _, clip) = rig_with_clip();
    rig.notes.set_controller_lane(clip, MOD, vec![point(0, 0.5)]).unwrap();
    rig.notes.set_controller_lane(clip, MOD, vec![point(0, 1.0)]).unwrap();
    assert!(rig.service.undo());
    assert_eq!(content(&rig, clip).controllers[0].points, [point(0, 0.5)]);
}
