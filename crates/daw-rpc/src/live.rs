//! `live.*` methods: notes and controllers played live on top of clips (`LiveNoteSink`).

use daw_contracts::{ControllerKind, TrackId};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct NoteOn {
    track: TrackId,
    pitch: u8,
    velocity: f32,
}

#[derive(Deserialize, JsonSchema)]
struct NoteOff {
    track: TrackId,
    pitch: u8,
}

#[derive(Deserialize, JsonSchema)]
struct Controller {
    track: TrackId,
    kind: ControllerKind,
    value: f32,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add("live.noteOn", "Plays a note live on a track (on top of clips); velocity 0…1.", |c, p: NoteOn| {
        Ok(c.live_notes.note_on(p.track, p.pitch, p.velocity))
    });
    table.add("live.noteOff", "Releases a live note.", |c, p: NoteOff| Ok(c.live_notes.note_off(p.track, p.pitch)));
    table.add(
        "live.controller",
        "Sends a live controller to a track: kind is { type: \"cc\", number: 0–119 }, { type: \"pitchBend\" } or \
         { type: \"aftertouch\" }; value 0–1 (pitch bend −1…1).",
        |c, p: Controller| Ok(c.live_notes.controller(p.track, p.kind, p.value)),
    );
}
