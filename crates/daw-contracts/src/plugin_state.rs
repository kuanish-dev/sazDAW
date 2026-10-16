//! Saved plugin state (a preset chosen in the plugin window, and everything not expressed by `p<id>` params) and the
//! contract "keep the plugin states of a project".

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContractError, EditorTarget};

/// Opaque bytes of a plugin's state: what the plugin itself gave to the host (in VST3, `IComponent::getState` and
/// `IEditController::getState`). The host does not parse them, only stores them and gives them back to a plugin of the
/// same kind.
///
/// In the project JSON they are base64 strings (`{ "component": "…", "controller": "…" }`; an empty `controller` is not
/// written). State can be large (~100 KB for Surge XT, a third more in base64): in the model it sits in an `Arc` so
/// copies of the project (undo history, engine snapshots) do not copy the bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PluginState {
    /// State of the plugin's audio part.
    #[serde(with = "base64_bytes")]
    #[schemars(with = "String")]
    pub component: Vec<u8>,
    /// State of the "parameters and window" part; empty for many plugins.
    #[serde(default, with = "base64_bytes", skip_serializing_if = "Vec::is_empty")]
    #[schemars(with = "String")]
    pub controller: Vec<u8>,
}

/// Plugin states of a project. Job: keep the plugin state of an effect or instrument in the project.
///
/// Contract:
/// * `get` returns the target's saved state (`None` if never saved); no such node or track is `ErrorKind::NotFound`,
///   a node that is not an effect or an audio track is `ErrorKind::Invalid`;
/// * `set` writes the state in one undo step "Plugin state" (the same state changes nothing and creates no step). The
///   sounding plugin instance is recreated with this state and the project params are set on top; undo restores the
///   previous state the same way. Target errors are as in `get`;
/// * changing the instrument kind forgets the state (it belongs to another plugin).
pub trait PluginStates: Send + Sync {
    fn get(&self, target: EditorTarget) -> Result<Option<Arc<PluginState>>, ContractError>;
    fn set(&self, target: EditorTarget, state: PluginState) -> Result<(), ContractError>;
}

/// Bytes as a base64 string in JSON.
mod base64_bytes {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(|error| D::Error::custom(format!("plugin state is not base64: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_go_to_json_as_base64_and_back() {
        let state = PluginState { component: vec![0, 1, 2, 255], controller: b"ctl".to_vec() };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json, serde_json::json!({ "component": "AAEC/w==", "controller": "Y3Rs" }));
        assert_eq!(serde_json::from_value::<PluginState>(json).unwrap(), state);
    }

    #[test]
    fn empty_controller_is_not_written_and_may_be_missing() {
        let state = PluginState { component: vec![7], controller: Vec::new() };
        let json = serde_json::to_value(&state).unwrap();
        assert!(json.get("controller").is_none(), "{json}");
        assert_eq!(serde_json::from_value::<PluginState>(json).unwrap(), state);
    }

    #[test]
    fn broken_base64_is_an_error() {
        assert!(serde_json::from_value::<PluginState>(serde_json::json!({ "component": "not base64" })).is_err());
    }
}
