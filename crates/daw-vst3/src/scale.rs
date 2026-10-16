//! `ParamScale`: "convert a plugin parameter value between real units and normalized 0..1".
//!
//! A VST3 plugin accepts only normalized values, while the project and the interface keep real ones (hertz, seconds,
//! choice index). The "normalized -> real" conversion is known only to the plugin's controller (`normalizedParamToPlain`,
//! value texts), but values are sent to the plugin from the audio thread, where the controller cannot be called. So the
//! conversion table is built in advance on the control side (`instance::params`), and here is pure math without allocations:
//! it is used both by the audio thread (`set_param`) and by the plugin window (values from the window to the project).

/// How many curve points to take for a continuous parameter. 129 points give an error below 0.1% even for a frequency
/// of 40..18000 Hz with a logarithmic knob, and a table for 128 parameters takes ~260 KB.
pub const CURVE_POINTS: usize = 129;
/// More choices than this is no longer a list but a number: such a parameter is treated as continuous.
pub const MAX_STEPS: i32 = 256;
/// Deviation from a straight line (as a fraction of the range) up to which a curve counts as straight: plugins compute in `f32`.
const LINEAR_TOLERANCE: f64 = 1e-5;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    normalized: f64,
    plain: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamScale {
    /// By ascending normalized value; the real values of a continuous curve are monotonic.
    points: Vec<Point>,
    /// A stepped parameter: values are only table points (the nearest), nothing in between.
    stepped: bool,
}

impl ParamScale {
    /// The real value equals the normalized one (0..1): this is how parameters without a conversion behave.
    pub fn identity() -> Self {
        Self::line(0.0, 1.0)
    }

    fn line(min: f64, max: f64) -> Self {
        Self { points: vec![Point { normalized: 0.0, plain: min }, Point { normalized: 1.0, plain: max }], stepped: false }
    }

    /// Steps `0, 1/n, …, 1` with real values `plains` (`n + 1` of them). Non-numbers mean no conversion.
    pub fn stepped(plains: &[f64]) -> Self {
        let Some(last) = plains.len().checked_sub(1).filter(|&n| n > 0) else {
            return Self::identity();
        };
        if plains.iter().any(|v| !v.is_finite()) {
            return Self::identity();
        }
        let points = plains.iter().enumerate().map(|(k, &plain)| Point { normalized: k as f64 / last as f64, plain }).collect();
        Self { points, stepped: true }
    }

    /// Steps with choice indexes `0..n` (choice and switch).
    pub fn indexed(steps: usize) -> Self {
        let plains: Vec<f64> = (0..=steps).map(|k| k as f64).collect();
        Self::stepped(&plains)
    }

    /// The normalized values at which continuous curve points are taken: `0, 1/128, …, 1`.
    pub fn curve_positions() -> impl Iterator<Item = f64> {
        (0..CURVE_POINTS).map(|i| i as f64 / (CURVE_POINTS - 1) as f64)
    }

    /// A continuous parameter from the `to_plain` conversion (see [`ParamScale::curve`]).
    #[cfg(test)]
    pub fn continuous(to_plain: impl Fn(f64) -> f64) -> Self {
        Self::curve(&Self::curve_positions().map(to_plain).collect::<Vec<_>>())
    }

    /// A continuous parameter from real values at the points [`ParamScale::curve_positions`]. A straight line is kept as two
    /// points, a curve as all of them. A non-monotonic, flat or non-number curve is unfit for the reverse conversion:
    /// then there is no conversion.
    pub fn curve(plains: &[f64]) -> Self {
        if plains.len() != CURVE_POINTS {
            return Self::identity();
        }
        let points: Vec<Point> = Self::curve_positions().zip(plains).map(|(normalized, &plain)| Point { normalized, plain }).collect();
        let (first, end) = (points[0].plain, points[CURVE_POINTS - 1].plain);
        let rising = points.windows(2).all(|w| w[1].plain >= w[0].plain);
        let falling = points.windows(2).all(|w| w[1].plain <= w[0].plain);
        if points.iter().any(|p| !p.plain.is_finite()) || first == end || !(rising || falling) {
            return Self::identity();
        }
        let span = end - first;
        let straight = points.iter().all(|p| (p.plain - (first + span * p.normalized)).abs() <= LINEAR_TOLERANCE * span.abs());
        if straight { Self::line(first, end) } else { Self { points, stepped: false } }
    }

