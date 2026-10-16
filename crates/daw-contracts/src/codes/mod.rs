//! Registry of error codes, one module per area. A code is `AA-KNN`: `AA` the area prefix below, `K` the kind
//! (`ErrorKind`), `NN` the number within the area and kind. Every code is used for exactly one situation; its English
//! template lives here, other languages translate it by code in the interface (`app/src/i18n/errors/`).
//!
//! How to add a failure: pick the area, add `NAME = "AA-KNN", "English template with {param}";` to its module, build the
//! error with `codes::NAME.with("param", value)` (or `.err()`), and add the translation to the interface dictionaries.
//!
//! | Prefix | Area |
//! |---|---|
//! | PJ | project files |
//! | TR | tracks |
//! | CL | clips, warp, takes |
//! | NT | notes, controllers, note effects |
//! | GR | sound graph, mixer, effects |
//! | AT | automation |
//! | TM | transport, tempo, signatures, metronome |
//! | MK | markers, folders |
//! | SM | samples, sounds, preview |
//! | AU | audio devices, audio recording |
//! | RC | MIDI recording |
//! | MD | MIDI devices, files, mappings |
//! | PT | patterns and parts: drums, chords |
//! | PL | plugins |
//! | EX | export, bounce, analysis |
//! | PR | preferences, presets, templates |
//! | HS | undo history |
//! | RP | calls (RPC, MCP) |
//! | GN | internal |

use crate::error::ErrorCode;

/// Declares the codes of one area and its `ALL` list for the registry checks.
#[macro_export]
#[doc(hidden)]
macro_rules! codes {
    ($($name:ident = $code:literal, $template:literal;)*) => {
        $(pub const $name: $crate::error::ErrorCode = $crate::error::ErrorCode::new($code, $template);)*
        pub(super) const ALL: &[$crate::error::ErrorCode] = &[$($name),*];
    };
}

pub mod audio;
pub mod automation;
pub mod clips;
pub mod export;
pub mod general;
pub mod graph;
pub mod history;
pub mod markers;
pub mod midi;
pub mod notes;
pub mod patterns;
pub mod plugins;
pub mod project;
pub mod record;
pub mod rpc;
pub mod samples;
pub mod settings;
pub mod tempo;
pub mod tracks;

/// Every area with its prefix.
const AREAS: &[(&str, &[ErrorCode])] = &[
    ("PJ", project::ALL),
    ("TR", tracks::ALL),
    ("CL", clips::ALL),
    ("NT", notes::ALL),
    ("GR", graph::ALL),
    ("AT", automation::ALL),
    ("TM", tempo::ALL),
    ("MK", markers::ALL),
    ("SM", samples::ALL),
    ("AU", audio::ALL),
    ("RC", record::ALL),
    ("MD", midi::ALL),
    ("PT", patterns::ALL),
    ("PL", plugins::ALL),
    ("EX", export::ALL),
    ("PR", settings::ALL),
    ("HS", history::ALL),
    ("RP", rpc::ALL),
    ("GN", general::ALL),
];

/// All registered codes.
pub fn all() -> impl Iterator<Item = ErrorCode> {
    AREAS.iter().flat_map(|(_, codes)| codes.iter().copied())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn codes_are_well_formed_and_in_their_area() {
        for (prefix, codes) in AREAS {
            for code in *codes {
                let bytes = code.code.as_bytes();
                assert_eq!(bytes.len(), 6, "{}", code.code);
                assert!(code.code.starts_with(&format!("{prefix}-")), "{} is not in area {prefix}", code.code);
                assert!(bytes[3..].iter().all(u8::is_ascii_digit), "{}", code.code);
                assert!(b"0123459".contains(&bytes[3]), "{}: unknown kind digit", code.code);
                assert!(!code.template.is_empty(), "{}", code.code);
            }
        }
    }

    #[test]
    fn codes_are_unique() {
        let mut seen = HashSet::new();
        for code in all() {
            assert!(seen.insert(code.code), "{} is registered twice", code.code);
        }
    }

    /// Every code has a Russian translation in the interface, and the dictionary has no codes that are gone.
    #[test]
    fn interface_translates_every_code() {
        let dictionary = include_str!("../../../../app/src/i18n/errors/ru.ts");
        let registered: HashSet<&str> = all().map(|c| c.code).collect();
        for code in &registered {
            assert!(dictionary.contains(&format!("\"{code}\":")), "{code} has no Russian translation");
        }
        let translated: HashSet<&str> = dictionary
            .split('"')
            .filter(|part| part.len() == 6 && part.as_bytes()[2] == b'-' && part[3..].bytes().all(|b| b.is_ascii_digit()))
            .collect();
        let stale: Vec<_> = translated.difference(&registered).collect();
        assert!(stale.is_empty(), "translated but not registered: {stale:?}");
    }
}
