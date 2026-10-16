//! `record.*` methods: recording notes played live into a clip of the chosen track.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct Armed {
    armed: bool,
}

#[derive(Deserialize, JsonSchema)]
struct Quantize {
    ticks: u32,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "record.setArmed",
        "Arms or disarms recording notes from the live keyboard into a clip of the chosen track.",
        |c, p: Armed| Ok(c.record.set_armed(p.armed)),
    );
    table.add(
        "record.setQuantize",
        "Sets the grid in ticks that recorded note starts snap to (0 means no snapping, at most one bar).",
        |c, p: Quantize| c.record.set_quantize(p.ticks),
    );
}
