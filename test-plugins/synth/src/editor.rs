//! The synth window (egui): sections laid out like a regular synth — OSC, FILTER, OUTPUT, AMP ENVELOPE, keyboard.
//! The window only shows and asks the host to change parameters (`ParamSetter`); it does not own the sound.

use nih_plug::prelude::*;
use nih_plug_egui::create_egui_editor;
use nih_plug_egui::egui::{self, pos2, vec2, Color32, Context, Id, Rect};
use std::sync::{Arc, Mutex};

use crate::curves::{draw_envelope, draw_filter, draw_scope, level_from_peak, peak};
use crate::engine::Waveform;
use crate::params::{SynthParams, WaveParam};
use crate::visuals::{self, Visuals};
use crate::widgets::{keyboard, knob, lcd, meter, section, wave_button, AMBER, CASE, TEXT};

pub const WIDTH: u32 = 720;
pub const HEIGHT: u32 = 440;

const KEY_RANGE: std::ops::RangeInclusive<u8> = 36..=96;

/// What the LCD shows: the name and value of the knob being touched now.
pub fn set_readout(ctx: &Context, name: &str, value: &str) {
    ctx.data_mut(|d| d.insert_temp(Id::new("readout"), (name.to_string(), value.to_string())));
}

fn take_readout(ctx: &Context) -> Option<(String, String)> {
    ctx.data_mut(|d| {
        let readout = d.get_temp::<(String, String)>(Id::new("readout"));
        d.remove::<(String, String)>(Id::new("readout"));
        readout
    })
}

/// Level for the meter: instant, falling smoothly.
fn smoothed_level(ctx: &Context, level_now: f32) -> f32 {
    let id = Id::new("level");
    let level = level_now.max(ctx.data(|d| d.get_temp::<f32>(id)).unwrap_or(0.0) * 0.9);
    ctx.data_mut(|d| d.insert_temp(id, level));
    level
}

/// The lamp in the header: lit while the synth sounds.
fn activity_led(painter: &egui::Painter, center: egui::Pos2, on: bool) {
    painter.circle_filled(center, 4.5, if on { AMBER } else { Color32::from_rgb(0x2b, 0x1d, 0x10) });
    if on {
        painter.circle_filled(center, 9.0, AMBER.gamma_multiply(0.2));
    }
}

pub fn create(params: Arc<SynthParams>, own_visuals: Arc<Visuals>) -> Option<Box<dyn Editor>> {
    // Which plugin instance is shown now (the window may be open on a different one than the one sounding).
    let followed = Mutex::new(None::<Arc<Visuals>>);
    create_egui_editor(
        params.editor_state.clone(),
        (),
        |_, ()| {},
        move |ctx, setter, ()| {
            // The oscilloscope, meter and keyboard are live: draw frame after frame.
            ctx.request_repaint();
            let visuals = visuals::follow(&own_visuals, &mut followed.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
            let readout = take_readout(ctx);
            egui::CentralPanel::default().frame(egui::Frame::new().fill(CASE)).show(ctx, |ui| {
                let origin = ui.max_rect().min + vec2(14.0, 12.0);
                let area = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(origin + vec2(x, y), vec2(w, h));
                let settings = params.current_settings();
                let painter = ui.painter().clone();

                // Header: name and LCD.
                painter.text(origin + vec2(2.0, 4.0), egui::Align2::LEFT_TOP, "SYNTH", egui::FontId::proportional(26.0), TEXT);
                activity_led(&painter, origin + vec2(112.0, 18.0), visuals.is_sounding());
                let (name, value) = readout.unwrap_or_else(|| ("WAVEFORM".into(), format!("{:?}", settings.waveform).to_uppercase()));
                lcd(&painter, area(430.0, 0.0, 262.0, 40.0), &name, &value);

                // Row 1: OSC, FILTER, OUTPUT.
                let inner = section(&painter, area(0.0, 52.0, 250.0, 154.0), "OSCILLATOR");
                let current = params.waveform.value();
                for (i, (wave, shape)) in [
                    (WaveParam::Sine, Waveform::Sine),
                    (WaveParam::Triangle, Waveform::Triangle),
                    (WaveParam::Saw, Waveform::Saw),
                    (WaveParam::Square, Waveform::Square),
                ]
                .into_iter()
                .enumerate()
                {
                    let cell = Rect::from_min_size(inner.min + vec2((i % 2) as f32 * 58.0, (i / 2) as f32 * 58.0), vec2(52.0, 52.0));
                    if wave_button(ui, cell, &format!("wave{i}"), shape, wave == current).clicked() {
                        setter.begin_set_parameter(&params.waveform);
                        setter.set_parameter(&params.waveform, wave);
                        setter.end_set_parameter(&params.waveform);
                    }
                }
                let scope = visuals.scope();
                draw_scope(&painter, Rect::from_min_size(inner.min + vec2(126.0, 3.0), vec2(inner.width() - 130.0, 106.0)), &scope);

                let inner = section(&painter, area(258.0, 52.0, 250.0, 154.0), "FILTER");
                draw_filter(&painter, Rect::from_min_size(inner.min + vec2(3.0, 3.0), vec2(120.0, 106.0)), &settings);
                knob(ui, setter, &params.cutoff, "CUTOFF", pos2(inner.right() - 48.0, inner.min.y + 42.0), 24.0);

                let inner = section(&painter, area(516.0, 52.0, 176.0, 154.0), "OUTPUT");
                knob(ui, setter, &params.gain, "GAIN", pos2(inner.min.x + 52.0, inner.min.y + 42.0), 24.0);
                let level = smoothed_level(ctx, level_from_peak(peak(&scope)));
                meter(&painter, Rect::from_min_size(pos2(inner.right() - 22.0, inner.min.y + 3.0), vec2(14.0, 106.0)), level);

                // Row 2: amp envelope.
                let inner = section(&painter, area(0.0, 214.0, 692.0, 138.0), "AMP ENVELOPE");
                draw_envelope(&painter, Rect::from_min_size(inner.min + vec2(3.0, 3.0), vec2(230.0, 92.0)), &settings);
                let knobs: [(&FloatParam, &str); 5] = [
                    (&params.attack, "ATTACK"),
                    (&params.hold, "HOLD"),
                    (&params.decay, "DECAY"),
                    (&params.sustain, "SUSTAIN"),
                    (&params.release, "RELEASE"),
                ];
                let first = inner.min.x + 290.0;
                let step = (inner.right() - 40.0 - first) / 4.0;
                for (i, (param, label)) in knobs.into_iter().enumerate() {
                    knob(ui, setter, param, label, pos2(first + step * i as f32, inner.min.y + 38.0), 21.0);
                }

                keyboard(&painter, area(0.0, 364.0, 692.0, 52.0), KEY_RANGE, visuals.notes());
            });
        },
    )
}
