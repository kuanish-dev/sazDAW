//! `presets.*` methods: thin adapters over `Presets`.

use daw_contracts::EditorTarget;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct Kind {
    /// Instrument or effect kind (`synth`, `eq`, `vst3:…`).
    kind: String,
}

#[derive(Deserialize, JsonSchema)]
struct KindName {
    kind: String,
    name: String,
}

#[derive(Deserialize, JsonSchema)]
struct TargetName {
    /// `{ "type": "instrument", "track": id }` or `{ "type": "effect", "node": id }`.
    target: EditorTarget,
    name: String,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add("presets.list", "Lists preset names of an instrument or effect kind, sorted by name.", |c, p: Kind| c.presets.list(&p.kind));
    table.add_flushing(
        "presets.save",
        "Saves the params (and plugin state) of a track's instrument or an effect as a named preset.",
        |c, p: TargetName| c.presets.save(p.target, &p.name),
    );
    table.add(
        "presets.apply",
        "Applies a preset to a track's instrument or an effect of the same kind; one undo step.",
        |c, p: TargetName| c.presets.apply(p.target, &p.name),
    );
    table.add("presets.remove", "Removes a preset of a kind by name.", |c, p: KindName| c.presets.remove(&p.kind, &p.name));
}
