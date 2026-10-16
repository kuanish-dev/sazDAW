//! `transport.*` and `metronome.setEnabled` methods: playback, the song loop and the metronome (tempo and signature are
//! in `tempo.*`).

use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct Seek {
    tick: u64,
}

#[derive(Deserialize, JsonSchema)]
struct Cue {
    /// The start position in ticks; `null` clears it.
    tick: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
struct Enabled {
    enabled: bool,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "transport.play",
        "Starts playback: from the cue if one is set (also after a pause), otherwise from the current position.",
        |c, ()| Ok(c.transport.play()),
    );
    table.add("transport.pause", "Pauses playback (the position stays).", |c, ()| Ok(c.transport.pause()));
    table.add("transport.stop", "Stops playback and returns to the cue, or to the start without one.", |c, ()| Ok(c.transport.stop()));
    table.add("transport.seek", "Moves to a position in ticks.", |c, p: Seek| Ok(c.transport.seek(p.tick)));
    table.add(
        "transport.setCue",
        "Sets the start position (cue) in ticks, or clears it with null: play from pause/stop and stop return to it. \
         Does not move the playhead (use transport.seek). The current cue is in the `tick` telemetry (`cue`).",
        |c, p: Cue| Ok(c.transport.set_cue(p.tick)),
    );
    table.add(
        "transport.setSongLoop",
        "Turns the song loop on or off (on by default): playback that reaches the song end — the end of the last clip, \
         rounded up to a whole bar — jumps back to the start and keeps playing. The loop region (clips.setLoopRegion) \
         takes priority, and while recording playback runs on past the end. The state is `songLoop` in the `tick` \
         telemetry, and `wrap` there is what playback wraps around now.",
        |c, p: Enabled| Ok(c.transport.set_song_loop(p.enabled)),
    );
    table.add(
        "metronome.setEnabled",
        "Turns the metronome on or off. The choice is remembered between launches (off on the first launch); the state is \
         `metronome` in the `tick` telemetry.",
        |c, p: Enabled| Ok(c.metronome.set_enabled(p.enabled)),
    );
}
