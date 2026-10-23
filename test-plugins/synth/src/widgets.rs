//! Parts of the "hardware" interface: section panel, 3D knob, button with LED, LCD display, level meter, keyboard.
//! All parts are drawn at a given position (center or rectangle) and ask the host to change parameters through `ParamSetter`.

use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui};
use std::f32::consts::PI;

use crate::engine::{oscillator, Waveform};

pub const CASE: Color32 = Color32::from_rgb(0x10, 0x12, 0x16);
pub const PANEL: Color32 = Color32::from_rgb(0x1c, 0x20, 0x28);
pub const PANEL_EDGE: Color32 = Color32::from_rgb(0x30, 0x36, 0x42);
pub const GRID: Color32 = Color32::from_rgb(0x2a, 0x30, 0x3d);
pub const TEXT: Color32 = Color32::from_rgb(0xd5, 0xdb, 0xe6);
pub const DIM: Color32 = Color32::from_rgb(0x7a, 0x84, 0x98);
/// Highlight color of knobs and buttons — amber, like classic synths.
pub const AMBER: Color32 = Color32::from_rgb(0xff, 0xa2, 0x3a);
pub const LCD_BG: Color32 = Color32::from_rgb(0x0b, 0x14, 0x10);
pub const LCD_TEXT: Color32 = Color32::from_rgb(0x6c, 0xf0, 0xa4);

/// A section with its title on a strip; returns the area for the content.
pub fn section(painter: &Painter, rect: Rect, title: &str) -> Rect {
    painter.rect_filled(rect, 7.0, PANEL);
    painter.rect_stroke(rect, 7.0, Stroke::new(1.0_f32, PANEL_EDGE), StrokeKind::Inside);
    painter.rect_filled(Rect::from_min_size(rect.min + vec2(10.0, 9.0), vec2(3.0, 10.0)), 1.0, AMBER);
    painter.text(rect.min + vec2(19.0, 7.0), Align2::LEFT_TOP, title, FontId::proportional(11.0), TEXT);
    Rect::from_min_max(rect.min + vec2(10.0, 28.0), rect.max - vec2(10.0, 8.0))
}

pub fn lcd(painter: &Painter, rect: Rect, top: &str, bottom: &str) {
    painter.rect_filled(rect, 4.0, LCD_BG);
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, Color32::from_rgb(0x05, 0x08, 0x07)), StrokeKind::Inside);
    painter.text(rect.min + vec2(10.0, 5.0), Align2::LEFT_TOP, top, FontId::monospace(11.0), LCD_TEXT.gamma_multiply(0.6));
    painter.text(rect.min + vec2(10.0, 19.0), Align2::LEFT_TOP, bottom, FontId::monospace(15.0), LCD_TEXT);
}

const ARC_START: f32 = 0.75 * PI;
const ARC_SWEEP: f32 = 1.5 * PI;

fn polar(center: Pos2, radius: f32, angle: f32) -> Pos2 {
    center + radius * vec2(angle.cos(), angle.sin())
}

fn arc(center: Pos2, radius: f32, to_value: f32) -> Vec<Pos2> {
    let steps = ((to_value * 48.0).ceil() as usize).max(2);
    (0..=steps).map(|i| polar(center, radius, ARC_START + ARC_SWEEP * to_value * i as f32 / steps as f32)).collect()
}

/// How far the normalized value moves for `drag_pixels` of mouse movement up (down is minus).
pub fn drag_step(drag_pixels: f32, fine: bool) -> f32 {
    -drag_pixels * if fine { 0.0005 } else { 0.005 }
}

