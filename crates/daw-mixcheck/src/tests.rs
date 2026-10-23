//! Check rules on synthetic projects and levels (no render), and how `MixChecker` gathers findings with test doubles.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use daw_contracts::codes::{graph, tracks};
use daw_contracts::*;

use crate::MixChecker;
use crate::rules::{self, Kinds};

fn clip(id: ClipId, track: TrackId, start: u32, length: u32) -> Clip {
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

fn info(name: &'static str, label: &'static str, min: f32, max: f32) -> ParamInfo {
    ParamInfo { name, label: L10n::same(label), min, max, default: max, kind: ParamKind::Number, choices: &[], about: L10n::EMPTY }
}

/// "Bass" (track 1, node 10) and "Lead" (track 2, node 11) on the synth → master; clips: bass in bars 1–2, lead in bar 3.
fn project() -> Project {
    let mut project = Project::empty();
    for (id, name, node) in [(1, "Bass", 10), (2, "Lead", 11)] {
        project.tracks.push(Track::new(id, name, Some(InstrumentSpec::new("synth", Params::new())), 60));
        project.graph.nodes.push(Node {
            id: node,
            name: name.into(),
            kind: NodeKind::Instrument { track: id, strip: Strip::default() },
            x: 0.0,
            y: 0.0,
        });
        project.graph.edges.push(Edge { from: node, to: MASTER, level: 1.0, sidechain: false, pre_fader: false });
    }
    project.clips = vec![clip(100, 1, 0, BAR_TICKS * 2), clip(101, 2, BAR_TICKS * 2, BAR_TICKS)];
    project
}

fn kinds() -> Kinds {
    let synth = vec![
        info("gain", "Gain", 0.0, 1.0),
        info("cutoff", "Cutoff, Hz", 40.0, 20_000.0),
        info("pitch", "Pitch", -24.0, 24.0),
        ParamInfo { kind: ParamKind::Switch, ..info("volume_lock", "Volume lock", 0.0, 1.0) },
        ParamInfo { default: 0.0, ..info("lfo_volume", "LFO to volume, %", 0.0, 100.0) },
    ];
    Kinds {
        instruments: HashMap::from([("synth".to_string(), synth)]),
        effects: HashMap::from([("eq".to_string(), vec![info("gain", "Gain, dB", -24.0, 24.0)])]),
        disabled: BTreeSet::new(),
        missing: BTreeSet::new(),
    }
}

#[allow(clippy::unnecessary_wraps, reason = "section levels in a report are `Option`: this keeps the test tables short")]
fn level(peak_db: f32, rms_db: f32) -> Option<Level> {
    Some(Level { peak_db, rms_db })
}

/// A per-bar report over 4 bars.
fn report(targets: Vec<(u32, Vec<Option<Level>>)>) -> LevelReport {
    let sections = (0..4).map(|i| Section { bar: i + 1, from_tick: i * BAR_TICKS, to_tick: (i + 1) * BAR_TICKS }).collect();
    let targets = targets
        .into_iter()
        .map(|(id, sections)| TargetLevels {
            id,
            name: String::new(),
            level: sections.iter().flatten().next().copied(),
            loudness_lufs: Some(-14.0),
            sections,
        })
        .collect();
    LevelReport { from_tick: 0, to_tick: BAR_TICKS * 4, sections, targets }
}

fn codes(findings: &[Finding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.code).collect()
}

#[test]
fn silent_track_with_clips_is_an_error_with_its_bars() {
    let levels = report(vec![(1, vec![None, None, None, level(-6.0, -12.0)]), (2, vec![None, None, level(-3.0, -10.0), None])]);
    let findings = rules::track_sound(&project(), &levels);
    assert_eq!(codes(&findings), ["silentTrack"]);
    assert_eq!((findings[0].severity, findings[0].track), (Severity::Error, Some(1)));
    assert_eq!(findings[0].ranges, [(0, BAR_TICKS * 2)], "adjacent bars are merged");
    assert!(findings[0].message.contains("\"Bass\"") && findings[0].message.contains("bars 1–2"), "{}", findings[0].message);
}

#[test]
fn quiet_track_is_a_warning_and_loud_enough_is_fine() {
    let levels =
        report(vec![(1, vec![level(-30.0, -50.0), level(-31.0, -46.0), None, None]), (2, vec![None, None, level(-20.0, -45.0), None])]);
    let findings = rules::track_sound(&project(), &levels);
    assert_eq!(codes(&findings), ["quietTrack"], "exactly -45 is not quiet any more");
    assert_eq!(findings[0].track, Some(1));
    assert!(findings[0].message.contains("-46"), "{}", findings[0].message);
}

#[test]
fn frozen_and_clipless_tracks_are_not_judged_by_sound() {
    let mut project = project();
    project.tracks[0].instrument.as_mut().unwrap().frozen = Some(9);
    project.clips.retain(|c| c.track != 2);
    let levels = report(vec![(1, vec![None; 4]), (2, vec![None; 4])]);
    assert!(rules::track_sound(&project, &levels).is_empty());
}

#[test]
fn master_clipping_is_reported_with_tick_ranges() {
    let levels = report(vec![(MASTER, vec![level(-0.1, -8.0), level(-3.0, -9.0), level(2.5, -4.0), level(0.4, -5.0)])]);
    let findings = rules::master_sound(&project(), &levels);
    assert_eq!(codes(&findings), ["masterClipping"]);
    assert_eq!(findings[0].ranges, [(0, BAR_TICKS), (BAR_TICKS * 2, BAR_TICKS * 4)]);
    assert!(findings[0].message.contains("+2.5") && findings[0].message.contains("bars 1, 3–4"), "{}", findings[0].message);
}

#[test]
fn silent_and_quiet_master_are_reported() {
    let silent = rules::master_sound(&project(), &report(vec![(MASTER, vec![None; 4])]));
    assert_eq!(codes(&silent), ["silentMaster"]);
    let mut quiet = report(vec![(MASTER, vec![level(-20.0, -35.0); 4])]);
    quiet.targets[0].loudness_lufs = Some(-33.0);
    assert_eq!(codes(&rules::master_sound(&project(), &quiet)), ["quietMaster"]);
    quiet.targets[0].loudness_lufs = Some(-14.0);
    assert!(rules::master_sound(&project(), &quiet).is_empty());
}

#[test]
fn muted_and_soloed_nodes_are_reported() {
    let mut project = project();
    project.graph.node_mut(10).unwrap().kind.strip_mut().unwrap().mute = true;
    project.graph.node_mut(11).unwrap().kind.strip_mut().unwrap().solo = true;
    let findings = rules::mute_and_solo(&project);
    assert_eq!(codes(&findings), ["mutedTrack", "soloedTrack"]);
    assert_eq!((findings[0].track, findings[0].node, findings[0].severity), (Some(1), Some(10), Severity::Warning));
    assert_eq!(findings[1].severity, Severity::Info);
}

#[test]
fn level_and_cutoff_params_at_their_minimum_are_suspicious() {
    let mut project = project();
    let params = &mut project.tracks[0].instrument.as_mut().unwrap().params;
    params.extend([
        ("gain".to_string(), 0.0032),
        ("cutoff".to_string(), 41.0),
        ("pitch".to_string(), -24.0),
        ("volume_lock".to_string(), 0.0),
        ("lfo_volume".to_string(), 0.0),
    ]);
    project.tracks[1].instrument.as_mut().unwrap().params.extend([("gain".to_string(), 0.2), ("cutoff".to_string(), 150.0)]);
    let findings = rules::suspicious_params(&project, &kinds());
    assert_eq!(codes(&findings), ["suspiciousParam", "suspiciousParam"]);
    assert!(findings.iter().all(|f| f.track == Some(1)));
    assert!(findings[1].message.contains("Cutoff, Hz"), "{}", findings[1].message);
    project.tracks[0].instrument.as_mut().unwrap().params.clear();
    assert!(rules::suspicious_params(&project, &kinds()).is_empty());
}

#[test]
fn unrouted_sources_and_empty_buses_are_reported_but_sidechain_keys_are_fine() {
    let mut project = project();
    project.graph.edges.retain(|e| e.from != 11);
    project.graph.nodes.push(Node { id: 30, name: "Bus".into(), kind: NodeKind::Bus { strip: Strip::default() }, x: 0.0, y: 0.0 });
    project.graph.edges.push(Edge { from: 30, to: MASTER, level: 1.0, sidechain: false, pre_fader: false });
    let findings = rules::routing(&project);
    assert_eq!(codes(&findings), ["notRouted", "emptyBus"]);
    assert_eq!((findings[0].track, findings[1].node), (Some(2), Some(30)));

    project.graph.nodes.push(Node {
        id: 20,
        name: "Comp".into(),
        kind: NodeKind::Effect { effect: "eq".into(), bypass: false, params: Params::new(), state: None },
        x: 0.0,
        y: 0.0,
    });
    project.graph.edges.extend([
        Edge { from: 10, to: 20, level: 1.0, sidechain: false, pre_fader: false },
        Edge { from: 20, to: 30, level: 1.0, sidechain: false, pre_fader: false },
        Edge { from: 11, to: 20, level: 1.0, sidechain: true, pre_fader: false },
    ]);
    assert!(rules::routing(&project).is_empty(), "{:?}", rules::routing(&project));
}

#[test]
fn automation_outside_the_target_range_is_reported() {
    let mut project = project();
    let points = |values: &[f32]| values.iter().enumerate().map(|(i, &value)| AutoPoint::new(i as u32 * BAR_TICKS, value)).collect();
    project.automation = vec![
        AutomationLane {
            id: 1,
            target: ParamTarget::NodeVolume { node: 10 },
            clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, points(&[0.5, 1.0]))],
        },
        AutomationLane {
            id: 2,
            target: ParamTarget::NodePan { node: 10 },
            clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, points(&[0.0, -2.0]))],
        },
        AutomationLane {
            id: 3,
            target: ParamTarget::InstrumentParam { track: 2, param: "cutoff".into() },
            clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, points(&[0.1, 0.5, 30_000.0]))],
        },
        AutomationLane {
            id: 4,
            target: ParamTarget::InstrumentParam { track: 2, param: "unknown".into() },
            clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, points(&[99.0]))],
        },
    ];
    let findings = rules::automation_ranges(&project, &kinds());
    assert_eq!(findings.iter().map(|f| f.lane).collect::<Vec<_>>(), [Some(2), Some(3)]);
    assert_eq!(findings[1].ranges, [(0, BAR_TICKS * 2 + 1)], "from the first to the last point out of range");
    assert!(findings[1].message.contains("\"Lead\"") && findings[1].message.contains("at 3 points"), "{}", findings[1].message);
}

