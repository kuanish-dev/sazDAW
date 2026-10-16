//! Kind catalogs: built-in instruments (synth, sampler, drums), effects and MIDI effects, plus third-party plugins from the scan cache.

use std::sync::Arc;

use daw_catalog::{EffectRegistry, InstrumentRegistry};
use daw_contracts::{NoteEffectCatalog, PluginScanner, SampleStore};
use daw_note_effects::NoteEffects;
use daw_vst3::PluginDirectory;

use super::Devices;
use crate::plugins::PluginDeps;
use crate::{SessionConfig, plugins};

pub(super) struct Catalogs {
    pub instruments: Arc<InstrumentRegistry>,
    pub effects: Arc<EffectRegistry>,
    pub note_effects: Arc<dyn NoteEffectCatalog>,
    pub plugins: Arc<dyn PluginScanner>,
    pub directory: Arc<PluginDirectory>,
}

pub(super) fn build(config: &SessionConfig, devices: &Devices, samples: Arc<dyn SampleStore>, deps: PluginDeps) -> Catalogs {
    let instruments = Arc::new(InstrumentRegistry::new());
    instruments.register_all(daw_drums::factories());
    instruments.register(daw_sampler::factory(samples));
    instruments.register(daw_synth::factory());
    instruments.register_all(daw_strings::factories());
    let effects = Arc::new(EffectRegistry::new());
    effects.register_all(daw_effects::factories());
    // Third-party plugins only in a real session and only from the previous scan cache: tests must not depend on what is
    // installed on the machine, and startup must not load foreign code.
    let (plugins, directory) = plugins::scanner(config, devices, &effects, &instruments, deps);
    Catalogs { instruments, effects, note_effects: Arc::new(NoteEffects), plugins, directory }
}
