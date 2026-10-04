//! Structured validation errors with JSON paths (spec §5.5).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single validation error.
///
/// Errors are structured (spec §5.5): they carry the JSON path of the
/// offending value (e.g. `$.address.zip` or `$.tags[2]`), a human-readable
/// message, the expected constraint, the actual observation, and (when
/// representable) the offending value itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationError {
    /// JSON path to the offending value, e.g. `"$.age"`, `"$.tags[0]"`.
    pub path: String,
    /// Human-readable description of the violation.
    pub message: String,
    /// What the schema expected (type name, bound, format, ...).
    pub expected: String,
    /// What was actually found (type name, value snippet, ...).
    pub actual: String,
    /// The offending value, when it is representable in JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// A "did you mean" hint for typo-level property-name mistakes, e.g.
    /// `"name"` when `nme` was written (see [`did_you_mean`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Closest candidate to `input` among `candidates` within edit-distance
/// tolerance (Levenshtein, case-insensitive): `None` when nothing is close.
pub fn did_you_mean(input: &str, candidates: &[String]) -> Option<String> {
    let input_lower = input.to_ascii_lowercase();
    let mut best: Option<(usize, &String)> = None;
    for candidate in candidates {
        let distance = levenshtein(&input_lower, &candidate.to_ascii_lowercase());
        // Tolerate ~1 edit per four characters, at least 1, at most 3.
        let tolerance = (candidate.chars().count() / 4).clamp(1, 3);
        if distance <= tolerance && best.map_or(true, |(d, _)| distance < d) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, c)| c.clone())
}

/// Levenshtein edit distance (insert / delete / substitute), Unicode scalar
/// units.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (current[j] + 1)
                .min(previous[j + 1] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

impl ValidationError {
    /// Build a new error.
    pub fn new(
        path: impl Into<String>,
        message: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            expected: expected.into(),
            actual: actual.into(),
            value: None,
            suggestion: None,
        }
    }

    /// Attach a "did you mean" suggestion (also appended to the message).
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        let suggestion = suggestion.into();
        self.message
            .push_str(&format!(" (did you mean \"{suggestion}\")?"));
        self.suggestion = Some(suggestion);
        self
    }

    /// Attach the offending value.
    pub fn with_value(mut self, value: Value) -> Self {
        self.value = Some(value);
        self
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

/// A validation report serializing to the spec §5.5 envelope:
/// `{"errors": [ ... ]}`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ValidationReport {
    /// All collected errors; empty when the instance is valid.
    pub errors: Vec<ValidationError>,
}

impl ValidationReport {
    /// Whether the validated instance is valid (no errors).
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Serialize to the JSON envelope `{"errors": [...]}`.
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"errors\":[]}".to_string())
    }

    /// Serialize to a pretty-printed JSON envelope.
    pub fn to_json_string_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{\"errors\":[]}".to_string())
    }
}

/// Collects validation errors during a traversal.
///
/// In fail-fast mode the collector signals "stopped" after the first error so
/// the engine can prune the rest of the traversal (spec: "fail-fast" mode for
/// performance-critical paths).
#[derive(Debug, Default)]
pub struct ErrorCollector {
    errors: Vec<ValidationError>,
    fail_fast: bool,
    max_errors: usize,
}

/// Default cap on collected errors per instance (guards against pathological
/// documents producing unbounded error lists).
pub const DEFAULT_MAX_ERRORS: usize = 1000;

impl ErrorCollector {
    /// Create a collector. `fail_fast` stops collection after one error;
    /// `max_errors` caps total collection.
    pub fn new(fail_fast: bool, max_errors: usize) -> Self {
        Self {
            errors: Vec::new(),
            fail_fast,
            max_errors: max_errors.max(1),
        }
    }

    /// Create a collector with defaults (collect all errors, capped at
    /// [`DEFAULT_MAX_ERRORS`]).
    pub fn collect_all() -> Self {
        Self::new(false, DEFAULT_MAX_ERRORS)
    }

    /// Create a fail-fast collector.
    pub fn fail_fast() -> Self {
        Self::new(true, 1)
    }

    /// Record an error.
    pub fn push(&mut self, error: ValidationError) {
        if self.errors.len() < self.max_errors {
            self.errors.push(error);
        }
    }

    /// Whether traversal should stop (fail-fast hit, or error cap reached).
    pub fn stopped(&self) -> bool {
        (self.fail_fast && !self.errors.is_empty()) || self.errors.len() >= self.max_errors
    }

    /// Errors collected so far.
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    /// Consume the collector into its errors.
    pub fn into_errors(self) -> Vec<ValidationError> {
        self.errors
    }

    /// Whether no errors were collected.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Number of collected errors.
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Whether collection is exhausted.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Finish into a [`ValidationReport`].
    pub fn into_report(self) -> ValidationReport {
        ValidationReport {
            errors: self.errors,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_spec_envelope() {
        let mut c = ErrorCollector::collect_all();
        c.push(
            ValidationError::new("$.age", "Expected integer, got string", "integer", "string")
                .with_value(json!("25")),
        );
        c.push(
            ValidationError::new("$.email", "Invalid email format", "email", "invalid-email")
                .with_value(json!("invalid-email")),
        );
        let report = c.into_report();
        let s = report.to_json_string();
        assert!(s.starts_with("{\"errors\":["));
        assert!(s.contains(r#""path":"$.age""#));
        assert!(s.contains(r#""expected":"integer""#));
        assert!(s.contains(r#""value":"25""#));
        assert!(!report.is_valid());
    }

    #[test]
    fn fail_fast_stops_after_first() {
        let mut c = ErrorCollector::fail_fast();
        assert!(!c.stopped());
        c.push(ValidationError::new("$", "a", "b", "c"));
        assert!(c.stopped());
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn max_errors_caps_collection() {
        let mut c = ErrorCollector::new(false, 2);
        for i in 0..5 {
            c.push(ValidationError::new(format!("$.f{i}"), "m", "e", "a"));
        }
        assert_eq!(c.len(), 2);
        assert!(c.stopped());
    }

    #[test]
    fn empty_report_is_valid() {
        let report = ErrorCollector::collect_all().into_report();
        assert!(report.is_valid());
        assert_eq!(report.to_json_string(), "{\"errors\":[]}");
    }
}
