//! Registries of instrument and effect kinds: finding a factory by its kind name.

mod effects;
mod groups;
mod instruments;
#[cfg(test)]
mod tests;

use daw_contracts::{ParamInfo, Params};

pub use effects::EffectRegistry;
pub use instruments::InstrumentRegistry;

fn defaults_of(info: &[ParamInfo]) -> Params {
    info.iter().map(|p| (p.name.to_string(), p.default)).collect()
}
