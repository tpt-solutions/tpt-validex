//! The [`Validator`]: compiled schema + validation entry points, mirroring
//! the target APIs of spec §6 (single object, batch, streaming CSV/JSONL).

use std::io::{BufReader, Read, Write};
use std::sync::Arc;

use serde_json::Value;

use tpt_valid_core::{
    validate_batch, validate_csv_stream, validate_jsonl_stream, CsvDialect, CsvStats,
    JsonlRowOutcome, JsonlStats, RowOutcome, ValidationError, ValidationNode, ValidationOptions,
    ValidationOutcome, ValidationReport,
};

use crate::ast::{parse_schema, parse_schema_with};
use crate::error::SchemaError;
use crate::Warning;

/// A compiled, reusable validator for one schema.
///
/// Compile once (via [`Validator::new`], [`Validator::cached`], or the
/// `schema!` DSL through [`Validator::from_node`]) and validate millions of
/// records against it.
#[derive(Clone, Debug)]
pub struct Validator {
    root: Arc<ValidationNode>,
    warnings: Arc<Vec<Warning>>,
}

impl Validator {
    /// Compile a JSON Schema (Draft 2020-12 subset) from its text form.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_valid_schema::Validator;
    /// use serde_json::json;
    ///
    /// let v = Validator::new(r#"{
    ///     "type": "object",
    ///     "properties": {"age": {"type": "integer", "minimum": 0}},
    ///     "required": ["age"]
    /// }"#).unwrap();
    ///
    /// assert!(v.validate(&json!({"age": 30})).is_valid());
    /// assert!(!v.validate(&json!({"age": -1})).is_valid());
    /// ```
    pub fn new(schema: &str) -> Result<Validator, SchemaError> {
        let value = crate::json::from_str(schema)?;
        Self::from_value(&value)
    }

    /// Compile a JSON Schema, resolving cross-document `$ref`s through
    /// `registry`.
    pub fn new_with(
        schema: &str,
        registry: &crate::ast::SchemaRegistry,
    ) -> Result<Validator, SchemaError> {
        let value = crate::json::from_str(schema)?;
        Self::from_value_with(&value, registry)
    }

    /// Compile a schema, reusing the process-wide cache when the same schema
    /// text was compiled before.
    pub fn cached(schema: &str) -> Result<Arc<Validator>, SchemaError> {
        crate::cache::global_cache().get_or_compile(schema)
    }

    /// Compile a schema from an already-parsed JSON value.
    pub fn from_value(schema: &Value) -> Result<Validator, SchemaError> {
        let ast = parse_schema(schema)?;
        let (node, warnings) = crate::compiler::compile(&ast)?;
        Ok(Validator {
            root: Arc::new(node),
            warnings: Arc::new(warnings),
        })
    }

    /// Compile a schema from an already-parsed JSON value, resolving
    /// cross-document `$ref`s through `registry`.
    pub fn from_value_with(
        schema: &Value,
        registry: &crate::ast::SchemaRegistry,
    ) -> Result<Validator, SchemaError> {
        let ast = parse_schema_with(schema, registry)?;
        let (node, warnings) = crate::compiler::compile(&ast)?;
        Ok(Validator {
            root: Arc::new(node),
            warnings: Arc::new(warnings),
        })
    }

    /// Build a validator directly from a state machine node. Used by the
    /// `schema!` DSL macro, which builds an AST at compile time and runs it
    /// through the same IR pipeline.
    pub fn from_ast(ast: &crate::ast::SchemaAst) -> Result<Validator, SchemaError> {
        let (node, warnings) = crate::compiler::compile(ast)?;
        Ok(Validator {
            root: Arc::new(node),
            warnings: Arc::new(warnings),
        })
    }

    /// Wrap a pre-built state machine node (zero compilation).
    pub fn from_node(node: ValidationNode) -> Validator {
        Validator {
            root: Arc::new(node),
            warnings: Arc::new(Vec::new()),
        }
    }