    /// The smallest and largest real value.
    pub fn range(&self) -> (f64, f64) {
        self.points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.plain), hi.max(p.plain)))
    }

    /// Normalized -> real (outside 0..1 it is clamped).
    pub fn to_plain(&self, normalized: f64) -> f64 {
        let normalized = normalized.clamp(0.0, 1.0);
        if self.stepped {
            let last = self.points.len() - 1;
            let index = ((normalized * last as f64).round() as usize).min(last);
            return self.points[index].plain;
        }
        let after = self.points.partition_point(|p| p.normalized < normalized);
        interpolate(&self.points, after, |p| p.normalized, |p| p.plain, normalized)
    }

    /// Real -> normalized (outside the range it is clamped; a stepped one snaps to the nearest step).
    pub fn to_normalized(&self, plain: f64) -> f64 {
        if self.stepped {
            let nearest = self.points.iter().min_by(|a, b| (a.plain - plain).abs().total_cmp(&(b.plain - plain).abs()));
            return nearest.map_or(0.0, |p| p.normalized);
        }
        let (lo, hi) = self.range();
        let plain = plain.clamp(lo, hi);
        let rising = self.points[self.points.len() - 1].plain >= self.points[0].plain;
        let after = self.points.partition_point(|p| if rising { p.plain < plain } else { p.plain > plain });
        interpolate(&self.points, after, |p| p.plain, |p| p.normalized, plain)
    }
}

/// Linear interpolation between points `after - 1` and `after` (find `y` by key `x`).
fn interpolate(points: &[Point], after: usize, x: impl Fn(&Point) -> f64, y: impl Fn(&Point) -> f64, at: f64) -> f64 {
    let Some(b) = points.get(after) else {
        return points.last().map_or(0.0, &y);
    };
    let Some(a) = after.checked_sub(1).and_then(|i| points.get(i)) else {
        return y(b);
    };
    let span = x(b) - x(a);
    if span == 0.0 { y(a) } else { y(a) + (at - x(a)) / span * (y(b) - y(a)) }
}

/// Conversion table of all exposed parameters of one plugin instance: id -> conversion.
/// Lookup without allocations — fit for the audio thread.
#[derive(Debug, Default)]
pub struct ParamScales {
    /// By ascending id.
    by_id: Vec<(u32, ParamScale)>,
}

impl ParamScales {
    pub fn new(mut by_id: Vec<(u32, ParamScale)>) -> Self {
        by_id.sort_by_key(|(id, _)| *id);
        Self { by_id }
    }

    fn get(&self, id: u32) -> Option<&ParamScale> {
        self.by_id.binary_search_by_key(&id, |(known, _)| *known).ok().map(|index| &self.by_id[index].1)
    }

    /// Real value of parameter `id` -> normalized. A parameter missing from the table (the project does not know it)
    /// is treated as normalized: the value is only clamped to 0..1.
    pub fn to_normalized(&self, id: u32, plain: f64) -> f64 {
        self.get(id).map_or(plain.clamp(0.0, 1.0), |scale| scale.to_normalized(plain))
    }

