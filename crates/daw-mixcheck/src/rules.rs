//! Mix check rules as pure functions: project (+ sound levels, + kind information) → findings. Finding messages are
//! English: they are read by AI clients and logs, not shown in the interface.
//!
//! The heuristics are deliberately simple (easy to explain and to verify):
//! * a track is silent when in every bar where it has clips the sound is below `SILENCE_DB`; it is quiet when even its
//!   loudest bar with clips is below [`QUIET_TRACK_RMS_DB`] RMS (the track's sound at its output, without mute/solo);
//! * the master clips when a bar's peak is at least [`CLIP_PEAK_DB`]; the mix is quiet when its integrated loudness is
//!   below [`QUIET_MASTER_LUFS`];
//! * an instrument parameter is suspicious when it is numeric, its name or label contains "gain", "volume",
//!   "cutoff" or "high cut" (case-insensitive; a low cut at its minimum is normal), and it sits in the first 1 % of its range (for ranges from a positive minimum wider
//!   than 10×, on a log scale like frequencies): this catches a volume of 0.3 % and a cutoff of 40 Hz;
//! * automation is out of range when it leaves the target's range: volume and edge level `0..MAX_LEVEL`, pan `-1..1`,
//!   width `0..MAX_WIDTH`, parameters by the catalog's `ParamInfo` (kinds missing from the catalog are not checked).

use std::collections::{BTreeSet, HashMap};

use daw_contracts::{
    Finding, LevelReport, MASTER, MAX_LEVEL, MAX_WIDTH, NodeId, NodeKind, ParamInfo, ParamKind, ParamTarget, Project, Section, Severity,
    TempoMap,
};

/// Below this RMS (the loudest bar with clips) a track counts as nearly inaudible.
pub(crate) const QUIET_TRACK_RMS_DB: f32 = -45.0;
/// From this peak on the master counts as clipping.
pub(crate) const CLIP_PEAK_DB: f32 = -0.1;
/// Below this loudness the mix counts as too quiet (a finished track is usually -16..-8 LUFS).
pub(crate) const QUIET_MASTER_LUFS: f32 = -30.0;
/// The share of the range above the minimum in which a level or cutoff parameter is suspicious.
const NEAR_MIN: f32 = 0.01;
const LEVEL_WORDS: [&str; 5] = ["gain", "volume", "cutoff", "high_cut", "high cut"];

/// What the check knows about kinds: catalog parameters (per kind) and project kinds that cannot be created.
pub(crate) struct Kinds {
    pub instruments: HashMap<String, Vec<ParamInfo>>,
    pub effects: HashMap<String, Vec<ParamInfo>>,
    /// Disabled by the user (`PluginScanner::set_disabled`).
    pub disabled: BTreeSet<String>,
    /// Unknown to the catalog (the plugin is not found on this machine).
    pub missing: BTreeSet<String>,
}

fn finding(severity: Severity, code: &'static str, message: String) -> Finding {
    Finding::new(severity, code, message)
}

/// The bar number on the ruler (from 1).
fn bar(map: &TempoMap, tick: u32) -> i64 {
    map.bar_index(f64::from(tick)) + 1
}

/// "bar 5", "bars 1–4, 9–12".
fn bars_text(map: &TempoMap, ranges: &[(u32, u32)]) -> String {
    let parts: Vec<String> = ranges
        .iter()
        .map(|&(from, to)| {
            let (first, last) = (bar(map, from), bar(map, to.saturating_sub(1).max(from)));
            if first == last { first.to_string() } else { format!("{first}–{last}") }
        })
        .collect();
    let single = ranges.len() == 1 && !parts[0].contains('–');
    format!("{} {}", if single { "bar" } else { "bars" }, parts.join(", "))
}

/// Tick ranges of the sections `indices` (ascending); adjacent sections merge.
fn merged(sections: &[Section], indices: &[usize]) -> Vec<(u32, u32)> {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for &i in indices {
        let section = sections[i];
        match ranges.last_mut() {
            Some(last) if last.1 == section.from_tick => last.1 = section.to_tick,
            _ => ranges.push((section.from_tick, section.to_tick)),
        }
    }
    ranges
}