    /// The compiled state machine root.
    pub fn root(&self) -> &ValidationNode {
        &self.root
    }

    /// Non-fatal warnings collected during compilation.
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Validate a JSON value, collecting all errors.
    pub fn validate(&self, value: &Value) -> ValidationReport {
        self.validate_with(value, &ValidationOptions::default())
    }

    /// Validate with explicit options (e.g. fail-fast).
    pub fn validate_with(&self, value: &Value, opts: &ValidationOptions) -> ValidationReport {
        let errors = tpt_valid_core::validate(&self.root, value, opts);
        ValidationReport { errors }
    }

    /// Validate raw JSON text. Parse failures are returned as errors; parse
    /// success returns the validation report.
    pub fn validate_json(
        &self,
        json: &str,
    ) -> Result<ValidationReport, tpt_valid_parser::ParseError> {
        let value = tpt_valid_parser::parse(json)?;
        Ok(self.validate(&value))
    }

    /// Boolean-only validation (fail-fast traversal, no error materialization).
    pub fn is_valid(&self, value: &Value) -> bool {
        tpt_valid_core::validate_value(&self.root, value, &ValidationOptions::default())
    }

    /// Validate a batch of values in parallel (spec §5.5 "Batch").
    pub fn validate_batch(&self, values: &[Value]) -> Vec<ValidationOutcome> {
        validate_batch(&self.root, values, &ValidationOptions::default())
    }

    /// Stream-validate newline-delimited JSON. Invalid rows are optionally
    /// written to `errors_out` as JSONL objects:
    /// `{"line": N, "row": ..., "errors": [...]}`.
    pub fn validate_jsonl_to<E: Write>(
        &self,
        input: impl Read,
        mut errors_out: E,
        opts: &ValidationOptions,
    ) -> Result<JsonlStats, std::io::Error> {
        let root = Arc::clone(&self.root);
        let mut io_err: Option<std::io::Error> = None;
        let stats = validate_jsonl_stream(&root, BufReader::new(input), opts, |row| {
            let line = match &row {
                JsonlRowOutcome::Valid { line, .. }
                | JsonlRowOutcome::Invalid { line, .. }
                | JsonlRowOutcome::ParseError { line, .. } => *line,
            };
            let record = match &row {
                JsonlRowOutcome::Invalid { value, errors, .. } => serde_json::json!({
                    "line": line,
                    "row": value,
                    "errors": errors,
                }),
                JsonlRowOutcome::ParseError { message, .. } => serde_json::json!({
                    "line": line,
                    "error": message,
                }),
                JsonlRowOutcome::Valid { .. } => return true,
            };
            if let Err(e) = writeln!(errors_out, "{record}") {
                io_err = Some(e);
                return false;
            }
            true
        })?;
        match io_err {
            Some(e) => Err(e),
            None => Ok(stats),
        }
    }

    /// Stream-validate newline-delimited JSON, returning stats only.
    pub fn validate_jsonl(
        &self,
        input: impl Read,
        opts: &ValidationOptions,
    ) -> Result<JsonlStats, std::io::Error> {
        let root = Arc::clone(&self.root);
        validate_jsonl_stream(&root, BufReader::new(input), opts, |_| true)
    }

