//! Language of the data labels the backend builds (English or Russian): the current language, bilingual `&'static`
//! labels (`L10n`) and the `tr!` macro for formatted labels (undo labels, default names).
//! Errors are not localized here: they carry structured codes (`ErrorCode`) that the interface translates.
//!
//! Why a process-wide global: the language is a display setting read everywhere a label is built (catalog, patterns,
//! undo labels, default names), like a locale. Passing it through every interface would touch every contract for no
//! gain. It is set in one place — the preferences store (`PreferencesStore`), on load and on a successful `set` — and
//! only read elsewhere.
//! The audio thread never reads it for logic (texts are not built there).
//!
//! Texts live next to the code that uses them, as an English + Russian pair; there are no key catalogs.

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize, Serializer};

/// Language of the interface and of the texts the backend builds. JSON: `"en"` | `"ru"`; English by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Ru,
}

static CURRENT: AtomicU8 = AtomicU8::new(Language::En as u8);

/// The current language (English until something sets another).
pub fn current() -> Language {
    match CURRENT.load(Ordering::Relaxed) {
        x if x == Language::Ru as u8 => Language::Ru,
        _ => Language::En,
    }
}

/// Makes `language` current for the whole process. Texts built afterwards (and catalogs serialized afterwards) use it;
/// texts already built (undo labels, names of created tracks) stay as they were.
pub fn set_current(language: Language) {
    CURRENT.store(language as u8, Ordering::Relaxed);
}

/// A fixed text in both languages, for `&'static` labels (catalog parameters, kinds, pattern names).
/// Serializes and displays as the text in the current language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L10n {
    pub en: &'static str,
    pub ru: &'static str,
}

impl L10n {
    /// The same text in every language: names that are not translated (plugin parameters, "1/16", "Q").
    pub const fn same(text: &'static str) -> Self {
        Self { en: text, ru: text }
    }

    /// The text in the current language.
    pub fn get(&self) -> &'static str {
        match current() {
            Language::En => self.en,
            Language::Ru => self.ru,
        }
    }
}

impl fmt::Display for L10n {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.get())
    }
}

impl Serialize for L10n {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.get())
    }
}

/// A formatted text in the current language: `tr!("Bounce «{name}»", "Свести в аудио «{name}»")`.
/// Both format strings take the same arguments (inline `{name}` or trailing `, value`), and every argument must be used
/// in both; only the current language's branch is formatted. Returns a `String`.
#[macro_export]
macro_rules! tr {
    ($en:literal, $ru:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::current() {
            $crate::i18n::Language::En => format!($en $(, $arg)*),
            $crate::i18n::Language::Ru => format!($ru $(, $arg)*),
        }
    };
}