/// Track sound where the tracks have clips: silent (`Error`) or nearly inaudible (`Warning`). Frozen tracks are
/// skipped: their audio copy plays their sound.
pub(crate) fn track_sound(project: &Project, report: &LevelReport) -> Vec<Finding> {
    let map = &project.tempo_map;
    let mut findings = Vec::new();
    for target in &report.targets {
        let Some(track) = project.track(target.id) else { continue };
        if track.instrument.as_ref().is_some_and(|spec| spec.frozen.is_some()) {
            continue;
        }
        let with_clips: Vec<usize> = (0..report.sections.len())
            .filter(|&i| {
                let s = report.sections[i];
                project.clips.iter().any(|c| c.track == track.id && c.overlaps(u64::from(s.from_tick), u64::from(s.to_tick)))
            })
            .collect();
        if with_clips.is_empty() {
            continue;
        }
        let ranges = merged(&report.sections, &with_clips);
        let loudest = with_clips
            .iter()
            .filter_map(|&i| target.sections.get(i).copied().flatten().map(|level| (i, level.rms_db)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let mut found = match loudest {
            None => finding(
                Severity::Error,
                "silentTrack",
                format!(
                    "Track \"{}\" is silent although it has clips ({}). Check the notes in the clips, the instrument's \
                     volume and filter, the track fader and inserts.",
                    track.name,
                    bars_text(map, &ranges)
                ),
            ),
            Some((i, rms)) if rms < QUIET_TRACK_RMS_DB => finding(
                Severity::Warning,
                "quietTrack",
                format!(
                    "Track \"{}\" is nearly inaudible: even its loudest bar with clips ({}) is {rms} dBFS RMS (threshold \
                     {QUIET_TRACK_RMS_DB}). Check the instrument's volume and filter, the fader and inserts.",
                    track.name,
                    bar(map, report.sections[i].from_tick)
                ),
            ),
            Some(_) => continue,
        };
        found.track = Some(track.id);
        found.ranges = ranges;
        findings.push(found);
    }
    findings
}

/// The master: silent throughout, clipping (with bars) or too quiet.
pub(crate) fn master_sound(project: &Project, report: &LevelReport) -> Vec<Finding> {
    let map = &project.tempo_map;
    let Some(master) = report.targets.first() else { return Vec::new() };
    let whole = [(report.from_tick, report.to_tick)];
    if master.sections.iter().all(Option::is_none) {
        let mut silent = finding(
            Severity::Error,
            "silentMaster",
            format!(
                "The master is silent ({}): check mute/solo, the outputs of tracks and buses (they must reach the master) \
                 and the master volume.",
                bars_text(map, &whole)
            ),
        );
        silent.node = Some(MASTER);
        silent.ranges = whole.to_vec();
        return vec![silent];
    }
    let mut findings = Vec::new();
    let hot: Vec<usize> = (0..master.sections.len()).filter(|&i| master.sections[i].is_some_and(|l| l.peak_db >= CLIP_PEAK_DB)).collect();
    if !hot.is_empty() {
        let peak = hot.iter().filter_map(|&i| master.sections[i]).map(|l| l.peak_db).fold(f32::MIN, f32::max);
        let ranges = merged(&report.sections, &hot);
        let mut clipping = finding(
            Severity::Error,
            "masterClipping",
            format!(
                "The master clips: peak {peak:+} dBFS (threshold {CLIP_PEAK_DB}) in {}. Lower the track or master volume \
                 or put a limiter before the master (graph.insertEffectBefore).",
                bars_text(map, &ranges)
            ),
        );
        clipping.node = Some(MASTER);
        clipping.ranges = ranges;
        findings.push(clipping);
    }
    if let Some(lufs) = master.loudness_lufs.filter(|&l| l < QUIET_MASTER_LUFS) {
        let mut quiet = finding(
            Severity::Warning,
            "quietMaster",
            format!("The mix is very quiet: {lufs} LUFS (threshold {QUIET_MASTER_LUFS}; finished tracks are usually -16..-8 LUFS)."),
        );
        quiet.node = Some(MASTER);
        findings.push(quiet);
    }
    findings
}

/// A node caption for messages: `Track "Bass"`, `Bus "Drums"`.
fn node_text(kind: &NodeKind, name: &str) -> String {
    match kind {
        NodeKind::Bus { .. } => format!("Bus \"{name}\""),
        NodeKind::Effect { .. } => format!("Effect \"{name}\""),
        NodeKind::Master { .. } => "The master".to_string(),
        NodeKind::Instrument { .. } | NodeKind::Audio { .. } => format!("Track \"{name}\""),
    }
}

/// Muted (`Warning`) and soloed (`Info`) tracks and buses.
pub(crate) fn mute_and_solo(project: &Project) -> Vec<Finding> {
    let mut findings = Vec::new();
    for node in &project.graph.nodes {
        let Some(strip) = node.kind.strip() else { continue };
        let who = node_text(&node.kind, &node.name);
        let bus = matches!(node.kind, NodeKind::Bus { .. });
        let mut push = |severity, code, message: String| {
            let mut found = finding(severity, code, message);
            found.node = Some(node.id);
            found.track = node.kind.track();
            findings.push(found);
        };
        if strip.mute {
            let code = if bus { "mutedBus" } else { "mutedTrack" };
            push(Severity::Warning, code, format!("{who} is muted: it is not heard in the mix."));
        }
        if strip.solo {
            let code = if bus { "soloedBus" } else { "soloedTrack" };
            push(Severity::Info, code, format!("{who} is soloed: while solo is on, everything without solo is not heard."));
        }
    }
    findings
}

/// A level or cutoff parameter (by name or label).
fn is_level_param(info: &ParamInfo) -> bool {
    let text = format!("{} {}", info.name, info.label).to_lowercase();
    info.kind == ParamKind::Number && LEVEL_WORDS.iter().any(|word| text.contains(word))
}

/// Whether the value sits in the first 1 % of the range (a wide range from a positive minimum on a log scale).
fn near_min(info: &ParamInfo, value: f32) -> bool {
    if info.max <= info.min {
        return false;
    }
    let position = if info.min > 0.0 && info.max / info.min >= 10.0 {
        (value.max(info.min) / info.min).ln() / (info.max / info.min).ln()
    } else {
        (value - info.min) / (info.max - info.min)
    };
    position <= NEAR_MIN
}

/// An instrument volume or cutoff almost at its minimum: the sound may disappear.
pub(crate) fn suspicious_params(project: &Project, kinds: &Kinds) -> Vec<Finding> {
    let mut findings = Vec::new();
    for track in &project.tracks {
        let Some(spec) = track.sounding_instrument() else { continue };
        let Some(infos) = kinds.instruments.get(&spec.kind) else { continue };
        // A parameter at its minimum by default (an LFO or envelope depth) is off there, not lost.
        for info in infos.iter().filter(|info| is_level_param(info) && !near_min(info, info.default)) {
            let value = spec.params.get(info.name).copied().unwrap_or(info.default);
            if near_min(info, value) {
                let mut found = finding(
                    Severity::Warning,
                    "suspiciousParam",
                    format!(
                        "On track \"{}\" the instrument parameter \"{}\" ({}) is almost at its minimum: {value} in the range \
                         {}..{}. The sound may disappear; check the value (tracks.setParam, units as in catalog.get).",
                        track.name, info.label, info.name, info.min, info.max
                    ),
                );
                found.track = Some(track.id);
                findings.push(found);
            }
        }
    }
    findings
}

/// Tracks and buses whose sound does not reach the master (`Warning`; a node with only sidechain outputs is fine), and
/// empty buses (`Info`).
pub(crate) fn routing(project: &Project) -> Vec<Finding> {
    let reaching = project.graph.reaching_master();
    let mut findings = Vec::new();
    for node in project.graph.nodes.iter().filter(|n| n.kind.strip().is_some()) {
        let who = node_text(&node.kind, &node.name);
        let keys_sidechain = project.graph.edges.iter().any(|e| e.from == node.id && e.sidechain);
        if !reaching.contains(&node.id) && !keys_sidechain {
            let mut found = finding(
                Severity::Warning,
                "notRouted",
                format!("{who}: the sound does not reach the master. Connect the output to a bus or the master (graph.setOutput)."),
            );
            found.node = Some(node.id);
            found.track = node.kind.track();
            findings.push(found);
        }
        let fed = project.graph.edges.iter().any(|e| e.to == node.id && !e.sidechain);
        if matches!(node.kind, NodeKind::Bus { .. }) && !fed {
            let mut found = finding(Severity::Info, "emptyBus", format!("{who} is empty: nothing feeds it."));
            found.node = Some(node.id);
            findings.push(found);
        }
    }
    findings
}

/// The lane target's range and caption; `None` when the target is unknown (not checked).
fn lane_range(project: &Project, target: &ParamTarget, kinds: &Kinds) -> Option<(f32, f32, String)> {
    let name_of = |id: NodeId| project.graph.node(id).map_or_else(|| format!("#{id}"), |n| n.name.clone());
    let param = |infos: Option<&Vec<ParamInfo>>, param: &str| infos?.iter().find(|i| i.name == param).map(|i| (i.min, i.max));
    match target {
        ParamTarget::NodeVolume { node } => Some((0.0, MAX_LEVEL, format!("volume of \"{}\"", name_of(*node)))),
        ParamTarget::NodePan { node } => Some((-1.0, 1.0, format!("pan of \"{}\"", name_of(*node)))),
        ParamTarget::NodeWidth { node } => Some((0.0, MAX_WIDTH, format!("width of \"{}\"", name_of(*node)))),
        ParamTarget::StageDepth { node } => Some((-1.0, 1.0, format!("stage depth of \"{}\"", name_of(*node)))),
        ParamTarget::StageHeight { node } => Some((-1.0, 1.0, format!("stage height of \"{}\"", name_of(*node)))),
        ParamTarget::EdgeLevel { from, to, .. } => {
            Some((0.0, MAX_LEVEL, format!("edge level \"{}\" → \"{}\"", name_of(*from), name_of(*to))))
        }
        ParamTarget::InstrumentParam { track, param: name } => {
            let track = project.track(*track)?;
            let (min, max) = param(kinds.instruments.get(&track.instrument.as_ref()?.kind), name)?;
            Some((min, max, format!("instrument parameter \"{name}\" of track \"{}\"", track.name)))
        }
        ParamTarget::EffectParam { node, param: name } => {
            let NodeKind::Effect { effect, .. } = &project.graph.node(*node)?.kind else { return None };
            let (min, max) = param(kinds.effects.get(effect), name)?;
            Some((min, max, format!("parameter \"{name}\" of effect \"{}\"", name_of(*node))))
        }
    }
}

/// Automation points outside the target's range (the engine clamps them; most likely the units are mixed up).
pub(crate) fn automation_ranges(project: &Project, kinds: &Kinds) -> Vec<Finding> {
    let mut findings = Vec::new();
    for lane in &project.automation {
        let Some((min, max, what)) = lane_range(project, &lane.target, kinds) else { continue };
        // Song ticks of the out-of-range points in each clip's first pattern repeat.
        let outside: Vec<(u32, f32)> = lane
            .clips
            .iter()
            .flat_map(|clip| {
                clip.points
                    .iter()
                    .filter(|p| p.value < min || p.value > max)
                    .map(move |p| (clip.start_tick.saturating_add(p.tick.min(clip.length_ticks)), p.value))
            })
            .collect();
        let (Some(first), Some(last)) = (outside.first(), outside.last()) else { continue };
        let mut found = finding(
            Severity::Warning,
            "automationOutOfRange",
            format!(
                "Automation: {what} leaves the range {min}..{max} at {} points, for example {} at bar {}. The values \
                 are clamped; check the units (catalog.get).",
                outside.len(),
                first.1,
                bar(&project.tempo_map, first.0)
            ),
        );
        found.lane = Some(lane.id);
        found.ranges = vec![(first.0, last.0.saturating_add(1))];
        findings.push(found);
    }
    findings
}

/// Disabled and missing plugins (`Error`: such an instrument or effect does not sound) and bypassed effects (`Info`).
pub(crate) fn plugins(project: &Project, kinds: &Kinds) -> Vec<Finding> {
    let problem = |kind: &str| {
        if kinds.disabled.contains(kind) {
            Some(("disabledPlugin", "is disabled in the plugin list (plugins.setDisabled)"))
        } else if kinds.missing.contains(kind) {
            Some(("missingPlugin", "is not in the catalog (the plugin is not on this machine)"))
        } else {
            None
        }
    };
    let mut findings = Vec::new();
    for track in &project.tracks {
        let Some(spec) = track.sounding_instrument() else { continue };
        if let Some((code, why)) = problem(&spec.kind) {
            let mut found = finding(
                Severity::Error,
                code,
                format!(
                    "Instrument \"{}\" of track \"{}\" {why}: the track is silent. Change the instrument (tracks.setInstrument).",
                    spec.kind, track.name
                ),
            );
            found.track = Some(track.id);
            findings.push(found);
        }
    }
    for node in &project.graph.nodes {
        let NodeKind::Effect { effect, bypass, .. } = &node.kind else { continue };
        let mut push = |severity, code, message: String| {
            let mut found = finding(severity, code, message);
            found.node = Some(node.id);
            findings.push(found);
        };
        if let Some((code, why)) = problem(effect) {
            push(Severity::Error, code, format!("Effect \"{}\" ({effect}) {why}: it does not work.", node.name));
        } else if *bypass {
            push(Severity::Info, "bypassedEffect", format!("Effect \"{}\" is bypassed: the sound goes around it.", node.name));
        }
    }
    findings
}
