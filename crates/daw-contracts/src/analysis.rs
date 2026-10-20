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

/// Loudness of a sound over the whole range, measured as the export and the live master meter measure it (BS.1770 /
/// EBU R128). Each value is `None` when nothing passed its gate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoudnessLevels {
    /// Integrated loudness, LUFS (gated at −70 LUFS and −10 LU): the number to balance by. `None` below the gate or
    /// for a range shorter than 0.4 s.
    pub loudness_lufs: Option<f32>,
    /// The loudest 3 s (short-term loudness), LUFS: where a part that is quiet on average stands out. `None` below the
    /// gate or for a range shorter than 3 s.
    pub max_short_term_lufs: Option<f32>,
    /// True peak (4× oversampled), dBTP. `None` below [`SILENCE_DB`].
    pub true_peak_db: Option<f32>,
}

/// One track as it is heard in the mix (`MixAnalysis::balance`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackBalance {
    pub track: TrackId,
    pub name: String,
    /// The track's source node: the fader to move (`GraphControl::set_volume`).
    pub node: NodeId,
    /// The fader, a linear gain (1 is 0 dB): the track's loudness follows it, 20·log10 of the change in dB, unless a
    /// compressor after it reacts.
    pub volume: f32,
    /// The track is muted in the project; its numbers say how loud it is when it plays.
    pub muted: bool,
    /// The track reaches the master only by keying a sidechain: its numbers are what it opens or ducks, not a level its
    /// fader sets.
    pub keys_only: bool,
    /// The track adds nothing to the mix in the range (no clips, volume 0, only a muted bus…); `levels` are all `None`
    /// then.
    pub silent: bool,
    #[serde(flatten)]
    pub levels: LoudnessLevels,
    /// The track's integrated loudness in the mix minus the mix's, in LU: 0 is as loud as the whole mix, negative is
    /// quieter.
    pub relative_lu: Option<f32>,
}

/// How loud each track is in the mix.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceReport {
    pub from_tick: u32,
    pub to_tick: u32,
    /// The whole mix, measured the same way: the reference for `TrackBalance::relative_lu`.
    pub mix: LoudnessLevels,
    /// How much two renders of the same mix differ, LUFS; `None`: not at all, so every track is measured exactly. A
    /// plugin that sounds different each time raises it, and a track within about 10 LU of it is not measured reliably.
    pub floor_lufs: Option<f32>,
    pub tracks: Vec<TrackBalance>,
}

/// The project's sound levels. Job: measure how the project sounds.
///
/// Contract:
/// * the sound is an offline render of a copy of `project` (usually a `ProjectView` snapshot), so the project itself
///   is not locked during the calculation and live playback is not interrupted; nothing is written to disk;
/// * range `[from_tick, to_tick)`: by default from 0 to the end of the last clip; rendering starts at `from_tick`, so
///   notes that started earlier are not heard in the range; the tail after `to_tick` is not included;
/// * errors: an empty arrangement without `to_tick`, or `from_tick >= to_tick`, is `ErrorKind::Invalid`; a range
///   longer than [`MAX_ANALYSIS_SECONDS`] is `ErrorKind::Limit`; an instrument or effect that fails to be created gives
///   the catalog's error.
pub trait MixAnalysis: Send + Sync {
    /// Levels of each target (`request.per`):
    /// * sections are `resolution_bars` bars each, counted from the start of the bar containing `from_tick` (the first
    ///   and last are clipped to the range); bars follow the tempo map (`TempoMap::bar_tick`);
    /// * targets (`LevelTargets`): tracks in `project.tracks` order, buses in graph node order, the master as one target;
    /// * peak is sample peak (not true peak), RMS is over both channels, loudness is BS.1770 (no surround channel
    ///   weights);
    /// * `resolution_bars` outside `1..=MAX_RESOLUTION_BARS` is `ErrorKind::Limit`.
    fn levels(&self, project: &Project, request: &LevelRequest) -> Result<LevelReport, ContractError>;

    /// How loud each track is in the mix, for balancing their volumes:
    /// * a track's sound in the mix is the mix minus the mix without its clips, measured at the master output, after
    ///   the fader (balancing moves the fader, so the number must follow it); it keeps what the other tracks do to it:
    ///   a sidechain they key ducks it or opens its gate, the master's dynamics react to the whole mix;
    /// * the difference is exact when two renders of a project are equal sample for sample, as with the built-in
    ///   instruments and effects; `floor_lufs` is how much two renders of the mix differ;
    /// * solo is a listening aid and is ignored on every node; a muted track is measured as the mix with it playing
    ///   minus the mix (`muted` reports it); every other mute stays, so a track that only feeds a muted bus is `silent`;
    /// * `mix` is the whole project with solo off, measured the same way; `relative_lu` compares against it;
    /// * tracks in `project.tracks` order; a track without clips in the range is `silent` without a render; the cost
    ///   is one render of the whole mix per track with clips, plus two.
    fn balance(&self, project: &Project, from_tick: Option<u32>, to_tick: Option<u32>) -> Result<BalanceReport, ContractError>;
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
