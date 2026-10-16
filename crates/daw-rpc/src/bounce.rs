//! `tracks.bounce` / `tracks.freeze` / `tracks.unfreeze` methods: thin adapters over `TrackBounce`.

use daw_contracts::{ExportRequest, TrackId};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;
use crate::params::Id;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Render {
    id: TrackId,
    /// Seconds after the end of the last clip (reverb, release), 0…30; 2 by default.
    #[serde(default)]
    tail_seconds: Option<f32>,
}

impl Render {
    fn tail(&self) -> f32 {
        self.tail_seconds.unwrap_or(ExportRequest::DEFAULT_TAIL_SECONDS)
    }
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add_flushing(
        "tracks.bounce",
        "Bounces a track to audio: its sound (instrument, fader, inserts) goes to a WAV, a new audio track with the clip appears below it, the source is muted; one undo step; returns the new track id.",
        |c, p: Render| c.bounce.bounce(p.id, p.tail()),
    );
    table.add_flushing(
        "tracks.freeze",
        "Freezes an instrument track: bounces it to audio without creating a new track (less CPU load); returns the id of the audio copy.",
        |c, p: Render| c.bounce.freeze(p.id, p.tail()),
    );
    table.add("tracks.unfreeze", "Unfreezes a track: removes its audio copy and brings the instrument back; one undo step.", |c, p: Id| {
        c.bounce.unfreeze(p.id)
    });
}
