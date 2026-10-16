//! `stage.*` methods: thin adapters over `StageControl` (the virtual stage). Who stands where comes inside `project.get`:
//! a source's or bus's `pan` and `stage { depth, height }` (no `stage`: off the stage), the room as `room`.

use daw_contracts::{NodeId, Room, StagePlace};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct Place {
    /// A track's source node or a bus (graph node id).
    node: NodeId,
    /// Left/right: −1 the left wall … 1 the right wall. It is the channel's pan.
    pan: f32,
    /// −1 the wall behind the listener … 0 level with the listener … 1 the wall ahead.
    depth: f32,
    /// −1 the floor … 0 the listener's ear height … 1 the ceiling.
    height: f32,
}

#[derive(Deserialize, JsonSchema)]
struct Channel {
    /// A track's source node or a bus (graph node id).
    node: NodeId,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "stage.place",
        "Puts a channel (a track's source node or a bus) on the virtual stage or moves it there. The listener stands in the \
         middle of the room facing ahead, and 0 is their head on every axis: pan −1…1 is left/right from wall to wall, depth \
         −1…1 from the wall behind the listener to the wall ahead, height −1…1 from the floor to the ceiling (0 = ear height). \
         The engine then plays it from that place in the shared room: farther is quieter, duller and more reverberant; on \
         headphones (stage.room listening) a place behind or above the listener sounds behind or above. Up to 64 channels. \
         Consecutive moves of one channel are one undo step.",
        |c, p: Place| c.stage.place(p.node, p.pan, StagePlace { depth: p.depth, height: p.height }),
    );
    table.add(
        "stage.takeOff",
        "Takes a channel off the stage: it plays with plain pan again (at the pan it stood at); its stage depth and height \
         automation lanes are removed.",
        |c, p: Channel| c.stage.take_off(p.node),
    );
    table.add(
        "stage.room",
        "The stage's shared room: size 0…1 (a small studio room … a concert hall), reverb 0…1 (open air … hard, live walls), \
         listening \"speakers\" (equal-power panning: left/right, near/far and height, but a place behind sounds like its \
         mirror ahead) or \"headphones\" (binaural cues all around the head, behind and above included). The same is in \
         project.get → room.",
        |c, ()| Ok(c.stage.room()),
    );
    table.add(
        "stage.setRoom",
        "Changes the stage's shared room: { size, reverb, listening } as in stage.room (values are clamped to 0…1). Consecutive \
         changes are one undo step.",
        |c, p: Room| c.stage.set_room(p),
    );
}
