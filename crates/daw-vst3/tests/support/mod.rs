//! Shared by the integration checks: fake log, catalogs, cache and project, and a "world" of plugins actually found.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "helper code of integration tests"
)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use daw_contracts::codes::{general, graph, tracks};
use daw_contracts::{
    AppLog, ContractError, EditorTarget, EffectFactory, EffectGroups, Graph, GraphControl, HistoryControl, HistoryMark, InstrumentFactory,
    InstrumentGroups, InstrumentSpec, LiveNoteSink, LogEntry, MonitorMode, NewTrack, Node, NodeId, NodeKind, Params, PluginCacheStore,
    PluginScanner, PluginState, PluginStates, Polarity, Track, TrackControl, TrackId,
};
use daw_vst3::{Vst3Editors, Vst3Plugins};

pub const SURGE: &str = "/Library/Audio/Plug-Ins/VST3/Surge XT.vst3";
/// The dev-only fixture plugins (`test-plugins/`, not shipped): built by `scripts/build-test-plugins.sh`.
pub const DISTORTION: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-plugins/distortion/target/bundled/Distortion.vst3");
pub const SURGE_FX: &str = "/Library/Audio/Plug-Ins/VST3/Surge XT Effects.vst3";
pub const SYNTH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-plugins/synth/target/bundled/Synth.vst3");

/// Live notes are not needed by windows in these checks.
struct NoLiveNotes;

impl LiveNoteSink for NoLiveNotes {
    fn note_on(&self, _: TrackId, _: u8, _: f32) {}
    fn note_off(&self, _: TrackId, _: u8) {}
    fn controller(&self, _: TrackId, _: daw_contracts::ControllerKind, _: f32) {}
}

#[derive(Default)]
pub struct MemoryLog(pub Mutex<Vec<LogEntry>>);

impl AppLog for MemoryLog {
    fn record(&self, entry: LogEntry) {
        self.0.lock().unwrap().push(entry);
    }
    fn recent(&self, _limit: usize) -> Vec<LogEntry> {
        self.0.lock().unwrap().clone()
    }
    fn file(&self) -> Option<PathBuf> {
        None
    }
}

fn no(what: &str) -> ContractError {
    general::INTERNAL.with("details", format!("the test fake does not support {what}"))
}

#[derive(Default)]
pub struct Graphs(pub Mutex<Graph>);

