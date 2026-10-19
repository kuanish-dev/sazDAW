//! `ProjectGraph`: the sound processing graph — effect nodes, buses and the edges between them, by the graph rules.
//! Sources and the master live and die with their owners (the track, the project); the track fader lives in its source.

use std::sync::Arc;

use daw_contracts::codes::graph;
use daw_contracts::{
    ContractError, Edge, EffectCatalog, ErrorCode, Graph, GraphControl, MAX_CHANNEL_DELAY_MS, MAX_EDGES, MAX_LEVEL, MAX_NODES, MAX_WIDTH,
    Node, NodeId, NodeKind, Params, Polarity, Project, Strip, tr,
};

use crate::ProjectService;
use crate::ops;

/// The catalog parameter that tells an effect to read its sidechain key (compressor, gate): an effect kind without it
/// ignores the key, so `set_sidechain` refuses it.
const SIDECHAIN_PARAM: &str = "sidechain";

pub struct ProjectGraph {
    project: Arc<ProjectService>,
    effects: Arc<dyn EffectCatalog>,
}

impl ProjectGraph {
    pub fn new(project: Arc<ProjectService>, effects: Arc<dyn EffectCatalog>) -> Self {
        Self { project, effects }
    }

    /// The effect kind of node `id`; another node is `graph::NODE_NOT_EFFECT`.
    fn effect_kind(&self, id: NodeId) -> Result<String, ContractError> {
        self.project.read(|p| match &ops::node(p, id)?.kind {
            NodeKind::Effect { effect, .. } => Ok(effect.clone()),
            _ => Err(graph::NODE_NOT_EFFECT.with("id", id)),
        })
    }

    /// Node name and params of a new effect: the catalog defaults with `overrides` on top, each checked and clamped as
    /// in `set_effect_param`.
    fn new_effect(&self, effect: &str, overrides: &Params) -> Result<(String, Params), ContractError> {
        let mut params = self.effects.defaults(effect)?;
        let kinds = self.effects.kinds();
        for (name, value) in overrides {
            let value = ops::finite(*value, graph::PARAM_NOT_A_NUMBER)?;
            let info = ops::param_info(kinds.clone(), effect, name, graph::UNKNOWN_EFFECT_PARAM)?;
            params.insert(name.clone(), info.fit(value));
        }
        let name = kinds.into_iter().find(|k| k.kind == effect).map_or_else(|| effect.to_string(), |k| k.label.to_string());
        Ok((name, params))
    }
}

fn push_effect(project: &mut Project, effect: &str, name: String, params: Params, x: f32, y: f32) -> Result<NodeId, ContractError> {
    ops::limit(project.graph.nodes.len(), MAX_NODES, graph::TOO_MANY_NODES)?;
    let id = project.alloc_id();
    let kind = NodeKind::Effect { effect: effect.to_string(), bypass: false, params, state: None };
    project.graph.nodes.push(Node { id, name, kind, x, y });
    Ok(id)
}

fn same_edge(from: NodeId, to: NodeId) -> impl Fn(&Edge) -> bool {
    move |e| e.from == from && e.to == to && !e.sidechain
}

fn finite_pos(x: f32, y: f32) -> Result<(f32, f32), ContractError> {
    Ok((ops::finite(x, graph::POSITION_NOT_A_NUMBER)?, ops::finite(y, graph::POSITION_NOT_A_NUMBER)?))
}

/// The mixer strip of a node (a source or a bus) for pan, mute, solo and the utility controls; other nodes are `missing`.
fn strip_mut(project: &mut Project, id: NodeId, missing: ErrorCode) -> Result<&mut Strip, ContractError> {
    ops::node_mut(project, id)?.kind.strip_mut().ok_or_else(|| missing.with("id", id))
}

impl GraphControl for ProjectGraph {
    fn graph(&self) -> Graph {
        self.project.read(|p| p.graph.clone())
    }

    fn add_effect(&self, effect: &str, params: &Params, x: f32, y: f32) -> Result<NodeId, ContractError> {
        let (name, params) = self.new_effect(effect, params)?;
        let (x, y) = finite_pos(x, y)?;
        self.project.edit(&tr!("Add effect", "Добавить эффект"), |project| push_effect(project, effect, name, params, x, y))
    }

