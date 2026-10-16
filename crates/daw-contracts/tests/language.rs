//! The current language switches `tr!` and `L10n` texts. A separate test program is a separate process: the language is
//! global, and switching it here must not race with tests that expect the default (English) texts.
#![allow(clippy::unwrap_used, reason = "tests: a failure with a clear message is the check")]

use daw_contracts::i18n::{self, Language};
use daw_contracts::{KindInfo, L10n, ParamInfo, ParamKind, tr};

const RATES: [L10n; 2] = [L10n { en: "Off", ru: "Выкл" }, L10n::same("1/4")];

fn kind() -> KindInfo {
    let rate = ParamInfo {
        name: "rate",
        label: L10n { en: "Rate", ru: "Шаг" },
        min: 0.0,
        max: 1.0,
        default: 0.0,
        kind: ParamKind::Choice,
        choices: &RATES,
    };
    KindInfo { kind: "arp", label: L10n { en: "Arpeggiator", ru: "Арпеджиатор" }, params: vec![rate] }
}

// One test: the steps share the global language and must run in order.
#[test]
fn texts_follow_the_current_language() {
    assert_eq!(i18n::current(), Language::En, "English until something sets another language");
    let name = "Bass";
    let bars = 4;
    assert_eq!(tr!("Bounce «{name}», {} bars", "Свести «{name}», тактов: {}", bars), "Bounce «Bass», 4 bars");
    assert_eq!(kind().label.get(), "Arpeggiator");
    let english = serde_json::to_value(kind()).unwrap();
    assert_eq!(english["label"], "Arpeggiator");
    assert_eq!(english["params"][0]["label"], "Rate");
    assert_eq!(english["params"][0]["choices"], serde_json::json!(["Off", "1/4"]));

    i18n::set_current(Language::Ru);
    assert_eq!(i18n::current(), Language::Ru);
    assert_eq!(tr!("Bounce «{name}», {} bars", "Свести «{name}», тактов: {}", bars), "Свести «Bass», тактов: 4");
    assert_eq!(format!("{}", L10n { en: "Kick", ru: "Кик" }), "Кик");
    let russian = serde_json::to_value(kind()).unwrap();
    assert_eq!(russian["label"], "Арпеджиатор");
    assert_eq!(russian["params"][0]["label"], "Шаг");
    assert_eq!(russian["params"][0]["choices"], serde_json::json!(["Выкл", "1/4"]));

    i18n::set_current(Language::En);
    assert_eq!(serde_json::to_value(kind()).unwrap(), english, "switching back restores English");
}
