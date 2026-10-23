//! `ProjectAutomation`: automation — parameter automation lanes, their clips and the clips' pattern points.

use std::sync::Arc;

use daw_contracts::codes::automation;
use daw_contracts::{
    AutoPoint, AutomationClip, AutomationClipEdit, AutomationClipId, AutomationControl, AutomationLane, ClipDrawing, ContractError,
    DrawnClip, LaneId, MAX_AUTOMATION_CLIPS, MAX_CLIP_LOOP_TICKS, MAX_LANES, MAX_POINTS, ParamTarget, Project, ShapeRequest, tr,
};

use crate::ProjectService;
use crate::automation_shapes::{replace_range, shape_points};
use crate::ops;

pub struct ProjectAutomation {
    project: Arc<ProjectService>,
}

impl ProjectAutomation {
    pub fn new(project: Arc<ProjectService>) -> Self {
        Self { project }
    }
}

fn lane_mut(project: &mut Project, id: LaneId) -> Result<&mut AutomationLane, ContractError> {
    project.automation.iter_mut().find(|lane| lane.id == id).ok_or_else(|| automation::LANE_NOT_FOUND.with("id", id))
}

fn clip_mut(lane: &mut AutomationLane, id: AutomationClipId) -> Result<&mut AutomationClip, ContractError> {
    let lane_id = lane.id;
    lane.clips.iter_mut().find(|clip| clip.id == id).ok_or_else(|| automation::CLIP_NOT_FOUND.with("id", id).with("lane", lane_id))
}

/// Length, loop and end of a clip are valid.
fn check_span(start: u32, length: u32, loop_ticks: u32) -> Result<(), ContractError> {
    if length == 0 || loop_ticks == 0 {
        return Err(automation::CLIP_EMPTY.err());
    }
    if loop_ticks > MAX_CLIP_LOOP_TICKS {
        return Err(automation::LOOP_TOO_LONG.with("max", MAX_CLIP_LOOP_TICKS));
    }
    if start.checked_add(length).is_none() {
        return Err(automation::CLIP_PAST_END.with("start", start).with("length", length));
    }
    Ok(())
}

/// No other clip of the lane overlaps `start..start + length` (touching is fine).
fn check_free(lane: &AutomationLane, except: Option<AutomationClipId>, start: u32, length: u32) -> Result<(), ContractError> {
    let end = u64::from(start) + u64::from(length);
    let taken = lane
        .clips
        .iter()
        .filter(|c| Some(c.id) != except)
        .find(|c| u64::from(c.start_tick) < end && u64::from(start) < u64::from(c.start_tick) + u64::from(c.length_ticks));
    match taken {
        Some(c) => Err(automation::CLIP_OVERLAPS.with("start", c.start_tick).with("end", c.start_tick.saturating_add(c.length_ticks))),
        None => Ok(()),
    }
}

fn sort_clips(lane: &mut AutomationLane) {
    lane.clips.sort_by_key(|c| c.start_tick);
}

/// Points as stored: valid numbers, curves clamped, sorted by tick (of equal ticks the last stays), within the loop.
fn normalize(mut points: Vec<AutoPoint>, loop_ticks: u32) -> Result<Vec<AutoPoint>, ContractError> {
    if points.len() > MAX_POINTS {
        return Err(automation::TOO_MANY_POINTS.with("max", MAX_POINTS));
    }
    if let Some(point) = points.iter().find(|p| !p.value.is_finite()) {
        return Err(automation::POINT_NOT_A_NUMBER.with("value", point.value));
    }
    if let Some(point) = points.iter().find(|p| !p.curve.is_finite()) {
        return Err(automation::POINT_CURVE_NOT_A_NUMBER.with("curve", point.curve));
    }
    if let Some(point) = points.iter().find(|p| p.tick > loop_ticks) {
        return Err(automation::POINT_OUTSIDE_PATTERN.with("tick", point.tick).with("loop", loop_ticks));
    }
    for point in &mut points {
        point.curve = point.curve.clamp(-1.0, 1.0);
    }
    points.sort_by_key(|p| p.tick);
    points.reverse();
    points.dedup_by_key(|p| p.tick);
    points.reverse();
    Ok(points)
}

