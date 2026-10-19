//! Registry tests: lookup, replacing built-in kinds and plugin groups. Both registries are thin wrappers over the shared
//! `FactoryGroups`, so the shared group behaviour is tested on one of them, not twice.

use std::sync::Arc;

use daw_contracts::codes::{graph, plugins, tracks};
use daw_contracts::{
    AudioBlock, ContractError, Effect, EffectCatalog, EffectFactory, EffectGroups, Instrument, InstrumentCatalog, InstrumentFactory,
    InstrumentGroups, L10n, ParamInfo, Params, PluginState, StreamConfig,
};

use crate::{EffectRegistry, InstrumentRegistry};

struct Silent;

impl Instrument for Silent {
    fn prepare(&mut self, _: StreamConfig) {}
    fn note_on(&mut self, _: u8, _: f32) {}
    fn note_off(&mut self, _: u8) {}
    fn all_notes_off(&mut self) {}
    fn controller(&mut self, _: daw_contracts::ControllerKind, _: f32) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn render(&mut self, _: &mut AudioBlock) {}
}

struct SilentFactory(&'static str);

impl InstrumentFactory for SilentFactory {
    fn kind(&self) -> &'static str {
        self.0
    }
    fn label(&self) -> L10n {
        L10n::same("Silence")
    }
    fn describe(&self) -> Vec<ParamInfo> {
        vec![ParamInfo {
            name: "level",
            label: L10n::same("Level"),
            min: 0.0,
            max: 1.0,
            default: 0.5,
            kind: daw_contracts::ParamKind::Number,
            choices: &[],
            about: L10n::EMPTY,
        }]
    }
    fn create(&self, _: &Params, _: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        Ok(Box::new(Silent))
    }
}

#[test]
fn finds_creates_and_reports_defaults() {
    let registry = InstrumentRegistry::new();
    registry.register(Box::new(SilentFactory("silent")));
    assert!(registry.create("silent", &Params::new(), None).is_ok());
    assert_eq!(registry.defaults("silent").unwrap()["level"], 0.5);
    assert_eq!(registry.kinds().len(), 1);
    assert_eq!(registry.rebuild_on("silent"), [] as [&str; 0]);
}

#[test]
fn unknown_kind_is_an_error_and_registering_twice_replaces() {
    let registry = InstrumentRegistry::new();
    assert_eq!(registry.defaults("nope").unwrap_err(), tracks::UNKNOWN_INSTRUMENT.with("kind", "nope"));
    assert!(EffectRegistry::new().defaults("nope").is_err_and(|e| e.is(graph::UNKNOWN_EFFECT)));
    registry.register(Box::new(SilentFactory("a")));
    registry.register(Box::new(SilentFactory("a")));
    assert_eq!(registry.kinds().len(), 1);
}

#[test]
fn instruments_created_before_a_replacement_keep_working() {
    let registry = InstrumentRegistry::new();
    registry.replace_group("vst3", vec![Box::new(SilentFactory("vst3:a"))]);
    let mut instrument = registry.create("vst3:a", &Params::new(), None).unwrap();
    registry.replace_group("vst3", Vec::new());
    instrument.note_on(60, 1.0);
}

struct Gain;

impl Effect for Gain {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, _: &mut [f32], _: Option<&[f32]>) {}
}

struct GainFactory(&'static str);

impl EffectFactory for GainFactory {
    fn kind(&self) -> &'static str {
        self.0
    }
    fn label(&self) -> L10n {
        L10n::same("Gain")
    }
    fn describe(&self) -> Vec<ParamInfo> {
        Vec::new()
    }
    fn create(&self, _: &Params, _: Option<&PluginState>) -> Result<Box<dyn Effect>, ContractError> {
        Ok(Box::new(Gain))
    }
}

fn kinds_of(registry: &EffectRegistry) -> Vec<&'static str> {
    registry.kinds().iter().map(|k| k.kind).collect()
}

