//! Automation of params over song time (`AutomationControl`: lanes and their clips) and the one formula everybody uses
//! for the value at a tick ([`curve_shape`], [`value_at`], [`clip_value_at`], [`lane_value_at`]).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{AutoPoint, AutomationClip, AutomationClipId, AutomationLane, ContractError, LaneId, ParamTarget};

/// How many automation lanes a project can have.
pub const MAX_LANES: usize = 256;
/// How many clips one lane can have.
pub const MAX_AUTOMATION_CLIPS: usize = 1024;
/// How many points one automation clip (its pattern) can have.
pub const MAX_POINTS: usize = 4096;
/// The shortest period of a repeating shape ([`AutomationControl::draw_shape`]).
pub const MIN_SHAPE_PERIOD: u32 = 2;
/// How strongly `curve = ±1` bends a segment: the exponent of the power curve is `CURVE_STRENGTH^|curve|`.
pub const CURVE_STRENGTH: f32 = 4.0;

/// The bent progress `0..=1` at linear progress `t` (`0..=1`) along a segment with `curve` (`-1..=1`, clamped):
/// `curve = 0` is a straight line `t`; `curve > 0` starts slowly and speeds up, `t^e`; `curve < 0` starts fast and
/// slows down, `1 - (1 - t)^e`; where `e = CURVE_STRENGTH^|curve|`. Mirror-symmetric: `curve` and `-curve` bend the
/// same amount in opposite directions. The interface draws with the same formula (`app/src/automation/automationCurve.ts`, `curveShape`).
pub fn curve_shape(t: f32, curve: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let curve = if curve.is_finite() { curve.clamp(-1.0, 1.0) } else { 0.0 };
    if curve == 0.0 {
        return t;
    }
    let exponent = CURVE_STRENGTH.powf(curve.abs());
    if curve > 0.0 { t.powf(exponent) } else { 1.0 - (1.0 - t).powf(exponent) }
}

/// The pattern value at `tick` (points sorted by tick): before the first point its value, after the last one its value,
/// between two points `a → b` the value `a.value + (b.value - a.value) * curve_shape(t, a.curve)`. Without points, `None`.
pub fn value_at(points: &[AutoPoint], tick: f64) -> Option<f32> {
    let first = points.first()?;
    if tick <= f64::from(first.tick) {
        return Some(first.value);
    }
    let last = points.last()?;
    if tick >= f64::from(last.tick) {
        return Some(last.value);
    }
    let after = points.partition_point(|p| f64::from(p.tick) <= tick);
    let (a, b) = (points[after - 1], points[after]);
    let span = f64::from(b.tick - a.tick);
    let t = ((tick - f64::from(a.tick)) / span) as f32;
    Some(a.value + (b.value - a.value) * curve_shape(t, a.curve))
}

/// The clip value at song `tick`: `None` outside `start_tick..start_tick + length_ticks` or without points; inside,
/// [`value_at`] of the pattern at `(tick - start_tick) mod loop_ticks`. At the loop seam the pattern does not join
/// itself: up to the end of a repeat the value follows the last points (towards a point at `loop_ticks`, if there is
/// one, otherwise holding the last value), and the next repeat starts again at its first point's value.
pub fn clip_value_at(clip: &AutomationClip, tick: f64) -> Option<f32> {
    let start = f64::from(clip.start_tick);
    if tick < start || tick >= start + f64::from(clip.length_ticks) || clip.loop_ticks == 0 {
        return None;
    }
    value_at(&clip.points, (tick - start) % f64::from(clip.loop_ticks))
}

/// The lane value at song `tick`: the value of the clip under it ([`clip_value_at`]), `None` between clips (the param
/// plays its own value). Clips must be sorted by start (as lanes keep them). Allocation-free: the engine calls it.
pub fn lane_value_at(lane: &AutomationLane, tick: f64) -> Option<f32> {
    let after = lane.clips.partition_point(|c| f64::from(c.start_tick) <= tick);
    clip_value_at(lane.clips.get(after.checked_sub(1)?)?, tick)
}

/// A shape drawn over a range of a clip's pattern by [`AutomationControl::draw_shape`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AutomationShape {
    /// Starts in the middle going up: `mid + amplitude * sin(2π·phase)`.
    Sine,
    /// `min` at the period start, `max` in its middle.
    Triangle,
    /// Rises from `min` to `max` over the period, then jumps back.
    Saw,
    /// `max` in the first half of the period, `min` in the second.
    Square,
    /// One straight line from `min` at `from_tick` to `max` at `to_tick` (the period is ignored).
    RampUp,
    /// One straight line from `max` at `from_tick` to `min` at `to_tick` (the period is ignored).
    RampDown,
}

/// What [`AutomationControl::draw_shape`] draws; ticks are pattern ticks (from the clip start, `0..=loop_ticks`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShapeRequest {
    pub from_tick: u32,
    pub to_tick: u32,
    pub shape: AutomationShape,
    /// Length of one repetition in ticks; ignored by the ramps.
    pub period_ticks: u32,
    /// Lowest and highest value in param units (not clamped, like `set_points`).
    pub min: f32,
    pub max: f32,
}

