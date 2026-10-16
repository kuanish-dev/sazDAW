//! Sound graph commands: nodes, volume and pan, buses, effects, connections, meters.

use daw_contracts::{Controls, Node, NodeKind};

use super::{Words, number, report};

/// Where nodes created from the console appear on the graph diagram: buses on the right, effects in the middle.
const BUS_POSITION: (f32, f32) = (420.0, 40.0);
const EFFECT_POSITION: (f32, f32) = (220.0, 40.0);

/// Graph nodes and channels are addressed by their numbers in the lists.
fn node_id(controls: &Controls, index: usize) -> Option<u32> {
    let id = controls.graph.graph().nodes.get(index).map(|n| n.id);
    if id.is_none() {
        eprintln!("no graph node #{index} (list: graph)");
    }
    id
}

fn describe(node: &Node) -> String {
    let role = match &node.kind {
        NodeKind::Instrument { track, .. } => format!("track {track}, instrument"),
        NodeKind::Audio { track, .. } => format!("track {track}, audio"),
        NodeKind::Effect { effect, bypass, .. } => return format!("effect {effect}{}", if *bypass { " (bypass)" } else { "" }),
        NodeKind::Bus { .. } => "bus".to_string(),
        NodeKind::Master { volume } => return format!("master vol {volume:.2}"),
    };
    let Some(strip) = node.kind.strip() else { return role };
    let flags = format!("{}{}", if strip.mute { " M" } else { "" }, if strip.solo { " S" } else { "" });
    format!("{role} vol {:.2} pan {:+.2}{flags}", strip.volume, strip.pan)
}

pub(super) fn run(controls: &Controls, command: &str, words: &Words) -> bool {
    match command {
        "graph" => {
            let graph = controls.graph.graph();
            for (index, node) in graph.nodes.iter().enumerate() {
                println!("{index} {:<10} {}", node.name, describe(node));
            }
            let index_of = |id| graph.nodes.iter().position(|n| n.id == id).unwrap_or(usize::MAX);
            for edge in &graph.edges {
                println!(
                    "  {} → {}  level {:.2}{}",
                    index_of(edge.from),
                    index_of(edge.to),
                    edge.level,
                    if edge.sidechain { "  sidechain" } else { "" }
                );
            }
        }
        "vol" | "pan" => {
            if let (Some(id), Some(value)) =
                (number(words.arg(1), "node").and_then(|i| node_id(controls, i)), number::<f32>(words.arg(2), "value"))
            {
                report(if command == "vol" { controls.graph.set_volume(id, value) } else { controls.graph.set_pan(id, value) });
            }
        }
        "mute" | "solo" => {
            if let Some(id) = number(words.arg(1), "node").and_then(|i| node_id(controls, i)) {
                let Some(strip) = controls.graph.graph().node(id).and_then(|n| n.kind.strip().copied()) else {
                    eprintln!("only tracks and buses have mute and solo");
                    return true;
                };
                report(if command == "mute" { controls.graph.set_mute(id, !strip.mute) } else { controls.graph.set_solo(id, !strip.solo) });
            }
        }
        "bus" => report(controls.graph.add_bus(&words.rest(1), BUS_POSITION.0, BUS_POSITION.1)),
        "fxkinds" => {
            for kind in controls.effects.kinds() {
                println!("{:<11} {}", kind.kind, kind.label);
            }
        }
        "fx" => {
            if let Some(kind) = words.arg(1) {
                report(controls.graph.add_effect(kind, EFFECT_POSITION.0, EFFECT_POSITION.1));
            }
        }
        "rmnode" => {
            if let Some(id) = number(words.arg(1), "node").and_then(|i| node_id(controls, i)) {
                report(controls.graph.remove_node(id));
            }
        }
        "link" | "unlink" => {
            if let (Some(from), Some(to)) = (
                number(words.arg(1), "from").and_then(|i| node_id(controls, i)),
                number(words.arg(2), "to").and_then(|i| node_id(controls, i)),
            ) {
                let sidechain = words.0.last() == Some(&"side");
                let level = words.arg(3).and_then(|w| w.parse::<f32>().ok()).unwrap_or(1.0);
                report(if command == "link" {
                    controls.graph.connect(from, to, level, sidechain)
                } else {
                    controls.graph.disconnect(from, to, sidechain)
                });
            }
        }
        "meters" => {
            let graph = controls.graph.graph();
            for meter in controls.stats.meters() {
                let name = graph.node(meter.node).map_or("?", |n| n.name.as_str());
                println!("{name:<10} L {:.3} R {:.3}", meter.peak_left, meter.peak_right);
            }
        }
        "cpu" => println!("load: {:.1} %", controls.stats.cpu_load() * 100.0),
        _ => return false,
    }
    true
}
