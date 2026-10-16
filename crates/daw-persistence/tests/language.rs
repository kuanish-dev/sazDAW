//! `PreferencesFile` makes the saved language current. A separate test program is a separate process: the language is
//! global, and switching it here must not race with the crate's other tests.
#![allow(clippy::unwrap_used, reason = "tests: a failure with a clear message is the check")]

use std::path::PathBuf;
use std::sync::Arc;

use daw_contracts::codes::settings;
use daw_contracts::i18n::{self, Language};
use daw_contracts::{AppLog, Preferences, PreferencesStore};
use daw_persistence::{FileLog, PreferencesFile};

fn fresh_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("daw-persistence-language-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// One test: the steps share the global language and must run in order.
#[test]
fn the_store_applies_the_language_on_open_and_on_a_successful_set() {
    let dir = fresh_dir();
    let log: Arc<dyn AppLog> = Arc::new(FileLog::open(&dir.join("logs")));
    let store = PreferencesFile::open(&dir, log.clone());
    assert_eq!((store.get().language, i18n::current()), (Language::En, Language::En), "no file — English");

    let saved = store.set(Preferences { language: Language::Ru, ..Preferences::default() }).unwrap();
    assert_eq!((saved.language, i18n::current()), (Language::Ru, Language::Ru));

    let rejected = store.set(Preferences { language: Language::En, autosave_seconds: 5, ..Preferences::default() });
    assert!(rejected.is_err_and(|e| e.is(settings::AUTOSAVE_OUT_OF_RANGE)));
    assert_eq!(i18n::current(), Language::Ru, "a rejected set keeps the language");

    i18n::set_current(Language::En);
    let reopened = PreferencesFile::open(&dir, log);
    assert_eq!((reopened.get().language, i18n::current()), (Language::Ru, Language::Ru), "the next start applies the saved one");
    i18n::set_current(Language::En);
    std::fs::remove_dir_all(dir).ok();
}