impl GraphControl for Graphs {
    fn graph(&self) -> Graph {
        self.0.lock().unwrap().clone()
    }
    fn add_effect(&self, _: &str, _: f32, _: f32) -> Result<NodeId, ContractError> {
        Err(no("add_effect"))
    }
    fn add_bus(&self, _: &str, _: f32, _: f32) -> Result<NodeId, ContractError> {
        Err(no("add_bus"))
    }
    fn remove_node(&self, _: NodeId) -> Result<(), ContractError> {
        Err(no("remove_node"))
    }
    fn rename_node(&self, _: NodeId, _: &str) -> Result<(), ContractError> {
        Err(no("rename_node"))
    }
    fn move_node(&self, _: NodeId, _: f32, _: f32) -> Result<(), ContractError> {
        Err(no("move_node"))
    }
    fn connect(&self, _: NodeId, _: NodeId, _: f32, _: bool) -> Result<(), ContractError> {
        Err(no("connect"))
    }
    fn disconnect(&self, _: NodeId, _: NodeId, _: bool) -> Result<(), ContractError> {
        Err(no("disconnect"))
    }
    fn set_volume(&self, _: NodeId, _: f32) -> Result<(), ContractError> {
        Err(no("set_volume"))
    }
    fn set_pan(&self, _: NodeId, _: f32) -> Result<(), ContractError> {
        Err(no("set_pan"))
    }
    fn set_mute(&self, _: NodeId, _: bool) -> Result<(), ContractError> {
        Err(no("set_mute"))
    }
    fn set_solo(&self, _: NodeId, _: bool) -> Result<(), ContractError> {
        Err(no("set_solo"))
    }
    fn set_polarity(&self, _: NodeId, _: Polarity) -> Result<(), ContractError> {
        Err(no("set_polarity"))
    }
    fn set_width(&self, _: NodeId, _: f32) -> Result<(), ContractError> {
        Err(no("set_width"))
    }
    fn set_mono(&self, _: NodeId, _: bool) -> Result<(), ContractError> {
        Err(no("set_mono"))
    }
    fn set_delay(&self, _: NodeId, _: f32) -> Result<(), ContractError> {
        Err(no("set_delay"))
    }
    fn set_effect_param(&self, id: NodeId, name: &str, value: f32) -> Result<(), ContractError> {
        let mut graph = self.0.lock().unwrap();
        match graph.node_mut(id).map(|n| &mut n.kind) {
            Some(NodeKind::Effect { params, .. }) => {
                params.insert(name.to_string(), value);
                Ok(())
            }
            _ => Err(graph::NODE_NOT_EFFECT.with("id", id)),
        }
    }
    fn set_effect_bypass(&self, _: NodeId, _: bool) -> Result<(), ContractError> {
        Err(no("set_effect_bypass"))
    }
    fn insert_effect(&self, _: NodeId, _: &str, _: f32, _: f32) -> Result<NodeId, ContractError> {
        Err(no("insert_effect"))
    }
    fn insert_effect_before(&self, _: NodeId, _: &str, _: f32, _: f32) -> Result<NodeId, ContractError> {
        Err(no("insert_effect_before"))
    }
    fn remove_effect_keep_path(&self, _: NodeId) -> Result<(), ContractError> {
        Err(no("remove_effect_keep_path"))
    }
    fn set_output(&self, _: NodeId, _: NodeId) -> Result<(), ContractError> {
        Err(no("set_output"))
    }
    fn set_edge_pre_fader(&self, _: NodeId, _: NodeId, _: bool) -> Result<(), ContractError> {
        Err(no("set_edge_pre_fader"))
    }
    fn set_sidechain(&self, _: NodeId, _: Option<NodeId>) -> Result<(), ContractError> {
        Err(no("set_sidechain"))
    }
}

#[derive(Default)]
pub struct Tracks(pub Mutex<Vec<Track>>);

impl TrackControl for Tracks {
    fn tracks(&self) -> Vec<Track> {
        self.0.lock().unwrap().clone()
    }
    fn add_tracks(&self, _: Vec<NewTrack>) -> Result<Vec<TrackId>, ContractError> {
        Err(no("add_tracks"))
    }
    fn add_track(&self, _: &str, _: Option<&str>, _: u8) -> Result<TrackId, ContractError> {
        Err(no("add_track"))
    }
    fn remove_track(&self, id: TrackId) -> Result<(), ContractError> {
        self.0.lock().unwrap().retain(|t| t.id != id);
        Ok(())
    }
    fn rename_track(&self, _: TrackId, _: &str) -> Result<(), ContractError> {
        Err(no("rename_track"))
    }
    fn set_instrument(&self, _: TrackId, _: &str, _: daw_contracts::Params, _: Option<&str>) -> Result<(), ContractError> {
        Err(no("set_instrument"))
    }
    fn set_instrument_param(&self, id: TrackId, name: &str, value: f32) -> Result<(), ContractError> {
        let mut tracks = self.0.lock().unwrap();
        let spec = tracks
            .iter_mut()
            .find(|t| t.id == id)
            .and_then(|t| t.instrument.as_mut())
            .ok_or_else(|| tracks::NO_INSTRUMENT.with("id", id))?;
        spec.params.insert(name.to_string(), value);
        Ok(())
    }
    fn move_track(&self, _: TrackId, _: usize) -> Result<(), ContractError> {
        Err(no("move_track"))
    }
    fn set_monitor(&self, _: TrackId, _: MonitorMode) -> Result<(), ContractError> {
        Err(no("set_monitor"))
    }
    fn set_color(&self, _: TrackId, _: Option<&str>) -> Result<(), ContractError> {
        Err(no("set_color"))
    }
    fn set_height(&self, _: TrackId, _: Option<u16>) -> Result<(), ContractError> {
        Err(no("set_height"))
    }
}

