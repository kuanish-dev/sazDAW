//! One track's sound on its own (`TrackRender`, for "Bounce to audio" and "Freeze"): a graph in which only the track's
//! path to its output sounds (`isolate`), and rendering its span into memory.

use daw_contracts::codes::{export, tracks};
use daw_contracts::{
    ContractError, Edge, Graph, MASTER, Node, NodeId, NodeKind, ParamTarget, Project, SampleData, StreamConfig, TrackId, TrackRender,
    TrackSound, check_tail,
};

use crate::{CHUNK_FRAMES, OfflineExporter};

/// Sample rate of the bounced audio: the engine resamples it to the output rate itself.
const BOUNCE_SAMPLE_RATE: u32 = 48_000;
/// The longest bounce: 20 minutes of stereo, about 460 MB in memory.
const MAX_BOUNCE_SECONDS: u64 = 20 * 60;

/// A project in which only `track` sounds: its source (volume and pan stay, mute and solo are cleared), the inserts of
/// its chain and the master at volume 1. Other tracks, their clips, buses, sends and automation of other nodes are
/// removed; automation of the track itself, its inserts and the edges between them stays.
pub(crate) fn isolate(project: &Project, track: TrackId) -> Result<Project, ContractError> {
    let not_found = tracks::NOT_FOUND.with("id", track);
    let owner = project.track(track).ok_or(not_found.clone())?.clone();
    let source = project.graph.source_of(track).ok_or(not_found)?;
    let path: Vec<NodeId> = std::iter::once(source.id).chain(project.graph.insert_path(source.id).effects).collect();

    let mut nodes = vec![Node { id: MASTER, name: "Master".into(), kind: NodeKind::Master { volume: 1.0 }, x: 0.0, y: 0.0 }];
    nodes.extend(project.graph.nodes.iter().filter(|n| path.contains(&n.id)).cloned().map(|mut node| {
        if let Some(strip) = node.kind.strip_mut() {
            strip.mute = false;
            strip.solo = false;
        }
        node
    }));
    // Path edges keep their levels (they are part of the track's sound); the last insert goes to the master at level 1:
    // the output level is repeated by the audio copy's edge.
    let mut edges: Vec<Edge> = path
        .windows(2)
        .map(|pair| {
            let original = project.graph.edges.iter().find(|e| e.from == pair[0] && e.to == pair[1] && !e.sidechain && !e.pre_fader);
            Edge { from: pair[0], to: pair[1], level: original.map_or(1.0, |e| e.level), sidechain: false, pre_fader: false }
        })
        .collect();
    let last = *path.last().unwrap_or(&source.id);
    edges.push(Edge { from: last, to: MASTER, level: 1.0, sidechain: false, pre_fader: false });

    let keeps_lane = |target: &ParamTarget| match target {
        ParamTarget::InstrumentParam { track: lane_track, .. } => *lane_track == track,
        ParamTarget::NodeVolume { node }
        | ParamTarget::NodePan { node }
        | ParamTarget::NodeWidth { node }
        | ParamTarget::StageDepth { node }
        | ParamTarget::StageHeight { node }
        | ParamTarget::EffectParam { node, .. } => path.contains(node) && *node != MASTER,
        ParamTarget::EdgeLevel { from, to, sidechain } => {
            !sidechain && edges.iter().any(|e| e.from == *from && e.to == *to && *to != MASTER)
        }
    };
    let mut reduced = project.clone();
    reduced.automation.retain(|lane| keeps_lane(&lane.target));
    reduced.graph = Graph { nodes, edges };
    reduced.tracks = vec![owner];
    reduced.clips.retain(|clip| clip.track == track);
    reduced.loop_region = None;
    Ok(reduced)
}

/// Ticks from the start of the track's first clip to the end of its last one; no clips is `export::TRACK_HAS_NO_CLIPS`.
fn used_range(project: &Project, track: TrackId) -> Result<(u32, u64), ContractError> {
    let clips = project.clips.iter().filter(|clip| clip.track == track);
    let start = clips.clone().map(|clip| clip.start_tick).min();
    let end = clips.map(daw_contracts::Clip::end_tick).max();
    match (start, end) {
        (Some(start), Some(end)) => Ok((start, end)),
        _ => Err(export::TRACK_HAS_NO_CLIPS.with("id", track)),
    }
}

