//! The `AutomationControl` contract: automation lanes, their clips and pattern points.

use super::*;

/// A master volume lane with one clip `0..10_000` (loop = length).
fn master_clip(rig: &Rig) -> (LaneId, AutomationClipId) {
    let lane = rig.automation.add_lane(ParamTarget::NodeVolume { node: MASTER }).unwrap();
    (lane, rig.automation.add_clip(lane, 0, 10_000).unwrap())
}

fn points(rig: &Rig) -> Vec<AutoPoint> {
    rig.automation.lanes()[0].clips[0].points.clone()
}

#[test]
fn automation_lanes_are_validated_and_points_normalized() {
    let rig = Rig::new();
    let target = ParamTarget::NodeVolume { node: MASTER };
    let lane = rig.automation.add_lane(target.clone()).unwrap();
    assert!(matches!(rig.automation.add_lane(target), Err(e) if e.is(codes::automation::LANE_EXISTS)), "duplicate target");
    assert!(matches!(rig.automation.add_lane(ParamTarget::NodePan { node: 9999 }), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
    let clip = rig.automation.add_clip(lane, 0, 3_840).unwrap();

    rig.automation.set_points(lane, clip, vec![AutoPoint::new(960, 1.0), AutoPoint::new(0, 0.0), AutoPoint::new(960, 0.5)]).unwrap();
    assert_eq!(points(&rig), [AutoPoint::new(0, 0.0), AutoPoint::new(960, 0.5)]);
    let nan = rig.automation.set_points(lane, clip, vec![AutoPoint::new(0, f32::NAN)]);
    assert!(matches!(nan, Err(e) if e.is(codes::automation::POINT_NOT_A_NUMBER)));
    let outside = rig.automation.set_points(lane, clip, vec![AutoPoint::new(3_841, 0.5)]);
    assert!(matches!(outside, Err(e) if e.is(codes::automation::POINT_OUTSIDE_PATTERN)), "after the loop end");
    rig.automation.set_points(lane, clip, vec![AutoPoint::new(3_840, 0.5)]).unwrap();
    assert!(matches!(rig.automation.set_points(lane, 999, Vec::new()), Err(e) if e.is(codes::automation::CLIP_NOT_FOUND)));
}

#[test]
fn instrument_automation_needs_an_instrument_track_with_that_param() {
    let rig = Rig::new();
    let synth = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let audio = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let lane = |track, param: &str| rig.automation.add_lane(ParamTarget::InstrumentParam { track, param: param.into() });
    assert!(lane(synth, "gain").is_ok());
    assert!(matches!(lane(synth, "gain"), Err(e) if e.is(codes::automation::LANE_EXISTS)), "duplicate");
    assert!(matches!(lane(synth, "nope"), Err(e) if e.is(codes::tracks::UNKNOWN_INSTRUMENT_PARAM)), "no such parameter");
    assert!(matches!(lane(audio, "gain"), Err(e) if e.is(codes::tracks::NO_INSTRUMENT)), "an audio track has no instrument");
    assert!(matches!(lane(9999, "gain"), Err(e) if e.is(codes::tracks::NOT_FOUND)));
}

#[test]
fn curves_are_kept_clamped_and_checked() {
    let rig = Rig::new();
    let (lane, clip) = master_clip(&rig);
    let bent = |tick, curve| AutoPoint { tick, value: 0.5, curve };
    rig.automation.set_points(lane, clip, vec![bent(0, 0.5), bent(960, 3.0), bent(1920, -7.0)]).unwrap();
    let curves: Vec<f32> = points(&rig).iter().map(|p| p.curve).collect();
    assert_eq!(curves, [0.5, 1.0, -1.0]);
    let nan = rig.automation.set_points(lane, clip, vec![bent(0, f32::NAN)]);
    assert!(matches!(nan, Err(e) if e.is(codes::automation::POINT_CURVE_NOT_A_NUMBER)));
}

fn shape(shape: AutomationShape, from_tick: u32, to_tick: u32, period_ticks: u32) -> ShapeRequest {
    ShapeRequest { from_tick, to_tick, shape, period_ticks, min: 0.0, max: 1.0 }
}

#[test]
fn a_shape_replaces_the_points_in_its_range_in_one_undo_step() {
    let rig = Rig::new();
    let (lane, clip) = master_clip(&rig);
    rig.automation.set_points(lane, clip, vec![AutoPoint::new(0, 0.3), AutoPoint::new(500, 0.9), AutoPoint::new(5_000, 0.7)]).unwrap();
    let undo_before = rig.service.undo_label();

    rig.automation.draw_shape(lane, clip, shape(AutomationShape::Triangle, 400, 2_320, 960)).unwrap();
    let points = points(&rig);
    let ticks: Vec<u32> = points.iter().map(|p| p.tick).collect();
    assert_eq!(ticks, [0, 400, 880, 1_360, 1_840, 2_320, 5_000], "outside points stay, the 500 one is replaced");
    let values: Vec<f32> = points.iter().map(|p| p.value).collect();
    assert_eq!(values, [0.3, 0.0, 1.0, 0.0, 1.0, 0.0, 0.7]);

    assert!(rig.service.undo());
    assert_eq!(rig.automation.lanes()[0].clips[0].points.len(), 3, "one undo step");
    assert_eq!(rig.service.undo_label(), undo_before);
}

#[test]
fn square_saw_and_ramps_have_their_shapes() {
    let rig = Rig::new();
    let (lane, clip) = master_clip(&rig);
    rig.automation.set_points(lane, clip, Vec::new()).unwrap();
    let draw = |request| {
        rig.automation.draw_shape(lane, clip, request).unwrap();
        points(&rig).iter().map(|p| (p.tick, p.value)).collect::<Vec<_>>()
    };
    assert_eq!(
        draw(shape(AutomationShape::Square, 0, 200, 100)),
        [(0, 1.0), (49, 1.0), (50, 0.0), (99, 0.0), (100, 1.0), (149, 1.0), (150, 0.0), (199, 0.0), (200, 0.0)]
    );
    rig.automation.set_points(lane, clip, Vec::new()).unwrap();
    assert_eq!(draw(shape(AutomationShape::Saw, 0, 200, 100)), [(0, 0.0), (99, 1.0), (100, 0.0), (199, 1.0), (200, 1.0)]);
    rig.automation.set_points(lane, clip, Vec::new()).unwrap();
    assert_eq!(draw(shape(AutomationShape::RampDown, 10, 20, 0)), [(10, 1.0), (20, 0.0)], "a ramp ignores the period");
    rig.automation.set_points(lane, clip, Vec::new()).unwrap();
    let sine = draw(shape(AutomationShape::Sine, 0, 960, 960));
    assert_eq!(sine.len(), 17, "16 steps per period and the end");
    assert!((sine[4].1 - 1.0).abs() < 1e-6 && (sine[12].1).abs() < 1e-6, "peak at a quarter, trough at three quarters");
}

#[test]
fn bad_shapes_are_refused() {
    let rig = Rig::new();
    let (lane, clip) = master_clip(&rig);
    let before = points(&rig);
    let draw = |request| rig.automation.draw_shape(lane, clip, request);
    assert!(matches!(draw(shape(AutomationShape::Sine, 100, 100, 960)), Err(e) if e.is(codes::automation::SHAPE_EMPTY_RANGE)));
    assert!(matches!(draw(shape(AutomationShape::Sine, 0, 960, 1)), Err(e) if e.is(codes::automation::SHAPE_PERIOD_TOO_SHORT)));
    assert!(matches!(draw(shape(AutomationShape::Sine, 0, 960_000, 2)), Err(e) if e.is(codes::automation::TOO_MANY_POINTS)));
    let nan = ShapeRequest { min: f32::NAN, ..shape(AutomationShape::Saw, 0, 960, 960) };
    assert!(matches!(draw(nan), Err(e) if e.is(codes::automation::POINT_NOT_A_NUMBER)));
    assert!(matches!(draw(shape(AutomationShape::Saw, 0, 10_001, 960)), Err(e) if e.is(codes::automation::POINT_OUTSIDE_PATTERN)));
    assert!(
        matches!(rig.automation.draw_shape(999, clip, shape(AutomationShape::Saw, 0, 960, 960)), Err(e) if e.is(codes::automation::LANE_NOT_FOUND))
    );
    assert_eq!(points(&rig), before);
}

#[test]
fn a_new_clip_is_a_flat_line_at_the_current_value_and_loops_over_its_length() {
    let rig = Rig::new();
    let lane = rig.automation.add_lane(ParamTarget::NodeVolume { node: MASTER }).unwrap();
    let volume = rig.service.read(|p| match p.graph.node(MASTER).map(|n| &n.kind) {
        Some(NodeKind::Master { volume }) => *volume,
        _ => panic!("no master"),
    });
    let clip = rig.automation.add_clip(lane, 3_840, 1_920).unwrap();
    let added = rig.automation.lanes()[0].clips[0].clone();
    assert_eq!((added.id, added.start_tick, added.length_ticks, added.loop_ticks), (clip, 3_840, 1_920, 1_920));
    assert_eq!(added.points, [AutoPoint::new(0, volume)]);
    assert!(rig.service.undo());
    assert!(rig.automation.lanes()[0].clips.is_empty(), "one undo step");
    let long = rig.automation.add_clip(lane, 0, MAX_CLIP_LOOP_TICKS * 2).unwrap();
    assert_eq!(rig.automation.lanes()[0].clips[0].loop_ticks, MAX_CLIP_LOOP_TICKS, "the loop is capped");
    rig.automation.remove_clip(lane, long).unwrap();
    assert!(matches!(rig.automation.add_clip(lane, 0, 0), Err(e) if e.is(codes::automation::CLIP_EMPTY)));
    assert!(matches!(rig.automation.add_clip(lane, u32::MAX, 10), Err(e) if e.is(codes::automation::CLIP_PAST_END)));
    assert!(matches!(rig.automation.add_clip(999, 0, 10), Err(e) if e.is(codes::automation::LANE_NOT_FOUND)));
}

#[test]
fn clips_of_a_lane_never_overlap_and_stay_sorted() {
    let rig = Rig::new();
    let (lane, first) = master_clip(&rig);
    assert!(matches!(rig.automation.add_clip(lane, 9_999, 100), Err(e) if e.is(codes::automation::CLIP_OVERLAPS)));
    let second = rig.automation.add_clip(lane, 20_000, 1_000).unwrap();
    let touching = rig.automation.add_clip(lane, 10_000, 1_000).unwrap();
    let order: Vec<_> = rig.automation.lanes()[0].clips.iter().map(|c| c.id).collect();
    assert_eq!(order, [first, touching, second], "sorted by start; touching is fine");
    let moved = rig.automation.edit_clip(lane, second, AutomationClipEdit { start_tick: Some(10_500), ..Default::default() });
    assert!(matches!(moved, Err(e) if e.is(codes::automation::CLIP_OVERLAPS)));
    let grown = rig.automation.edit_clip(lane, first, AutomationClipEdit { length_ticks: Some(10_001), ..Default::default() });
    assert!(matches!(grown, Err(e) if e.is(codes::automation::CLIP_OVERLAPS)));
    assert!(matches!(rig.automation.duplicate_clip(lane, first), Err(e) if e.is(codes::automation::CLIP_OVERLAPS)), "the place is taken");
}

#[test]
fn editing_a_clip_moves_resizes_and_trims_the_pattern() {
    let rig = Rig::new();
    let (lane, clip) = master_clip(&rig);
    rig.automation.set_points(lane, clip, vec![AutoPoint::new(0, 0.1), AutoPoint::new(960, 0.5), AutoPoint::new(5_000, 0.9)]).unwrap();
    let edit = AutomationClipEdit { start_tick: Some(1_000), length_ticks: Some(4_000), loop_ticks: Some(960) };
    let edited = rig.automation.edit_clip(lane, clip, edit).unwrap();
    assert_eq!((edited.start_tick, edited.length_ticks, edited.loop_ticks), (1_000, 4_000, 960));
    assert_eq!(edited.points, [AutoPoint::new(0, 0.1), AutoPoint::new(960, 0.5)], "points after the loop are dropped");
    assert_eq!(rig.automation.lanes()[0].clips[0], edited);
    let zero = rig.automation.edit_clip(lane, clip, AutomationClipEdit { loop_ticks: Some(0), ..Default::default() });
    assert!(matches!(zero, Err(e) if e.is(codes::automation::CLIP_EMPTY)));
    let long = AutomationClipEdit { loop_ticks: Some(MAX_CLIP_LOOP_TICKS + 1), ..Default::default() };
    assert!(matches!(rig.automation.edit_clip(lane, clip, long), Err(e) if e.is(codes::automation::LOOP_TOO_LONG)));
    assert!(rig.service.undo());
    assert_eq!(rig.automation.lanes()[0].clips[0].start_tick, 0, "one undo step");
}

#[test]
fn a_duplicate_follows_the_clip_and_removal_forgets_it() {
    let rig = Rig::new();
    let (lane, clip) = master_clip(&rig);
    rig.automation.set_points(lane, clip, vec![AutoPoint::new(0, 0.2), AutoPoint::new(100, 0.8)]).unwrap();
    let copy = rig.automation.duplicate_clip(lane, clip).unwrap();
    let clips = rig.automation.lanes()[0].clips.clone();
    assert_eq!(clips.len(), 2);
    assert_eq!((clips[1].id, clips[1].start_tick, clips[1].points.clone()), (copy, 10_000, clips[0].points.clone()));
    rig.automation.remove_clip(lane, clip).unwrap();
    assert_eq!(rig.automation.lanes()[0].clips.iter().map(|c| c.id).collect::<Vec<_>>(), [copy]);
    assert!(matches!(rig.automation.remove_clip(lane, clip), Err(e) if e.is(codes::automation::CLIP_NOT_FOUND)));
}

fn sine(period_ticks: u32) -> ClipDrawing {
    ClipDrawing::Shape { shape: AutomationShape::Sine, period_ticks, min: 0.2, max: 0.8 }
}

#[test]
fn a_drawn_clip_creates_its_lane_and_clip_in_one_undo_step() {
    let rig = Rig::new();
    let target = ParamTarget::NodeVolume { node: MASTER };
    let drawn = rig.automation.add_drawn_clip(target.clone(), 3_840, 7_680, sine(960)).unwrap();
    let lanes = rig.automation.lanes();
    assert_eq!((lanes.len(), lanes[0].id, lanes[0].target.clone()), (1, drawn.lane, target.clone()));
    let clip = &lanes[0].clips[0];
    assert_eq!((clip.id, clip.start_tick, clip.length_ticks, clip.loop_ticks), (drawn.clip, 3_840, 7_680, 7_680));
    assert_eq!(clip.points.first().map(|p| (p.tick, p.value)), Some((0, 0.5)), "a sine starts in the middle");
    assert_eq!(clip.points.last().map(|p| p.tick), Some(7_680), "and covers the whole pattern");
    assert!(clip.points.iter().all(|p| (0.2 - 1e-6..=0.8 + 1e-6).contains(&p.value)));

    assert!(rig.service.undo(), "one step");
    assert!(rig.automation.lanes().is_empty(), "the lane goes with its clip");
}

#[test]
fn a_drawn_clip_joins_the_lane_already_there_and_takes_points() {
    let rig = Rig::new();
    let (lane, _) = master_clip(&rig);
    let points = vec![AutoPoint::new(960, 1.0), AutoPoint::new(0, 0.0)];
    let drawn =
        rig.automation.add_drawn_clip(ParamTarget::NodeVolume { node: MASTER }, 10_000, 3_840, ClipDrawing::Points { points }).unwrap();
    assert_eq!(drawn.lane, lane);
    let lanes = rig.automation.lanes();
    assert_eq!((lanes.len(), lanes[0].clips.len()), (1, 2));
    assert_eq!(lanes[0].clips[1].points, [AutoPoint::new(0, 0.0), AutoPoint::new(960, 1.0)], "sorted like set_points");
}

#[test]
fn a_refused_drawn_clip_leaves_nothing_behind() {
    let rig = Rig::new();
    let before = rig.service.snapshot();
    let missing = rig.automation.add_drawn_clip(ParamTarget::NodePan { node: 9_999 }, 0, 3_840, sine(960));
    assert!(matches!(missing, Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
    let short = rig.automation.add_drawn_clip(ParamTarget::NodeVolume { node: MASTER }, 0, 3_840, sine(1));
    assert!(short.is_err(), "a period under MIN_SHAPE_PERIOD");
    let outside = ClipDrawing::Points { points: vec![AutoPoint::new(5_000, 0.5)] };
    let late = rig.automation.add_drawn_clip(ParamTarget::NodeVolume { node: MASTER }, 0, 3_840, outside);
    assert!(matches!(late, Err(e) if e.is(codes::automation::POINT_OUTSIDE_PATTERN)));
    assert_eq!(rig.service.snapshot(), before, "no empty lane is left");

    let (lane, _) = master_clip(&rig);
    let overlap = rig.automation.add_drawn_clip(ParamTarget::NodeVolume { node: MASTER }, 5_000, 3_840, sine(960));
    assert!(matches!(overlap, Err(e) if e.is(codes::automation::CLIP_OVERLAPS)));
    assert_eq!(rig.automation.lanes()[0].clips.len(), 1, "the lane {lane} keeps its one clip");
}
