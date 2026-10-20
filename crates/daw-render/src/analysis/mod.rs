//! `LevelMeter` (`MixAnalysis`): "measure how the project sounds". Each target (a track: its path to the output, as in
//! "Bounce to audio", `track::isolate`; a bus: everything feeding it, `stems::stem_graph`; or the master: the whole mix)
//! is rendered by the same offline engine as the export and measured while rendering (`meter.rs`), without files.
//! The balance (`balance.rs`) measures each track as the mix minus the mix without it.

mod balance;
pub(crate) mod meter;

use std::sync::Arc;

use daw_contracts::codes::export;
use daw_contracts::{
    BalanceReport, ContractError, DEFAULT_RESOLUTION_BARS, LevelReport, LevelRequest, LevelTargets, MASTER, MAX_ANALYSIS_SECONDS,
    MAX_RESOLUTION_BARS, MixAnalysis, NodeId, NodeKind, ParamTarget, Project, Section, TargetLevels, TempoMap,
};

use crate::{CHUNK_FRAMES, OfflineExporter, stems, track};
use meter::{ANALYSIS_SAMPLE_RATE, Measured, Meter};

pub struct LevelMeter {
    renderer: Arc<OfflineExporter>,
}

impl LevelMeter {
    pub fn new(renderer: Arc<OfflineExporter>) -> Self {
        Self { renderer }
    }

    /// Plays `project` from tick `from` for `frames` frames and hands each rendered chunk (interleaved stereo) to `sink`.
    fn render(&self, project: &Project, from: u32, frames: u64, mut sink: impl FnMut(&[f32])) -> Result<(), ContractError> {
        let mut engine = self.renderer.load_engine(project, ANALYSIS_SAMPLE_RATE)?;
        engine.transport.seek(u64::from(from));
        engine.transport.play();
        let mut buffer = vec![0.0f32; CHUNK_FRAMES * 2];
        let mut done = 0u64;
        while done < frames {
            let count = (frames - done).min(CHUNK_FRAMES as u64) as usize;
            let chunk = &mut buffer[..count * 2];
            engine.renderer.render(chunk);
            // What the engine released (finished voices) is freed here, as during export.
            engine.dsp.collect_garbage();
            sink(chunk);
            done += count as u64;
        }
        Ok(())
    }

    /// Plays `project` from `from` and measures the sound in sections ending at frames `ends`.
    fn measure(&self, project: &Project, from: u32, ends: &[u64]) -> Result<Measured, ContractError> {
        let mut meter = Meter::new(ends.to_vec());
        self.render(project, from, ends.last().copied().unwrap_or(0), |chunk| meter.push(chunk))?;
        Ok(meter.finish())
    }
}

impl MixAnalysis for LevelMeter {
    fn levels(&self, project: &Project, request: &LevelRequest) -> Result<LevelReport, ContractError> {
        let bars = check_resolution(request.resolution_bars)?;
        let (from, to) = range(project, request.from_tick, request.to_tick)?;
        let sections = sections(&project.tempo_map, from, to, bars);
        let ends = section_ends(&project.tempo_map, from, &sections);
        let silent = || Measured { level: None, loudness_lufs: None, sections: vec![None; sections.len()] };
        let mut targets = Vec::new();
        for target in targets_of(project, request.per, from, to)? {
            // A track without clips in the range is silent and needs no render.
            let measured = match &target.project {
                Some(reduced) => self.measure(reduced, from, &ends)?,
                None => silent(),
            };
            targets.push(TargetLevels {
                id: target.id,
                name: target.name,
                level: measured.level,
                loudness_lufs: measured.loudness_lufs,
                sections: measured.sections,
            });
        }
        Ok(LevelReport { from_tick: from, to_tick: to, sections, targets })
    }

    fn balance(&self, project: &Project, from_tick: Option<u32>, to_tick: Option<u32>) -> Result<BalanceReport, ContractError> {
        let (from, to) = range(project, from_tick, to_tick)?;
        self.balance_of(project, from, to)
    }
}

