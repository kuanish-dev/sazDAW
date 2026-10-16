//! Recording sound from the input onto audio tracks: arming and track inputs, start, stop (returns the created
//! clips), status.

use daw_contracts::{InputChannels, TrackId};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct SetArmed {
    track: TrackId,
    armed: bool,
}

#[derive(Deserialize, JsonSchema)]
struct SetInput {
    track: TrackId,
    /// `stereo` is channels 1–2, `left` channel 1, `right` channel 2 (with a mono input any choice records its only channel).
    input: InputChannels,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "audioRecord.setArmed",
        "Arms or disarms audio recording on an audio track (an armed track opens its input: levels, auto monitoring).",
        |c, p: SetArmed| c.audio_record.set_armed(p.track, p.armed),
    );
    table.add(
        "audioRecord.setInput",
        "Sets which input channels an audio track records and monitors: stereo, left or right.",
        |c, p: SetInput| c.audio_record.set_input(p.track, p.input),
    );
    table.add(
        "audioRecord.start",
        "Starts recording from the input onto all armed audio tracks at the current position (a stopped transport starts).",
        |c, ()| c.audio_record.start(),
    );
    table.add(
        "audioRecord.stop",
        "Stops recording: each track gets a new clip, or a new take of a clip the recording covered at least halfway (or lies \
         entirely inside); returns the clips. With a loop region, every loop pass becomes a take of one clip on the loop \
         region (the last full pass plays).",
        |c, ()| c.audio_record.stop(),
    );
    table.add(
        "audioRecord.status",
        "Returns the audio recording state: whether a take is running, time, levels, arming and input of each track, monitoring problems.",
        |c, ()| Ok(c.audio_record.status()),
    );
}