    /// Normalized value of parameter `id` -> real; not in the table gives the same number.
    pub fn to_plain(&self, id: u32, normalized: f64) -> f64 {
        self.get(id).map_or(normalized, |scale| scale.to_plain(normalized))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    /// Like the synth's cutoff frequency: 40..18000 Hz skewed (nih-plug `Skewed`, factor = 2^-2).
    fn skewed(normalized: f64) -> f64 {
        40.0 + normalized.powf(4.0) * (18_000.0 - 40.0)
    }

    #[test]
    fn linear_parameters_are_two_points_and_exact() {
        let scale = ParamScale::continuous(|n| -24.0 + 30.0 * n);
        assert_eq!(scale.points.len(), 2, "a straight line is kept by its ends");
        assert_eq!(scale.range(), (-24.0, 6.0));
        assert!(close(scale.to_normalized(-9.0), 0.5, 1e-12));
        assert!(close(scale.to_plain(0.25), -16.5, 1e-12));
        assert_eq!(ParamScale::continuous(|n| n), ParamScale::identity());
    }

    #[test]
    fn skewed_parameters_follow_the_plugin_curve_both_ways() {
        let scale = ParamScale::continuous(skewed);
        assert_eq!(scale.points.len(), CURVE_POINTS);
        assert_eq!(scale.range(), (40.0, 18_000.0));
        for normalized in [0.0, 0.1, 0.33, 0.5, 0.77, 1.0] {
            let plain = skewed(normalized);
            assert!(close(scale.to_plain(normalized), plain, plain * 2e-3 + 0.05), "{normalized}: {}", scale.to_plain(normalized));
            assert!(close(scale.to_normalized(plain), normalized, 2e-3), "{plain}: {}", scale.to_normalized(plain));
            assert!(close(scale.to_normalized(scale.to_plain(normalized)), normalized, 1e-9), "there and back");
        }
    }

    #[test]
    fn falling_curves_are_inverted_too() {
        let scale = ParamScale::continuous(|n| 100.0 * (1.0 - n * n));
        assert_eq!(scale.range(), (0.0, 100.0));
        assert!(close(scale.to_normalized(75.0), 0.5, 1e-3));
        assert!(close(scale.to_normalized(100.0), 0.0, 1e-12));
        assert!(close(scale.to_normalized(-5.0), 1.0, 1e-12));
    }

    #[test]
    fn unusable_curves_fall_back_to_normalized_values() {
        assert_eq!(ParamScale::continuous(|_| 5.0), ParamScale::identity(), "flat");
        assert_eq!(ParamScale::continuous(|n| (n * 6.0).sin()), ParamScale::identity(), "non-monotonic");
        assert_eq!(ParamScale::continuous(|n| if n > 0.5 { f64::NAN } else { n }), ParamScale::identity(), "non-numbers");
        assert_eq!(ParamScale::stepped(&[1.0]), ParamScale::identity(), "a single step");
    }

    #[test]
    fn stepped_parameters_snap_to_the_nearest_step() {
        let waveform = ParamScale::indexed(3);
        assert_eq!(waveform.range(), (0.0, 3.0));
        assert!(close(waveform.to_normalized(2.0), 2.0 / 3.0, 1e-12));
        assert!(close(waveform.to_normalized(1.4), 1.0 / 3.0, 1e-12), "between choices, the nearest");
        assert_eq!(waveform.to_plain(0.6), 2.0);
        assert_eq!(waveform.to_plain(0.1), 0.0);
        let octave = ParamScale::stepped(&[-2.0, -1.0, 0.0, 1.0, 2.0]);
        assert_eq!(octave.to_normalized(1.0), 0.75);
        assert_eq!(octave.to_plain(0.25), -1.0);
    }

    #[test]
    fn values_out_of_range_are_clamped() {
        let cutoff = ParamScale::continuous(skewed);
        assert_eq!(cutoff.to_normalized(5.0), 0.0);
        assert_eq!(cutoff.to_normalized(1e9), 1.0);
        assert_eq!(cutoff.to_plain(-1.0), 40.0);
        assert_eq!(cutoff.to_plain(2.0), 18_000.0);
        let waveform = ParamScale::indexed(3);
        assert_eq!(waveform.to_normalized(-7.0), 0.0);
        assert_eq!(waveform.to_normalized(42.0), 1.0);
        let switch = ParamScale::indexed(1);
        assert_eq!((switch.to_normalized(0.4), switch.to_normalized(0.6)), (0.0, 1.0));
    }

    #[test]
    fn table_looks_up_by_id_and_unknown_ids_stay_normalized() {
        let scales = ParamScales::new(vec![(9, ParamScale::indexed(3)), (2, ParamScale::continuous(|n| 10.0 * n))]);
        assert!(close(scales.to_normalized(2, 5.0), 0.5, 1e-12));
        assert_eq!(scales.to_normalized(9, 3.0), 1.0);
        assert_eq!(scales.to_plain(9, 1.0), 3.0);
        assert_eq!(scales.to_normalized(77, 3.0), 1.0, "an unknown parameter is treated as normalized, clamped");
        assert_eq!(scales.to_plain(77, 0.3), 0.3);
    }
}