fn check_resolution(bars: Option<u32>) -> Result<u32, ContractError> {
    let bars = bars.unwrap_or(DEFAULT_RESOLUTION_BARS);
    if (1..=MAX_RESOLUTION_BARS).contains(&bars) {
        Ok(bars)
    } else {
        Err(export::RESOLUTION_OUT_OF_RANGE.with("bars", bars).with("max", MAX_RESOLUTION_BARS))
    }
}

/// The range `[from, to)`: by default from 0 to the end of the last clip; at most `MAX_ANALYSIS_SECONDS` long.
fn range(project: &Project, from: Option<u32>, to: Option<u32>) -> Result<(u32, u32), ContractError> {
    let from = from.unwrap_or(0);
    let to = if let Some(to) = to {
        to
    } else {
        let end = project.song_end();
        if end == 0 {
            return Err(export::NOTHING_TO_MEASURE.err());
        }
        u32::try_from(end).unwrap_or(u32::MAX)
    };
    if from >= to {
        return Err(export::RANGE_BACKWARDS.with("from", from).with("to", to));
    }
    let seconds = project.tempo_map.seconds_at(f64::from(to)) - project.tempo_map.seconds_at(f64::from(from));
    if seconds > f64::from(MAX_ANALYSIS_SECONDS) {
        return Err(export::ANALYSIS_TOO_LONG.with("seconds", seconds.round()).with("max", MAX_ANALYSIS_SECONDS));
    }
    Ok((from, to))
}

/// Sections of `bars` bars from the start of the bar containing `from`; the outer ones are cut by the range bounds.
fn sections(map: &TempoMap, from: u32, to: u32, bars: u32) -> Vec<Section> {
    let mut bar = map.bar_index(f64::from(from)).max(0) as u32;
    let mut start = from;
    let mut result = Vec::new();
    while start < to {
        let next = bar + bars;
        let end = map.bar_tick(next).clamp(u64::from(start) + 1, u64::from(to)) as u32;
        result.push(Section { bar: bar + 1, from_tick: start, to_tick: end });
        (start, bar) = (end, next);
    }
    result
}

/// The end frame of each section, counted from the range start (by the tempo map).
fn section_ends(map: &TempoMap, from: u32, sections: &[Section]) -> Vec<u64> {
    let ends: Vec<u64> =
        sections.iter().map(|s| map.frames_between(f64::from(from), f64::from(s.to_tick), ANALYSIS_SAMPLE_RATE).round() as u64).collect();
    // A very short section could round to zero frames; end frames must strictly increase or the meter would skip them.
    ends.iter()
        .scan(0u64, |last, &end| {
            *last = end.max(*last + 1);
            Some(*last)
        })
        .collect()
}

/// What to render for one target; `project: None` means the target is known to be silent.
struct Target {
    id: u32,
    name: String,
    project: Option<Project>,
}

fn targets_of(project: &Project, per: LevelTargets, from: u32, to: u32) -> Result<Vec<Target>, ContractError> {
    Ok(match per {
        LevelTargets::Master => {
            let name = project.graph.node(MASTER).map_or_else(|| "Master".to_string(), |n| n.name.clone());
            vec![Target { id: MASTER, name, project: Some(project.clone()) }]
        }
        LevelTargets::Track => {
            let mut targets = Vec::new();
            for track in &project.tracks {
                let plays = project.clips.iter().any(|c| c.track == track.id && c.overlaps(u64::from(from), u64::from(to)));
                let reduced = if plays { Some(track::isolate(project, track.id)?) } else { None };
                targets.push(Target { id: track.id, name: track.name.clone(), project: reduced });
            }
            targets
        }
        LevelTargets::Bus => project
            .graph
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Bus { .. }))
            .map(|n| Target { id: n.id, name: n.name.clone(), project: Some(bus_only(project, n.id)) })
            .collect(),
    })
}