#[test]
fn disabled_and_missing_plugins_are_errors_and_bypass_is_info() {
    let mut project = project();
    project.tracks[1].instrument.as_mut().unwrap().kind = "vst3:gone".into();
    let effect = |id, effect: &str, bypass| Node {
        id,
        name: format!("e{id}"),
        kind: NodeKind::Effect { effect: effect.into(), bypass, params: Params::new(), state: None },
        x: 0.0,
        y: 0.0,
    };
    project.graph.nodes.extend([effect(20, "vst3:off", false), effect(21, "eq", true)]);
    let mut kinds = kinds();
    kinds.disabled.insert("vst3:off".into());
    kinds.missing.insert("vst3:gone".into());
    let findings = rules::plugins(&project, &kinds);
    assert_eq!(codes(&findings), ["missingPlugin", "disabledPlugin", "bypassedEffect"]);
    assert_eq!((findings[0].track, findings[1].node, findings[2].severity), (Some(2), Some(20), Severity::Info));
}

/// An analysis double: answers with prepared reports (or an error) and remembers the requests.
struct FakeAnalysis {
    result: Result<(LevelReport, LevelReport), ContractError>,
    asked: Mutex<Vec<LevelRequest>>,
}

impl MixAnalysis for FakeAnalysis {
    fn levels(&self, _: &Project, request: &LevelRequest) -> Result<LevelReport, ContractError> {
        self.asked.lock().unwrap().push(*request);
        let (tracks, master) = self.result.clone()?;
        Ok(if request.per == LevelTargets::Track { tracks } else { master })
    }