/// What a clip made by [`AutomationControl::add_drawn_clip`] plays over its whole pattern (`0..=loop_ticks`).
/// JSON: `{ "type": "shape", "shape": "sine", "periodTicks": 960, "min": 0, "max": 1 }` or
/// `{ "type": "points", "points": [{ "tick": 0, "value": 0.2 }, …] }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ClipDrawing {
    /// A shape from the pattern start to its end, as [`AutomationControl::draw_shape`].
    Shape {
        shape: AutomationShape,
        /// Length of one repetition in ticks; ignored by the ramps.
        period_ticks: u32,
        /// Lowest and highest value in param units.
        min: f32,
        max: f32,
    },
    /// The pattern points, as [`AutomationControl::set_points`].
    Points { points: Vec<AutoPoint> },
}

/// Where [`AutomationControl::add_drawn_clip`] put the drawing: the lane (found or created) and the new clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DrawnClip {
    pub lane: LaneId,
    pub clip: AutomationClipId,
}

/// Partial change of an automation clip; `None` (field omitted) leaves the value unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct AutomationClipEdit {
    pub start_tick: Option<u32>,
    pub length_ticks: Option<u32>,
    pub loop_ticks: Option<u32>,
}

/// Automation. Job: edit the automation lanes of params and their clips. Every mutating method is one undo step.
///
/// A lane belongs to one param and holds clips ([`AutomationClip`]): each repeats its point pattern every
/// `loop_ticks` over its length; outside clips the param plays its own value. Clips of one lane never overlap
/// (touching is fine); an edit that would overlap is `ErrorKind::Invalid` and changes nothing. Common errors: an
/// unknown lane or clip is `ErrorKind::NotFound`; a zero length or loop, a loop over `MAX_CLIP_LOOP_TICKS` or a clip
/// ending past `u32::MAX` ticks is `ErrorKind::Invalid`.
pub trait AutomationControl: Send + Sync {
    fn lanes(&self) -> Vec<AutomationLane>;
    /// A new lane without clips for a param. The target must exist and have this param (volume on a source, bus or
    /// master; pan on a source or bus; stage depth or height on a source or bus standing on the stage — otherwise
    /// `graph::NOT_ON_STAGE`; an effect or instrument param in its list; a connection level on an existing connection of
    /// this pair and kind): an unknown node or track is `ErrorKind::NotFound`, anything else is
    /// `ErrorKind::Invalid`. A lane for this target already exists is `ErrorKind::Invalid`; more than [`MAX_LANES`]
    /// is `ErrorKind::Limit`.
    fn add_lane(&self, target: ParamTarget) -> Result<LaneId, ContractError>;
    fn remove_lane(&self, id: LaneId) -> Result<(), ContractError>;
    /// A new clip at `start_tick` with `loop_ticks = min(length_ticks, MAX_CLIP_LOOP_TICKS)` and one point at tick 0
    /// holding the param's current value (a flat line: placing a clip changes nothing audible until it is drawn).
    /// More than [`MAX_AUTOMATION_CLIPS`] in the lane is `ErrorKind::Limit`.
    fn add_clip(&self, lane: LaneId, start_tick: u32, length_ticks: u32) -> Result<AutomationClipId, ContractError>;
    /// Moves, resizes or changes the loop of a clip; returns the changed clip. A shorter loop drops the pattern points
    /// after the new `loop_ticks`.
    fn edit_clip(&self, lane: LaneId, clip: AutomationClipId, edit: AutomationClipEdit) -> Result<AutomationClip, ContractError>;
    /// A copy of the clip right after it (at its end); returns the copy's id. If that place is taken,
    /// `ErrorKind::Invalid`.
    fn duplicate_clip(&self, lane: LaneId, clip: AutomationClipId) -> Result<AutomationClipId, ContractError>;
    fn remove_clip(&self, lane: LaneId, clip: AutomationClipId) -> Result<(), ContractError>;
    /// Replaces the clip's pattern points: they are sorted by time, and of points with the same tick the last in the
    /// list stays. Values are not clamped (whoever applies them does that); `curve` is clamped to `-1..=1`. A point
    /// after `loop_ticks` is `ErrorKind::Invalid`, more than [`MAX_POINTS`] is `ErrorKind::Limit`, a non-number value
    /// or curve is `ErrorKind::Invalid`. Consecutive replacements of the same clip's points merge into one undo step.
    fn set_points(&self, lane: LaneId, clip: AutomationClipId, points: Vec<AutoPoint>) -> Result<(), ContractError>;
    /// Replaces the pattern points inside `from_tick..=to_tick` with `shape`; points outside stay. The shape starts
    /// its first period at `from_tick` and ends with a point at `to_tick`; repeating shapes are straight-line points (a
    /// sine is sampled 16 times per period). `from_tick >= to_tick`, `to_tick` after the clip's `loop_ticks`, a period
    /// shorter than [`MIN_SHAPE_PERIOD`] for a repeating shape, or a non-number `min`/`max` is `ErrorKind::Invalid`;
    /// a pattern over [`MAX_POINTS`] is `ErrorKind::Limit`.
    fn draw_shape(&self, lane: LaneId, clip: AutomationClipId, shape: ShapeRequest) -> Result<(), ContractError>;
    /// A param's automation in one call and one undo step: the lane of `target` (created under the rules of
    /// `add_lane` when there is none), a new clip in it (as `add_clip`) and `drawing` over the clip's whole pattern
    /// (a shape as `draw_shape` from 0 to `loop_ticks`, or points as `set_points`). Their errors are this method's;
    /// on any of them the project is unchanged — no empty lane or clip is left behind.
    fn add_drawn_clip(
        &self,
        target: ParamTarget,
        start_tick: u32,
        length_ticks: u32,
        drawing: ClipDrawing,
    ) -> Result<DrawnClip, ContractError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(tick: u32, value: f32) -> AutoPoint {
        AutoPoint::new(tick, value)
    }

