//! Curves for the window: envelope, filter response, oscilloscope. Geometry is pure functions (tested), drawing a thin wrapper over them.

use nih_plug_egui::egui::{pos2, Color32, Painter, Pos2, Rect, Shape, Stroke};

use crate::engine::Settings;
use crate::widgets::{AMBER, GRID, LCD_BG, LCD_TEXT};

/// The AHDSR envelope as a polyline in the unit square (x left to right, y bottom to top).
/// Stage width ~ √time: short stages stay visible next to long ones; the sustain part is nominal.
pub fn envelope_points(s: &Settings) -> Vec<(f32, f32)> {
    let widths = [s.attack, s.hold, s.decay, s.release].map(f32::sqrt);
    let sustain_width = widths.iter().sum::<f32>() / 4.0 + 0.1;
    let total = widths.iter().sum::<f32>() + sustain_width;
    let at = |x: f32| x / total;
    let [a, h, d, r] = widths;
    vec![
        (0.0, 0.0),
        (at(a), 1.0),
        (at(a + h), 1.0),
        (at(a + h + d), s.sustain),
        (at(a + h + d + sustain_width), s.sustain),
        (at(a + h + d + sustain_width + r), 0.0),
    ]
}

/// Filter response in dB at frequency `hz` with cutoff `cutoff_hz` (one-pole lowpass).
pub fn filter_db(hz: f32, cutoff_hz: f32) -> f32 {
    -10.0 * (1.0 + (hz / cutoff_hz).powi(2)).log10()
}

const FILTER_MIN_HZ: f32 = 20.0;
const FILTER_MAX_HZ: f32 = 20_000.0;
const FILTER_FLOOR_DB: f32 = -36.0;

/// Position of a frequency on a logarithmic `0..1` axis.
pub fn hz_to_x(hz: f32) -> f32 {
    (hz / FILTER_MIN_HZ).log10() / (FILTER_MAX_HZ / FILTER_MIN_HZ).log10()
}

/// Start index of a stable piece of the oscilloscope: a rising zero crossing in the first half.
pub fn scope_trigger(scope: &[f32]) -> usize {
    (1..scope.len() / 2).find(|&i| scope[i - 1] <= 0.0 && scope[i] > 0.0).unwrap_or(0)
}

/// Peak of the latest oscilloscope samples.
pub fn peak(scope: &[f32]) -> f32 {
    scope.iter().fold(0.0_f32, |a, v| a.max(v.abs()))
}

/// Peak level for the meter `0..1`: a decibel scale from −48 dB to 0 dB.
pub fn level_from_peak(peak: f32) -> f32 {
    ((20.0 * peak.max(1e-4).log10() + 48.0) / 48.0).clamp(0.0, 1.0)
}

fn to_screen(plot: Rect, x: f32, y: f32) -> Pos2 {
    pos2(plot.left() + x * plot.width(), plot.bottom() - y * plot.height())
}

/// The dark "glass" of the display with a grid.
fn display(painter: &Painter, plot: Rect) {
    painter.rect_filled(plot.expand(3.0), 4.0, LCD_BG);
    for i in 1..4 {
        let y = i as f32 / 4.0;
        painter.line_segment([to_screen(plot, 0.0, y), to_screen(plot, 1.0, y)], Stroke::new(1.0_f32, GRID.gamma_multiply(0.6)));
        painter.line_segment([to_screen(plot, y, 0.0), to_screen(plot, y, 1.0)], Stroke::new(1.0_f32, GRID.gamma_multiply(0.6)));
    }
}

fn draw_line(painter: &Painter, plot: Rect, points: &[(f32, f32)], color: Color32) {
    let points: Vec<Pos2> = points.iter().map(|&(x, y)| to_screen(plot, x, y)).collect();
    painter.add(Shape::line(points.clone(), Stroke::new(5.0_f32, color.gamma_multiply(0.18))));
    painter.add(Shape::line(points, Stroke::new(1.6_f32, color)));
}

