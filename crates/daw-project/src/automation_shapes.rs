//! Shapes drawn over a range of an automation clip pattern (`AutomationControl::draw_shape`): pure point generation.
//! The interface mock mirrors it (`app/src/automation/automationCurve.ts`, `shapePoints`).

use std::f32::consts::TAU;

use daw_contracts::codes::automation;
use daw_contracts::{AutoPoint, AutomationShape, ContractError, MAX_POINTS, MIN_SHAPE_PERIOD, ShapeRequest};

/// A sine is drawn with this many straight segments per period.
const SINE_STEPS: u32 = 16;

/// Value fraction `0..=1` (min → max) of a repeating shape at `phase` (`0..1`).
fn level(shape: AutomationShape, phase: f32) -> f32 {
    match shape {
        AutomationShape::Sine => 0.5 + 0.5 * (TAU * phase).sin(),
        AutomationShape::Triangle => 1.0 - (2.0 * phase - 1.0).abs(),
        AutomationShape::Saw | AutomationShape::RampUp => phase,
        AutomationShape::Square => {
            if phase < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        AutomationShape::RampDown => 1.0 - phase,
    }
}

/// The level at the very end of a period (just before the next one starts).
fn end_level(shape: AutomationShape) -> f32 {
    match shape {
        AutomationShape::Saw | AutomationShape::RampUp => 1.0,
        AutomationShape::Sine => 0.5,
        AutomationShape::Triangle | AutomationShape::Square | AutomationShape::RampDown => 0.0,
    }
}

/// Offsets inside one period (ticks from the period start) where a repeating shape gets a point.
fn period_offsets(shape: AutomationShape, period: u32) -> Vec<u32> {
    let half = period / 2;
    match shape {
        AutomationShape::Sine => (0..SINE_STEPS).map(|i| (u64::from(period) * u64::from(i) / u64::from(SINE_STEPS)) as u32).collect(),
        AutomationShape::Triangle => vec![0, half],
        // A jump is two points a tick apart: the end of one level and the start of the next.
        AutomationShape::Saw => vec![0, period - 1],
        AutomationShape::Square => vec![0, half - 1, half, period - 1],
        AutomationShape::RampUp | AutomationShape::RampDown => vec![0],
    }
}

/// The points of `request`, by ascending tick (no two on one tick).
pub(crate) fn shape_points(request: &ShapeRequest) -> Result<Vec<AutoPoint>, ContractError> {
    let ShapeRequest { from_tick: from, to_tick: to, shape, period_ticks, min, max } = *request;
    if from >= to {
        return Err(automation::SHAPE_EMPTY_RANGE.with("from", from).with("to", to));
    }
    if let Some(bad) = [min, max].into_iter().find(|v| !v.is_finite()) {
        return Err(automation::POINT_NOT_A_NUMBER.with("value", bad));
    }
    let value = |fraction: f32| min + (max - min) * fraction;
    let span = to - from;
    if matches!(shape, AutomationShape::RampUp | AutomationShape::RampDown) {
        return Ok(vec![AutoPoint::new(from, value(level(shape, 0.0))), AutoPoint::new(to, value(level(shape, 1.0)))]);
    }
    if period_ticks < MIN_SHAPE_PERIOD {
        return Err(automation::SHAPE_PERIOD_TOO_SHORT.with("min", MIN_SHAPE_PERIOD));
    }
    let offsets = period_offsets(shape, period_ticks);
    let periods = u64::from(span).div_ceil(u64::from(period_ticks));
    if periods.saturating_mul(offsets.len() as u64) > MAX_POINTS as u64 {
        return Err(automation::TOO_MANY_POINTS.with("max", MAX_POINTS));
    }
    let phase_at = |tick: u32| ((tick - from) % period_ticks) as f32 / period_ticks as f32;
    let mut points: Vec<AutoPoint> = Vec::new();
    for period in 0..periods {
        let start = u64::from(from) + period * u64::from(period_ticks);
        for &offset in &offsets {
            let tick = start + u64::from(offset);
            if tick >= u64::from(to) {
                break;
            }
            let tick = tick as u32;
            let jump_end = offset + 1 == period_ticks && matches!(shape, AutomationShape::Saw | AutomationShape::Square);
            let fraction = if jump_end { end_level(shape) } else { level(shape, phase_at(tick)) };
            if points.last().is_none_or(|p| p.tick < tick) {
                points.push(AutoPoint::new(tick, value(fraction)));
            }
        }
    }
    let end = if span % period_ticks == 0 { end_level(shape) } else { level(shape, phase_at(to)) };
    points.push(AutoPoint::new(to, value(end)));
    Ok(points)
}

/// `pattern` with the points inside `from..=to` replaced by `shape` (both sorted by tick).
pub(crate) fn replace_range(pattern: &[AutoPoint], from: u32, to: u32, shape: Vec<AutoPoint>) -> Vec<AutoPoint> {
    let mut points: Vec<AutoPoint> = pattern.iter().copied().filter(|p| p.tick < from || p.tick > to).chain(shape).collect();
    points.sort_by_key(|p| p.tick);
    points
}
