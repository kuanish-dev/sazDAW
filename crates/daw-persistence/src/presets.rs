//! `PresetFolder` (`Presets`): "remember a kind's settings under a name and put them on a track or an effect".
//!
//! A preset file is `presets/<kind>/<name>.json` in the app state folder: `{ kind, params, state? }` (plugin state as
//! base64, as in the project). A preset is applied through the project contracts (`PluginStates`, `TrackControl`,
//! `GraphControl`) as one undo group.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_contracts::codes::{graph, settings, tracks};
use daw_contracts::{
    ContractError, EditorTarget, GraphControl, HistoryControl, NodeKind, Params, PluginState, PluginStates, Presets, Project, ProjectView,
    TrackControl, check_preset_name, tr,
};
use serde::{Deserialize, Serialize};

use crate::atomic::write_json;

/// Contents of a preset file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PresetFile {
    kind: String,
    params: Params,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state: Option<PluginState>,
}

/// Who a preset is applied to its target through.
pub struct PresetTargets {
    pub view: Arc<dyn ProjectView>,
    pub tracks: Arc<dyn TrackControl>,
    pub graph: Arc<dyn GraphControl>,
    pub states: Arc<dyn PluginStates>,
    pub history: Arc<dyn HistoryControl>,
}

pub struct PresetFolder {
    dir: PathBuf,
    targets: PresetTargets,
}

/// The kind's folder name: `vst3:surge_xt` → `vst3_surge_xt` (colons and other signs are awkward in folder names).
fn kind_folder(kind: &str) -> String {
    kind.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect()
}

/// Kind, parameters and state of the target in the project.
fn read_target(project: &Project, target: EditorTarget) -> Result<PresetFile, ContractError> {
    match target {
        EditorTarget::Instrument { track } => {
            let track = project.track(track).ok_or_else(|| tracks::NOT_FOUND.with("id", track))?;
            let spec = track.instrument.as_ref().ok_or_else(|| tracks::NO_INSTRUMENT.with("id", track.id))?;
            Ok(PresetFile { kind: spec.kind.clone(), params: spec.params.clone(), state: spec.state.as_deref().cloned() })
        }
        EditorTarget::Effect { node } => {
            let node = project.graph.node(node).ok_or_else(|| graph::NODE_NOT_FOUND.with("id", node))?;
            match &node.kind {
                NodeKind::Effect { effect, params, state, .. } => {
                    Ok(PresetFile { kind: effect.clone(), params: params.clone(), state: state.as_deref().cloned() })
                }
                _ => Err(graph::NODE_NOT_EFFECT.with("id", node.id)),
            }
        }
    }
}

impl PresetFolder {
    /// Presets live in `<state_dir>/presets`; kind folders are created on the first save.
    pub fn new(state_dir: &Path, targets: PresetTargets) -> Self {
        Self { dir: state_dir.join("presets"), targets }
    }

    fn file(&self, kind: &str, name: &str) -> Result<PathBuf, ContractError> {
        Ok(self.dir.join(kind_folder(kind)).join(format!("{}.json", check_preset_name(name)?)))
    }

    fn load(&self, kind: &str, name: &str) -> Result<PresetFile, ContractError> {
        let file = self.file(kind, name)?;
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(settings::PRESET_NOT_FOUND.with("name", name.trim()));
            }
            Err(error) => return Err(settings::PRESET_NOT_READ.with("name", name.trim()).with("details", error)),
        };
        let preset: PresetFile =
            serde_json::from_str(&text).map_err(|e| settings::PRESET_DAMAGED.with("name", name.trim()).with("details", e))?;
        if preset.kind != kind {
            return Err(settings::PRESET_WRONG_KIND.with("name", name.trim()).with("preset_kind", &preset.kind).with("kind", kind));
        }
        Ok(preset)
    }

    /// The state, then the parameters on top; parameters the target does not have now are skipped.
    fn put(&self, target: EditorTarget, current: &Params, preset: PresetFile) -> Result<(), ContractError> {
        let targets = &self.targets;
        if let Some(state) = preset.state {
            targets.states.set(target, state)?;
        }
        for (name, value) in preset.params.into_iter().filter(|(name, _)| current.contains_key(name)) {
            match target {
                EditorTarget::Instrument { track } => targets.tracks.set_instrument_param(track, &name, value)?,
                EditorTarget::Effect { node } => targets.graph.set_effect_param(node, &name, value)?,
            }
        }
        Ok(())
    }
}

impl Presets for PresetFolder {
    fn list(&self, kind: &str) -> Result<Vec<String>, ContractError> {
        let folder = self.dir.join(kind_folder(kind));
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(settings::PRESETS_NOT_READ.with("path", folder.display()).with("details", error)),
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|path| path.file_stem().and_then(|stem| stem.to_str()).map(str::to_string))
            .collect();
        names.sort();
        Ok(names)
    }

    fn save(&self, target: EditorTarget, name: &str) -> Result<(), ContractError> {
        let preset = read_target(&self.targets.view.snapshot(), target)?;
        let file = self.file(&preset.kind, name)?;
        write_json(&file, &preset, settings::PRESET_NOT_WRITTEN)
    }

    fn apply(&self, target: EditorTarget, name: &str) -> Result<(), ContractError> {
        let current = read_target(&self.targets.view.snapshot(), target)?;
        let preset = self.load(&current.kind, name)?;
        let history = &self.targets.history;
        history.begin_group(&tr!("Preset \"{}\"", "Пресет «{}»", name.trim()));
        match self.put(target, &current.params, preset) {
            Ok(()) => {
                history.end_group();
                Ok(())
            }
            Err(error) => {
                history.cancel_group();
                Err(error)
            }
        }
    }

    fn remove(&self, kind: &str, name: &str) -> Result<(), ContractError> {
        let file = self.file(kind, name)?;
        match fs::remove_file(&file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(settings::PRESET_NOT_FOUND.with("name", name.trim())),
            Err(error) => Err(settings::PRESET_NOT_DELETED.with("name", name.trim()).with("details", error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_folders_are_safe_names() {
        assert_eq!(kind_folder("vst3:surge_xt"), "vst3_surge_xt");
        assert_eq!(kind_folder("eq"), "eq");
        assert_eq!(kind_folder("../x"), ".._x", "the path separator does not get through");
    }

    #[test]
    fn preset_file_keeps_state_as_base64_and_skips_missing_state() {
        let preset = PresetFile { kind: "eq".into(), params: [("gain".to_string(), 3.0)].into(), state: None };
        let json = serde_json::to_value(&preset).unwrap();
        assert!(json.get("state").is_none(), "{json}");
        let with_state = PresetFile { state: Some(PluginState { component: vec![1, 2], controller: Vec::new() }), ..preset };
        let back: PresetFile = serde_json::from_value(serde_json::to_value(&with_state).unwrap()).unwrap();
        assert_eq!(back, with_state);
    }
}
