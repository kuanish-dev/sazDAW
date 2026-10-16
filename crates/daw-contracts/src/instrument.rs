//! Instruments: the realtime side (`Instrument`), kind factories and the catalog, parameter descriptions.

use serde::Serialize;

use crate::{AudioBlock, BlockTiming, ContractError, ControllerKind, L10n, Params, PluginState, StreamConfig};

/// Kind of the built-in sampler and its "which sample" (value is a `SampleId`) and "root note" parameters. The only
/// place these names are written: the project uses them to clear a removed sample's selection, and the drum part
/// builder uses them to create a sampler track.
pub const SAMPLER_KIND: &str = "sampler";
pub const SAMPLER_SAMPLE_PARAM: &str = "sample";
pub const SAMPLER_ROOT_PARAM: &str = "root";

/// Kind of the built-in synth (`daw-synth`): the instrument of new tracks and of the starter beat's bass.
pub const SYNTH_KIND: &str = "synth";

/// Kinds of the built-in plucked strings (`daw-strings`). Named here because the chord part builder plays each one in
/// its own way: it voices chords for the instrument's tuning, and the bass plays roots instead of strumming.
pub const GUITAR_KIND: &str = "guitar";
pub const ELECTRIC_GUITAR_KIND: &str = "electric_guitar";
pub const UKULELE_KIND: &str = "ukulele";
pub const BASS_GUITAR_KIND: &str = "bass_guitar";

/// An instrument: takes notes, produces sound. Created on the control side (where `prepare` is also called); once
/// handed to the engine, all methods are called from the realtime thread: no allocations, locks or I/O.
pub trait Instrument: Send {
    fn prepare(&mut self, config: StreamConfig);
    fn note_on(&mut self, note: u8, velocity: f32);
    fn note_off(&mut self, note: u8);
    /// Releases every sounding voice, also those `note_off` would not end (one-shot hits, notes held by the sustain
    /// pedal); each fades out like a released note, and a one-shot fades out quickly, so nothing rings on. The engine
    /// calls it when playback starts or stops and when it jumps (a seek, a wrap at the loop or song end).
    fn all_notes_off(&mut self);
    /// A channel MIDI controller: pitch bend (`-1..=1`), CC (`0..=1`, number up to `MAX_CC_NUMBER`) or aftertouch
    /// (`0..=1`). The value holds until the next message of the same kind; the engine itself resets pitch bend,
    /// aftertouch, modulation (CC1) and sustain (CC64) to 0 on stop and seek. Whatever the instrument does not
    /// understand, it ignores.
    fn controller(&mut self, kind: ControllerKind, value: f32);
    /// Unknown names are ignored.
    fn set_param(&mut self, name: &str, value: f32);
    /// **Adds** sound to the stereo buffer. Called even when no notes are playing, for release tails.
    fn render(&mut self, block: &mut AudioBlock);
    /// The instrument's latency in frames: how far its output lags behind the notes (plugin lookahead). The engine
    /// uses it to align parallel paths (plugin delay compensation, PDC).
    ///
    /// Contract: the value is valid after `prepare` and may change after `prepare` and parameter changes; the engine
    /// rereads it on every snapshot apply and after `prepare` (not every block), so the call must be cheap, with no
    /// allocations or locks, because it is made from the audio thread. Instruments without latency return 0.
    fn latency_samples(&self) -> u32 {
        0
    }
    /// Timing of the segment the engine is about to play: called from the audio thread before the first `render` of
    /// each segment (no allocations or locks). A segment may be played by several `render` calls in a row (notes inside
    /// the segment); the position advances by the frames already played. Needed by tempo-synced instruments
    /// (arpeggiators, beat-synced LFOs); others do not need timing, so by default the call does nothing.
    fn set_timing(&mut self, _timing: &BlockTiming) {}
}

/// How the interface shows a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    /// A slider between `min` and `max`.
    Number,
    /// A 0 / 1 switch.
    Switch,
    /// A reference to a sample from the project library (the value is its id).
    Sample,
    /// One option out of `ParamInfo::choices`: the value is the option index 0…n-1 (`min` = 0, `max` = n-1).
    Choice,
}