/// Plugin states kept right in the fake graph and tracks; `writes` counts how many times windows wrote state.
pub struct States {
    pub graph: Arc<Graphs>,
    pub tracks: Arc<Tracks>,
    pub writes: Mutex<Vec<EditorTarget>>,
}

impl States {
    fn with_slot<R>(&self, target: EditorTarget, job: impl FnOnce(&mut Option<Arc<PluginState>>) -> R) -> Result<R, ContractError> {
        match target {
            EditorTarget::Effect { node } => {
                let mut graph = self.graph.0.lock().unwrap();
                match graph.node_mut(node).map(|n| &mut n.kind) {
                    Some(NodeKind::Effect { state, .. }) => Ok(job(state)),
                    _ => Err(graph::NODE_NOT_FOUND.with("id", node)),
                }
            }
            EditorTarget::Instrument { track } => {
                let mut tracks = self.tracks.0.lock().unwrap();
                match tracks.iter_mut().find(|t| t.id == track).and_then(|t| t.instrument.as_mut()) {
                    Some(spec) => Ok(job(&mut spec.state)),
                    None => Err(tracks::NOT_FOUND.with("id", track)),
                }
            }
        }
    }
}

impl PluginStates for States {
    fn get(&self, target: EditorTarget) -> Result<Option<Arc<PluginState>>, ContractError> {
        self.with_slot(target, |slot| slot.clone())
    }
    fn set(&self, target: EditorTarget, state: PluginState) -> Result<(), ContractError> {
        self.with_slot(target, |slot| *slot = Some(Arc::new(state)))?;
        self.writes.lock().unwrap().push(target);
        Ok(())
    }
}

#[derive(Default)]
pub struct History(pub Mutex<Vec<String>>);

impl HistoryControl for History {
    fn undo(&self) -> bool {
        false
    }
    fn redo(&self) -> bool {
        false
    }
    fn can_undo(&self) -> bool {
        false
    }
    fn can_redo(&self) -> bool {
        false
    }
    fn undo_label(&self) -> Option<String> {
        None
    }
    fn begin_group(&self, _: &str) {
        self.0.lock().unwrap().push("begin".into());
    }
    fn end_group(&self) -> bool {
        self.0.lock().unwrap().push("end".into());
        true
    }
    fn cancel_group(&self) {
        self.0.lock().unwrap().push("cancel".into());
    }
    fn mark_baseline(&self) {}
    fn checkpoint(&self) -> HistoryMark {
        0
    }
    fn squash(&self, _: HistoryMark, _: &str) -> Result<bool, ContractError> {
        Ok(false)
    }
}

#[derive(Default)]
pub struct Effects(pub Mutex<Vec<Box<dyn EffectFactory>>>);

impl EffectGroups for Effects {
    fn replace_group(&self, _: &str, factories: Vec<Box<dyn EffectFactory>>) {
        *self.0.lock().unwrap() = factories;
    }
}

#[derive(Default)]
pub struct Instruments(pub Mutex<Vec<Box<dyn InstrumentFactory>>>);

impl InstrumentGroups for Instruments {
    fn replace_group(&self, _: &str, factories: Vec<Box<dyn InstrumentFactory>>) {
        *self.0.lock().unwrap() = factories;
    }
}

#[derive(Default)]
pub struct Cache(Mutex<Option<String>>);

impl PluginCacheStore for Cache {
    fn load(&self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }
    fn save(&self, text: &str) -> Result<(), ContractError> {
        *self.0.lock().unwrap() = Some(text.to_string());
        Ok(())
    }
}

