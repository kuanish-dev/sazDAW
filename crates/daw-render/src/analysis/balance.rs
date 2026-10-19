//! The balance (`MixAnalysis::balance`): how loud each track is in the mix. The mix is the project with solo off
//! (`without_solo`), rendered once and kept. A track's sound in the mix is the mix minus the mix without its clips
//! (`without`), measured at the master output (`LoudnessMeter`); a muted track is the mix with it playing (`unmuted`)
//! minus the mix. Why the difference and not the track alone: it keeps what the other tracks do to the track — a
//! compressor keyed by the kick ducks it, a gate another track opens lets it through, the master's compressor and
//! limiter react to the whole mix — while a track alone is ducked by nothing, gated shut and squeezed by itself.
//!
//! The difference cancels the rest of the mix only when two renders of one project are equal sample for sample, as with
//! the built-in instruments and effects (their noise has fixed seeds). A plugin may not be, so the mix is rendered a
//! second time and how much the two differ is the floor (`BalanceReport::floor_lufs`).

use daw_contracts::codes::tracks;
use daw_contracts::{BalanceReport, ContractError, LoudnessLevels, NodeId, Project, TrackBalance, TrackId};

use super::LevelMeter;
use super::meter::{ANALYSIS_SAMPLE_RATE, LoudnessMeter};
use crate::loudness::round_tenth;

const SILENT: LoudnessLevels = LoudnessLevels { loudness_lufs: None, max_short_term_lufs: None, true_peak_db: None };

impl LevelMeter {
    /// The balance over the checked range `[from, to)`: the mix twice, then one render per track with clips in it.
    pub(super) fn balance_of(&self, project: &Project, from: u32, to: u32) -> Result<BalanceReport, ContractError> {
        let frames = project.tempo_map.frames_between(f64::from(from), f64::from(to), ANALYSIS_SAMPLE_RATE).round() as u64;
        let mix = without_solo(project);
        // One stereo copy of the range at the analysis rate: about 23 MB a minute, at most `MAX_ANALYSIS_SECONDS` long.
        let mut heard = Vec::with_capacity(frames as usize * 2);
        self.render(&mix, from, frames, |chunk| heard.extend_from_slice(chunk))?;
        let mut mix_meter = LoudnessMeter::new();
        mix_meter.push(&heard);
        let mix_levels = mix_meter.finish();
        let floor = self.difference(&mix, from, &heard)?;
        let reaching = mix.graph.reaching_master();
        let mut balance = Vec::new();
        for track in &project.tracks {
            let source = project.graph.source_of(track.id).ok_or_else(|| tracks::NOT_FOUND.with("id", track.id))?;
            let strip = source.kind.strip().copied().unwrap_or_default();
            let plays = project.clips.iter().any(|c| c.track == track.id && c.overlaps(u64::from(from), u64::from(to)));
            // A track without clips in the range adds nothing and needs no render.
            let levels = match (plays, strip.mute) {
                (false, _) => SILENT,
                (true, false) => self.difference(&without(&mix, track.id), from, &heard)?,
                (true, true) => self.difference(&unmuted(&mix, source.id), from, &heard)?,
            };
            balance.push(TrackBalance {
                track: track.id,
                name: track.name.clone(),
                node: source.id,
                volume: strip.volume,
                muted: strip.mute,
                keys_only: !reaching.contains(&source.id),
                silent: levels.true_peak_db.is_none(),
                levels,
                relative_lu: relative(levels.loudness_lufs, mix_levels.loudness_lufs),
            });
        }
        Ok(BalanceReport { from_tick: from, to_tick: to, mix: mix_levels, floor_lufs: floor.loudness_lufs, tracks: balance })
    }

    /// Plays `project` over the range `heard` was rendered from and measures, chunk by chunk, how the two differ.
    /// Loudness and peaks do not depend on the sign, so it does not matter which of the two has the track.
    fn difference(&self, project: &Project, from: u32, heard: &[f32]) -> Result<LoudnessLevels, ContractError> {
        let mut meter = LoudnessMeter::new();
        let mut delta = Vec::new();
        let mut at = 0;
        self.render(project, from, (heard.len() / 2) as u64, |chunk| {
            delta.clear();
            delta.extend(chunk.iter().zip(&heard[at..]).map(|(now, then)| now - then));
            at += chunk.len();
            meter.push(&delta);
        })?;
        Ok(meter.finish())
    }
}

