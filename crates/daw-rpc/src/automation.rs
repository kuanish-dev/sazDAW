//! `automation.*` methods: thin adapters over `AutomationControl` (lanes, their clips and pattern points) and
//! `AutomationWriter` (writing param moves live while the song plays).

use daw_contracts::{
    AutoPoint, AutomationClipEdit, AutomationClipId, AutomationWriteMode, ClipDrawing, LaneId, ParamTarget, ShapeRequest, TouchEnd,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;
use crate::params::Id;

#[derive(Deserialize, JsonSchema)]
struct AddLane {
    target: ParamTarget,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AddDrawnClip {
    /// The parameter: its lane is found or created.
    target: ParamTarget,
    start_tick: u32,
    length_ticks: u32,
    /// What the clip plays over its whole pattern: a shape or points.
    drawing: ClipDrawing,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AddClip {
    lane: LaneId,
    start_tick: u32,
    length_ticks: u32,
}

#[derive(Deserialize, JsonSchema)]
struct ClipRef {
    lane: LaneId,
    clip: AutomationClipId,
}

#[derive(Deserialize, JsonSchema)]
struct EditClip {
    lane: LaneId,
    clip: AutomationClipId,
    #[serde(flatten)]
    edit: AutomationClipEdit,
}

#[derive(Deserialize, JsonSchema)]
struct SetPoints {
    lane: LaneId,
    clip: AutomationClipId,
    points: Vec<AutoPoint>,
}

#[derive(Deserialize, JsonSchema)]
struct DrawShape {
    lane: LaneId,
    clip: AutomationClipId,
    #[serde(flatten)]
    shape: ShapeRequest,
}

#[derive(Deserialize, JsonSchema)]
struct WriteMode {
    mode: AutomationWriteMode,
}

#[derive(Deserialize, JsonSchema)]
struct Touch {
    target: ParamTarget,
    value: f32,
}

#[derive(Deserialize, JsonSchema)]
struct Release {
    target: ParamTarget,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "automation.addLane",
        "Creates an empty automation lane (a timeline row) for a parameter (instrument, effect, volume, pan, width, \
         connection level). A lane plays only inside its clips: add them with automation.addClip.",
        |c, p: AddLane| c.automation.add_lane(p.target),
    );
    table.add("automation.removeLane", "Removes an automation lane with all its clips.", |c, p: Id| c.automation.remove_lane(p.id));
    table.add(
        "automation.addClip",
        "Adds an automation clip to a lane at startTick, lengthTicks long; returns its id. Like a looped note clip, a clip \
         repeats its pattern of points every loopTicks (initially = the length) over its length; outside clips the \
         parameter keeps its own value. The new clip is a flat line at the parameter's current value. Clips of one lane \
         cannot overlap.",
        |c, p: AddClip| c.automation.add_clip(p.lane, p.start_tick, p.length_ticks),
    );
    table.add(
        "automation.editClip",
        "Moves (startTick), resizes (lengthTicks) or changes the pattern length (loopTicks) of an automation clip; omitted \
         fields stay. A shorter loop drops pattern points after it. Returns the clip.",
        |c, p: EditClip| c.automation.edit_clip(p.lane, p.clip, p.edit),
    );
    table.add(
        "automation.duplicateClip",
        "Copies an automation clip right after itself (the place must be free); returns the copy's id.",
        |c, p: ClipRef| c.automation.duplicate_clip(p.lane, p.clip),
    );
    table.add("automation.removeClip", "Removes an automation clip.", |c, p: ClipRef| c.automation.remove_clip(p.lane, p.clip));
    table.add(
        "automation.setPoints",
        "Replaces the pattern points of an automation clip: tick from the clip start (0..loopTicks; a point at loopTicks \
         sets the value the repeat ends on), value in parameter units, and optional curve -1..1 bending the segment to \
         the next point (0 straight, positive starts slowly, negative starts fast). Each repeat starts again from the \
         first point.",
        |c, p: SetPoints| c.automation.set_points(p.lane, p.clip, p.points),
    );
    table.add(
        "automation.drawShape",
        "Draws a shape over fromTick..toTick of an automation clip's pattern (ticks from the clip start, toTick at most \
         loopTicks), replacing the points there (one undo step): sine, triangle, saw, square (repeating every \
         periodTicks, e.g. 960 = a quarter note) or rampUp/rampDown (one line), between min and max in parameter units.",
        |c, p: DrawShape| c.automation.draw_shape(p.lane, p.clip, p.shape),
    );
    table.add(
        "automation.addDrawnClip",
        "Automates a parameter in ONE call and one undo step: finds the target's lane or creates it, adds a clip at \
         startTick, lengthTicks long, and draws it over its whole pattern — drawing { type: \"shape\", shape (sine, \
         triangle, saw, square, rampUp, rampDown), periodTicks, min, max } or { type: \"points\", points }. Prefer it to \
         addLane + addClip + drawShape. Returns { lane, clip }.",
        |c, p: AddDrawnClip| c.automation.add_drawn_clip(p.target, p.start_tick, p.length_ticks, p.drawing),
    );
    table.add(
        "automation.writeMode",
        "The live automation write mode: off, touch (a moved parameter is written while its control is held) or latch \
         (written until playback stops).",
        |c, ()| Ok(c.automation_writer.mode()),
    );
    table.add(
        "automation.setWriteMode",
        "Sets the live automation write mode (off, touch, latch). While playing and not off, parameter moves become \
         automation points; one playback pass is one undo step.",
        |c, p: WriteMode| Ok(c.automation_writer.set_mode(p.mode)),
    );
    table.add(
        "automation.touch",
        "Reports that the interface is moving a parameter to value (call it right before setting the parameter): while \
         playing with the write mode on, the value is written at the play position into the clip there (its pattern), or \
         into a new clip from the bar start that grows while writing.",
        |c, p: Touch| c.automation_writer.touch(p.target, p.value, TouchEnd::Release),
    );
    table.add(
        "automation.release",
        "The control of a parameter was released: in touch mode its writing stops and the lane plays again.",
        |c, p: Release| Ok(c.automation_writer.release(&p.target)),
    );
    table.add("automation.releaseAll", "Every control held in the interface was released (the mouse button went up).", |c, ()| {
        Ok(c.automation_writer.release_all())
    });
}
