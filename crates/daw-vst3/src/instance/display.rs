//! What the plugin shows: numbers and names in its value texts (`getParamStringByValue`). Pure logic.
//!
//! The VST3 "plain" value (`normalizedParamToPlain`) is not always the number a person sees: in nih-plug
//! gain is stored as a ratio but shown in decibels, a fraction is 0..1 but shown in percent; in JUCE the "plain"
//! value is simply the normalized one. Users and the model need the numbers the plugin shows,
//! so those are taken as the real values, and the plugin's conversion only when it matches them (it is more precise
//! than the rounded text) or when the text is not a number.

/// A number from a plugin text: the value, how many decimals are shown (for rounding tolerance) and what follows the number.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub value: f64,
    decimals: i32,
    pub suffix: String,
}

/// A number at the start of the text ("440 Hz", "-12.5 dB", "50%"); infinity ("-inf dB") and text ("Saw", "1/4") are not numbers.
pub fn shown_number(text: &str) -> Option<Shown> {
    let text = text.trim();
    let token = text.split_whitespace().next()?;
    let number = token.trim_end_matches('%');
    let value = number.parse::<f64>().ok().filter(|v| v.is_finite())?;
    let decimals = number.split_once('.').map_or(0, |(_, fraction)| fraction.len() as i32);
    let suffix = if number.len() < token.len() { "%" } else { text[token.len()..].trim() };
    Some(Shown { value, decimals, suffix: suffix.to_string() })
}

/// What to show for a stepped parameter.
#[derive(Debug, PartialEq)]
pub enum Steps {
    /// Numbers: a step's value is the number the plugin shows (or its index if the plugin is silent).
    Values(Vec<f64>),
    /// Names: a choice among options.
    Names(Vec<String>),
}

/// Steps with numbers ("-2", "12 st") are a number; with names ("Sine", "1/4") a choice.
pub fn classify_steps(texts: Vec<Option<String>>) -> Steps {
    let Some(texts) = texts.iter().cloned().collect::<Option<Vec<String>>>() else {
        return Steps::Values((0..texts.len()).map(|k| k as f64).collect());
    };
    match texts.iter().map(|text| shown_number(text).map(|shown| shown.value)).collect::<Option<Vec<f64>>>() {
        Some(values) => Steps::Values(values),
        None => Steps::Names(texts),
    }
}

/// Real values of a continuous parameter at the curve points, and their units.
#[derive(Debug, PartialEq)]
pub struct Curve {
    pub values: Vec<f64>,
    /// The units belong to these numbers; `false` when the plugin shows no numbers, so units do not go into the label.
    pub units_apply: bool,
}

/// Choosing between the plugin's conversion `plugin` and the shown numbers `shown` (at the same curve points).
pub fn pick_curve(plugin: Vec<f64>, shown: &[Option<Shown>]) -> Curve {
    let Some(shown) = shown.iter().map(Option::as_ref).collect::<Option<Vec<&Shown>>>() else {
        return Curve { values: plugin, units_apply: false };
    };
    if shown.len() == plugin.len() && shown.iter().zip(&plugin).all(|(s, &p)| within_rounding(s, p)) {
        return Curve { values: plugin, units_apply: true };
    }
    let values: Vec<f64> = shown.iter().map(|s| s.value).collect();
    let rising = values.windows(2).all(|w| w[1] >= w[0]);
    let falling = values.windows(2).all(|w| w[1] <= w[0]);
    if (rising || falling) && values.first() != values.last() {
        let (first, last) = (values[0], values[values.len() - 1]);
        let line: Vec<f64> = (0..values.len()).map(|i| first + (last - first) * i as f64 / (values.len() - 1) as f64).collect();
        let straight = shown.iter().zip(&line).all(|(s, &v)| within_rounding(s, v));
        Curve { values: if straight { line } else { values }, units_apply: true }
    } else {
        Curve { values: plugin, units_apply: false }
    }
}