    fn balance(&self, _: &Project, _: Option<u32>, _: Option<u32>) -> Result<BalanceReport, ContractError> {
        unreachable!("the mix check does not ask for the balance")
    }
}

struct Catalog;

impl InstrumentCatalog for Catalog {
    fn kinds(&self) -> Vec<KindInfo> {
        vec![KindInfo { kind: "synth", label: L10n::same("Synth"), params: vec![info("gain", "Gain", 0.0, 1.0)], about: L10n::EMPTY }]
    }
    fn defaults(&self, kind: &str) -> Result<Params, ContractError> {
        if kind == "synth" { Ok(Params::new()) } else { Err(tracks::UNKNOWN_INSTRUMENT.with("kind", kind)) }
    }
    fn create(&self, kind: &str, _: &Params, _: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        Err(tracks::UNKNOWN_INSTRUMENT.with("kind", kind))
    }
    fn rebuild_on(&self, _: &str) -> Vec<&'static str> {
        Vec::new()
    }
}

impl EffectCatalog for Catalog {
    fn kinds(&self) -> Vec<KindInfo> {
        Vec::new()
    }
    fn defaults(&self, kind: &str) -> Result<Params, ContractError> {
        Err(graph::UNKNOWN_EFFECT.with("kind", kind))
    }
    fn create(&self, kind: &str, _: &Params, _: Option<&PluginState>) -> Result<Box<dyn Effect>, ContractError> {
        Err(graph::UNKNOWN_EFFECT.with("kind", kind))
    }
}

