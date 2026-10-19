//! Project summary (`Summarizer`, contract `ProjectSummary`): "describe the project briefly" for a reader who must not
//! read every note — mainly an AI, whose context `project.get` of a real song would fill. Sections, tracks (instrument,
//! channel, effect path, where they play), buses, shared effects and automation.
//!
//! Everything is a pure function of the project. What counts as an inserted effect is the graph's own path rule
//! (`Graph::insert_path`, the one the mixer view uses), so the summary and the mixer agree. A new field: the contract
//! type in `daw-contracts/src/summary.rs`, a line here, a test in `tests.rs`.

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use daw_contracts::{
    BusSummary, Clip, ClipContent, EffectSummary, Graph, LaneSummary, MASTER, NodeId, NodeKind, Project, ProjectSummary, SectionSummary,
    SharedEffectSummary, Summary, Track, TrackChain, TrackId, TrackSummary,
};

pub struct Summarizer;

impl ProjectSummary for Summarizer {
    fn summarize(&self, project: &Project) -> Summary {
        let map = &project.tempo_map;
        let end_tick = u32::try_from(project.song_end()).unwrap_or(u32::MAX);
        Summary {
            name: project.name.clone(),
            bpm: map.base_tempo(),
            signature: map.base_signature(),
            tempo_changes: map.has_changes(),
            key: project.key,
            end_tick,
            bars: if end_tick == 0 { 0 } else { bar(project, end_tick - 1) },
            loop_region: project.loop_region,
            sections: sections(project, end_tick),
            tracks: project.tracks.iter().map(|track| track_summary(project, track)).collect(),
            buses: buses(project),
            shared_effects: shared_effects(&project.graph),
            master_volume: master_volume(&project.graph),
            automation: project
                .automation
                .iter()
                .map(|lane| LaneSummary {
                    id: lane.id,
                    target: lane.target.clone(),
                    spans: merge(lane.clips.iter().map(|clip| (clip.start_tick, clip.start_tick.saturating_add(clip.length_ticks)))),
                })
                .collect(),
        }
    }
}

/// The bar (from 1) that contains `tick`.
fn bar(project: &Project, tick: u32) -> u32 {
    u32::try_from(project.tempo_map.bar_index(f64::from(tick)) + 1).unwrap_or(1)
}

fn clip_end(clip: &Clip) -> u32 {
    u32::try_from(clip.end_tick()).unwrap_or(u32::MAX)
}