/// The shown number is `plain` rounded to the shown decimals (with a margin for `f32` math).
fn within_rounding(shown: &Shown, plain: f64) -> bool {
    let half_digit = 0.5 * 10f64.powi(-shown.decimals);
    (shown.value - plain).abs() <= half_digit * 1.01 + 1e-5 * plain.abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(list: &[&str]) -> Vec<Option<String>> {
        list.iter().map(|n| Some((*n).to_string())).collect()
    }

    fn shown(list: &[&str]) -> Vec<Option<Shown>> {
        list.iter().map(|text| shown_number(text)).collect()
    }

    #[test]
    fn numbers_are_read_with_their_precision_and_units() {
        assert_eq!(shown_number(" 440 Hz"), Some(Shown { value: 440.0, decimals: 0, suffix: "Hz".into() }));
        assert_eq!(shown_number("-12.50 dB"), Some(Shown { value: -12.5, decimals: 2, suffix: "dB".into() }));
        assert_eq!(shown_number("50%"), Some(Shown { value: 50.0, decimals: 0, suffix: "%".into() }));
        assert_eq!(shown_number("0.25"), Some(Shown { value: 0.25, decimals: 2, suffix: String::new() }));
        assert_eq!(shown_number("-inf dB"), None);
        assert_eq!(shown_number("Saw"), None);
        assert_eq!(shown_number(""), None);
    }

    #[test]
    fn named_steps_are_a_choice_and_numeric_ones_a_number() {
        assert_eq!(
            classify_steps(texts(&["Sine", "Triangle", "Saw", "Square"])),
            Steps::Names(vec!["Sine".into(), "Triangle".into(), "Saw".into(), "Square".into()])
        );
        assert_eq!(classify_steps(texts(&["-2", "-1", "0", "+1", "2"])), Steps::Values(vec![-2.0, -1.0, 0.0, 1.0, 2.0]));
        assert_eq!(classify_steps(texts(&["0 st", "12 st"])), Steps::Values(vec![0.0, 12.0]));
        assert_eq!(classify_steps(texts(&["0%", "50%", "100%"])), Steps::Values(vec![0.0, 50.0, 100.0]));
        assert!(matches!(classify_steps(texts(&["1/4", "1/8"])), Steps::Names(_)), "bar fractions are names");
        assert!(matches!(classify_steps(texts(&["-inf dB", "0 dB"])), Steps::Names(_)));
        assert!(matches!(classify_steps(texts(&["1", "Off"])), Steps::Names(_)), "a single name makes it a choice");
    }

    #[test]
    fn silent_plugin_steps_are_numbered() {
        assert_eq!(classify_steps(vec![Some("A".into()), None, Some("C".into())]), Steps::Values(vec![0.0, 1.0, 2.0]));
    }

    #[test]
    fn plugin_values_win_when_they_are_what_the_plugin_shows() {
        let curve = pick_curve(vec![40.0, 1000.4, 18_000.0], &shown(&["40 Hz", "1000 Hz", "18000 Hz"]));
        assert_eq!(curve, Curve { values: vec![40.0, 1000.4, 18_000.0], units_apply: true }, "more precise than the rounded text");
        let seconds = pick_curve(vec![0.0005, 0.1], &shown(&["0.001", "0.100"]));
        assert_eq!(seconds.values, [0.0005, 0.1], "rounding to the shown decimals is not a mismatch");
    }

    #[test]
    fn shown_numbers_win_when_the_plugin_stores_something_else() {
        let percent = pick_curve(vec![0.0, 0.25, 0.5, 0.75, 1.0], &shown(&["0", "25", "50", "75", "100"]));
        assert_eq!(percent, Curve { values: vec![0.0, 25.0, 50.0, 75.0, 100.0], units_apply: true });
        let rounded = pick_curve(vec![0.0, 0.333, 0.667, 1.0], &shown(&["0", "33", "67", "100"]));
        let third = 100.0 / 3.0;
        assert_eq!(rounded.values, [0.0, third, 2.0 * third, 100.0], "rounding steps are smoothed into a straight line");
        let decibels = pick_curve(vec![1.0, 10.0, 100.0], &shown(&["0.0", "20.0", "40.0"]));
        assert_eq!(decibels.values, [0.0, 20.0, 40.0]);
    }

    #[test]
    fn plugin_values_without_units_when_the_text_is_not_a_usable_number() {
        let notes = pick_curve(vec![0.0, 0.5, 1.0], &[None, shown_number("C3"), None]);
        assert_eq!(notes, Curve { values: vec![0.0, 0.5, 1.0], units_apply: false });
        let jumpy = pick_curve(vec![0.0, 0.5, 1.0], &shown(&["980 Hz", "1.2 kHz", "5 kHz"]));
        assert_eq!(jumpy, Curve { values: vec![0.0, 0.5, 1.0], units_apply: false }, "non-monotonic numbers");
        let flat = pick_curve(vec![0.0, 0.5, 1.0], &shown(&["7", "7", "7"]));
        assert!(!flat.units_apply);
    }
}