/// A project in which only `bus` sounds, at its own output: everything feeding it, the bus itself (mute/solo cleared,
/// the fader stays) and the master at unity without its automation; the bus goes to the master at level 1.
fn bus_only(project: &Project, bus: NodeId) -> Project {
    let mut reduced = project.clone();
    reduced.graph = stems::stem_graph(&project.graph, bus);
    for edge in reduced.graph.edges.iter_mut().filter(|e| e.from == bus && e.to == MASTER) {
        edge.level = 1.0;
    }
    if let Some(master) = reduced.graph.node_mut(MASTER) {
        master.kind = NodeKind::Master { volume: 1.0 };
    }
    let kept: Vec<NodeId> = reduced.graph.nodes.iter().map(|n| n.id).collect();
    reduced.automation.retain(|lane| match &lane.target {
        ParamTarget::NodeVolume { node }
        | ParamTarget::NodePan { node }
        | ParamTarget::NodeWidth { node }
        | ParamTarget::StageDepth { node }
        | ParamTarget::StageHeight { node }
        | ParamTarget::EffectParam { node, .. } => *node != MASTER && kept.contains(node),
        ParamTarget::EdgeLevel { from, to, .. } => *to != MASTER && kept.contains(from) && kept.contains(to),
        ParamTarget::InstrumentParam { .. } => true,
    });
    reduced
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::codes::export;
    use daw_contracts::{
        AutoPoint, AutomationClip, AutomationLane, Bpm, Clip, ClipContent, Edge, MAX_CLIP_LOOP_TICKS, Node, NoteContent, SignatureChange,
        Strip, TimeSignature,
    };

    fn clip(track: u32, start: u32, length: u32) -> Clip {
        Clip {
            id: start + 1,
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

    fn ticks(sections: &[Section]) -> Vec<(u32, u32, u32)> {
        sections.iter().map(|s| (s.bar, s.from_tick, s.to_tick)).collect()
    }

    #[test]
    fn sections_follow_bars_and_are_cut_by_the_range() {
        let map = TempoMap::default();
        assert_eq!(ticks(&sections(&map, 0, 3840 * 8, 4)), [(1, 0, 15360), (5, 15360, 30720)]);
        // Starts mid bar 2, ends mid bar 4, one bar per section.
        assert_eq!(ticks(&sections(&map, 5000, 13000, 1)), [(2, 5000, 7680), (3, 7680, 11520), (4, 11520, 13000)]);
    }

    #[test]
    fn sections_follow_signature_changes() {
        // Two bars of 4/4, then 3/4: bars 3 and 4 are 2880 ticks each.
        let three = SignatureChange { tick: 7680, signature: TimeSignature::new(3, 4).unwrap() };
        let map =
            TempoMap::default().with_signatures(vec![SignatureChange { tick: 0, signature: TimeSignature::FOUR_FOUR }, three]).unwrap();
        assert_eq!(ticks(&sections(&map, 0, 7680 + 2880 * 2, 1)), [(1, 0, 3840), (2, 3840, 7680), (3, 7680, 10560), (4, 10560, 13440)]);
    }

    #[test]
    fn section_ends_are_frames_from_the_range_start() {
        let map = TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR);
        // 120 BPM, 48 kHz: a bar is 2 s = 96,000 frames.
        let ends = section_ends(&map, 1920, &sections(&map, 1920, 3840 * 3, 1));
        assert_eq!(ends, [48_000, 144_000, 240_000]);
    }

    #[test]
    fn range_defaults_to_the_arrangement_and_is_checked() {
        let mut project = Project::empty();
        assert!(matches!(range(&project, None, None), Err(e) if e.is(export::NOTHING_TO_MEASURE)), "empty arrangement");
        assert_eq!(range(&project, None, Some(3840)).unwrap(), (0, 3840));
        project.clips = vec![clip(1, 0, 3840), clip(1, 7680, 1920)];
        assert_eq!(range(&project, None, None).unwrap(), (0, 9600));
        assert_eq!(range(&project, Some(960), None).unwrap(), (960, 9600));
        assert!(matches!(range(&project, Some(9600), None), Err(e) if e.is(export::RANGE_BACKWARDS)));
        // 120 BPM: 10 minutes = 300 bars.
        project.tempo_map = TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR);
        assert!(range(&project, None, Some(3840 * 300)).is_ok());
        assert!(matches!(range(&project, None, Some(3840 * 301)), Err(e) if e.is(export::ANALYSIS_TOO_LONG)));
        assert!(check_resolution(None).is_ok_and(|b| b == DEFAULT_RESOLUTION_BARS));
        assert!(matches!(check_resolution(Some(0)), Err(e) if e.is(export::RESOLUTION_OUT_OF_RANGE)));
        assert!(matches!(check_resolution(Some(MAX_RESOLUTION_BARS + 1)), Err(e) if e.is(export::RESOLUTION_OUT_OF_RANGE)));
    }

    fn node(id: NodeId, kind: NodeKind) -> Node {
        Node { id, name: format!("n{id}"), kind, x: 0.0, y: 0.0 }
    }

    fn edge(from: NodeId, to: NodeId, level: f32) -> Edge {
        Edge { from, to, level, sidechain: false, pre_fader: false }
    }

    fn lane(id: u32, target: ParamTarget) -> AutomationLane {
        AutomationLane { id, target, clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 0.5)])] }
    }

    /// Sources 10 → bus 30 (level 0.5) → master (0.7); source 11 → master.
    #[test]
    fn bus_sounds_on_its_own_output_without_master() {
        let mut project = Project::empty();
        project.graph.nodes = vec![
            node(MASTER, NodeKind::Master { volume: 0.7 }),
            node(10, NodeKind::Instrument { track: 1, strip: Strip::default() }),
            node(11, NodeKind::Instrument { track: 2, strip: Strip::default() }),
            node(30, NodeKind::Bus { strip: Strip { volume: 0.6, pan: 0.0, mute: true, solo: false, ..Strip::default() } }),
        ];
        project.graph.edges = vec![edge(10, 30, 1.0), edge(30, MASTER, 0.5), edge(11, MASTER, 1.0)];
        project.automation = vec![
            lane(1, ParamTarget::NodeVolume { node: 30 }),
            lane(2, ParamTarget::NodeVolume { node: MASTER }),
            lane(3, ParamTarget::EdgeLevel { from: 30, to: MASTER, sidechain: false }),
            lane(4, ParamTarget::NodeVolume { node: 11 }),
        ];
        let reduced = bus_only(&project, 30);
        assert_eq!(reduced.graph.node(MASTER).unwrap().kind, NodeKind::Master { volume: 1.0 });
        assert_eq!(reduced.graph.edges, [edge(10, 30, 1.0), edge(30, MASTER, 1.0)]);
        let strip = *reduced.graph.node(30).unwrap().kind.strip().unwrap();
        assert!(!strip.mute && strip.volume == 0.6, "the bus fader stays, mute is cleared");
        assert_eq!(
            reduced.automation.iter().map(|l| l.id).collect::<Vec<_>>(),
            [1],
            "the bus's own volume stays; master, output and other lanes do not"
        );
    }

    #[test]
    fn tracks_without_clips_in_the_range_are_not_rendered() {
        let mut project = Project::empty();
        for (id, node_id) in [(1, 10), (2, 11)] {
            project.tracks.push(daw_contracts::Track::new(id, format!("t{id}"), None, 60));
            project.graph.nodes.push(node(node_id, NodeKind::Audio { track: id, strip: Strip::default(), monitor: Default::default() }));
            project.graph.edges.push(edge(node_id, MASTER, 1.0));
        }
        project.clips = vec![clip(1, 0, 3840), clip(2, 7680, 3840)];
        let targets = targets_of(&project, LevelTargets::Track, 0, 3840).unwrap();
        assert_eq!(targets.iter().map(|t| (t.id, t.project.is_some())).collect::<Vec<_>>(), [(1, true), (2, false)]);
    }
}