    #[test]
    fn interpolates_holds_and_handles_empty() {
        assert_eq!(value_at(&[], 5.0), None);
        let points = [p(100, 0.0), p(200, 1.0), p(300, 0.5)];
        assert_eq!(value_at(&points, 0.0), Some(0.0));
        assert_eq!(value_at(&points, 100.0), Some(0.0));
        assert_eq!(value_at(&points, 150.0), Some(0.5));
        assert_eq!(value_at(&points, 250.0), Some(0.75));
        assert_eq!(value_at(&points, 9_999.0), Some(0.5));
        assert_eq!(value_at(&[p(10, 3.0)], 500.0), Some(3.0));
    }

    fn clip(start_tick: u32, length_ticks: u32, loop_ticks: u32, points: Vec<AutoPoint>) -> AutomationClip {
        AutomationClip { id: 1, start_tick, length_ticks, loop_ticks, points }
    }

    #[test]
    fn a_clip_repeats_its_pattern_and_is_silent_outside() {
        let ramp = clip(1_000, 300, 100, vec![p(0, 0.0), p(100, 1.0)]);
        assert_eq!(clip_value_at(&ramp, 999.0), None, "before the clip");
        assert_eq!(clip_value_at(&ramp, 1_300.0), None, "the end is exclusive");
        assert_eq!(clip_value_at(&ramp, 1_050.0), Some(0.5));
        assert_eq!(clip_value_at(&ramp, 1_150.0), Some(0.5), "the second repeat");
        assert_eq!(clip_value_at(&ramp, 1_200.0), Some(0.0), "a repeat starts at its first point");
        let held = clip(0, 400, 200, vec![p(50, 0.3), p(100, 0.7)]);
        assert_eq!(clip_value_at(&held, 10.0), Some(0.3), "before the first point its value");
        assert_eq!(clip_value_at(&held, 190.0), Some(0.7), "the last point holds to the seam");
        assert_eq!(clip_value_at(&clip(0, 100, 100, Vec::new()), 10.0), None, "no points: no override");
    }

    #[test]
    fn a_lane_plays_the_clip_under_the_tick() {
        let lane = AutomationLane {
            id: 1,
            target: ParamTarget::NodeVolume { node: 1 },
            clips: vec![clip(0, 100, 100, vec![p(0, 0.2)]), clip(200, 100, 100, vec![p(0, 0.8)])],
        };
        assert_eq!(lane_value_at(&lane, 50.0), Some(0.2));
        assert_eq!(lane_value_at(&lane, 150.0), None, "between clips");
        assert_eq!(lane_value_at(&lane, 250.0), Some(0.8));
        assert_eq!(lane_value_at(&lane, 300.0), None);
    }

    #[test]
    fn curves_bend_symmetrically_and_keep_the_ends() {
        for curve in [-1.0, -0.4, 0.0, 0.7, 1.0] {
            assert_eq!(curve_shape(0.0, curve), 0.0);
            assert!((curve_shape(1.0, curve) - 1.0).abs() < 1e-6);
        }
        assert!((curve_shape(0.5, 0.0) - 0.5).abs() < 1e-6);
        assert!(curve_shape(0.5, 1.0) < 0.5, "a positive curve starts slowly");
        assert!(curve_shape(0.5, -1.0) > 0.5, "a negative curve starts fast");
        assert!((curve_shape(0.5, 1.0) + curve_shape(0.5, -1.0) - 1.0).abs() < 1e-6, "mirror-symmetric");
        assert_eq!(curve_shape(0.5, 5.0), curve_shape(0.5, 1.0), "clamped");
        let bent = [AutoPoint { tick: 0, value: 0.0, curve: 1.0 }, p(100, 1.0)];
        assert!((value_at(&bent, 50.0).unwrap() - 0.0625).abs() < 1e-6, "0.5^4");
    }
}
