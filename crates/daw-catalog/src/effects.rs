//! Registry of effect kinds: finding an effect factory by its kind name.

use daw_contracts::codes::graph;
use daw_contracts::{ContractError, Effect, EffectCatalog, EffectFactory, EffectGroups, ErrorCode, KindInfo, L10n, Params, PluginState};

use crate::defaults_of;
use crate::groups::{FactoryGroups, Kinded};

impl Kinded for dyn EffectFactory {
    const UNKNOWN: ErrorCode = graph::UNKNOWN_EFFECT;

    fn kind(&self) -> &'static str {
        EffectFactory::kind(self)
    }
}

/// Registry of effect kinds: built-in kinds and plugin groups (see `FactoryGroups`).
#[derive(Default)]
pub struct EffectRegistry {
    groups: FactoryGroups<dyn EffectFactory>,
}

impl EffectRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a built-in kind, replacing one with the same name.
    pub fn register(&self, factory: Box<dyn EffectFactory>) {
        self.groups.register(factory);
    }

    pub fn register_all(&self, factories: Vec<Box<dyn EffectFactory>>) {
        for factory in factories {
            self.register(factory);
        }
    }
}

impl EffectGroups for EffectRegistry {
    fn replace_group(&self, group: &str, factories: Vec<Box<dyn EffectFactory>>) {
        self.groups.replace_group(group, factories);
    }
}

impl EffectCatalog for EffectRegistry {
    fn kinds(&self) -> Vec<KindInfo> {
        self.groups
            .describe_all(|f| f.listed().then(|| KindInfo { kind: f.kind(), label: f.label(), about: L10n::EMPTY, params: f.describe() }))
    }

    fn defaults(&self, kind: &str) -> Result<Params, ContractError> {
        Ok(defaults_of(&self.groups.find(kind)?.describe()))
    }

    fn create(&self, kind: &str, params: &Params, state: Option<&PluginState>) -> Result<Box<dyn Effect>, ContractError> {
        self.groups.find(kind)?.create(params, state)
    }
}