#[test]
fn replacing_a_group_swaps_only_that_group() {
    let registry = EffectRegistry::new();
    registry.register(Box::new(GainFactory("eq")));
    registry.replace_group("vst3", vec![Box::new(GainFactory("vst3:a")), Box::new(GainFactory("vst3:b"))]);
    assert_eq!(kinds_of(&registry), ["eq", "vst3:a", "vst3:b"]);

    registry.replace_group("vst3", vec![Box::new(GainFactory("vst3:c"))]);
    assert_eq!(kinds_of(&registry), ["eq", "vst3:c"], "the previous plugin set is replaced, built-in kinds stay");
    assert!(registry.create("vst3:a", &Params::new(), None).is_err());
    assert!(registry.create("vst3:c", &Params::new(), None).is_ok());

    registry.replace_group("vst3", Vec::new());
    assert_eq!(kinds_of(&registry), ["eq"]);
}

#[test]
fn registry_can_be_read_and_replaced_from_many_threads() {
    let registry = Arc::new(EffectRegistry::new());
    let writers: Vec<_> = (0..4)
        .map(|_| {
            let registry = registry.clone();
            std::thread::spawn(move || {
                for _ in 0..50 {
                    registry.replace_group("vst3", vec![Box::new(GainFactory("vst3:x"))]);
                    let _ = registry.kinds();
                    let _ = registry.create("vst3:x", &Params::new(), None);
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }
    assert_eq!(kinds_of(&registry), ["vst3:x"]);
}

/// A hidden kind: it is not in the catalog list, but `create` reaches it and returns the factory's refusal.
struct Hidden;

impl EffectFactory for Hidden {
    fn kind(&self) -> &'static str {
        "vst3:hidden"
    }
    fn label(&self) -> L10n {
        L10n::same("Hidden")
    }
    fn describe(&self) -> Vec<ParamInfo> {
        Vec::new()
    }
    fn create(&self, _: &Params, _: Option<&PluginState>) -> Result<Box<dyn Effect>, ContractError> {
        Err(plugins::DISABLED.with("name", "Hidden"))
    }
    fn listed(&self) -> bool {
        false
    }
}

struct HiddenInstrument;

impl InstrumentFactory for HiddenInstrument {
    fn kind(&self) -> &'static str {
        "vst3:hidden"
    }
    fn label(&self) -> L10n {
        L10n::same("Hidden")
    }
    fn describe(&self) -> Vec<ParamInfo> {
        Vec::new()
    }
    fn create(&self, _: &Params, _: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        Err(plugins::DISABLED.with("name", "Hidden"))
    }
    fn listed(&self) -> bool {
        false
    }
}

#[test]
fn unlisted_kinds_are_not_offered_but_still_explain_why_they_are_not_created() {
    let effects = EffectRegistry::new();
    effects.replace_group("vst3", vec![Box::new(GainFactory("vst3:a")), Box::new(Hidden)]);
    assert_eq!(kinds_of(&effects), ["vst3:a"]);
    let error = effects.create("vst3:hidden", &Params::new(), None).err().unwrap();
    assert!(error.is(plugins::DISABLED), "the factory's reason, not \"unknown kind\": {error}");

    let instruments = InstrumentRegistry::new();
    instruments.replace_group("vst3", vec![Box::new(HiddenInstrument)]);
    assert!(instruments.kinds().is_empty());
    assert!(matches!(instruments.create("vst3:hidden", &Params::new(), None), Err(e) if e.is(plugins::DISABLED)));
}

/// A kind that requires plugin state: shows that the catalog passes the state to the factory as is.
struct NeedsState;

impl EffectFactory for NeedsState {
    fn kind(&self) -> &'static str {
        "vst3:stateful"
    }
    fn label(&self) -> L10n {
        L10n::same("Stateful")
    }
    fn describe(&self) -> Vec<ParamInfo> {
        Vec::new()
    }
    fn create(&self, _: &Params, state: Option<&PluginState>) -> Result<Box<dyn Effect>, ContractError> {
        match state {
            Some(state) if state.component == b"preset" => Ok(Box::new(Gain)),
            _ => Err(plugins::LOAD_FAILED.with("details", "no state")),
        }
    }
}

#[test]
fn plugin_state_reaches_the_factory() {
    let effects = EffectRegistry::new();
    effects.register(Box::new(NeedsState));
    let state = PluginState { component: b"preset".to_vec(), controller: Vec::new() };
    assert!(effects.create("vst3:stateful", &Params::new(), Some(&state)).is_ok());
    assert!(effects.create("vst3:stateful", &Params::new(), None).is_err());
}