impl AutomationControl for ProjectAutomation {
    fn lanes(&self) -> Vec<AutomationLane> {
        self.project.read(|p| p.automation.clone())
    }

    fn add_lane(&self, target: ParamTarget) -> Result<LaneId, ContractError> {
        self.project.edit(&tr!("Add automation", "Добавить автоматизацию"), |project| {
            ops::limit(project.automation.len(), MAX_LANES, automation::TOO_MANY_LANES)?;
            ops::validate_target(project, &target)?;
            if project.automation.iter().any(|lane| lane.target == target) {
                return Err(automation::LANE_EXISTS.err());
            }
            let id = project.alloc_id();
            project.automation.push(AutomationLane { id, target, clips: Vec::new() });
            Ok(id)
        })
    }

    fn remove_lane(&self, id: LaneId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Delete automation", "Удалить автоматизацию"), |project| {
            let before = project.automation.len();
            project.automation.retain(|lane| lane.id != id);
            if project.automation.len() == before {
                return Err(automation::LANE_NOT_FOUND.with("id", id));
            }
            Ok(())
        })
    }

    fn add_clip(&self, lane: LaneId, start_tick: u32, length_ticks: u32) -> Result<AutomationClipId, ContractError> {
        let loop_ticks = length_ticks.min(MAX_CLIP_LOOP_TICKS);
        check_span(start_tick, length_ticks, loop_ticks)?;
        self.project.edit(&tr!("Add automation clip", "Добавить клип автоматизации"), |project| {
            let id = project.alloc_id();
            let target = lane_mut(project, lane)?.target.clone();
            let value = ops::target_value(project, &target);
            let lane = lane_mut(project, lane)?;
            ops::limit(lane.clips.len(), MAX_AUTOMATION_CLIPS, automation::TOO_MANY_CLIPS)?;
            check_free(lane, None, start_tick, length_ticks)?;
            let points = value.map(|v| vec![AutoPoint::new(0, v)]).unwrap_or_default();
            lane.clips.push(AutomationClip { id, start_tick, length_ticks, loop_ticks, points });
            sort_clips(lane);
            Ok(id)
        })
    }

    fn edit_clip(&self, lane: LaneId, clip: AutomationClipId, edit: AutomationClipEdit) -> Result<AutomationClip, ContractError> {
        self.project.edit(&tr!("Edit automation clip", "Изменить клип автоматизации"), |project| {
            let lane = lane_mut(project, lane)?;
            let current = clip_mut(lane, clip)?.clone();
            let start = edit.start_tick.unwrap_or(current.start_tick);
            let length = edit.length_ticks.unwrap_or(current.length_ticks);
            let loop_ticks = edit.loop_ticks.unwrap_or(current.loop_ticks);
            check_span(start, length, loop_ticks)?;
            check_free(lane, Some(clip), start, length)?;
            let changed = clip_mut(lane, clip)?;
            changed.start_tick = start;
            changed.length_ticks = length;
            changed.loop_ticks = loop_ticks;
            changed.points.retain(|p| p.tick <= loop_ticks);
            let result = changed.clone();
            sort_clips(lane);
            Ok(result)
        })
    }

    fn duplicate_clip(&self, lane: LaneId, clip: AutomationClipId) -> Result<AutomationClipId, ContractError> {
        self.project.edit(&tr!("Duplicate automation clip", "Дублировать клип автоматизации"), |project| {
            let id = project.alloc_id();
            let lane = lane_mut(project, lane)?;
            ops::limit(lane.clips.len(), MAX_AUTOMATION_CLIPS, automation::TOO_MANY_CLIPS)?;
            let source = clip_mut(lane, clip)?.clone();
            let start = source
                .start_tick
                .checked_add(source.length_ticks)
                .ok_or_else(|| automation::CLIP_PAST_END.with("start", source.start_tick).with("length", source.length_ticks))?;
            check_span(start, source.length_ticks, source.loop_ticks)?;
            check_free(lane, None, start, source.length_ticks)?;
            lane.clips.push(AutomationClip { id, start_tick: start, ..source });
            sort_clips(lane);
            Ok(id)
        })
    }

    fn remove_clip(&self, lane: LaneId, clip: AutomationClipId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Delete automation clip", "Удалить клип автоматизации"), |project| {
            let lane = lane_mut(project, lane)?;
            clip_mut(lane, clip)?;
            lane.clips.retain(|c| c.id != clip);
            Ok(())
        })
    }

    fn set_points(&self, lane: LaneId, clip: AutomationClipId, points: Vec<AutoPoint>) -> Result<(), ContractError> {
        let key = Some(format!("auto:{lane}:{clip}"));
        self.project.edit_keyed(&tr!("Automation points", "Точки автоматизации"), key, |project| {
            let clip = clip_mut(lane_mut(project, lane)?, clip)?;
            clip.points = normalize(points, clip.loop_ticks)?;
            Ok(())
        })
    }

    fn draw_shape(&self, lane: LaneId, clip: AutomationClipId, request: ShapeRequest) -> Result<(), ContractError> {
        let shape = shape_points(&request)?;
        self.project.edit(&tr!("Draw automation shape", "Нарисовать форму автоматизации"), |project| {
            let clip = clip_mut(lane_mut(project, lane)?, clip)?;
            if request.to_tick > clip.loop_ticks {
                return Err(automation::POINT_OUTSIDE_PATTERN.with("tick", request.to_tick).with("loop", clip.loop_ticks));
            }
            let points = replace_range(&clip.points, request.from_tick, request.to_tick, shape);
            if points.len() > MAX_POINTS {
                return Err(automation::TOO_MANY_POINTS.with("max", MAX_POINTS));
            }
            clip.points = points;
            Ok(())
        })
    }

    fn add_drawn_clip(
        &self,
        target: ParamTarget,
        start_tick: u32,
        length_ticks: u32,
        drawing: ClipDrawing,
    ) -> Result<DrawnClip, ContractError> {
        let loop_ticks = length_ticks.min(MAX_CLIP_LOOP_TICKS);
        check_span(start_tick, length_ticks, loop_ticks)?;
        let points = match drawing {
            ClipDrawing::Shape { shape, period_ticks, min, max } => {
                let points = shape_points(&ShapeRequest { from_tick: 0, to_tick: loop_ticks, shape, period_ticks, min, max })?;
                if points.len() > MAX_POINTS {
                    return Err(automation::TOO_MANY_POINTS.with("max", MAX_POINTS));
                }
                points
            }
            ClipDrawing::Points { points } => normalize(points, loop_ticks)?,
        };
        self.project.edit(&tr!("Draw automation", "Нарисовать автоматизацию"), |project| {
            let existing = project.automation.iter().find(|lane| lane.target == target).map(|lane| lane.id);
            let lane = match existing {
                Some(id) => id,
                None => {
                    ops::limit(project.automation.len(), MAX_LANES, automation::TOO_MANY_LANES)?;
                    ops::validate_target(project, &target)?;
                    let id = project.alloc_id();
                    project.automation.push(AutomationLane { id, target, clips: Vec::new() });
                    id
                }
            };
            let clip = project.alloc_id();
            let entry = lane_mut(project, lane)?;
            ops::limit(entry.clips.len(), MAX_AUTOMATION_CLIPS, automation::TOO_MANY_CLIPS)?;
            check_free(entry, None, start_tick, length_ticks)?;
            entry.clips.push(AutomationClip { id: clip, start_tick, length_ticks, loop_ticks, points });
            sort_clips(entry);
            Ok(DrawnClip { lane, clip })
        })
    }
}
