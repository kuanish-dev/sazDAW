//! Synth parameters for the host: names, ranges, defaults.

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::Arc;

use crate::editor;
use crate::engine::{Settings, Waveform};

/// Waveform for the host (`engine::Waveform` knows nothing about nih-plug).
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveParam {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "saw"]
    #[name = "Saw"]
    Saw,
    #[id = "square"]
    #[name = "Square"]
    Square,
}

impl From<WaveParam> for Waveform {
    fn from(wave: WaveParam) -> Self {
        match wave {
            WaveParam::Sine => Waveform::Sine,
            WaveParam::Triangle => Waveform::Triangle,
            WaveParam::Saw => Waveform::Saw,
            WaveParam::Square => Waveform::Square,
        }
    }
}

#[derive(Params)]
pub struct SynthParams {
    /// Size and state of the plugin window (saved together with the state).
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,
    #[id = "waveform"]
    pub waveform: EnumParam<WaveParam>,
    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "hold"]
    pub hold: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "sustain"]
    pub sustain: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
    #[id = "cutoff"]
    pub cutoff: FloatParam,
    #[id = "gain"]
    pub gain: FloatParam,
}

/// Envelope time: the closer to zero, the finer the knob.
fn seconds(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-2.0) })
        .with_unit(" s")
        .with_value_to_string(formatters::v2s_f32_rounded(3))
}

impl Default for SynthParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(editor::WIDTH, editor::HEIGHT),
            waveform: EnumParam::new("Waveform", WaveParam::Saw),
            attack: seconds("Attack", d.attack, 0.0005, 4.0),
            hold: seconds("Hold", d.hold, 0.0, 4.0),
            decay: seconds("Decay", d.decay, 0.0005, 4.0),
            sustain: FloatParam::new("Sustain", d.sustain, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            release: seconds("Release", d.release, 0.0005, 8.0),
            cutoff: FloatParam::new(
                "Cutoff",
                d.cutoff_hz,
                FloatRange::Skewed { min: 40.0, max: 18_000.0, factor: FloatRange::skew_factor(-2.0) },
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            gain: FloatParam::new("Gain", d.gain, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }
}

impl SynthParams {
    /// Current values; `cutoff` and `gain` are smoothed — call once per frame.
    pub fn next_settings(&self) -> Settings {
        Settings {
            waveform: self.waveform.value().into(),
            attack: self.attack.value(),
            hold: self.hold.value(),
            decay: self.decay.value(),
            sustain: self.sustain.value(),
            release: self.release.value(),
            cutoff_hz: self.cutoff.smoothed.next(),
            gain: self.gain.smoothed.next(),
        }
    }

    /// Values without smoothing, for the window (it draws the curves from them).
    pub fn current_settings(&self) -> Settings {
        Settings {
            waveform: self.waveform.value().into(),
            attack: self.attack.value(),
            hold: self.hold.value(),
            decay: self.decay.value(),
            sustain: self.sustain.value(),
            release: self.release.value(),
            cutoff_hz: self.cutoff.value(),
            gain: self.gain.value(),
        }
    }
}
