//! Streaming JSONL (newline-delimited JSON) validation — built from scratch,
//! zero external dependencies (spec §3.5, §5.3).
//!
//! Each line is parsed and validated independently; errors are reported with
//! 1-based line numbers. Blank lines are skipped.

use std::io::{BufRead, BufReader};

use serde::Serialize;
use serde_json::Value;

use crate::engine::{check, PathCursor, ValidationOptions};
use crate::error::{ErrorCollector, ValidationError};
use crate::node::ValidationNode;

/// What happened to one JSONL line during streaming validation.
#[derive(Debug)]
pub enum JsonlRowOutcome {
    /// Line parsed and validated clean.
    Valid {
        /// 1-based line number.
        line: usize,
        /// The parsed value.
        value: Value,
    },
    /// Line parsed but failed validation.
    Invalid {
        /// 1-based line number.
        line: usize,
        /// The parsed value (when parsing succeeded — always `Some` for this
        /// variant).
        value: Option<Value>,
        /// All validation errors.
        errors: Vec<ValidationError>,
    },
    /// Line could not be parsed as JSON.
    ParseError {
        /// 1-based line number.
        line: usize,
        /// Parse failure description.
        message: String,
    },
}

/// Aggregate result of a streaming JSONL validation run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct JsonlStats {
    /// Non-blank lines seen.
    pub total_lines: usize,
    /// Lines that passed validation.
    pub valid_lines: usize,
    /// Lines that failed validation.
    pub invalid_lines: usize,
    /// Lines that could not be parsed as JSON.
    pub parse_errors: usize,
}