    /// Stream-validate CSV, writing valid rows to `valid_out` (re-quoted
    /// CSV) and invalid rows to `errors_out` as JSONL objects:
    /// `{"line": N, "row": ..., "errors": [...]}`.
    ///
    /// This mirrors the spec §6.1 Python API
    /// `validator.validate_csv(input, valid_output=..., errors_output=...)`.
    pub fn validate_csv_to<R: Read, V: Write, E: Write>(
        &self,
        input: R,
        mut valid_out: V,
        mut errors_out: E,
        dialect: &CsvDialect,
        opts: &ValidationOptions,
    ) -> Result<CsvStats, FlowError> {
        let root = Arc::clone(&self.root);
        let quote = dialect.quote;
        let delim = dialect.delimiter;
        let mut io_err: Option<std::io::Error> = None;
        let stats = validate_csv_stream(&root, BufReader::new(input), dialect, opts, |row| {
            let outcome = match row {
                RowOutcome::Valid {
                    value: _, fields, ..
                } => {
                    let line = fields
                        .iter()
                        .map(|f| csv_quote(f, delim, quote))
                        .collect::<Vec<_>>()
                        .join(&(delim as char).to_string());
                    writeln!(valid_out, "{line}")
                }
                RowOutcome::Invalid {
                    line,
                    value,
                    errors,
                    fields: _,
                } => {
                    let record = serde_json::json!({
                        "line": line,
                        "row": value,
                        "errors": errors,
                    });
                    writeln!(errors_out, "{record}")
                }
                RowOutcome::ParseError { line, message } => {
                    let record = serde_json::json!({ "line": line, "error": message });
                    writeln!(errors_out, "{record}")
                }
            };
            if let Err(e) = outcome {
                io_err = Some(e);
                return false;
            }
            true
        })
        .map_err(FlowError::Csv)?;
        match io_err {
            Some(e) => Err(FlowError::Io(e)),
            None => Ok(stats),
        }
    }

    /// Stream-validate CSV, returning stats plus per-row errors (in memory).
    pub fn validate_csv(
        &self,
        input: impl Read,
        dialect: &CsvDialect,
        opts: &ValidationOptions,
    ) -> Result<(CsvStats, tpt_valid_core::RowErrors), FlowError> {
        let root = Arc::clone(&self.root);
        let mut collected = Vec::new();
        let stats = validate_csv_stream(&root, BufReader::new(input), dialect, opts, |row| {
            if let RowOutcome::Invalid { line, errors, .. } = row {
                collected.push((line, errors));
            }
            true
        })
        .map_err(FlowError::Csv)?;
        Ok((stats, collected))
    }
}

