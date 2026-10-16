//! User presets: instrument and effect presets (`Presets`) and project templates (`ProjectTemplates`).
//! Both live in the app's state folder, not in the project: every project uses them.

use std::path::Path;

use crate::codes::settings as codes;
use crate::{ContractError, EditorTarget};

/// Longest preset or template name, in characters.
pub const MAX_PRESET_NAME_CHARS: usize = 64;

/// A preset or template name with surrounding spaces trimmed. Empty, longer than [`MAX_PRESET_NAME_CHARS`], containing
/// `/ \ :` or control characters, or starting with a dot is `ErrorKind::Invalid` (`settings::NAME_EMPTY`,
/// `NAME_TOO_LONG`, `NAME_BAD_CHARS`): the name becomes a file name.
pub fn check_preset_name(name: &str) -> Result<&str, ContractError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(codes::NAME_EMPTY.err());
    }
    if name.chars().count() > MAX_PRESET_NAME_CHARS {
        return Err(codes::NAME_TOO_LONG.with("name", name).with("max", MAX_PRESET_NAME_CHARS));
    }
    if name.starts_with('.') || name.chars().any(|c| matches!(c, '/' | '\\' | ':') || c.is_control()) {
        return Err(codes::NAME_BAD_CHARS.with("name", name));
    }
    Ok(name)
}

/// Instrument and effect presets. Job: remember a kind's settings under a name and apply them to a track or effect.
///
/// Contract:
/// * a preset belongs to a **kind** (`synth`, `eq`, `vst3:…`): its params and the saved plugin state (`PluginStates`);
///   it can be applied to any target of the same kind;
/// * `list` returns the kind's preset names alphabetically (empty if there are none);
/// * `save` stores the target's params and state under a name (the same name for the same kind overwrites); the
///   project does not change;
/// * `apply` applies a preset of the target's kind: first the plugin state (if saved), then the params on top; params
///   the kind no longer has are skipped. Everything is **one undo step**; on error the project does not change;
/// * `remove` deletes a preset; a missing one is `ErrorKind::Invalid` naming it;
/// * target errors: no such track or node is `ErrorKind::NotFound`; an audio track or a node that is not an effect is
///   `ErrorKind::Invalid`; a wrong name, see [`check_preset_name`]; a disk failure is `ErrorKind::System`.
pub trait Presets: Send + Sync {
    fn list(&self, kind: &str) -> Result<Vec<String>, ContractError>;
    fn save(&self, target: EditorTarget, name: &str) -> Result<(), ContractError>;
    fn apply(&self, target: EditorTarget, name: &str) -> Result<(), ContractError>;
    fn remove(&self, kind: &str, name: &str) -> Result<(), ContractError>;
}

/// Project templates. Job: remember project structures and start new projects from them.
///
/// Contract:
/// * a template is the **structure** of a project: tracks with instruments (params and plugin states), track colors
///   and folders, the graph (effects, buses, connections, faders), tempo and time signature. Not included: clips and
///   notes, automation, loop, markers and samples, except those referenced by instruments (sampler sounds are copied
///   with the template); the freeze mark is cleared;
/// * `list` returns names alphabetically; `save` stores the current project's structure under a name (the same name
///   overwrites); `remove` deletes a template (a missing one is `ErrorKind::Invalid`);
/// * `create_project` creates a new project in `project_file` (the folder is created) from the template and opens it
///   like `ProjectIo::load`; the project name is the file name. An existing file or a missing template is
///   `ErrorKind::Invalid`; a wrong name, see [`check_preset_name`].
pub trait ProjectTemplates: Send + Sync {
    fn list(&self) -> Result<Vec<String>, ContractError>;
    fn save(&self, name: &str) -> Result<(), ContractError>;
    fn remove(&self, name: &str) -> Result<(), ContractError>;
    fn create_project(&self, template: &str, project_file: &Path) -> Result<(), ContractError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorKind;

    #[test]
    fn names_are_trimmed_and_safe_for_files() {
        assert_eq!(check_preset_name("  Warm bass ").unwrap(), "Warm bass");
        for bad in ["", "   ", ".hidden", "a/b", "a\\b", "a:b", "tab\there", &"x".repeat(MAX_PRESET_NAME_CHARS + 1)] {
            assert!(matches!(check_preset_name(bad), Err(e) if e.kind() == ErrorKind::Invalid), "{bad:?}");
        }
        assert!(check_preset_name(&"é".repeat(MAX_PRESET_NAME_CHARS)).is_ok(), "limit is in chars, not bytes");
    }
}