/// A 3D knob: a scale with ticks, an amber value arc, a pointer.
/// Drag up/down to change, Shift for fine, double-click for the default.
/// While the knob is touched, its name and value go to `readout` (shown by the LCD).
pub fn knob<P: Param>(ui: &mut Ui, setter: &ParamSetter, param: &P, label: &str, center: Pos2, radius: f32) {
    let hit = Rect::from_center_size(center, vec2(radius * 2.0 + 24.0, radius * 2.0 + 24.0));
    let response = ui.interact(hit, ui.id().with(label), Sense::click_and_drag());
    if response.drag_started() {
        setter.begin_set_parameter(param);
    }
    if response.dragged() {
        let step = drag_step(response.drag_delta().y, ui.input(|i| i.modifiers.shift));
        setter.set_parameter_normalized(param, (param.unmodulated_normalized_value() + step).clamp(0.0, 1.0));
    }
    if response.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if response.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, param.default_normalized_value());
        setter.end_set_parameter(param);
    }

    let value = param.unmodulated_normalized_value();
    let text = param.normalized_value_to_string(value, true);
    if response.hovered() || response.dragged() {
        crate::editor::set_readout(ui.ctx(), label, &text);
    }

    let p = ui.painter();
    for i in 0..=10 {
        let angle = ARC_START + ARC_SWEEP * i as f32 / 10.0;
        p.line_segment([polar(center, radius + 7.0, angle), polar(center, radius + 11.0, angle)], Stroke::new(1.0_f32, DIM));
    }
    p.add(Shape::line(arc(center, radius + 5.0, 1.0), Stroke::new(3.0_f32, GRID)));
    if value > 0.001 {
        p.add(Shape::line(arc(center, radius + 5.0, value), Stroke::new(3.0_f32, AMBER)));
    }
    p.circle_filled(center + vec2(0.0, 2.5), radius + 1.0, Color32::from_black_alpha(110));
    p.circle_filled(center, radius, Color32::from_rgb(0x4a, 0x50, 0x5e));
    p.circle_filled(center + vec2(0.0, 0.8), radius - 1.5, Color32::from_rgb(0x2a, 0x2e, 0x38));
    p.circle_filled(center, radius * 0.78, Color32::from_rgb(0x3a, 0x3f, 0x4c));
    p.circle_stroke(center, radius * 0.78, Stroke::new(1.0_f32, Color32::from_rgb(0x55, 0x5c, 0x6c)));
    let angle = ARC_START + ARC_SWEEP * value;
    p.line_segment([polar(center, radius * 0.25, angle), polar(center, radius * 0.72, angle)], Stroke::new(2.6_f32, Color32::WHITE));
    p.text(center + vec2(0.0, radius + 22.0), Align2::CENTER_TOP, label, FontId::proportional(10.0), DIM);
}

fn wave_glyph(waveform: Waveform, area: Rect) -> Vec<Pos2> {
    (0..=40)
        .map(|i| {
            let t = i as f32 / 40.0;
            let y = 0.5 + 0.5 * oscillator(waveform, (t * std::f32::consts::TAU).min(std::f32::consts::TAU - 1e-4));
            pos2(area.left() + t * area.width(), area.bottom() - y * area.height())
        })
        .collect()
}

/// A push button with a waveform; the selected one is "pressed in" and its LED is lit.
pub fn wave_button(ui: &mut Ui, rect: Rect, id: &str, waveform: Waveform, selected: bool) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    let p = ui.painter();
    let (top, bottom) = if selected { (0x20, 0x26) } else { (0x3c, 0x30) };
    p.rect_filled(rect.translate(vec2(0.0, 1.5)), 5.0, Color32::from_black_alpha(100));
    p.rect_filled(rect, 5.0, Color32::from_rgb(top + 0x10, top + 0x14, top + 0x1c));
    p.rect_filled(
        rect.shrink2(vec2(1.0, 1.0)).with_max_y(rect.center().y),
        4.0,
        Color32::from_rgb(top + 0x14, top + 0x18, top + 0x22).gamma_multiply(if selected { 0.0 } else { 1.0 }),
    );
    p.rect_filled(
        rect.shrink(2.0),
        4.0,
        Color32::from_rgb(bottom, bottom + 4, bottom + 10).gamma_multiply(if response.hovered() { 1.25 } else { 1.0 }),
    );
    let led = pos2(rect.right() - 8.0, rect.top() + 8.0);
    p.circle_filled(led, 3.2, if selected { AMBER } else { Color32::from_rgb(0x2b, 0x1d, 0x10) });
    if selected {
        p.circle_filled(led, 6.0, AMBER.gamma_multiply(0.18));
    }
    let glyph = Rect::from_min_max(rect.min + vec2(9.0, 13.0), rect.max - vec2(9.0, 9.0));
    p.add(Shape::line(wave_glyph(waveform, glyph), Stroke::new(1.8_f32, if selected { AMBER } else { DIM })));
    response
}

