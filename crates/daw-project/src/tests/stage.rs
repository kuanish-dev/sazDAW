//! The `StageControl` contract: where each channel stands on the virtual stage, and the shared room.

use super::*;

fn strip(rig: &Rig, node: NodeId) -> Strip {
    *rig.graph.graph().node(node).unwrap().kind.strip().unwrap()
}

fn spot(depth: f32, height: f32) -> StagePlace {
    StagePlace { depth, height }
}

#[test]
fn a_channel_stands_where_it_is_placed_and_off_the_stage_keeps_its_pan() {
    let (rig, track, _) = rig_with_clip();
    let node = source(&rig, track);
    assert_eq!(strip(&rig, node).stage, None, "a new track is off the stage");

    rig.stage.place(node, -0.4, spot(0.7, 0.25)).unwrap();
    let placed = strip(&rig, node);
    assert_eq!((placed.pan, placed.stage), (-0.4, Some(spot(0.7, 0.25))), "its left/right is its pan");

    rig.stage.take_off(node).unwrap();
    let off = strip(&rig, node);
    assert_eq!((off.pan, off.stage), (-0.4, None), "plain pan again, where it stood");
    let version = rig.service.revision();
    rig.stage.take_off(node).unwrap();
    assert_eq!(rig.service.revision(), version, "a channel off the stage stays as it is");
}

#[test]
fn buses_stand_on_the_stage_too_but_effects_and_the_master_do_not() {
    let rig = Rig::new();
    let bus = rig.graph.add_bus("Drums", 0.0, 0.0).unwrap();
    rig.stage.place(bus, 0.0, StagePlace::DEFAULT).unwrap();
    assert_eq!(strip(&rig, bus).stage, Some(StagePlace::DEFAULT));

    let fx = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    for node in [fx, MASTER] {
        assert!(matches!(rig.stage.place(node, 0.0, StagePlace::DEFAULT), Err(e) if e.is(codes::graph::NO_STAGE_PLACE)));
        assert!(matches!(rig.stage.take_off(node), Err(e) if e.is(codes::graph::NO_STAGE_PLACE)));
    }
    assert!(matches!(rig.stage.place(999, 0.0, StagePlace::DEFAULT), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
}

#[test]
fn values_are_clamped_and_non_numbers_refused() {
    let (rig, track, _) = rig_with_clip();
    let node = source(&rig, track);
    rig.stage.place(node, 3.0, spot(-4.0, 9.0)).unwrap();
    let placed = strip(&rig, node);
    assert_eq!((placed.pan, placed.stage), (1.0, Some(spot(-1.0, 1.0))), "the wall behind the listener and the ceiling");

    for (pan, place) in [(f32::NAN, StagePlace::DEFAULT), (0.0, spot(f32::INFINITY, 0.0)), (0.0, spot(0.5, f32::NAN))] {
        assert!(matches!(rig.stage.place(node, pan, place), Err(e) if e.is(codes::graph::STAGE_NOT_A_NUMBER)));
    }
    let bad_room = Room { size: f32::NAN, ..Room::default() };
    assert!(matches!(rig.stage.set_room(bad_room), Err(e) if e.is(codes::graph::STAGE_NOT_A_NUMBER)));
    assert_eq!(strip(&rig, node).stage, Some(spot(-1.0, 1.0)), "a refused edit changes nothing");
}

#[test]
fn a_drag_is_one_undo_step() {
    let (rig, track, _) = rig_with_clip();
    let node = source(&rig, track);
    for step in 0..10 {
        rig.stage.place(node, step as f32 / 10.0, spot(step as f32 / 10.0, 0.0)).unwrap();
    }
    assert_eq!(strip(&rig, node).stage, Some(spot(0.9, 0.0)));
    assert!(rig.service.undo());
    let before = strip(&rig, node);
    assert_eq!((before.pan, before.stage), (0.0, None), "one undo takes the whole drag back, placing included");
}

#[test]
fn the_room_is_saved_clamped_and_undoable() {
    let rig = Rig::new();
    assert_eq!(rig.stage.room(), Room::default());
    let hall = Room { size: 2.0, reverb: -1.0, listening: Listening::Headphones };
    rig.stage.set_room(hall).unwrap();
    assert_eq!(rig.stage.room(), Room { size: 1.0, reverb: 0.0, listening: Listening::Headphones });
    assert!(rig.service.undo());
    assert_eq!(rig.stage.room(), Room::default());
}

#[test]
fn the_stage_holds_at_most_its_limit_of_channels() {
    let rig = Rig::new();
    let buses: Vec<NodeId> = (0..=MAX_ON_STAGE).map(|i| rig.graph.add_bus(&format!("Bus {i}"), 0.0, 0.0).unwrap()).collect();
    for bus in &buses[..MAX_ON_STAGE] {
        rig.stage.place(*bus, 0.0, StagePlace::DEFAULT).unwrap();
    }
    let last = buses[MAX_ON_STAGE];
    assert!(matches!(rig.stage.place(last, 0.0, StagePlace::DEFAULT), Err(e) if e.is(codes::graph::TOO_MANY_ON_STAGE)));
    rig.stage.place(buses[0], 0.5, spot(1.0, 0.0)).expect("a channel already on the stage still moves");
    rig.stage.take_off(buses[1]).unwrap();
    rig.stage.place(last, 0.0, StagePlace::DEFAULT).expect("a freed place can be taken");
}

#[test]
fn depth_and_height_lanes_live_while_the_channel_stands_on_the_stage() {
    let (rig, track, _) = rig_with_clip();
    let node = source(&rig, track);
    let depth = ParamTarget::StageDepth { node };
    assert!(matches!(rig.automation.add_lane(depth.clone()), Err(e) if e.is(codes::graph::NOT_ON_STAGE)));
    let bus_effect = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    let height_of_effect = ParamTarget::StageHeight { node: bus_effect };
    assert!(matches!(rig.automation.add_lane(height_of_effect), Err(e) if e.is(codes::graph::NO_STAGE_PLACE)));

    rig.stage.place(node, 0.0, spot(0.6, -0.5)).unwrap();
    let lane = rig.automation.add_lane(depth.clone()).unwrap();
    rig.automation.add_lane(ParamTarget::StageHeight { node }).unwrap();
    let clip = rig.automation.add_clip(lane, 0, BAR_TICKS).unwrap();
    let lanes = rig.automation.lanes();
    let points = &lanes[0].clips.iter().find(|c| c.id == clip).unwrap().points;
    assert_eq!(points[0].value, 0.6, "a new clip holds the current depth");

    rig.stage.take_off(node).unwrap();
    assert!(rig.automation.lanes().is_empty(), "off the stage, its depth and height lanes go");
    assert!(rig.service.undo());
    assert_eq!(rig.automation.lanes().len(), 2, "and come back with it");
}
