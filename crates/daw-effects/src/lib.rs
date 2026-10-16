//! Built-in effects. Every effect is its own module with its own parameters;
//! the shared frame (parameter table, factory) lives here.

mod biquad;
mod chorus;
mod compressor;
mod crossover;
mod de_esser;
mod delay;
mod distortion;
mod dynamic_eq;
mod dynamics;
mod eq;
mod filter;
mod gate;
mod limiter;
mod multiband;
mod reverb;
mod saturator;
mod smooth;

#[cfg(test)]
mod testing;

use std::marker::PhantomData;

use daw_contracts::{ContractError, Effect, EffectFactory, L10n, ParamInfo, ParamKind, Params, PluginState, StreamConfig};

/// Description of an effect kind: name, label, parameters and constructor.
/// The factory is built from it, so a new effect is one module and one line.
pub(crate) trait EffectKind: Effect + Sized + 'static {
    const KIND: &'static str;
    const LABEL: L10n;
    const PARAMS: &'static [ParamInfo];

    /// The effect for `config` with any initial values: `build` applies the defaults from `PARAMS` through
    /// `set_param`, so they are written only in the table.
    fn new(config: StreamConfig) -> Self;

    /// Starts what the effect needs besides its fields, on the control side, after `build` (the factory calls it):
    /// the linear-phase EQ's filter designer thread. Nothing by default.
    fn start(&mut self) -> Result<(), ContractError> {
        Ok(())
    }
}

/// A number param of a table (`PARAMS`): the newer effects write their tables with these three helpers.
pub(crate) const fn number(name: &'static str, label: L10n, min: f32, max: f32, default: f32) -> ParamInfo {
    ParamInfo { name, label, min, max, default, kind: ParamKind::Number, choices: &[] }
}

/// An on/off param, off by default.
pub(crate) const fn switch(name: &'static str, label: L10n) -> ParamInfo {
    ParamInfo { name, label, min: 0.0, max: 1.0, default: 0.0, kind: ParamKind::Switch, choices: &[] }
}

/// A choice param: the value is the index of one of `choices`.
pub(crate) const fn choice(name: &'static str, label: L10n, choices: &'static [L10n], default: f32) -> ParamInfo {
    ParamInfo { name, label, min: 0.0, max: (choices.len() - 1) as f32, default, kind: ParamKind::Choice, choices }
}

/// The effect with the defaults from its table, `params` on top. Then a reset: smoothed parameters jump straight to
/// their values, so a new effect does not glide into them.
pub(crate) fn build<'a, E: EffectKind>(params: impl IntoIterator<Item = (&'a str, f32)>) -> E {
    let mut effect = E::new(StreamConfig::DEFAULT);
    for info in E::PARAMS {
        effect.set_param(info.name, info.default);
    }
    for (name, value) in params {
        effect.set_param(name, value);
    }
    effect.reset();
    effect
}

/// The parameter's index in the table and the value fitted to it (`ParamInfo::fit`: within range, a whole index for a
/// choice, since automation of a choice arrives fractional); `None` for an unknown name.
pub(crate) fn clamp_param(table: &[ParamInfo], name: &str, value: f32) -> Option<(usize, f32)> {
    let index = table.iter().position(|p| p.name == name)?;
    let info = &table[index];
    Some((index, if value.is_finite() { info.fit(value) } else { info.default }))
}

/// Mix for "copy" effects (chorus, delay): the dry signal only drops to half, so at full mix the copies sound on top
/// of the original rather than instead of it.
pub(crate) fn mix_over_dry(dry: f32, wet: f32, mix: f32) -> f32 {
    dry * (1.0 - mix * 0.5) + wet * mix
}

pub(crate) fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Gain reduction in dB (0 or more) of a linear gain at most 1; silence (gain 0) is capped at 120 dB.
pub(crate) fn gain_to_reduction(gain: f32) -> f32 {
    (-20.0 * gain.max(1e-6).log10()).max(0.0)
}

/// One-pole smoothing coefficient with time constant `ms` (attack, release).
pub(crate) fn time_coefficient(ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (ms * 0.001 * sample_rate).max(1.0)).exp()
}

struct Factory<E: EffectKind>(PhantomData<fn() -> E>);

impl<E: EffectKind> EffectFactory for Factory<E> {
    fn kind(&self) -> &'static str {
        E::KIND
    }

    fn label(&self) -> L10n {
        E::LABEL
    }

    fn describe(&self) -> Vec<ParamInfo> {
        E::PARAMS.to_vec()
    }

    // A built-in effect has no state of its own: everything is in the parameters.
    fn create(&self, params: &Params, _state: Option<&PluginState>) -> Result<Box<dyn Effect>, ContractError> {
        let mut effect = build::<E>(params.iter().map(|(name, value)| (name.as_str(), *value)));
        effect.start()?;
        Ok(Box::new(effect))
    }
}

fn factory<E: EffectKind>() -> Box<dyn EffectFactory> {
    Box::new(Factory::<E>(PhantomData))
}

/// Factories of all built-in effects.
pub fn factories() -> Vec<Box<dyn EffectFactory>> {
    vec![
        factory::<eq::Eq>(),
        factory::<compressor::Compressor>(),
        factory::<reverb::Reverb>(),
        factory::<delay::Delay>(),
        factory::<distortion::Distortion>(),
        factory::<limiter::Limiter>(),
        factory::<chorus::Chorus>(),
        factory::<filter::Filter>(),
        factory::<gate::Gate>(),
        factory::<saturator::Saturator>(),
        factory::<multiband::Multiband>(),
        factory::<dynamic_eq::DynamicEq>(),
        factory::<de_esser::DeEsser>(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_has_unique_kind_and_valid_params() {
        let factories = factories();
        let mut kinds: Vec<_> = factories.iter().map(|f| f.kind()).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), factories.len());
        for factory in &factories {
            for p in factory.describe() {
                assert!(p.min <= p.default && p.default <= p.max, "{}.{}", factory.kind(), p.name);
            }
            assert!(factory.create(&Params::new(), None).is_ok());
        }
    }

    #[test]
    fn every_effect_honours_the_effect_contract() {
        for factory in factories() {
            crate::testing::assert_honours_contract(factory.as_ref());
        }
    }
}
