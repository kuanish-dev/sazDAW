use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, widgets, EguiState};
use std::num::NonZeroU32;
use std::sync::Arc;

/// Distortion curve selector.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    #[id = "soft"]
    #[name = "Soft (tanh)"]
    Soft,
    #[id = "hard"]
    #[name = "Hard (clip)"]
    Hard,
    #[id = "fold"]
    #[name = "Foldback"]
    Fold,
}

/// Waveshaper; always returns a finite value in [-1, 1].
pub fn shape_fn(shape: Shape, x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    match shape {
        Shape::Soft => x.tanh(),
        Shape::Hard => x.clamp(-1.0, 1.0),
        Shape::Fold => {
            // Triangle fold: period 4, range [-1, 1].
            let t = (x + 1.0).rem_euclid(4.0);
            if t < 2.0 {
                t - 1.0
            } else {
                3.0 - t
            }
        }
    }
}

/// One-pole low-pass coefficient for cutoff `hz` at `sample_rate`.
pub fn lp_coeff(hz: f32, sample_rate: f32) -> f32 {
    let hz = hz.clamp(1.0, sample_rate * 0.49);
    1.0 - (-2.0 * std::f32::consts::PI * hz / sample_rate).exp()
}

/// Process one sample. `lp` is the one-pole filter state.
/// `gain`, `mix`, `out_gain` are linear values; `a` is the LP coefficient.
pub fn process_sample(x: f32, shape: Shape, gain: f32, a: f32, mix: f32, out_gain: f32, lp: &mut f32) -> f32 {
    let x = if x.is_finite() { x } else { 0.0 };
    let wet = shape_fn(shape, x * gain);
    *lp += a * (wet - *lp);
    if !lp.is_finite() {
        *lp = 0.0;
    }
    let y = x + (*lp - x) * mix;
    let y = y * out_gain;
    if y.is_finite() {
        y
    } else {
        0.0
    }
}

/// Size of the plugin's own window.
const EDITOR_WIDTH: u32 = 360;
const EDITOR_HEIGHT: u32 = 240;

#[derive(Params)]
struct DistortionParams {
    /// Size and state of the plugin window (saved together with the state).
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
    #[id = "drive"]
    drive: FloatParam,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "output"]
    output: FloatParam,
    #[id = "shape"]
    shape: EnumParam<Shape>,
}

impl Default for DistortionParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(EDITOR_WIDTH, EDITOR_HEIGHT),
            drive: FloatParam::new(
                "Drive",
                util::db_to_gain(12.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(0.0),
                    max: util::db_to_gain(40.0),
                    factor: FloatRange::gain_skew_factor(0.0, 40.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(30.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            tone: FloatParam::new("Tone", 6000.0, FloatRange::Skewed { min: 200.0, max: 12000.0, factor: FloatRange::skew_factor(-2.0) })
                .with_unit(" Hz")
                .with_value_to_string(formatters::v2s_f32_rounded(0)),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            output: FloatParam::new(
                "Output",
                util::db_to_gain(-6.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-24.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::gain_skew_factor(-24.0, 6.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(30.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            shape: EnumParam::new("Shape", Shape::Soft),
        }
    }
}

struct Distortion {
    params: Arc<DistortionParams>,
    sample_rate: f32,
    lp: [f32; 2],
}

impl Default for Distortion {
    fn default() -> Self {
        Self { params: Arc::new(DistortionParams::default()), sample_rate: 44100.0, lp: [0.0; 2] }
    }
}

impl Plugin for Distortion {
    const NAME: &'static str = "Distortion";
    const VENDOR: &'static str = "DAW Test";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, ()| {},
            move |ctx, setter, _state| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Distortion");
                    ui.add_space(6.0);
                    egui::Grid::new("controls").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("Drive");
                        ui.add(widgets::ParamSlider::for_param(&params.drive, setter));
                        ui.end_row();
                        ui.label("Tone");
                        ui.add(widgets::ParamSlider::for_param(&params.tone, setter));
                        ui.end_row();
                        ui.label("Mix");
                        ui.add(widgets::ParamSlider::for_param(&params.mix, setter));
                        ui.end_row();
                        ui.label("Output");
                        ui.add(widgets::ParamSlider::for_param(&params.output, setter));
                        ui.end_row();
                        ui.label("Shape");
                        ui.add(widgets::ParamSlider::for_param(&params.shape, setter));
                        ui.end_row();
                    });
                });
            },
        )
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        self.sample_rate = config.sample_rate;
        true
    }

    fn reset(&mut self) {
        self.lp = [0.0; 2];
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let p = &self.params;
        let shape = p.shape.value();
        for mut frame in buffer.iter_samples() {
            let gain = p.drive.smoothed.next();
            let mix = p.mix.smoothed.next();
            let out = p.output.smoothed.next();
            let a = lp_coeff(p.tone.value(), self.sample_rate);
            for (ch, s) in frame.iter_mut().enumerate() {
                *s = process_sample(*s, shape, gain, a, mix, out, &mut self.lp[ch & 1]);
            }
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for Distortion {
    const VST3_CLASS_ID: [u8; 16] = *b"DawTestDistortn1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

// The warning comes from the foreign macro `vst3_com::vtable` inside `nih_export_vst3!` (an extra `;`
// in an expression); it cannot be fixed in our code. The compiler ignores an attribute on the macro call itself,
// so the call is wrapped in a module and the warning is silenced only there.
#[allow(semicolon_in_expressions_from_non_local_macros, reason = "an extra `;` in the foreign vst3_com macro, not in our code")]
mod vst3_export {
    use super::Distortion;
    use nih_plug::nih_export_vst3;

    nih_export_vst3!(Distortion);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(x: f32, shape: Shape, gain: f32, tone_hz: f32, mix: f32, out: f32, n: usize) -> f32 {
        let a = lp_coeff(tone_hz, 48000.0);
        let mut lp = 0.0;
        let mut y = 0.0;
        for _ in 0..n {
            y = process_sample(x, shape, gain, a, mix, out, &mut lp);
        }
        y
    }

    #[test]
    fn silence_is_silence() {
        for s in [Shape::Soft, Shape::Hard, Shape::Fold] {
            assert_eq!(run(0.0, s, 100.0, 6000.0, 1.0, 1.0, 100), 0.0);
        }
    }

    #[test]
    fn drive0_mix0_is_passthrough() {
        let y = run(0.37, Shape::Soft, 1.0, 6000.0, 0.0, 1.0, 10);
        assert!((y - 0.37).abs() < 1e-6);
    }

    #[test]
    fn hard_clip_limits() {
        // Output 0 dB, mix 100%, filter settled.
        let y = run(0.9, Shape::Hard, 100.0, 12000.0, 1.0, 1.0, 5000);
        assert!(y <= 1.0 + 1e-5 && y > 0.99);
        let y = run(-0.9, Shape::Hard, 100.0, 12000.0, 1.0, 1.0, 5000);
        assert!((-1.0 - 1e-5..-0.99).contains(&y));
    }

    #[test]
    fn nan_inf_gives_zero() {
        for s in [Shape::Soft, Shape::Hard, Shape::Fold] {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                assert_eq!(run(bad, s, 10.0, 6000.0, 1.0, 1.0, 5), 0.0);
            }
        }
    }

    #[test]
    fn fold_bounded() {
        for i in -1000..1000 {
            let y = shape_fn(Shape::Fold, i as f32 * 0.1);
            assert!((-1.0..=1.0).contains(&y));
        }
    }
}