/// Stream-validate newline-delimited JSON against a state machine.
///
/// `on_row` receives each [`JsonlRowOutcome`]; return `false` to stop early.
///
/// # Examples
///
/// ```
/// use tpt_valid_core::{validate_jsonl_stream, ValidationNode, ValidationOptions, DataType};
/// use std::io::Cursor;
///
/// let data = "{\"age\": 1}\n{\"age\": \"x\"}\nnot json\n";
/// let node = ValidationNode::all(vec![
///     ValidationNode::CheckType(DataType::Object),
///     ValidationNode::CheckField("age".into(), Box::new(
///         ValidationNode::CheckType(DataType::Integer))),
/// ]);
/// let stats = validate_jsonl_stream(&node, Cursor::new(data),
///     &ValidationOptions::default(), |_row| true).unwrap();
/// assert_eq!(stats.total_lines, 3);
/// assert_eq!(stats.valid_lines, 1);
/// assert_eq!(stats.invalid_lines, 1);
/// assert_eq!(stats.parse_errors, 1);
/// ```
pub fn validate_jsonl_stream<R: BufRead>(
    node: &ValidationNode,
    reader: R,
    opts: &ValidationOptions,
    mut on_row: impl FnMut(JsonlRowOutcome) -> bool,
) -> Result<JsonlStats, std::io::Error> {
    let mut stats = JsonlStats::default();
    let reader = BufReader::new(reader);
    for (index, line) in reader.lines().enumerate() {
        let line_no = index + 1;
        let text = line?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        stats.total_lines += 1;
        match crate::parse_value(trimmed) {
            Ok(value) => {
                let mut collector = ErrorCollector::new(opts.fail_fast, opts.max_errors);
                let mut path = PathCursor::new();
                check(node, &value, &mut path, opts, &mut collector);
                let errors = collector.into_errors();
                if errors.is_empty() {
                    stats.valid_lines += 1;
                    if !on_row(JsonlRowOutcome::Valid {
                        line: line_no,
                        value,
                    }) {
                        break;
                    }
                } else {
                    stats.invalid_lines += 1;
                    if !on_row(JsonlRowOutcome::Invalid {
                        line: line_no,
                        value: Some(value),
                        errors,
                    }) {
                        break;
                    }
                }
            }
            Err(message) => {
                stats.parse_errors += 1;
                if !on_row(JsonlRowOutcome::ParseError {
                    line: line_no,
                    message,
                }) {
                    break;
                }
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    use crate::types::DataType;

    fn age_schema() -> ValidationNode {
        ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckField(
                "age".into(),
                Box::new(ValidationNode::all(vec![
                    ValidationNode::CheckType(DataType::Integer),
                    ValidationNode::CheckMinimum(0.0),
                ])),
            ),
        ])
    }

    #[test]
    fn jsonl_end_to_end() {
        let data = "{\"age\": 30}\n{\"age\": -1}\nnot json\n\n{\"age\": 25}\n";
        let mut seen: Vec<(usize, String)> = Vec::new();
        let stats = validate_jsonl_stream(
            &age_schema(),
            Cursor::new(data),
            &ValidationOptions::default(),
            |row| {
                match &row {
                    JsonlRowOutcome::Valid { line, value } => {
                        seen.push((*line, format!("valid:{}", value["age"])))
                    }
                    JsonlRowOutcome::Invalid { line, .. } => seen.push((*line, "invalid".into())),
                    JsonlRowOutcome::ParseError { line, message } => {
                        seen.push((*line, message.clone()))
                    }
                }
                true
            },
        )
        .unwrap();
        assert_eq!(stats.total_lines, 4);
        assert_eq!(stats.valid_lines, 2);
        assert_eq!(stats.invalid_lines, 1);
        assert_eq!(stats.parse_errors, 1);
        assert_eq!(seen[0], (1, "valid:30".to_string()));
        assert_eq!(seen[1], (2, "invalid".to_string()));
        // Parse-error message text is backend-specific; only the line is stable.
        assert_eq!(seen[2].0, 3);
        assert!(!seen[2].1.is_empty());
        assert_eq!(seen[3], (5, "valid:25".to_string()));
    }

    #[test]
    fn jsonl_blank_lines_skipped_and_line_numbers_preserved() {
        let data = "\n{\"age\": 30}\n\n{\"age\": -1}\n";
        let mut invalid_line = 0;
        let stats = validate_jsonl_stream(
            &age_schema(),
            Cursor::new(data),
            &ValidationOptions::default(),
            |row| {
                if let JsonlRowOutcome::Invalid { line, errors, .. } = &row {
                    invalid_line = *line;
                    assert_eq!(errors[0].path, "$.age");
                }
                true
            },
        )
        .unwrap();
        assert_eq!(stats.total_lines, 2);
        assert_eq!(invalid_line, 4);
    }

    #[test]
    fn jsonl_early_stop() {
        let data = "{\"age\": 30}\n{\"age\": 31}\n";
        let stats = validate_jsonl_stream(
            &age_schema(),
            Cursor::new(data),
            &ValidationOptions::default(),
            |_row| false,
        )
        .unwrap();
        assert_eq!(stats.total_lines, 1);
    }

    #[test]
    fn jsonl_collects_all_errors_per_line() {
        let data = r#"{"age": "x", "name": 5}"#;
        let node = ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckField(
                "age".into(),
                Box::new(ValidationNode::CheckType(DataType::Integer)),
            ),
            ValidationNode::CheckField(
                "name".into(),
                Box::new(ValidationNode::CheckType(DataType::String)),
            ),
        ]);
        let mut err_count = 0;
        validate_jsonl_stream(
            &node,
            Cursor::new(data),
            &ValidationOptions::default(),
            |row| {
                if let JsonlRowOutcome::Invalid { errors, .. } = row {
                    err_count = errors.len();
                    assert_eq!(errors[0].path, "$.age");
                    assert_eq!(errors[1].path, "$.name");
                } else {
                    panic!("expected invalid");
                }
                true
            },
        )
        .unwrap();
        assert_eq!(err_count, 2);
    }

    #[test]
    fn jsonl_value_preserved_on_valid_rows() {
        let data = "{\"age\": 30}";
        let mut got = None;
        validate_jsonl_stream(
            &age_schema(),
            Cursor::new(data),
            &ValidationOptions::default(),
            |row| {
                if let JsonlRowOutcome::Valid { value, .. } = row {
                    got = Some(value);
                }
                true
            },
        )
        .unwrap();
        assert_eq!(got, Some(json!({"age": 30})));
    }
}
