//! Plugin parameters as the rest of the app sees them: reading the list from the controller with the `MAX_EXPOSED_PARAMS` limit,
//! real units and choices, the "real <-> normalized" conversion table (`ParamScales`).
//! Which numbers count as real is decided by `display` (from the plugin's texts); here are only the controller calls.

use vst3::ComPtr;
use vst3::Steinberg::Vst::ParameterInfo_::ParameterFlags_;
use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait, ParameterInfo, String128};
use vst3::Steinberg::kResultOk;

use super::display::{Curve, Steps, classify_steps, pick_curve, shown_number};
use crate::scale::{MAX_STEPS, ParamScale, ParamScales};
use crate::text::from_utf16;

/// How a parameter is shown outside (in the contract this is `ParamKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ParamShape {
    /// A number in real units between `min` and `max`.
    Number,
    /// A 0 / 1 switch (the plugin has one step).
    Switch,
    /// One of the named choices: the value is the choice index.
    Choice,
}

/// A plugin parameter as the rest of the app sees it. Values are real (as the plugin shows them),
/// except `default_normalized`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParamDesc {
    pub id: u32,
    /// The plugin's name for it; for a number, with units after a comma ("Cutoff, Hz").
    pub label: String,
    pub shape: ParamShape,
    /// Names of the `ParamShape::Choice` options in order; empty for the others.
    pub choices: Vec<String>,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// The default value as the plugin understands it (0..1): the plugin window compares its knobs with it.
    pub default_normalized: f64,
}

/// How many plugin parameters are exposed (to the contract, the interface and the cache). Synths can have thousands
/// of parameters (Surge XT has over two thousand): nobody needs such a list, and it bloats the catalog, the cache and the window.
/// The first ones in the plugin's order are taken, hidden/service ones excluded. The plugin keeps the rest at their defaults.
pub const MAX_EXPOSED_PARAMS: usize = 128;

/// The plugin's parameters for the user (at most [`MAX_EXPOSED_PARAMS`]) and the conversion of their values.
pub struct ParamList {
    pub params: Vec<ParamDesc>,
    pub scales: ParamScales,
    /// How many suitable parameters the plugin has in total (`params` may be shorter because of the limit).
    pub total: usize,
}

/// Reads the parameters, skipping hidden, read-only, program-change and bypass ones.
pub fn read_parameters(controller: &ComPtr<IEditController>) -> ParamList {
    let mut params = Vec::new();
    let mut scales = Vec::new();
    let mut total = 0;
    let skipped =
        ParameterFlags_::kIsBypass | ParameterFlags_::kIsReadOnly | ParameterFlags_::kIsHidden | ParameterFlags_::kIsProgramChange;
    // SAFETY: a plain controller call without pointers.
    let count = unsafe { controller.getParameterCount() };
    for index in 0..count {
        // SAFETY: the struct holds only numbers and arrays, the zero value is valid.
        let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
        // SAFETY: `info` is a local struct the controller writes the description into.
        if unsafe { controller.getParameterInfo(index, &raw mut info) } != kResultOk || info.flags & skipped != 0 {
            continue;
        }
        total += 1;
        if params.len() < MAX_EXPOSED_PARAMS {
            let (desc, scale) = describe(controller, &info);
            scales.push((desc.id, scale));
            params.push(desc);
        }
    }
    ParamList { params, scales: ParamScales::new(scales), total }
}

/// One parameter: kind, choices, real limits and value conversion.
fn describe(controller: &ComPtr<IEditController>, info: &ParameterInfo) -> (ParamDesc, ParamScale) {
    let id = info.id;
    let steps = info.stepCount;
    let mut units = from_utf16(&info.units).trim().to_string();
    let (shape, choices, scale) = if steps == 1 {
        (ParamShape::Switch, Vec::new(), ParamScale::indexed(1))
    } else if (2..=MAX_STEPS).contains(&steps) {
        let texts = (0..=steps).map(|k| shown_text(controller, id, f64::from(k) / f64::from(steps))).collect();
        match classify_steps(texts) {
            Steps::Values(values) => (ParamShape::Number, Vec::new(), ParamScale::stepped(&values)),
            Steps::Names(names) => (ParamShape::Choice, names, ParamScale::indexed(usize::try_from(steps).unwrap_or(0))),
        }
    } else {
        let curve = continuous_curve(controller, id);
        if !curve.units_apply {
            units.clear();
        } else if units.is_empty() {
            units = curve.suffix;
        }
        (ParamShape::Number, Vec::new(), ParamScale::curve(&curve.values))
    };
    let label = label(&from_utf16(&info.title), &units, shape);
    let (min, max) = scale.range();
    let default = scale.to_plain(info.defaultNormalizedValue);
    (ParamDesc { id, label, shape, choices, min, max, default, default_normalized: info.defaultNormalizedValue }, scale)
}

struct ContinuousCurve {
    values: Vec<f64>,
    units_apply: bool,
    /// Units from the plugin's text ("Hz" in "440 Hz"): in case the parameter description has none.
    suffix: String,
}

/// Real values of a continuous parameter at the curve points: the plugin's conversion or the numbers it shows (`pick_curve`).
fn continuous_curve(controller: &ComPtr<IEditController>, id: u32) -> ContinuousCurve {
    // SAFETY: a plain controller call without pointers.
    let plugin = ParamScale::curve_positions().map(|normalized| unsafe { controller.normalizedParamToPlain(id, normalized) }).collect();
    let shown: Vec<_> = ParamScale::curve_positions()
        .map(|normalized| shown_text(controller, id, normalized).and_then(|text| shown_number(&text)))
        .collect();
    let suffix = shown.last().and_then(|s| s.as_ref()).map(|s| s.suffix.clone()).unwrap_or_default();
    let Curve { values, units_apply } = pick_curve(plugin, &shown);
    ContinuousCurve { values, units_apply, suffix }
}

/// The plugin's text for the value `normalized`; `None` when the plugin did not answer.
fn shown_text(controller: &ComPtr<IEditController>, id: u32, normalized: f64) -> Option<String> {
    let mut text: String128 = [0; 128];
    // SAFETY: `text` is a local array of the size the interface requires.
    let ok = unsafe { controller.getParamStringByValue(id, normalized, &raw mut text) } == kResultOk;
    ok.then(|| from_utf16(&text))
}

fn label(title: &str, units: &str, shape: ParamShape) -> String {
    if units.is_empty() || shape != ParamShape::Number { title.to_string() } else { format!("{title}, {units}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_go_into_the_label_of_numbers_only() {
        assert_eq!(label("Cutoff", "Hz", ParamShape::Number), "Cutoff, Hz");
        assert_eq!(label("Mix", "", ParamShape::Number), "Mix");
        assert_eq!(label("Waveform", "x", ParamShape::Choice), "Waveform");
    }
}
