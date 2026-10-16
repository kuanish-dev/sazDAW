//! Registry of instrument kinds: finding an instrument factory by its kind name.

use daw_contracts::codes::tracks;
use daw_contracts::{
    ContractError, ErrorCode, Instrument, InstrumentCatalog, InstrumentFactory, InstrumentGroups, KindInfo, Params, PluginState,
};

use crate::defaults_of;
use crate::groups::{FactoryGroups, Kinded};

impl Kinded for dyn InstrumentFactory {
    const UNKNOWN: ErrorCode = tracks::UNKNOWN_INSTRUMENT;

    fn kind(&self) -> &'static str {
        InstrumentFactory::kind(self)
    }
}

/// Registry of instrument kinds: built-in kinds and plugin groups (see `FactoryGroups`).
#[derive(Default)]
pub struct InstrumentRegistry {
    groups: FactoryGroups<dyn InstrumentFactory>,
}

impl InstrumentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a built-in kind, replacing one with the same name.
    pub fn register(&self, factory: Box<dyn InstrumentFactory>) {
        self.groups.register(factory);
    }

    pub fn register_all(&self, factories: Vec<Box<dyn InstrumentFactory>>) {
        for factory in factories {
            self.register(factory);
        }
    }
}

impl InstrumentGroups for InstrumentRegistry {
    fn replace_group(&self, group: &str, factories: Vec<Box<dyn InstrumentFactory>>) {
        self.groups.replace_group(group, factories);
    }
}

impl InstrumentCatalog for InstrumentRegistry {
    fn kinds(&self) -> Vec<KindInfo> {
        // Hidden kinds (disabled plugins) are not offered, but `create` still finds them and explains the refusal.
        self.groups.describe_all(|f| f.listed().then(|| KindInfo { kind: f.kind(), label: f.label(), params: f.describe() }))
    }

    fn defaults(&self, kind: &str) -> Result<Params, ContractError> {
        Ok(defaults_of(&self.groups.find(kind)?.describe()))
    }

    fn create(&self, kind: &str, params: &Params, state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        self.groups.find(kind)?.create(params, state)
    }

    fn rebuild_on(&self, kind: &str) -> Vec<&'static str> {
        self.groups.find(kind).map(|f| f.rebuild_on().to_vec()).unwrap_or_default()
    }
}
