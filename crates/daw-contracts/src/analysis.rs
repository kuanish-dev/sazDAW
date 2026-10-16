//! Checking the work by ear, for an AI or a human: the project's sound levels (`MixAnalysis`) and finding what makes
//! the project sound different from what was intended (`MixCheck`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContractError, LaneId, NodeId, Project, TrackId};

/// Longest analysed range: 10 minutes. Each target (track, bus) is a separate offline render, so analysis time is
/// roughly targets × range length ÷ render speed.
pub const MAX_ANALYSIS_SECONDS: u32 = 10 * 60;
/// Largest analysis section, in bars.
pub const MAX_RESOLUTION_BARS: u32 = 64;
/// Default analysis section: 4 bars (a phrase).
pub const DEFAULT_RESOLUTION_BARS: u32 = 4;
/// A peak below this counts as silence (levels are `null`).
pub const SILENCE_DB: f32 = -90.0;

/// What to measure: each track separately, each bus, or the whole mix on the master.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LevelTargets {
    /// Each track's sound at its output: instrument (or audio clips) → fader → inserts, as in "Bounce to audio"
    /// (`TrackRender`); track mute/solo, sends, buses and the master have no effect.
    Track,
    /// Each bus's output (after its fader): everything routed into it; the master has no effect.
    Bus,
    /// The whole mix at the master output, as heard and as exported by `project.export`.
    #[default]
    Master,
}

/// What to measure and in how much detail.
/// JSON (RPC `analysis.levels`): `{ fromTick?, toTick?, per?, resolutionBars? }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct LevelRequest {
    /// Range start in ticks; default 0.
    pub from_tick: Option<u32>,
    /// Range end in ticks; default is the end of the last clip.
    pub to_tick: Option<u32>,
    /// Targets: `track`, `bus` or `master` (default).
    pub per: LevelTargets,
    /// Section length in bars (1…64, default 4); bars follow the tempo map and time signature changes.
    pub resolution_bars: Option<u32>,
}

/// Level of a piece of sound: peak and RMS in dBFS. RMS is over both channels: a sine peaking at 0 dBFS gives
/// −3 dBFS RMS.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub peak_db: f32,
    pub rms_db: f32,
}

/// A section of the range: its start bar (1-based, as on the ruler) and its bounds in ticks `[fromTick, toTick)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub bar: u32,
    pub from_tick: u32,
    pub to_tick: u32,
}

/// Levels of one target. `None` (`null`) means silence (peak below [`SILENCE_DB`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetLevels {
    /// Track id (`per: track`), bus node id (`bus`) or 0 (master).
    pub id: u32,
    pub name: String,
    /// The whole range.
    pub level: Option<Level>,
    /// Integrated loudness per ITU-R BS.1770 (K-weighting, gating at −70 LUFS and −10 LU), in LUFS; `None` when the
    /// sound is below the gate or the range is shorter than 0.4 s.
    pub loudness_lufs: Option<f32>,
    /// One entry per `LevelReport::sections` element, in the same order.
    pub sections: Vec<Option<Level>>,
}

/// Measurement result.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelReport {
    pub from_tick: u32,
    pub to_tick: u32,
    pub sections: Vec<Section>,
    pub targets: Vec<TargetLevels>,
}

/// The project's sound levels. Job: measure how the project sounds.
///
/// Contract:
/// * the sound is an offline render of a copy of `project` (usually a `ProjectView` snapshot), so the project itself
///   is not locked during the calculation and live playback is not interrupted; nothing is written to disk;
/// * range `[from_tick, to_tick)`: by default from 0 to the end of the last clip; rendering starts at `from_tick`, so
///   notes that started earlier are not heard in the range; the tail after `to_tick` is not included;
/// * sections are `resolution_bars` bars each, counted from the start of the bar containing `from_tick` (the first
///   and last are clipped to the range); bars follow the tempo map (`TempoMap::bar_tick`);
/// * targets (`LevelTargets`): tracks in `project.tracks` order, buses in graph node order, the master as one target;
/// * peak is sample peak (not true peak), RMS is over both channels, loudness is BS.1770 (no surround channel weights);
/// * errors: an empty arrangement without `to_tick`, or `from_tick >= to_tick`, is `ErrorKind::Invalid`; a range
///   longer than [`MAX_ANALYSIS_SECONDS`] or `resolution_bars` outside `1..=MAX_RESOLUTION_BARS` is `ErrorKind::Limit`;
///   an instrument or effect that fails to be created gives the catalog's error.
pub trait MixAnalysis: Send + Sync {
    fn levels(&self, project: &Project, request: &LevelRequest) -> Result<LevelReport, ContractError>;
}

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The sound is definitely broken: a silent track, a clipping master, a missing plugin.
    Error,
    /// Most likely a mistake: too quiet, a suspicious parameter, muted.
    Warning,
    /// Worth knowing: solo, a bypassed effect, an empty bus.
    Info,
}

/// One check finding: what is wrong, in plain text, and what it refers to.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub severity: Severity,
    /// Machine-readable finding kind (`silentTrack`, `masterClipping`…); `message` is for people.
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane: Option<LaneId>,
    /// Where in the song, ticks `[start, end)`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ranges: Vec<(u32, u32)>,
}

impl Finding {
    /// A finding not tied to a track, node, lane or position.
    pub fn new(severity: Severity, code: &'static str, message: impl Into<String>) -> Self {
        Self { severity, code, message: message.into(), track: None, node: None, lane: None, ranges: Vec::new() }
    }
}

/// Mix check. Job: find what makes the project sound different from what was intended.
///
/// Contract:
/// * looks both at the project data (mute/solo, parameters, graph, automation, plugins) and at its sound
///   (`MixAnalysis`: tracks and master over `[from_tick, to_tick)`, bar by bar);
/// * findings are sorted by decreasing severity (`Error`, `Warning`, `Info`), then in rule order; an empty list means
///   all is well; the project is not changed;
/// * never fails: if the sound cannot be rendered (bad range, missing plugin…) that becomes a `renderFailed` finding
///   with the reason (and a log entry) and the other rules still run; for an empty arrangement only the project data
///   is checked.
pub trait MixCheck: Send + Sync {
    fn check(&self, project: &Project, from_tick: Option<u32>, to_tick: Option<u32>) -> Vec<Finding>;
}
