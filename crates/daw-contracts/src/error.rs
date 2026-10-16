//! Contract errors as codes: every failure a person or an AI can see has a stable code (`TR-001`), named params and an
//! English message built from the code's template. The interface translates by code (see `codes`); the English
//! message is what logs and MCP clients read.
//!
//! Why codes instead of free text: the text can be translated and reworded without breaking anyone who reacts to a
//! specific failure (tests, the interface, AI clients), and one code names one situation across the whole app.

use std::fmt;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

/// One registered failure: a code `AA-KNN` and its English template with `{param}` placeholders.
/// `AA` is the area (see `codes`), `K` the kind (`ErrorKind`), `NN` the number within the area and kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorCode {
    pub code: &'static str,
    pub template: &'static str,
}

impl ErrorCode {
    pub const fn new(code: &'static str, template: &'static str) -> Self {
        Self { code, template }
    }

    /// The error without params (for templates without placeholders).
    pub fn err(self) -> ContractError {
        ContractError { code: self, params: Vec::new() }
    }

    /// The error with its first param; chain `ContractError::with` for more.
    pub fn with(self, name: &'static str, value: impl fmt::Display) -> ContractError {
        self.err().with(name, value)
    }

    /// What kind of failure the code names (the digit after the dash).
    pub fn kind(self) -> ErrorKind {
        match self.code.as_bytes().get(3) {
            Some(b'0') => ErrorKind::NotFound,
            Some(b'1') => ErrorKind::Invalid,
            Some(b'2') => ErrorKind::Limit,
            Some(b'3') => ErrorKind::State,
            Some(b'4') => ErrorKind::System,
            Some(b'5') => ErrorKind::Unsupported,
            _ => ErrorKind::Internal,
        }
    }
}

impl From<ErrorCode> for ContractError {
    fn from(code: ErrorCode) -> Self {
        code.err()
    }
}

/// Kind of failure, encoded by the first digit of the number: `0` not found, `1` invalid value, `2` limit or range,
/// `3` not possible in the current state, `4` system (files, devices, plugins), `5` unsupported, `9` internal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    Invalid,
    Limit,
    State,
    System,
    Unsupported,
    Internal,
}

/// A failed contract operation: a registered code plus the values for its template.
/// JSON: `{ "code": "TR-001", "params": { "id": "5" }, "message": "Track 5 not found" }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractError {
    code: ErrorCode,
    params: Vec<(&'static str, String)>,
}

impl ContractError {
    /// Adds a template param (`{name}` in the template).
    #[must_use]
    pub fn with(mut self, name: &'static str, value: impl fmt::Display) -> Self {
        self.params.push((name, value.to_string()));
        self
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn kind(&self) -> ErrorKind {
        self.code.kind()
    }

    /// Whether this is the given registered failure.
    pub fn is(&self, code: ErrorCode) -> bool {
        self.code == code
    }

    pub fn params(&self) -> &[(&'static str, String)] {
        &self.params
    }

    /// The English message: the template with params filled in.
    pub fn message(&self) -> String {
        fill(self.code.template, &self.params)
    }
}

/// Replaces `{name}` placeholders with param values; unknown placeholders stay as they are, so a missing param is
/// visible in the text instead of silently vanishing.
pub fn fill(template: &str, params: &[(&str, String)]) -> String {
    let mut text = template.to_string();
    for (name, value) in params {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ContractError {}

impl Serialize for ContractError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Params<'a>(&'a [(&'static str, String)]);
        impl Serialize for Params<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(Some(self.0.len()))?;
                for (name, value) in self.0 {
                    map.serialize_entry(name, value)?;
                }
                map.end()
            }
        }
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("code", self.code.code)?;
        map.serialize_entry("params", &Params(&self.params))?;
        map.serialize_entry("message", &self.message())?;
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: ErrorCode = ErrorCode::new("TR-001", "Track {id} not found");

    #[test]
    fn message_fills_the_template() {
        let error = SAMPLE.with("id", 5);
        assert_eq!(error.message(), "Track 5 not found");
        assert_eq!(error.to_string(), "Track 5 not found");
        assert!(error.is(SAMPLE));
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn missing_params_stay_visible() {
        assert_eq!(SAMPLE.err().message(), "Track {id} not found");
    }

    #[test]
    fn serializes_code_params_and_message() {
        let json = serde_json::to_value(SAMPLE.with("id", 5)).unwrap();
        assert_eq!(json, serde_json::json!({ "code": "TR-001", "params": { "id": "5" }, "message": "Track 5 not found" }));
    }
}
