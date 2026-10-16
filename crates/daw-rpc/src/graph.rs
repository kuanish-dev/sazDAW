//! `graph.*` methods: thin adapters over `GraphControl`. The graph itself comes inside `project.get`.

use daw_contracts::{NodeId, Params, Polarity};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;
use crate::params::{Id, IdName};

#[derive(Deserialize, JsonSchema)]
struct AddEffect {
    effect: String,
    /// Parameter values on top of the kind's catalog defaults (`catalog.get` → effects → params), each clamped to its
    /// range; an unknown name is an error and creates nothing.
    #[serde(default)]
    params: Params,
    x: f32,
    y: f32,
}

#[derive(Deserialize, JsonSchema)]
struct AddBus {
    name: String,
    x: f32,
    y: f32,
}

#[derive(Deserialize, JsonSchema)]
struct Move {
    id: NodeId,
    x: f32,
    y: f32,
}

#[derive(Deserialize, JsonSchema)]
struct Connect {
    from: NodeId,
    to: NodeId,
    level: f32,
    sidechain: bool,
}

#[derive(Deserialize, JsonSchema)]
struct Disconnect {
    from: NodeId,
    to: NodeId,
    sidechain: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PreFader {
    from: NodeId,
    to: NodeId,
    pre_fader: bool,
}

#[derive(Deserialize, JsonSchema)]
struct Sidechain {
    effect: NodeId,
    /// The key source node; `null` removes the sidechain.
    source: Option<NodeId>,
}

#[derive(Deserialize, JsonSchema)]
struct Volume {
    id: NodeId,
    volume: f32,
}

#[derive(Deserialize, JsonSchema)]
struct Pan {
    id: NodeId,
    pan: f32,
}

#[derive(Deserialize, JsonSchema)]
struct Mute {
    id: NodeId,
    mute: bool,
}

#[derive(Deserialize, JsonSchema)]
struct Solo {
    id: NodeId,
    solo: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SetPolarity {
    id: NodeId,
    invert_left: bool,
    invert_right: bool,
}

#[derive(Deserialize, JsonSchema)]
struct Width {
    id: NodeId,
    width: f32,
}

#[derive(Deserialize, JsonSchema)]
struct Mono {
    id: NodeId,
    mono: bool,
}

#[derive(Deserialize, JsonSchema)]
struct Delay {
    id: NodeId,
    ms: f32,
}

#[derive(Deserialize, JsonSchema)]
struct EffectParam {
    id: NodeId,
    name: String,
    value: f32,
}

#[derive(Deserialize, JsonSchema)]
struct InsertEffect {
    after: NodeId,
    effect: String,
    /// Parameter values on top of the kind's catalog defaults (`catalog.get` → effects → params), each clamped to its
    /// range; an unknown name is an error and creates nothing.
    #[serde(default)]
    params: Params,
    x: f32,
    y: f32,
}

#[derive(Deserialize, JsonSchema)]
struct InsertEffectBefore {
    node: NodeId,
    effect: String,
    /// Parameter values on top of the kind's catalog defaults (`catalog.get` → effects → params), each clamped to its
    /// range; an unknown name is an error and creates nothing.
    #[serde(default)]
    params: Params,
    x: f32,
    y: f32,
}

#[derive(Deserialize, JsonSchema)]
struct SetOutput {
    node: NodeId,
    target: NodeId,
}

#[derive(Deserialize, JsonSchema)]
struct Bypass {
    id: NodeId,
    bypass: bool,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "graph.addEffect",
        "Creates an effect of a catalog kind (without connections), with `params` set in the same undo step.",
        |c, p: AddEffect| c.graph.add_effect(&p.effect, &p.params, p.x, p.y),
    );
    table.add("graph.addBus", "Creates a bus already connected to the master.", |c, p: AddBus| c.graph.add_bus(&p.name, p.x, p.y));
    table.add("graph.removeNode", "Removes an effect or a bus together with its connections.", |c, p: Id| c.graph.remove_node(p.id));
    table.add("graph.renameNode", "Renames a node.", |c, p: IdName| c.graph.rename_node(p.id, &p.name));
    table.add("graph.moveNode", "Moves a node on the diagram (x, y).", |c, p: Move| c.graph.move_node(p.id, p.x, p.y));
    table.add(
        "graph.connect",
        "Connects two nodes: a source (track) only sends sound, the master only receives, effects and buses connect in any order but without loops; sidechain only into an effect.",
        |c, p: Connect| c.graph.connect(p.from, p.to, p.level, p.sidechain),
    );
    table.add("graph.disconnect", "Removes a connection (and its automation).", |c, p: Disconnect| {
        c.graph.disconnect(p.from, p.to, p.sidechain)
    });
    table.add(
        "graph.setEdgePreFader",
        "Makes a send pre-fader (preFader = true: the sound is taken before the node's volume and pan, its mute silences it) or post-fader. Only for a normal connection from a track or bus.",
        |c, p: PreFader| c.graph.set_edge_pre_fader(p.from, p.to, p.pre_fader),
    );
    table.add(
        "graph.setSidechain",
        "Picks the sidechain (key) source of a compressor or gate in one step: removes its previous sidechain inputs, connects `source` as the key at level 1 and switches the effect's `sidechain` parameter on; `source: null` removes the key and switches it off. For ducking a bass under the kick: effect = the bass compressor, source = the kick track's node.",
        |c, p: Sidechain| c.graph.set_sidechain(p.effect, p.source),
    );
    table.add("graph.setVolume", "Sets a node's volume (source, bus, master).", |c, p: Volume| c.graph.set_volume(p.id, p.volume));
    table.add("graph.setPan", "Sets a node's pan from -1 to 1 (source, bus).", |c, p: Pan| c.graph.set_pan(p.id, p.pan));
    table.add("graph.setMute", "Mutes or unmutes a node (source, bus).", |c, p: Mute| c.graph.set_mute(p.id, p.mute));
    table.add("graph.setSolo", "Solos or unsolos a node (source, bus).", |c, p: Solo| c.graph.set_solo(p.id, p.solo));
    table.add(
        "graph.setPolarity",
        "Inverts the polarity (\"phase\") of a node's left and/or right side (source, bus): for a snare bottom mic against the top one, or a mic facing a speaker from behind.",
        |c, p: SetPolarity| c.graph.set_polarity(p.id, Polarity { invert_left: p.invert_left, invert_right: p.invert_right }),
    );
    table.add(
        "graph.setWidth",
        "Sets a node's stereo width from 0 (mono) through 1 (as recorded) to 2 (twice the side) (source, bus); automatable.",
        |c, p: Width| c.graph.set_width(p.id, p.width),
    );
    table.add("graph.setMono", "Sums a node to mono or back (source, bus); the width is kept for when mono is off.", |c, p: Mono| {
        c.graph.set_mono(p.id, p.mono)
    });
    table.add(
        "graph.setDelay",
        "Fine time alignment of a node in milliseconds, -10 to 10 (source, bus): positive plays it later, negative earlier (the rest of the mix waits through delay compensation). Lines up close mics with overheads or a DI with its amp mic; one sample is 1000 / sample rate ms.",
        |c, p: Delay| c.graph.set_delay(p.id, p.ms),
    );
    table.add("graph.setEffectParam", "Sets an effect parameter.", |c, p: EffectParam| c.graph.set_effect_param(p.id, &p.name, p.value));
    table.add("graph.setEffectBypass", "Bypasses an effect (sound goes around it).", |c, p: Bypass| {
        c.graph.set_effect_bypass(p.id, p.bypass)
    });
    table.add(
        "graph.insertEffect",
        "Inserts an effect right after a node (source, effect or bus): the node's outputs move to the effect, and the node connects to the effect (the track's and bus's \"+ Effect\" button); `params` are set in the same undo step. Returns the effect id.",
        |c, p: InsertEffect| c.graph.insert_effect(p.after, &p.effect, &p.params, p.x, p.y),
    );
    table.add(
        "graph.insertEffectBefore",
        "Inserts an effect right before a node (master, bus or effect): all normal inputs of the node move to the effect, and the effect connects to the node; sidechain inputs stay. This is how a limiter goes on the master (node = 0) or a compressor on a bus. Not allowed before a source; `params` are set in the same undo step. Returns the effect id.",
        |c, p: InsertEffectBefore| c.graph.insert_effect_before(p.node, &p.effect, &p.params, p.x, p.y),
    );
    table.add("graph.removeEffectKeepPath", "Removes an effect, connecting its input directly to its output.", |c, p: Id| {
        c.graph.remove_effect_keep_path(p.id)
    });
    table.add("graph.setOutput", "Routes a node's output into one chosen node (replacing previous connections).", |c, p: SetOutput| {
        c.graph.set_output(p.node, p.target)
    });
}