/// Errors from streaming validation flows.
#[derive(Debug)]
pub enum FlowError {
    /// The CSV payload itself was malformed.
    Csv(tpt_valid_core::CsvParseError),
    /// An I/O error on input or output.
    Io(std::io::Error),
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowError::Csv(e) => write!(f, "{e}"),
            FlowError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FlowError {}

fn csv_quote(field: &str, delim: u8, quote: u8) -> String {
    let needs_quoting = field.contains(delim as char)
        || field.contains(quote as char)
        || field.contains('\n')
        || field.contains('\r');
    if needs_quoting {
        format!(
            "{}{}{}",
            quote as char,
            field.replace(
                quote as char,
                &format!("{}{}", quote as char, quote as char)
            ),
            quote as char
        )
    } else {
        field.to_string()
    }
}

// Keep ValidationError referenced for downstream re-exports.
#[allow(unused_imports)]
use ValidationError as _ValidationErrorAlias;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    const USER_SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "name": {"type": "string", "minLength": 1},
            "age": {"type": "integer", "minimum": 0, "maximum": 150},
            "email": {"type": "string", "format": "email"}
        },
        "required": ["name", "age"]
    }"#;

    #[test]
    fn validates_single_object() {
        let v = Validator::new(USER_SCHEMA).unwrap();
        assert!(v.validate(&json!({"name": "Alice", "age": 30})).is_valid());
        let report = v.validate(&json!({"name": "", "age": 200}));
        assert_eq!(report.errors.len(), 2);
        let mut paths: Vec<&str> = report.errors.iter().map(|e| e.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(
            paths,
            vec!["$.age", "$.name"],
            "one error per violating property"
        );
    }

    #[test]
    fn validates_raw_json() {
        let v = Validator::new(USER_SCHEMA).unwrap();
        assert!(v
            .validate_json(r#"{"name": "Bob", "age": 25}"#)
            .unwrap()
            .is_valid());
        assert!(v.validate_json("not json").is_err());
    }

    #[test]
    fn syntax_errors_are_reported() {
        let err = Validator::new("{invalid").unwrap_err();
        assert!(matches!(err, SchemaError::Syntax(_)));
        assert!(err.to_string().contains("line 1"));
    }

    #[test]
    fn cached_returns_same_instance() {
        let a = Validator::cached(USER_SCHEMA).unwrap();
        let b = Validator::cached(USER_SCHEMA).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert!(a.validate(&json!({"name": "x", "age": 1})).is_valid());
    }

    #[test]
    fn batch_validation() {
        let v = Validator::new(USER_SCHEMA).unwrap();
        let batch = vec![
            json!({"name": "Alice", "age": 30}),
            json!({"name": "Bob", "age": "25"}),
            json!({"name": "", "age": 200}),
        ];
        let outcomes = v.validate_batch(&batch);
        assert_eq!(outcomes.len(), 3);
        assert!(outcomes[0].valid);
        assert!(!outcomes[1].valid);
        assert!(!outcomes[2].valid);
    }

    #[test]
    fn streaming_csv_writes_valid_and_errors() {
        let v = Validator::new(USER_SCHEMA).unwrap();
        let input =
            "name,age,email\nAlice,30,alice@example.com\nBob,25,bob example.com\nCarol,40,carol@example.com\n";
        let mut valid_out = Vec::new();
        let mut errors_out = Vec::new();
        let stats = v
            .validate_csv_to(
                Cursor::new(input),
                &mut valid_out,
                &mut errors_out,
                &CsvDialect::default(),
                &ValidationOptions::default(),
            )
            .unwrap();
        assert_eq!(stats.total_rows, 3);
        assert_eq!(stats.valid_rows, 2);
        assert_eq!(stats.invalid_rows, 1);
        let valid_csv = String::from_utf8(valid_out).unwrap();
        assert_eq!(
            valid_csv,
            "Alice,30,alice@example.com\nCarol,40,carol@example.com\n"
        );
        let errors_jsonl = String::from_utf8(errors_out).unwrap();
        assert!(errors_jsonl.contains(r#""line":3"#), "Bob row is line 3");
        assert!(errors_jsonl.contains(r#""path":"$.email""#));
    }

    #[test]
    fn streaming_jsonl_writes_errors() {
        let v = Validator::new(USER_SCHEMA).unwrap();
        let input = "{\"name\":\"A\",\"age\":1}\n{\"name\":\"B\",\"age\":-1}\n";
        let mut errors_out = Vec::new();
        let stats = v
            .validate_jsonl_to(
                Cursor::new(input),
                &mut errors_out,
                &ValidationOptions::default(),
            )
            .unwrap();
        assert_eq!(stats.total_lines, 2);
        assert_eq!(stats.valid_lines, 1);
        let errors_jsonl = String::from_utf8(errors_out).unwrap();
        assert!(errors_jsonl.contains(r#""line":2"#));
    }

    #[test]
    fn csv_quoting_round_trip() {
        let v = Validator::new(r#"{"type":"object"}"#).unwrap();
        let input = "name,note\nAlice,\"has, comma\"\n";
        let mut valid_out = Vec::new();
        let mut errors_out = Vec::new();
        v.validate_csv_to(
            Cursor::new(input),
            &mut valid_out,
            &mut errors_out,
            &CsvDialect::default(),
            &ValidationOptions::default(),
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(valid_out).unwrap(),
            "Alice,\"has, comma\"\n"
        );
    }

    #[test]
    fn from_node_wraps_machine() {
        let node = ValidationNode::CheckType(tpt_valid_core::DataType::Integer);
        let v = Validator::from_node(node);
        assert!(v.validate(&json!(1)).is_valid());
        assert!(!v.validate(&json!("1")).is_valid());
    }

    #[test]
    fn fail_fast_option() {
        let v = Validator::new(USER_SCHEMA).unwrap();
        let report = v.validate_with(
            &json!({"name": "", "age": 200}),
            &ValidationOptions::fail_fast(),
        );
        assert_eq!(report.errors.len(), 1);
    }
}
