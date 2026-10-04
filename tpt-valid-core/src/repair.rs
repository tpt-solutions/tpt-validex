//! Streaming-mode error clustering (Phase 13): group per-record failures by
//! their failure *pattern* (`path` + `expected`), with counts and one example
//! row — the "top failure patterns" view for large files.
//!
//! Also hosts the LLM structured-output helper: [`repair_prompt`] turns a
//! schema, the rejected document and its validation errors into a
//! ready-to-send repair prompt for a model.

use std::collections::HashMap;

use serde::Serialize;

use crate::error::ValidationError;

/// One failure pattern observed across records.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorCluster {
    /// Pattern key: `"<path>|<expected>"` — stable across records.
    pub pattern: String,
    /// JSON path of the failing value.
    pub path: String,
    /// The violated constraint (`expected` of the underlying error).
    pub expected: String,
    /// How many records hit this pattern.
    pub count: usize,
    /// Line / position of the first occurrence (1-based; source-dependent).
    pub first_position: usize,
    /// The full first error (message, value, suggestion, ...).
    pub example: ValidationError,
}

/// Accumulates per-record errors into clusters.
///
/// # Examples
///
/// ```
/// use tpt_valid_core::{ErrorClusterer, ValidationError};
///
/// let mut clusterer = ErrorClusterer::new();
/// for line in 1..=10 {
///     clusterer.record(line, &[ValidationError::new(
///         "$.age", "Expected integer, got string", "integer", "string",
///     )]);
/// }
/// let clusters = clusterer.finish();
/// assert_eq!(clusters.len(), 1);
/// assert_eq!(clusters[0].count, 10);
/// assert_eq!(clusters[0].first_position, 1);
/// ```
#[derive(Debug, Default)]
pub struct ErrorClusterer {
    clusters: HashMap<String, ErrorCluster>,
}

impl ErrorClusterer {
    /// A fresh clusterer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the errors of one record (line/position 1-based).
    pub fn record(&mut self, position: usize, errors: &[ValidationError]) {
        for error in errors {
            let pattern = format!("{}|{}", error.path, error.expected);
            self.clusters
                .entry(pattern.clone())
                .and_modify(|cluster| cluster.count += 1)
                .or_insert_with(|| ErrorCluster {
                    pattern: pattern.clone(),
                    path: error.path.clone(),
                    expected: error.expected.clone(),
                    count: 1,
                    first_position: position,
                    example: error.clone(),
                });
        }
    }

    /// Clusters sorted by count (descending), then pattern.
    pub fn finish(self) -> Vec<ErrorCluster> {
        let mut clusters: Vec<ErrorCluster> = self.clusters.into_values().collect();
        clusters.sort_by(|a, b| b.count.cmp(&a.count).then(a.pattern.cmp(&b.pattern)));
        clusters
    }

    /// Clusters so far, without consuming (same ordering as [`finish`]).
    pub fn snapshot(&self) -> Vec<&ErrorCluster> {
        let mut clusters: Vec<&ErrorCluster> = self.clusters.values().collect();
        clusters.sort_by(|a, b| b.count.cmp(&a.count).then(a.pattern.cmp(&b.pattern)));
        clusters
    }
}

/// Build an LLM repair prompt: given the schema, the rejected document and
/// its validation errors, produce instructions a model can act on to emit a
/// corrected document.
///
/// Errors are rendered with their JSON path, the violated constraint, and
/// any "did you mean" hint, so the model can fix root causes rather than
/// reformatting.
pub fn repair_prompt(schema_text: &str, data_text: &str, errors: &[ValidationError]) -> String {
    let mut prompt = String::with_capacity(schema_text.len() + data_text.len() + 512);
    prompt.push_str(
        "The following JSON document failed validation. Return ONLY the corrected \
         document as JSON — no commentary, no code fences.\n\n",
    );
    prompt.push_str("## Schema (Draft 2020-12)\n\n```json\n");
    prompt.push_str(schema_text.trim());
    prompt.push_str("\n```\n\n## Document to fix\n\n```json\n");
    prompt.push_str(data_text.trim());
    prompt.push_str("\n```\n\n## Validation errors (fix all of them)\n\n");
    for (i, error) in errors.iter().enumerate() {
        prompt.push_str(&format!(
            "{}. `{}` — {} (expected: {})\n",
            i + 1,
            error.path,
            error.message,
            error.expected
        ));
        if let Some(suggestion) = &error.suggestion {
            prompt.push_str(&format!("   (did you mean `{suggestion}`?)\n"));
        }
        if let Some(value) = &error.value {
            let value_text = serde_json::to_string(value).unwrap_or_default();
            if value_text.len() < 200 {
                prompt.push_str(&format!("   Offending value: `{value_text}`\n"));
            }
        }
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clusters_by_pattern_with_counts() {
        let mut clusterer = ErrorClusterer::new();
        clusterer.record(
            1,
            &[ValidationError::new("$.age", "m", "integer", "string").with_value(json!("25"))],
        );
        clusterer.record(
            2,
            &[ValidationError::new("$.age", "m", "integer", "string")],
        );
        clusterer.record(
            3,
            &[ValidationError::new("$.name", "m", "minLength 1", "0")],
        );
        let clusters = clusterer.finish();
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].count, 2);
        assert_eq!(clusters[0].path, "$.age");
        assert_eq!(clusters[0].first_position, 1);
        assert_eq!(clusters[0].example.value, Some(json!("25")));
        assert_eq!(clusters[1].count, 1);
    }

    #[test]
    fn snapshot_does_not_consume() {
        let mut clusterer = ErrorClusterer::new();
        clusterer.record(1, &[ValidationError::new("$.a", "m", "e", "x")]);
        assert_eq!(clusterer.snapshot().len(), 1);
        clusterer.record(2, &[ValidationError::new("$.a", "m", "e", "y")]);
        assert_eq!(clusterer.snapshot()[0].count, 2);
    }

    #[test]
    fn repair_prompt_includes_errors_and_suggestions() {
        let errors = vec![ValidationError::new(
            "$.nme",
            "Unknown property",
            "additionalProperties",
            "unknown",
        )
        .with_value(json!(1))
        .with_suggestion("name")];
        let prompt = repair_prompt(
            r#"{"type": "object", "required": ["name"]}"#,
            r#"{"nme": 1}"#,
            &errors,
        );
        assert!(prompt.contains("## Schema"));
        assert!(prompt.contains("\"nme\": 1"));
        assert!(prompt.contains("`$.nme`"));
        assert!(prompt.contains("did you mean `name`"));
        assert!(prompt.contains("Return ONLY the corrected document"));
    }
}
