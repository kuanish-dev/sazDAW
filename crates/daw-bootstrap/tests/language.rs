//! The language from the preferences reaches every label the backend builds: the saved one at startup, a new one right
//! after `preferences.set` (the interface then re-reads the catalog). A separate test program is a separate process:
//! the language is global, and switching it here must not race with the other tests, which expect English.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests: a failure with a clear message is the check")]

use daw_bootstrap::{SessionConfig, build_headless_session_in};
use daw_contracts::i18n::{self, Language};
use daw_contracts::{Controls, Preferences};

fn effect_label(controls: &Controls, kind: &str) -> &'static str {
    controls.effects.kinds().into_iter().find(|k| k.kind == kind).expect("built-in effect").label.get()
}

fn pattern_name(controls: &Controls) -> String {
    controls.patterns.pattern("rock_basic").unwrap().name
}

#[test]
fn startup_applies_the_saved_language_and_a_change_relabels_the_catalog() {
    let state = std::env::temp_dir().join(format!("daw-language-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("preferences.json"), r#"{ "language": "ru" }"#).unwrap();

    let session = build_headless_session_in(&SessionConfig::new(state.clone()));
    let controls = &session.controls;
    assert_eq!(i18n::current(), Language::Ru, "the saved language is current right after startup");
    assert_eq!(effect_label(controls, "compressor"), "Компрессор");
    assert_eq!(pattern_name(controls), "Рок: базовый");
    controls.demo.fill_demo().unwrap();
    assert!(controls.tracks.tracks().iter().any(|t| t.name == "Кик"), "default names in Russian");

    controls.preferences.set(Preferences { language: Language::En, ..controls.preferences.get() }).unwrap();
    assert_eq!(effect_label(controls, "compressor"), "Compressor", "no restart: the next catalog read is English");
    assert_eq!(pattern_name(controls), "Rock: basic");
    let reverb = controls.effects.kinds().into_iter().find(|k| k.kind == "reverb").unwrap();
    assert!(reverb.params.iter().all(|p| p.label.get() == p.label.en));
    std::fs::remove_dir_all(state).ok();
}
