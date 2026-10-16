//! Plugins as catalog kinds: "build effect and instrument factories from the descriptions of found plugins".
//!
//! * `factories_of` — the set of one publication: a factory per plugin and the "kind -> plugin" directory, reusing previous
//!   sources (`PluginSource`) for unchanged plugins;
//! * `CatalogEntry` — shared by both factories: kind, label, params and instance creation. A creation failure is logged
//!   with the plugin name and path and returned as `PL-460`. Creating the entry does NOT load the plugin.
//!   The instance gets a crash marker slot (`ProcessMarker`); a disabled plugin (`disabled`) is neither created nor offered.
//!   The saved plugin state is applied before the role and params; if the plugin rejects it, the instance is still
//!   created (with the project params) and the refusal goes to the log — the project must open;
//! * `Vst3EffectFactory`, `Vst3InstrumentFactory` — what the instance becomes: an effect or an instrument.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::Arc;

use daw_contracts::codes::plugins;
use daw_contracts::{AppLog, ContractError, Effect, EffectFactory, Instrument, InstrumentFactory, L10n, ParamInfo, Params, PluginState};

use crate::effect::Vst3Effect;
use crate::guard::ProcessMarker;
use crate::instance::{ClassId, PluginInstance};
use crate::instrument::Vst3Instrument;
use crate::journal;
use crate::scanner::{PluginInfo, PluginKind};
use crate::source::PluginSource;
use crate::text::leak;

/// Effect factories, instrument factories and the "kind -> plugin" directory.
pub type Factories = (Vec<Box<dyn EffectFactory>>, Vec<Box<dyn InstrumentFactory>>, HashMap<String, Arc<PluginSource>>);

/// One factory per plugin, separately for effects and instruments. `previous` are the sources of the previous publication: a
/// plugin with the same description takes its previous source, so an already loaded bundle is not opened and closed again
/// (`dlopen`/`dlclose` of foreign libraries is risky) and catalog strings are not leaked again. The whole description is
/// compared: a plugin updated in place may have changed its params. `disabled` are kinds turned off by the user.
pub fn factories_of(
    plugins: Vec<PluginInfo>,
    previous: &[Arc<PluginSource>],
    disabled: &HashSet<String>,
    marker: &Arc<ProcessMarker>,
    log: &Arc<dyn AppLog>,
) -> Factories {
    let mut taken = HashSet::new();
    let (mut effects, mut instruments, mut sources) = (Vec::new(), Vec::new(), HashMap::new());
    for info in plugins {
        let kind = unique_kind(&info.name, &info.class_id, &taken);
        taken.insert(kind.clone());
        let plugin_kind = info.kind;
        let source = previous.iter().find(|source| *source.info() == info).cloned().unwrap_or_else(|| Arc::new(PluginSource::new(info)));
        sources.insert(kind.clone(), source.clone());
        let entry = CatalogEntry { disabled: disabled.contains(&kind), kind: leak(kind), source, log: log.clone(), marker: marker.clone() };
        match plugin_kind {
            PluginKind::Effect => effects.push(Box::new(Vst3EffectFactory(entry)) as Box<dyn EffectFactory>),
            PluginKind::Instrument => instruments.push(Box::new(Vst3InstrumentFactory(entry)) as Box<dyn InstrumentFactory>),
        }
    }
    (effects, instruments, sources)
}

/// Catalog kind `vst3:<name in lowercase, spaces -> _>` not yet in `taken` (kinds of different catalogs are not
/// repeated either); on a name clash a short suffix from `class_id` is added.
fn unique_kind(name: &str, class_id: &ClassId, taken: &HashSet<String>) -> String {
    let words: Vec<String> = name.split_whitespace().map(str::to_lowercase).collect();
    let kind = format!("vst3:{}", words.join("_"));
    if !taken.contains(&kind) {
        return kind;
    }
    let suffix = class_id.iter().take(4).fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    });
    format!("{kind}_{suffix}")
}

struct CatalogEntry {
    source: Arc<PluginSource>,
    kind: &'static str,
    log: Arc<dyn AppLog>,
    marker: Arc<ProcessMarker>,
    disabled: bool,
}

impl CatalogEntry {
    /// A new instance with state `state`, turned into the needed role by `build` (`build` gets the instance, the plugin
    /// name and the log). Loading and starting the plugin happen under the crash marker: a crash during creation counts against it.
    fn create_with<T>(
        &self,
        state: Option<&PluginState>,
        build: impl FnOnce(PluginInstance, String, Arc<dyn AppLog>) -> Result<T, String>,
    ) -> Result<T, ContractError> {
        let name = self.source.info().name.clone();
        if self.disabled {
            // An expected refusal: whoever created it shows and logs it (project warnings).
            return Err(plugins::DISABLED.with("name", name));
        }
        let slot = self.marker.claim(self.kind, &name);
        let _inside = slot.as_ref().map(|slot| self.marker.enter(slot));
        let built = self.source.instance().and_then(|mut instance| {
            if let Some(slot) = slot {
                instance.watch(slot);
            }
            if let Some(state) = state
                && let Err(error) = instance.load_state(state)
            {
                let message =
                    self.source.describe_error(&format!("the saved state was not applied ({error}); playing with the project params"));
                journal::error(&*self.log, message);
            }
            build(instance, name, self.log.clone())
        });
        built.map_err(|error| {
            let message = self.source.describe_error(&error);
            journal::error(&*self.log, message.clone());
            plugins::LOAD_FAILED.with("details", message)
        })
    }
}

/// One found effect plugin as a catalog effect kind.
pub struct Vst3EffectFactory(CatalogEntry);

impl EffectFactory for Vst3EffectFactory {
    fn kind(&self) -> &'static str {
        self.0.kind
    }

    fn label(&self) -> L10n {
        self.0.source.label()
    }

    fn describe(&self) -> Vec<ParamInfo> {
        self.0.source.param_infos().to_vec()
    }

    fn listed(&self) -> bool {
        !self.0.disabled
    }

    fn create(&self, params: &Params, state: Option<&PluginState>) -> Result<Box<dyn Effect>, ContractError> {
        self.0.create_with(state, |instance, name, log| Ok(Box::new(Vst3Effect::new(instance, name, log, params)?) as Box<dyn Effect>))
    }
}

/// One found instrument plugin as a catalog instrument kind.
pub struct Vst3InstrumentFactory(CatalogEntry);

impl InstrumentFactory for Vst3InstrumentFactory {
    fn kind(&self) -> &'static str {
        self.0.kind
    }

    fn label(&self) -> L10n {
        self.0.source.label()
    }

    fn describe(&self) -> Vec<ParamInfo> {
        self.0.source.param_infos().to_vec()
    }

    fn listed(&self) -> bool {
        !self.0.disabled
    }

    fn create(&self, params: &Params, state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        self.0.create_with(state, |instance, name, log| {
            Ok(Box::new(Vst3Instrument::new(instance, name, log, params)?) as Box<dyn Instrument>)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_is_lowercase_with_underscores_and_a_duplicate_gets_a_class_suffix() {
        assert_eq!(unique_kind("Big  Fuzz Box", &[0; 16], &HashSet::new()), "vst3:big_fuzz_box");
        let taken = HashSet::from(["vst3:distortion".to_string()]);
        let class_id = [0xab, 0xcd, 0x01, 0x02, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9];
        assert_eq!(unique_kind("Distortion", &class_id, &taken), "vst3:distortion_abcd0102");
    }
}
