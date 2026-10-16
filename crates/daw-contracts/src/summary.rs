//! A short overview of the project (`ProjectSummary`): what a reader needs to find their way in it without reading all
//! of it. Mainly for AI: `project.get` of a real song carries every note, point and parameter and fills the AI's
//! context before it starts working; the summary answers "what is here and where" in a few hundred lines.

use serde::Serialize;

use crate::{Bpm, LaneId, MarkerId, NodeId, ParamTarget, Project, Strip, TimeSignature, TrackId};

/// "Describe the project briefly". The project is passed in (RPC passes `ProjectView::snapshot`); nothing is rendered
/// or changed, so it is cheap to call after every step. Never fails: every project has a summary. Sound levels are
/// not part of it (they need a render): `MixAnalysis`.
pub trait ProjectSummary: Send + Sync {
    fn summarize(&self, project: &Project) -> Summary;
}

/// The project in brief. Ticks are absolute; spans are `[start, end)`; bars are numbered from 1 (as in
/// `analysis.levels`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub name: String,
    /// Tempo and meter at the song start; `tempo_changes`: they change along the song (`project.get` → `tempoMap`).
    pub bpm: Bpm,
    pub signature: TimeSignature,
    pub tempo_changes: bool,
    /// The end of the last clip and the number of bars up to it (0 for an empty arrangement).
    pub end_tick: u32,
    pub bars: u32,
    pub loop_region: Option<(u32, u32)>,
    /// Song sections from the markers, in order.
    pub sections: Vec<SectionSummary>,
    /// Tracks in display order.
    pub tracks: Vec<TrackSummary>,
    pub buses: Vec<BusSummary>,
    /// Effects that are not an insert on a track's or bus's path: fed by several nodes (a limiter before the master, a
    /// shared reverb), only by a send, or by nothing.
    pub shared_effects: Vec<SharedEffectSummary>,
    pub master_volume: f32,
    pub automation: Vec<LaneSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionSummary {
    pub marker: MarkerId,
    pub name: String,
    /// From the marker to the next one; the last section ends at the song end (empty if the song ends before it).
    pub from_tick: u32,
    pub to_tick: u32,
    /// The bar the section starts in.
    pub bar: u32,
    /// Tracks with clips in the section, in display order.
    pub tracks: Vec<TrackId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackSummary {
    pub id: TrackId,
    pub name: String,
    /// Instrument kind (`synth`, `kick`, `vst3:Surge_XT`…); `None` (`null`) means an audio track.
    pub instrument: Option<String>,
    /// The instrument is frozen: its rendered audio copy plays instead.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub frozen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// MIDI effects in order, by kind.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub note_effects: Vec<String>,
    /// The track's source node (for `graph.*` methods) and its channel: volume, pan, mute, solo.
    pub node: Option<NodeId>,
    #[serde(flatten)]
    pub strip: Option<Strip>,
    /// Effects inserted on the track's path in order, and where the path goes (a bus, a shared effect or the master,
    /// node 0); `None` means it is not connected.
    pub effects: Vec<EffectSummary>,
    pub output: Option<NodeId>,
    /// How many clips the track has and where they play; touching and overlapping clips make one span.
    pub clips: usize,
    pub spans: Vec<(u32, u32)>,
    /// Note tracks: notes in the clips' content (a looped pattern counts once) and their lowest and highest pitch.
    pub notes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch_range: Option<(u8, u8)>,
    /// Audio tracks: the sounds the clips play, by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectSummary {
    pub id: NodeId,
    /// Effect kind (`eq`, `compressor`, `vst3:…`).
    pub kind: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BusSummary {
    pub id: NodeId,
    pub name: String,
    #[serde(flatten)]
    pub strip: Strip,
    /// Tracks whose sound reaches the bus (directly, through sends or other buses), in display order.
    pub tracks: Vec<TrackId>,
    /// As for a track: inserted effects and where the path goes.
    pub effects: Vec<EffectSummary>,
    pub output: Option<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedEffectSummary {
    #[serde(flatten)]
    pub effect: EffectSummary,
    /// As for a track: effects inserted after it and where the path goes.
    pub effects: Vec<EffectSummary>,
    pub output: Option<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneSummary {
    pub id: LaneId,
    pub target: ParamTarget,
    /// Where the lane's clips are.
    pub spans: Vec<(u32, u32)>,
}