/// `[start, end)` spans sorted, with overlapping and touching ones joined.
fn merge(spans: impl Iterator<Item = (u32, u32)>) -> Vec<(u32, u32)> {
    let mut spans: Vec<_> = spans.collect();
    spans.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

fn sections(project: &Project, end_tick: u32) -> Vec<SectionSummary> {
    let markers = &project.markers;
    markers
        .iter()
        .enumerate()
        .map(|(i, marker)| {
            let to_tick = markers.get(i + 1).map_or(end_tick, |next| next.tick).max(marker.tick);
            let (from, to) = (u64::from(marker.tick), u64::from(to_tick));
            let tracks = project
                .tracks
                .iter()
                .map(|track| track.id)
                .filter(|&id| project.clips.iter().any(|clip| clip.track == id && clip.overlaps(from, to)))
                .collect();
            SectionSummary {
                marker: marker.id,
                name: marker.name.clone(),
                from_tick: marker.tick,
                to_tick,
                bar: bar(project, marker.tick),
                tracks,
            }
        })
        .collect()
}

fn track_summary(project: &Project, track: &Track) -> TrackSummary {
    let clips: Vec<&Clip> = project.clips.iter().filter(|clip| clip.track == track.id).collect();
    let pitches: Vec<u8> = clips
        .iter()
        .filter_map(|clip| match &clip.content {
            ClipContent::Notes(content) => Some(content),
            ClipContent::Audio { .. } => None,
        })
        .flat_map(|content| content.notes.iter().map(|note| note.pitch))
        .collect();
    let source = project.graph.source_of(track.id);
    let path = source.map(|node| project.graph.insert_path(node.id)).unwrap_or_default();
    TrackSummary {
        id: track.id,
        name: track.name.clone(),
        instrument: track.instrument.as_ref().map(|spec| spec.kind.clone()),
        frozen: track.instrument.as_ref().is_some_and(|spec| spec.frozen.is_some()),
        folder: track.folder.and_then(|id| project.folders.iter().find(|folder| folder.id == id)).map(|folder| folder.name.clone()),
        note_effects: track.note_effects.iter().map(|effect| effect.kind.clone()).collect(),
        node: source.map(|node| node.id),
        strip: source.and_then(|node| node.kind.strip()).copied(),
        effects: effects(&project.graph, &path),
        output: path.output,
        clips: clips.len(),
        spans: merge(clips.iter().map(|clip| (clip.start_tick, clip_end(clip)))),
        notes: pitches.len(),
        pitch_range: pitches.iter().min().zip(pitches.iter().max()).map(|(low, high)| (*low, *high)),
        samples: sample_names(project, &clips),
    }
}

/// Names of the sounds the clips play, each once, in order of first use.
fn sample_names(project: &Project, clips: &[&Clip]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for id in clips.iter().filter_map(|clip| clip.content.sample()) {
        if let Some(sample) = project.samples.iter().find(|sample| sample.id == id)
            && !names.contains(&sample.name)
        {
            names.push(sample.name.clone());
        }
    }
    names
}

fn effect(graph: &Graph, id: NodeId) -> Option<EffectSummary> {
    match &graph.node(id)?.kind {
        NodeKind::Effect { effect, bypass, .. } => Some(EffectSummary { id, kind: effect.clone(), bypass: *bypass }),
        _ => None,
    }
}

fn effects(graph: &Graph, path: &TrackChain) -> Vec<EffectSummary> {
    path.effects.iter().filter_map(|&id| effect(graph, id)).collect()
}

fn buses(project: &Project) -> Vec<BusSummary> {
    let graph = &project.graph;
    graph
        .nodes
        .iter()
        .filter_map(|node| {
            let NodeKind::Bus { strip } = &node.kind else { return None };
            let path = graph.insert_path(node.id);
            Some(BusSummary {
                id: node.id,
                name: node.name.clone(),
                strip: *strip,
                tracks: tracks_reaching(project, node.id),
                effects: effects(graph, &path),
                output: path.output,
            })
        })
        .collect()
}

/// Tracks whose sound reaches `id` along regular connections (a sidechain key is not heard), in display order.
fn tracks_reaching(project: &Project, id: NodeId) -> Vec<TrackId> {
    let graph = &project.graph;
    let mut upstream = BTreeSet::new();
    let mut stack = vec![id];
    while let Some(current) = stack.pop() {
        for edge in graph.incoming(current).filter(|edge| !edge.sidechain) {
            if upstream.insert(edge.from) {
                stack.push(edge.from);
            }
        }
    }
    let reaching: BTreeSet<TrackId> = upstream.iter().filter_map(|&node| graph.node(node)?.kind.track()).collect();
    project.tracks.iter().map(|track| track.id).filter(|id| reaching.contains(id)).collect()
}

/// Effects no track or bus path takes as an insert. An effect inserted after another shared one belongs to that one's
/// path and is not listed on its own.
fn shared_effects(graph: &Graph) -> Vec<SharedEffectSummary> {
    let inserted: BTreeSet<NodeId> = graph
        .nodes
        .iter()
        .filter(|node| node.kind.is_source() || matches!(node.kind, NodeKind::Bus { .. }))
        .flat_map(|node| graph.insert_path(node.id).effects)
        .collect();
    let loose: Vec<(NodeId, TrackChain)> = graph
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Effect { .. }) && !inserted.contains(&node.id))
        .map(|node| (node.id, graph.insert_path(node.id)))
        .collect();
    let after_another: BTreeSet<NodeId> = loose.iter().flat_map(|(_, path)| path.effects.iter().copied()).collect();
    loose
        .iter()
        .filter(|(id, _)| !after_another.contains(id))
        .filter_map(|(id, path)| {
            Some(SharedEffectSummary { effect: effect(graph, *id)?, effects: effects(graph, path), output: path.output })
        })
        .collect()
}

fn master_volume(graph: &Graph) -> f32 {
    match graph.node(MASTER).map(|node| &node.kind) {
        Some(NodeKind::Master { volume }) => *volume,
        // Unreachable in a valid project: the master cannot be removed.
        _ => 0.0,
    }
}