/// Real found plugins and windows over a fake project.
pub struct World {
    pub editors: Vst3Editors,
    pub log: Arc<MemoryLog>,
    pub graph: Arc<Graphs>,
    pub tracks: Arc<Tracks>,
    pub history: Arc<History>,
    pub states: Arc<States>,
    /// Kinds of found effects and instruments (`vst3:…`) with params: (kind, parameter names `p<id>`).
    pub effects: Vec<(&'static str, Vec<&'static str>)>,
    pub instruments: Vec<(&'static str, Vec<&'static str>)>,
    /// Factories of found instruments (in the same order as `instruments`): to check the sound too, not only the window.
    pub instrument_factories: Vec<Box<dyn InstrumentFactory>>,
    /// Factories of found effects (in the same order as `effects`).
    pub effect_factories: Vec<Box<dyn EffectFactory>>,
    dir: PathBuf,
}

impl Drop for World {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// A world of bundles (links in a separate folder). `None` when one of them is missing on the machine (the test is skipped).
pub fn world(tag: &str, bundles: &[&str]) -> Option<World> {
    let dir = std::env::temp_dir().join(format!("daw-vst3-editors-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for bundle in bundles {
        if !Path::new(bundle).exists() {
            eprintln!("SKIPPED: no {bundle} (fixtures: scripts/build-test-plugins.sh)");
            std::fs::remove_dir_all(&dir).ok();
            return None;
        }
        std::os::unix::fs::symlink(bundle, dir.join(Path::new(bundle).file_name().unwrap())).unwrap();
    }
    let log = Arc::new(MemoryLog::default());
    let (effects, instruments) = (Arc::new(Effects::default()), Arc::new(Instruments::default()));
    let plugins = Vst3Plugins::new(vec![dir.clone()], effects.clone(), instruments.clone(), Arc::new(Cache::default()), log.clone());
    plugins.scan().expect("the scan ran");
    let describe = |kind: &'static str, params: Vec<daw_contracts::ParamInfo>| (kind, params.into_iter().map(|p| p.name).collect());
    let effect_kinds = effects.0.lock().unwrap().iter().map(|f| describe(f.kind(), f.describe())).collect();
    let instrument_kinds = instruments.0.lock().unwrap().iter().map(|f| describe(f.kind(), f.describe())).collect();
    let instrument_factories = std::mem::take(&mut *instruments.0.lock().unwrap());
    let effect_factories = std::mem::take(&mut *effects.0.lock().unwrap());
    let (graph, tracks, history) = (Arc::new(Graphs::default()), Arc::new(Tracks::default()), Arc::new(History::default()));
    let states = Arc::new(States { graph: graph.clone(), tracks: tracks.clone(), writes: Mutex::default() });
    let editors = Vst3Editors::new(
        plugins.directory(),
        graph.clone(),
        tracks.clone(),
        history.clone(),
        states.clone(),
        log.clone(),
        Arc::new(NoLiveNotes),
    );
    Some(World {
        editors,
        log,
        graph,
        tracks,
        history,
        states,
        effects: effect_kinds,
        instruments: instrument_kinds,
        instrument_factories,
        effect_factories,
        dir,
    })
}

impl World {
    pub fn add_effect_node(&self, id: NodeId, kind: &str) {
        let kind = NodeKind::Effect { effect: kind.to_string(), bypass: false, params: Params::new(), state: None };
        self.graph.0.lock().unwrap().nodes.push(Node { id, name: "Effect".into(), kind, x: 0.0, y: 0.0 });
    }

    pub fn add_instrument_track(&self, id: TrackId, kind: &str) {
        let spec = InstrumentSpec::new(kind, Params::new());
        self.tracks.0.lock().unwrap().push(Track::new(id, "Synth", Some(spec), 60));
    }

    pub fn add_audio_track(&self, id: TrackId) {
        self.tracks.0.lock().unwrap().push(Track::new(id, "Audio", None, 60));
    }
}