/// The mix as it is balanced: solo is a listening aid, so it is off on every node; everything else stays.
fn without_solo(project: &Project) -> Project {
    let mut mix = project.clone();
    for strip in mix.graph.nodes.iter_mut().filter_map(|node| node.kind.strip_mut()) {
        strip.solo = false;
    }
    mix
}

/// `mix` without `track`'s clips: everything else plays as in the mix.
fn without(mix: &Project, track: TrackId) -> Project {
    let mut without = mix.clone();
    without.clips.retain(|clip| clip.track != track);
    without
}

/// `mix` with the muted source `source` playing.
fn unmuted(mix: &Project, source: NodeId) -> Project {
    let mut unmuted = mix.clone();
    if let Some(strip) = unmuted.graph.node_mut(source).and_then(|node| node.kind.strip_mut()) {
        strip.mute = false;
    }
    unmuted
}

/// The track's loudness relative to the mix, LU.
fn relative(track: Option<f32>, mix: Option<f32>) -> Option<f32> {
    Some(round_tenth(f64::from(track?) - f64::from(mix?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::{Clip, ClipContent, Edge, InstrumentSpec, MASTER, Node, NodeKind, NoteContent, Strip, Track};

    fn clip(id: u32, track: TrackId) -> Clip {
        Clip {
            id,
            track,
            start_tick: 0,
            length_ticks: 3840,
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

    fn node(id: NodeId, kind: NodeKind) -> Node {
        Node { id, name: format!("n{id}"), kind, x: 0.0, y: 0.0 }
    }

    fn edge(from: NodeId, to: NodeId) -> Edge {
        Edge { from, to, level: 1.0, sidechain: false, pre_fader: false }
    }

    fn strip(node: &Node) -> Strip {
        *node.kind.strip().unwrap()
    }

    /// Track 1 (node 10, muted) and track 2 (node 11, soloed) → bus 30 (muted, soloed) → master.
    fn project() -> Project {
        let muted = Strip { mute: true, ..Strip::default() };
        let soloed = Strip { solo: true, ..Strip::default() };
        let mut project = Project::empty();
        project.tracks = vec![
            Track::new(1, "Bass", Some(InstrumentSpec::new("synth", Default::default())), 36),
            Track::new(2, "Lead", Some(InstrumentSpec::new("synth", Default::default())), 60),
        ];
        project.clips = vec![clip(100, 1), clip(101, 2), clip(102, 1)];
        project.graph.nodes.extend([
            node(10, NodeKind::Instrument { track: 1, strip: muted }),
            node(11, NodeKind::Instrument { track: 2, strip: soloed }),
            node(30, NodeKind::Bus { strip: Strip { mute: true, solo: true, ..Strip::default() } }),
        ]);
        project.graph.edges = vec![edge(10, 30), edge(11, 30), edge(30, MASTER)];
        project
    }

    #[test]
    fn the_mix_has_solo_off_everywhere_and_keeps_mutes() {
        let mix = without_solo(&project());
        assert!(mix.graph.nodes.iter().filter_map(|n| n.kind.strip()).all(|s| !s.solo));
        assert!(strip(mix.graph.node(10).unwrap()).mute && strip(mix.graph.node(30).unwrap()).mute, "mutes stay");
        assert_eq!(mix.clips, project().clips);
    }

    #[test]
    fn without_drops_only_the_tracks_clips() {
        let mix = without_solo(&project());
        let without = without(&mix, 1);
        assert_eq!(without.clips.iter().map(|c| c.id).collect::<Vec<_>>(), [101]);
        assert_eq!((&without.tracks, &without.graph), (&mix.tracks, &mix.graph), "every instrument and the routing stay");
    }

    #[test]
    fn unmuted_lets_only_that_source_play() {
        let mix = without_solo(&project());
        let unmuted = unmuted(&mix, 10);
        assert!(!strip(unmuted.graph.node(10).unwrap()).mute, "its own mute is cleared");
        assert!(strip(unmuted.graph.node(30).unwrap()).mute, "a muted bus stays muted");
        assert_eq!(unmuted.clips, mix.clips);
    }

    #[test]
    fn relative_loudness_needs_both_and_is_rounded() {
        assert_eq!(relative(Some(-20.3), Some(-14.1)), Some(-6.2));
        assert_eq!(relative(None, Some(-14.0)), None);
        assert_eq!(relative(Some(-20.0), None), None);
    }
}
