//! # tpt-valid-parser
//!
//! JSON parsing for `tpt-validex` (spec §5.3) with a strict
//! no-Apache-2.0-only-dependencies policy.
//!
//! Parser selection:
//!
//! 1. `jiter` (MIT) — high-performance iterative parser (the parser behind
//!    pydantic-core); used on all native targets. It replaced `sonic-rs`,
//!    which our dependency audit rejected as Apache-2.0-only (the spec had
//!    assumed MIT).
//! 2. `serde_json` (MIT / Apache-2.0) — guaranteed-compatible baseline on
//!    `wasm32` and other non-native targets.
//!
//! Every backend produces [`serde_json::Value`] behind one uniform API, so
//! callers never see which parser handled a document.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt;

/// Unified parse error across all JSON backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable error message.
    pub message: String,
    /// 1-based line number reported by the backend, when known (`0` if unknown).
    pub line: usize,
    /// 1-based column number reported by the backend, when known (`0` if unknown).
    pub column: usize,
}

impl ParseError {
    /// Build an error with position information.
    pub fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(
                f,
                "{} at line {} column {}",
                self.message, self.line, self.column
            )
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for ParseError {}

impl From<serde_json::Error> for ParseError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(e.to_string(), e.line(), e.column())
    }
}

/// Name of the JSON backend compiled into this build
/// (`"jiter"` on native targets, `"serde_json"` on wasm32 et al.).
pub fn backend_name() -> &'static str {
    #[cfg(target_arch = "wasm32")]
    {
        "serde_json"
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "jiter"
    }
}

/// Parse a JSON document into a [`serde_json::Value`], automatically using
/// the platform's primary parser.
///
/// # Examples
///
/// ```
/// let v = tpt_valid_parser::parse(r#"{"age": 25}"#).unwrap();
/// assert_eq!(v["age"], 25);
/// ```
pub fn parse(input: &str) -> Result<serde_json::Value, ParseError> {
    parse_bytes(input.as_bytes())
}

/// Parse a JSON document from raw bytes.
#[cfg(target_arch = "wasm32")]
pub fn parse_bytes(input: &[u8]) -> Result<serde_json::Value, ParseError> {
    serde_json::from_slice(input).map_err(ParseError::from)
}

/// Parse a JSON document from raw bytes.
#[cfg(not(target_arch = "wasm32"))]
pub fn parse_bytes(input: &[u8]) -> Result<serde_json::Value, ParseError> {
    match jiter::serde::from_slice::<serde_json::Value>(input) {
        Ok(value) => Ok(value),
        Err(e) => {
            let (line, column) = e
                .get_position(input)
                .map(|p| (p.line, p.column))
                .unwrap_or((0, 0));
            Err(ParseError::new(e.description(input), line, column))
        }
    }
}

/// Parse a JSON document, returning both the value and the backend used.
/// Primarily useful for tests and diagnostics.
pub fn parse_verbose(input: &str) -> Result<(serde_json::Value, &'static str), ParseError> {
    parse(input).map(|v| (v, backend_name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_objects() {
        let v = parse(r#"{"name": "Alice", "age": 30, "active": true}"#).unwrap();
        assert_eq!(v["name"], "Alice");
        assert_eq!(v["age"], 30);
        assert_eq!(v["active"], true);
    }

    #[test]
    fn parses_nested_and_arrays() {
        let v = parse(r#"{"tags": ["a", "b"], "nested": {"x": 1.5, "n": null}}"#).unwrap();
        assert_eq!(
            v,
            json!({"tags": ["a", "b"], "nested": {"x": 1.5, "n": null}})
        );
    }

    #[test]
    fn parses_scalars() {
        assert_eq!(parse("42").unwrap(), json!(42));
        assert_eq!(parse("-1.25").unwrap(), json!(-1.25));
        assert_eq!(parse("true").unwrap(), json!(true));
        assert_eq!(parse("null").unwrap(), json!(null));
        assert_eq!(parse("\"hi\"").unwrap(), json!("hi"));
    }

    #[test]
    fn rejects_invalid() {
        assert!(parse("{invalid}").is_err());
        assert!(parse("").is_err());
        assert!(parse_bytes(b"\xff\xfe").is_err());
        assert!(parse(r#"{"a": 1} trailing"#).is_err());
    }

    #[test]
    fn reports_backend_name() {
        let name = backend_name();
        assert!(
            name.contains("jiter") || name.contains("serde"),
            "got {name}"
        );
    }

    #[test]
    fn unicode_roundtrip() {
        let v = parse(r#"{"emoji": "🚀", "accent": "café"}"#).unwrap();
        assert_eq!(v["emoji"], "🚀");
        assert_eq!(v["accent"], "café");
    }
}