    fn add_bus(&self, name: &str, x: f32, y: f32) -> Result<NodeId, ContractError> {
        let (x, y) = finite_pos(x, y)?;
        self.project.edit(&tr!("Add bus", "Добавить шину"), |project| {
            ops::add_node_to_master(project, name, NodeKind::Bus { strip: Strip::default() }, x, y)
        })
    }

    fn remove_node(&self, id: NodeId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Delete node", "Удалить узел"), |project| {
            match ops::node(project, id)?.kind {
                NodeKind::Effect { .. } | NodeKind::Bus { .. } => {}
                _ => return Err(graph::NOT_REMOVABLE.with("id", id)),
            }
            ops::drop_node(project, id);
            Ok(())
        })
    }

    fn rename_node(&self, id: NodeId, name: &str) -> Result<(), ContractError> {
        self.project.edit(&tr!("Rename node", "Переименовать узел"), |project| {
            ops::node_mut(project, id)?.name = name.to_string();
            Ok(())
        })
    }

    fn move_node(&self, id: NodeId, x: f32, y: f32) -> Result<(), ContractError> {
        let (x, y) = finite_pos(x, y)?;
        self.project.edit_keyed(&tr!("Move node", "Переместить узел"), Some(format!("move:{id}")), |project| {
            let node = ops::node_mut(project, id)?;
            node.x = x;
            node.y = y;
            Ok(())
        })
    }

    fn connect(&self, from: NodeId, to: NodeId, level: f32, sidechain: bool) -> Result<(), ContractError> {
        let level = ops::finite(level, graph::LEVEL_NOT_A_NUMBER)?.clamp(0.0, MAX_LEVEL);
        let key = format!("edge:{from}:{to}:{sidechain}");
        self.project.edit_keyed(&tr!("Connection", "Связь"), Some(key), |project| {
            project.graph.check_edge(from, to, sidechain)?;
            let same = |e: &Edge| e.from == from && e.to == to && e.sidechain == sidechain;
            if let Some(edge) = project.graph.edges.iter_mut().find(|e| same(e)) {
                edge.level = level;
                return Ok(());
            }
            ops::limit(project.graph.edges.len(), MAX_EDGES, graph::TOO_MANY_EDGES)?;
            project.graph.edges.push(Edge { from, to, level, sidechain, pre_fader: false });
            Ok(())
        })
    }

    fn disconnect(&self, from: NodeId, to: NodeId, sidechain: bool) -> Result<(), ContractError> {
        self.project.edit(&tr!("Remove connection", "Убрать связь"), |project| {
            let before = project.graph.edges.len();
            project.graph.edges.retain(|e| !(e.from == from && e.to == to && e.sidechain == sidechain));
            if project.graph.edges.len() == before {
                return Err(graph::NO_SUCH_EDGE.with("from", from).with("to", to));
            }
            ops::drop_stale_edge_lanes(project);
            Ok(())
        })
    }

    fn set_edge_pre_fader(&self, from: NodeId, to: NodeId, pre_fader: bool) -> Result<(), ContractError> {
        self.project.edit(&tr!("Pre/post-fader send", "Посыл до/после фейдера"), |project| {
            ops::node(project, to)?;
            project.graph.check_pre_fader(from)?;
            let edge = project
                .graph
                .edges
                .iter_mut()
                .find(|e| same_edge(from, to)(e))
                .ok_or_else(|| graph::NO_SUCH_EDGE.with("from", from).with("to", to))?;
            edge.pre_fader = pre_fader;
            Ok(())
        })
    }

    fn set_sidechain(&self, effect: NodeId, source: Option<NodeId>) -> Result<(), ContractError> {
        let kind = self.effect_kind(effect)?;
        let info = ops::param_info(self.effects.kinds(), &kind, SIDECHAIN_PARAM, graph::NO_SIDECHAIN_INPUT)
            .map_err(|_| graph::NO_SIDECHAIN_INPUT.with("kind", &kind))?;
        self.project.edit(&tr!("Sidechain source", "Источник сайдчейна"), |project| {
            project.graph.edges.retain(|e| !(e.to == effect && e.sidechain));
            if let Some(source) = source {
                project.graph.check_edge(source, effect, true)?;
                ops::limit(project.graph.edges.len(), MAX_EDGES, graph::TOO_MANY_EDGES)?;
                project.graph.edges.push(Edge { from: source, to: effect, level: 1.0, sidechain: true, pre_fader: false });
            }
            ops::drop_stale_edge_lanes(project);
            let on = info.fit(if source.is_some() { 1.0 } else { 0.0 });
            ops::effect_mut(project, effect)?.1.insert(SIDECHAIN_PARAM.to_string(), on);
            Ok(())
        })
    }

    fn set_volume(&self, id: NodeId, volume: f32) -> Result<(), ContractError> {
        let value = ops::finite(volume, graph::VOLUME_NOT_A_NUMBER)?.clamp(0.0, MAX_LEVEL);
        self.project.edit_keyed(&tr!("Volume", "Громкость"), Some(format!("volume:{id}")), |project| {
            *ops::node_mut(project, id)?.kind.volume_mut().ok_or_else(|| graph::NO_VOLUME.with("id", id))? = value;
            Ok(())
        })
    }

    fn set_pan(&self, id: NodeId, pan: f32) -> Result<(), ContractError> {
        let value = ops::finite(pan, graph::PAN_NOT_A_NUMBER)?.clamp(-1.0, 1.0);
        self.project.edit_keyed(&tr!("Pan", "Панорама"), Some(format!("pan:{id}")), |project| {
            strip_mut(project, id, graph::NO_PAN)?.pan = value;
            Ok(())
        })
    }

    fn set_mute(&self, id: NodeId, mute: bool) -> Result<(), ContractError> {
        self.project.edit("Mute", |project| {
            strip_mut(project, id, graph::NO_MUTE)?.mute = mute;
            Ok(())
        })
    }

    fn set_solo(&self, id: NodeId, solo: bool) -> Result<(), ContractError> {
        self.project.edit("Solo", |project| {
            strip_mut(project, id, graph::NO_SOLO)?.solo = solo;
            Ok(())
        })
    }

    fn set_polarity(&self, id: NodeId, polarity: Polarity) -> Result<(), ContractError> {
        self.project.edit(&tr!("Polarity", "Полярность"), |project| {
            strip_mut(project, id, graph::NO_UTILITY)?.polarity = polarity;
            Ok(())
        })
    }

    fn set_width(&self, id: NodeId, width: f32) -> Result<(), ContractError> {
        let value = ops::finite(width, graph::WIDTH_NOT_A_NUMBER)?.clamp(0.0, MAX_WIDTH);
        self.project.edit_keyed(&tr!("Width", "Ширина"), Some(format!("width:{id}")), |project| {
            strip_mut(project, id, graph::NO_UTILITY)?.width = value;
            Ok(())
        })
    }

    fn set_mono(&self, id: NodeId, mono: bool) -> Result<(), ContractError> {
        self.project.edit(&tr!("Mono", "Моно"), |project| {
            strip_mut(project, id, graph::NO_UTILITY)?.mono = mono;
            Ok(())
        })
    }

    fn set_delay(&self, id: NodeId, ms: f32) -> Result<(), ContractError> {
        let value = ops::finite(ms, graph::DELAY_NOT_A_NUMBER)?.clamp(-MAX_CHANNEL_DELAY_MS, MAX_CHANNEL_DELAY_MS);
        self.project.edit_keyed(&tr!("Channel delay", "Задержка канала"), Some(format!("delay:{id}")), |project| {
            strip_mut(project, id, graph::NO_UTILITY)?.delay_ms = value;
            Ok(())
        })
    }

    fn set_effect_param(&self, id: NodeId, name: &str, value: f32) -> Result<(), ContractError> {
        let value = ops::finite(value, graph::PARAM_NOT_A_NUMBER)?;
        let kind = self.effect_kind(id)?;
        let info = ops::param_info(self.effects.kinds(), &kind, name, graph::UNKNOWN_EFFECT_PARAM)?;
        let value = info.fit(value);
        self.project.edit_keyed(&tr!("Effect parameter", "Параметр эффекта"), Some(format!("fx:{id}:{name}")), |project| {
            ops::effect_mut(project, id)?.1.insert(name.to_string(), value);
            Ok(())
        })
    }

    fn set_effect_bypass(&self, id: NodeId, bypass: bool) -> Result<(), ContractError> {
        self.project.edit("Bypass", |project| {
            *ops::effect_mut(project, id)?.0 = bypass;
            Ok(())
        })
    }

    fn insert_effect(&self, after: NodeId, effect: &str, params: &Params, x: f32, y: f32) -> Result<NodeId, ContractError> {
        let (name, params) = self.new_effect(effect, params)?;
        let (x, y) = finite_pos(x, y)?;
        self.project.edit(&tr!("Insert effect", "Вставить эффект"), |project| {
            ops::node(project, after)?;
            ops::limit(project.graph.edges.len(), MAX_EDGES, graph::TOO_MANY_EDGES)?;
            // Post-fader outputs of the node move to the effect with the same levels; sidechains and pre-fader sends
            // are branches, not the track path: they stay with the node.
            let path = |e: &Edge| !e.sidechain && !e.pre_fader;
            let moved: Vec<Edge> = project.graph.outgoing(after).filter(|e| path(e)).copied().collect();
            project.graph.edges.retain(|e| e.from != after || !path(e));
            let id = push_effect(project, effect, name, params, x, y)?;
            project.graph.check_edge(after, id, false)?;
            project.graph.edges.push(Edge { from: after, to: id, level: 1.0, sidechain: false, pre_fader: false });
            for edge in moved {
                project.graph.check_edge(id, edge.to, false)?;
                project.graph.edges.push(Edge { from: id, ..edge });
                ops::move_edge_lane(project, &edge, id, edge.to);
            }
            Ok(id)
        })
    }

    fn insert_effect_before(&self, node: NodeId, effect: &str, params: &Params, x: f32, y: f32) -> Result<NodeId, ContractError> {
        let (name, params) = self.new_effect(effect, params)?;
        let (x, y) = finite_pos(x, y)?;
        self.project.edit(&tr!("Insert effect", "Вставить эффект"), |project| {
            if !ops::node(project, node)?.kind.accepts_input() {
                return Err(graph::SOURCE_HAS_NO_INPUT.err());
            }
            ops::limit(project.graph.edges.len(), MAX_EDGES, graph::TOO_MANY_EDGES)?;
            // Regular inputs of the node move to the effect with the same levels; the sidechain stays with the node.
            let moved: Vec<Edge> = project.graph.incoming(node).filter(|e| !e.sidechain).copied().collect();
            project.graph.edges.retain(|e| e.to != node || e.sidechain);
            let id = push_effect(project, effect, name, params, x, y)?;
            for edge in moved {
                project.graph.check_edge(edge.from, id, false)?;
                project.graph.edges.push(Edge { to: id, ..edge });
                ops::move_edge_lane(project, &edge, edge.from, id);
            }
            project.graph.check_edge(id, node, false)?;
            project.graph.edges.push(Edge { from: id, to: node, level: 1.0, sidechain: false, pre_fader: false });
            Ok(id)
        })
    }

    fn remove_effect_keep_path(&self, id: NodeId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Remove effect", "Убрать эффект"), |project| {
            if !matches!(ops::node(project, id)?.kind, NodeKind::Effect { .. }) {
                return Err(graph::NODE_NOT_EFFECT.with("id", id));
            }
            let inputs: Vec<Edge> = project.graph.incoming(id).filter(|e| !e.sidechain).copied().collect();
            let outputs: Vec<Edge> = project.graph.outgoing(id).filter(|e| !e.sidechain).copied().collect();
            ops::drop_node(project, id);
            for input in &inputs {
                for output in &outputs {
                    project.graph.check_edge(input.from, output.to, false)?;
                    let level = (input.level * output.level).clamp(0.0, MAX_LEVEL);
                    if let Some(edge) = project.graph.edges.iter_mut().find(|e| same_edge(input.from, output.to)(e)) {
                        // A parallel path already existed: the sounds add up.
                        edge.level = (edge.level + level).min(MAX_LEVEL);
                    } else {
                        ops::limit(project.graph.edges.len(), MAX_EDGES, graph::TOO_MANY_EDGES)?;
                        project.graph.edges.push(Edge {
                            from: input.from,
                            to: output.to,
                            level,
                            sidechain: false,
                            pre_fader: input.pre_fader,
                        });
                    }
                }
            }
            Ok(())
        })
    }

    fn set_output(&self, node: NodeId, target: NodeId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Output", "Выход"), |project| {
            ops::node(project, node)?;
            // Pre-fader sends stay (except a send to the new output itself: a pair has one edge).
            project.graph.edges.retain(|e| e.from != node || e.sidechain || (e.pre_fader && e.to != target));
            project.graph.check_edge(node, target, false)?;
            project.graph.edges.push(Edge { from: node, to: target, level: 1.0, sidechain: false, pre_fader: false });
            ops::drop_stale_edge_lanes(project);
            Ok(())
        })
    }
}