pub fn draw_envelope(painter: &Painter, plot: Rect, settings: &Settings) {
    display(painter, plot);
    draw_line(painter, plot, &envelope_points(settings), LCD_TEXT);
}

pub fn draw_filter(painter: &Painter, plot: Rect, settings: &Settings) {
    display(painter, plot);
    let response: Vec<(f32, f32)> = (0..=96)
        .map(|i| {
            let x = i as f32 / 96.0;
            let hz = FILTER_MIN_HZ * (FILTER_MAX_HZ / FILTER_MIN_HZ).powf(x);
            (x, 1.0 - filter_db(hz, settings.cutoff_hz) / FILTER_FLOOR_DB)
        })
        .collect();
    draw_line(painter, plot, &response, AMBER);
    let x = hz_to_x(settings.cutoff_hz).clamp(0.0, 1.0);
    painter.line_segment([to_screen(plot, x, 0.0), to_screen(plot, x, 1.0)], Stroke::new(1.0_f32, AMBER.gamma_multiply(0.45)));
}

pub fn draw_scope(painter: &Painter, plot: Rect, scope: &[f32]) {
    display(painter, plot);
    let start = scope_trigger(scope);
    let shown = &scope[start..start + scope.len() / 2];
    let scale = 0.9 / peak(shown).max(0.045);
    let n = shown.len() as f32 - 1.0;
    let points: Vec<(f32, f32)> = shown.iter().enumerate().map(|(i, v)| (i as f32 / n, 0.5 + 0.5 * v * scale)).collect();
    painter.line_segment([to_screen(plot, 0.0, 0.5), to_screen(plot, 1.0, 0.5)], Stroke::new(1.0_f32, GRID));
    draw_line(painter, plot, &points, LCD_TEXT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_starts_at_zero_peaks_at_one_and_ends_at_zero() {
        let points = envelope_points(&Settings::default());
        assert_eq!(points.first(), Some(&(0.0, 0.0)));
        assert_eq!(points[1].1, 1.0);
        assert_eq!(points.last().map(|p| p.1), Some(0.0));
        assert!((points.last().map_or(0.0, |p| p.0) - 1.0).abs() < 1e-5);
        assert!(points.windows(2).all(|w| w[0].0 <= w[1].0), "time does not go backwards");
    }

    #[test]
    fn envelope_sustain_level_is_drawn_at_its_height() {
        let settings = Settings { sustain: 0.3, ..Settings::default() };
        let points = envelope_points(&settings);
        assert_eq!((points[3].1, points[4].1), (0.3, 0.3));
    }

    #[test]
    fn filter_is_flat_below_cutoff_and_minus_3_db_at_it() {
        assert!(filter_db(20.0, 5_000.0).abs() < 0.01);
        assert!((filter_db(1_000.0, 1_000.0) + 3.01).abs() < 0.01);
        assert!(filter_db(10_000.0, 1_000.0) < -15.0);
    }

    #[test]
    fn cutoff_marker_sits_between_the_ends_of_the_axis() {
        assert!(hz_to_x(20.0).abs() < 1e-5 && (hz_to_x(20_000.0) - 1.0).abs() < 1e-5);
        assert!((hz_to_x(632.5) - 0.5).abs() < 0.01);
    }

    #[test]
    fn level_is_zero_for_silence_and_one_at_full_scale() {
        assert_eq!(level_from_peak(0.0), 0.0);
        assert!((level_from_peak(1.0) - 1.0).abs() < 1e-6);
        assert!((level_from_peak(0.0631) - 0.5).abs() < 0.01, "−24 dB is the middle of the scale");
    }

    #[test]
    fn scope_trigger_finds_rising_zero_crossing() {
        let scope = [-0.5, -0.2, 0.3, 0.8, 0.2, -0.3, -0.6, -0.1];
        assert_eq!(scope_trigger(&scope), 2);
        assert_eq!(scope_trigger(&[0.5; 8]), 0);
    }
}