impl TrackRender for OfflineExporter {
    fn render_track(&self, project: &Project, track: TrackId, tail_seconds: f32) -> Result<TrackSound, ContractError> {
        let tail = check_tail(tail_seconds)?;
        let reduced = isolate(project, track)?;
        let (start, end) = used_range(&reduced, track)?;
        let rate = BOUNCE_SAMPLE_RATE;
        let body = reduced.tempo_map.frames_between(f64::from(start), end as f64, rate).ceil() as u64;
        let total = body + (tail * f64::from(rate)).round() as u64;
        if total > MAX_BOUNCE_SECONDS * u64::from(rate) {
            return Err(export::BOUNCE_TOO_LONG.with("seconds", total / u64::from(rate)).with("max", MAX_BOUNCE_SECONDS));
        }

        let mut engine = self.load_engine(&reduced, rate)?;
        engine.transport.seek(u64::from(start));
        engine.transport.play();
        let channels = usize::from(StreamConfig::DEFAULT.channels);
        let mut frames = vec![0.0f32; total as usize * channels];
        for chunk in frames.chunks_mut(CHUNK_FRAMES * channels) {
            engine.renderer.render(chunk);
            // What the engine released (finished voices) is freed here, as during export.
            engine.dsp.collect_garbage();
        }
        Ok(TrackSound { start_tick: start, data: SampleData { sample_rate: rate, channels: StreamConfig::DEFAULT.channels, frames } })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::codes::{export, tracks};
    use daw_contracts::{
        AutoPoint, AutomationClip, AutomationLane, Clip, ClipContent, InstrumentSpec, MAX_CLIP_LOOP_TICKS, MAX_TAIL_SECONDS, NoteContent,
        Strip, Track,
    };

    fn node(id: NodeId, kind: NodeKind) -> Node {
        Node { id, name: format!("n{id}"), kind, x: 0.0, y: 0.0 }
    }

    fn effect(id: NodeId) -> Node {
        node(id, NodeKind::Effect { effect: "eq".into(), bypass: false, params: Default::default(), state: None })
    }

    fn edge(from: NodeId, to: NodeId, level: f32) -> Edge {
        Edge { from, to, level, sidechain: false, pre_fader: false }
    }

    fn lane(id: u32, target: ParamTarget) -> AutomationLane {
        AutomationLane { id, target, clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 0.5)])] }
    }

    fn clip(id: u32, track: TrackId, start: u32, length: u32) -> Clip {
        Clip {
            id,
            track,
            start_tick: start,
            length_ticks: length,
            content: ClipContent::Notes(NoteContent::empty()),
            gain: 1.0,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            fade_in_shape: Default::default(),
            fade_out_shape: Default::default(),
            offset_ticks: 0,
            offset_frames: 0,
            warp: None,
            pitch_semitones: 0.0,
        }
    }

    /// Track 1 (node 10) → effect 20 → effect 21 → bus 30 → master; send 10 → bus 31; sidechain 11 → 21;
    /// track 2 (node 11) → master.
    fn project() -> Project {
        let quiet = Strip { volume: 0.5, pan: -0.3, mute: true, solo: true, ..Strip::default() };
        let mut project = Project::empty();
        project.tracks = vec![
            Track::new(1, "Bass", Some(InstrumentSpec::new("synth", Default::default())), 60),
            Track::new(2, "Pad", Some(InstrumentSpec::new("synth", Default::default())), 60),
        ];
        project.clips = vec![clip(100, 1, 3840, 960), clip(101, 1, 960, 960), clip(102, 2, 0, 9600)];
        project.graph.nodes.extend([
            node(10, NodeKind::Instrument { track: 1, strip: quiet }),
            node(11, NodeKind::Instrument { track: 2, strip: Strip::default() }),
            effect(20),
            effect(21),
            node(30, NodeKind::Bus { strip: Strip::default() }),
            node(31, NodeKind::Bus { strip: Strip::default() }),
        ]);
        project.graph.edges = vec![
            edge(10, 20, 0.9),
            edge(20, 21, 0.7),
            edge(21, 30, 0.6),
            edge(30, MASTER, 1.0),
            edge(10, 31, 0.4),
            Edge { sidechain: true, ..edge(11, 21, 1.0) },
            edge(11, MASTER, 1.0),
        ];
        project.automation = vec![
            lane(1, ParamTarget::NodeVolume { node: 10 }),
            lane(2, ParamTarget::EffectParam { node: 21, param: "gain".into() }),
            lane(3, ParamTarget::InstrumentParam { track: 1, param: "gain".into() }),
            lane(4, ParamTarget::InstrumentParam { track: 2, param: "gain".into() }),
            lane(5, ParamTarget::NodeVolume { node: 30 }),
            lane(6, ParamTarget::EdgeLevel { from: 20, to: 21, sidechain: false }),
            lane(7, ParamTarget::EdgeLevel { from: 21, to: 30, sidechain: false }),
            lane(8, ParamTarget::NodeVolume { node: MASTER }),
        ];
        project.loop_region = Some((0, 3840));
        project
    }

    #[test]
    fn only_the_track_path_to_its_output_sounds() {
        let reduced = isolate(&project(), 1).unwrap();
        let mut ids: Vec<NodeId> = reduced.graph.nodes.iter().map(|n| n.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, [MASTER, 10, 20, 21]);
        assert_eq!(reduced.graph.edges, [edge(10, 20, 0.9), edge(20, 21, 0.7), edge(21, MASTER, 1.0)], "path levels stay, the output is 1");
        assert_eq!(reduced.graph.order().len(), 4, "the graph has no loops or dangling edges");
        assert_eq!(reduced.tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [1]);
        assert_eq!(reduced.clips.iter().map(|c| c.id).collect::<Vec<_>>(), [100, 101]);
        assert_eq!(reduced.loop_region, None);
    }

    #[test]
    fn fader_stays_but_mute_solo_and_master_do_not_count() {
        let reduced = isolate(&project(), 1).unwrap();
        let strip = *reduced.graph.node(10).unwrap().kind.strip().unwrap();
        assert_eq!((strip.volume, strip.pan, strip.mute, strip.solo), (0.5, -0.3, false, false));
        assert_eq!(reduced.graph.node(MASTER).unwrap().kind, NodeKind::Master { volume: 1.0 });
        let lanes: Vec<u32> = reduced.automation.iter().map(|l| l.id).collect();
        assert_eq!(lanes, [1, 2, 3, 6], "own volume, inserts, instrument and the edge inside the path stay; bus, output and master do not");
    }

    #[test]
    fn track_without_effects_goes_straight_to_master_and_unknown_track_is_an_error() {
        let reduced = isolate(&project(), 2).unwrap();
        assert_eq!(reduced.graph.edges, [edge(11, MASTER, 1.0)]);
        assert!(matches!(isolate(&project(), 9), Err(e) if e.is(tracks::NOT_FOUND)));
    }

    #[test]
    fn range_spans_the_track_clips_and_tail_is_checked() {
        assert_eq!(used_range(&project(), 1).unwrap(), (960, 4800));
        let mut empty = project();
        empty.clips.retain(|c| c.track != 1);
        assert!(matches!(used_range(&empty, 1), Err(e) if e.is(export::TRACK_HAS_NO_CLIPS)));
        assert!(check_tail(0.0).is_ok() && check_tail(MAX_TAIL_SECONDS).is_ok());
        for tail in [-1.0, MAX_TAIL_SECONDS + 1.0, f32::NAN] {
            assert!(matches!(check_tail(tail), Err(e) if e.is(export::TAIL_OUT_OF_RANGE)), "{tail}");
        }
    }
}