/// A vertical meter of LED segments: green, yellow, red at the top.
pub fn meter(painter: &Painter, rect: Rect, level: f32) {
    const SEGMENTS: usize = 14;
    let gap = 2.0;
    let height = (rect.height() - gap * (SEGMENTS as f32 - 1.0)) / SEGMENTS as f32;
    let lit = (level.clamp(0.0, 1.0) * SEGMENTS as f32).round() as usize;
    for i in 0..SEGMENTS {
        let from_top = SEGMENTS - 1 - i;
        let color = match i {
            0..=8 => Color32::from_rgb(0x4c, 0xd9, 0x7a),
            9..=11 => Color32::from_rgb(0xf2, 0xc9, 0x4c),
            _ => Color32::from_rgb(0xf0, 0x5a, 0x4a),
        };
        let top = rect.top() + from_top as f32 * (height + gap);
        let segment = Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), height));
        painter.rect_filled(segment, 1.5, if i < lit { color } else { color.gamma_multiply(0.14) });
    }
}

fn is_black(note: u8) -> bool {
    matches!(note % 12, 1 | 3 | 6 | 8 | 10)
}

/// A keyboard over `notes`; keys from the `active` mask are highlighted.
pub fn keyboard(painter: &Painter, rect: Rect, notes: std::ops::RangeInclusive<u8>, active: u128) {
    let white_count = notes.clone().filter(|&n| !is_black(n)).count() as f32;
    let white_width = rect.width() / white_count;
    let lit = |note: u8| active & (1u128 << note) != 0;
    painter.rect_filled(rect.expand(2.0), 3.0, Color32::from_rgb(0x08, 0x09, 0x0c));

    let mut index = 0.0;
    for note in notes.clone().filter(|&n| !is_black(n)) {
        let key = Rect::from_min_size(pos2(rect.left() + index * white_width, rect.top()), vec2(white_width - 1.0, rect.height()));
        painter.rect_filled(key, 2.0, if lit(note) { AMBER } else { Color32::from_rgb(0xe8, 0xeb, 0xf1) });
        index += 1.0;
    }
    let mut index = 0.0;
    for note in notes {
        if is_black(note) {
            let width = white_width * 0.62;
            let key =
                Rect::from_min_size(pos2(rect.left() + index * white_width - width / 2.0, rect.top()), vec2(width, rect.height() * 0.62));
            painter.rect_filled(key, 2.0, if lit(note) { AMBER.gamma_multiply(0.85) } else { Color32::from_rgb(0x15, 0x17, 0x1d) });
        } else {
            index += 1.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_up_raises_the_value_and_shift_is_ten_times_finer() {
        assert!(drag_step(-10.0, false) > 0.0);
        assert!(drag_step(10.0, false) < 0.0);
        assert!((drag_step(-10.0, true) * 10.0 - drag_step(-10.0, false)).abs() < 1e-6);
    }

    #[test]
    fn black_keys_are_the_five_sharps() {
        let blacks: Vec<u8> = (60..72).filter(|&n| is_black(n)).collect();
        assert_eq!(blacks, [61, 63, 66, 68, 70]);
    }
}