/// A parameter as the interface shows and validates it. Values are in real units (hertz, seconds, decibels…); the unit
/// goes into the label after a comma («Cutoff, Hz»). Labels serialize in the current language (`crate::i18n`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParamInfo {
    pub name: &'static str,
    pub label: L10n,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub kind: ParamKind,
    /// Option names of a `ParamKind::Choice` parameter, in order; empty for the others (and absent from JSON).
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub choices: &'static [L10n],
}

impl ParamInfo {
    /// A value valid for the parameter: within `min…max`, and a whole option index for a choice.
    pub fn fit(&self, value: f32) -> f32 {
        let value = value.clamp(self.min, self.max);
        if self.kind == ParamKind::Choice { value.round() } else { value }
    }
}

/// Factory for one instrument kind.
pub trait InstrumentFactory: Send + Sync {
    fn kind(&self) -> &'static str;
    fn label(&self) -> L10n;
    fn describe(&self) -> Vec<ParamInfo>;
    /// A new instrument. `state` is the saved plugin state from the project (`InstrumentSpec::state`): it is applied
    /// before the parameters, and `params` are set on top. Kinds without their own state (built-ins) ignore it.
    fn create(&self, params: &Params, state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError>;
    /// Parameters whose change requires recreating the instrument (for example, the sample choice: a new sound cannot
    /// be passed to the realtime thread through `set_param`). Other parameters arrive via `set_param` without that.
    fn rebuild_on(&self) -> &'static [&'static str] {
        &[]
    }
    /// Whether to offer the kind in the catalog (`InstrumentCatalog::kinds`); see `EffectFactory::listed`.
    fn listed(&self) -> bool {
        true
    }
}

/// An instrument or effect kind from a catalog: name, label and parameters. Serialized fresh on every request, so a
/// language change shows up on the next `kinds()` (nothing caches the label text).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindInfo {
    pub kind: &'static str,
    pub label: L10n,
    pub params: Vec<ParamInfo>,
}

/// Catalog of available instruments.
pub trait InstrumentCatalog: Send + Sync {
    fn kinds(&self) -> Vec<KindInfo>;
    /// Default parameter values for a kind.
    fn defaults(&self, kind: &str) -> Result<Params, ContractError>;
    /// See [`InstrumentFactory::create`].
    fn create(&self, kind: &str, params: &Params, state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError>;
    /// See [`InstrumentFactory::rebuild_on`].
    fn rebuild_on(&self, kind: &str) -> Vec<&'static str>;
}

/// A changeable set of instrument factories split into named groups (built-in, VST3, …).
/// The counterpart of `EffectGroups`: the plugin scan puts found instruments into the catalog through it.
pub trait InstrumentGroups: Send + Sync {
    /// Replaces all factories of `group` with `factories` (creating the group if missing). Other groups are left
    /// alone. Instruments already created by the previous factories keep working.
    fn replace_group(&self, group: &str, factories: Vec<Box<dyn InstrumentFactory>>);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(kind: ParamKind, choices: &'static [L10n]) -> ParamInfo {
        ParamInfo { name: "p1", label: L10n { en: "Wave", ru: "Волна" }, min: 0.0, max: 3.0, default: 2.0, kind, choices }
    }

    const WAVES: [L10n; 4] = [L10n::same("Sine"), L10n::same("Triangle"), L10n::same("Saw"), L10n::same("Square")];

    #[test]
    fn choice_values_are_whole_option_numbers_and_others_are_only_clamped() {
        let choice = info(ParamKind::Choice, &WAVES);
        assert_eq!(choice.fit(1.4), 1.0);
        assert_eq!(choice.fit(2.6), 3.0);
        assert_eq!(choice.fit(-5.0), 0.0);
        assert_eq!(choice.fit(9.0), 3.0);
        let number = info(ParamKind::Number, &[]);
        assert_eq!(number.fit(1.4), 1.4);
        assert_eq!(number.fit(7.0), 3.0);
    }

    #[test]
    fn choices_appear_in_json_only_for_choice_parameters() {
        const AB: [L10n; 2] = [L10n::same("A"), L10n::same("B")];
        let choice = serde_json::to_value(info(ParamKind::Choice, &AB)).unwrap();
        assert_eq!(choice["kind"], "choice");
        assert_eq!(choice["choices"], serde_json::json!(["A", "B"]));
        let number = serde_json::to_value(info(ParamKind::Number, &[])).unwrap();
        assert!(number.get("choices").is_none(), "{number}");
    }
}