impl PluginScanner for Catalog {
    fn scan(&self) -> Result<ScanReport, ContractError> {
        Ok(ScanReport::default())
    }
    fn installed(&self) -> Vec<InstalledPlugin> {
        Vec::new()
    }
    fn folders(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn set_disabled(&self, _: &str, _: bool) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Default)]
struct MemoryLog(Mutex<Vec<LogEntry>>);

impl AppLog for MemoryLog {
    fn record(&self, entry: LogEntry) {
        self.0.lock().unwrap().push(entry);
    }
    fn recent(&self, _: usize) -> Vec<LogEntry> {
        self.0.lock().unwrap().clone()
    }
    fn file(&self) -> Option<PathBuf> {
        None
    }
}

fn checker(result: Result<(LevelReport, LevelReport), ContractError>) -> (MixChecker, Arc<FakeAnalysis>, Arc<MemoryLog>) {
    let analysis = Arc::new(FakeAnalysis { result, asked: Mutex::default() });
    let log = Arc::new(MemoryLog::default());
    let checker = MixChecker::new(analysis.clone(), Arc::new(Catalog), Arc::new(Catalog), Arc::new(Catalog), log.clone());
    (checker, analysis, log)
}

#[test]
fn checker_listens_bar_by_bar_and_sorts_findings_by_severity() {
    let tracks = report(vec![(1, vec![None; 4]), (2, vec![None, None, level(-6.0, -12.0), None])]);
    let master = report(vec![(MASTER, vec![level(1.0, -6.0), level(-6.0, -12.0), level(-6.0, -12.0), None])]);
    let (checker, analysis, _) = checker(Ok((tracks, master)));
    let mut project = project();
    project.graph.node_mut(11).unwrap().kind.strip_mut().unwrap().solo = true;
    let findings = checker.check(&project, Some(0), None);
    assert_eq!(codes(&findings), ["silentTrack", "masterClipping", "soloedTrack"]);
    let asked = analysis.asked.lock().unwrap().clone();
    assert!(asked.iter().all(|r| r.resolution_bars == Some(1) && r.from_tick == Some(0) && r.to_tick.is_none()));
    assert_eq!(asked.iter().map(|r| r.per).collect::<Vec<_>>(), [LevelTargets::Track, LevelTargets::Master]);
}

#[test]
fn render_failure_is_a_finding_and_a_log_entry_and_empty_projects_are_not_rendered() {
    let (checker, analysis, log) = checker(Err(tracks::UNKNOWN_INSTRUMENT.with("kind", "vst3:gone")));
    let findings = checker.check(&project(), None, None);
    assert_eq!(codes(&findings), ["renderFailed"]);
    assert!(findings[0].message.contains("vst3:gone"));
    assert_eq!(log.0.lock().unwrap().len(), 1);

    let mut empty = project();
    empty.clips.clear();
    analysis.asked.lock().unwrap().clear();
    assert!(checker.check(&empty, None, None).is_empty());
    assert!(analysis.asked.lock().unwrap().is_empty(), "without clips no sound is rendered");
}
