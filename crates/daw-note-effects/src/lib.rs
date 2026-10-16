//! Built-in MIDI effects (`NoteEffect`): arpeggiator (`arpeggiator.rs`), chord (`chord.rs`) and note repeat
//! (`repeat.rs`), and their catalog `NoteEffects` (`NoteEffectCatalog`): "which MIDI effects exist and how to create
//! them". A new kind is a module with `KIND`, `LABEL`, `PARAMS` and a row in `KINDS`.

mod arpeggiator;
mod chord;
mod repeat;
#[cfg(test)]
mod tests;

use daw_contracts::codes::notes;
use daw_contracts::{ContractError, KindInfo, L10n, NoteEffect, NoteEffectCatalog, ParamInfo, Params};

use arpeggiator::Arpeggiator;
use chord::Chord;
use repeat::Repeat;

/// A MIDI effect kind: name, label, parameters and constructor.
struct Kind {
    kind: &'static str,
    label: L10n,
    params: &'static [ParamInfo],
    new: fn() -> Box<dyn NoteEffect>,
}

const KINDS: [Kind; 3] = [
    Kind { kind: arpeggiator::KIND, label: arpeggiator::LABEL, params: &arpeggiator::PARAMS, new: || Box::new(Arpeggiator::new()) },
    Kind { kind: chord::KIND, label: chord::LABEL, params: &chord::PARAMS, new: || Box::new(Chord::new()) },
    Kind { kind: repeat::KIND, label: repeat::LABEL, params: &repeat::PARAMS, new: || Box::new(Repeat::new()) },
];

/// Converts segment ticks to frames for the grid-based effects: the segment's start tick and ticks per frame.
#[derive(Clone, Copy)]
pub(crate) struct Clock {
    pub base: f64,
    pub tpf: f64,
    pub frames: usize,
}

impl Clock {
    /// The first frame not earlier than `tick` (like `BlockTiming::frame_at`), within the segment.
    pub fn frame(self, tick: f64) -> usize {
        (((tick - self.base) / self.tpf - 1e-6).ceil().max(0.0) as usize).min(self.frames)
    }

    /// The tick of frame `frame`.
    pub fn tick(self, frame: usize) -> f64 {
        self.base + frame as f64 * self.tpf
    }

    pub fn end(self) -> f64 {
        self.tick(self.frames)
    }
}

/// The value of parameter `name` fitted to the table (`ParamInfo::fit`; not a number gives the default);
/// `None` if there is no such parameter.
pub(crate) fn fit(table: &[ParamInfo], name: &str, value: f32) -> Option<f32> {
    let info = table.iter().find(|p| p.name == name)?;
    Some(if value.is_finite() { info.fit(value) } else { info.default })
}

/// Catalog of the built-in MIDI effects.
pub struct NoteEffects;

impl NoteEffects {
    fn find(kind: &str) -> Result<&'static Kind, ContractError> {
        KINDS.iter().find(|k| k.kind == kind).ok_or_else(|| notes::UNKNOWN_NOTE_EFFECT.with("kind", kind))
    }
}

impl NoteEffectCatalog for NoteEffects {
    fn kinds(&self) -> Vec<KindInfo> {
        KINDS.iter().map(|k| KindInfo { kind: k.kind, label: k.label, params: k.params.to_vec() }).collect()
    }

    fn defaults(&self, kind: &str) -> Result<Params, ContractError> {
        Ok(Self::find(kind)?.params.iter().map(|p| (p.name.to_string(), p.default)).collect())
    }

    fn create(&self, kind: &str, params: &Params) -> Result<Box<dyn NoteEffect>, ContractError> {
        let kind = Self::find(kind)?;
        let mut effect = (kind.new)();
        for info in kind.params {
            effect.set_param(info.name, params.get(info.name).copied().unwrap_or(info.default));
        }
        Ok(effect)
    }
}
