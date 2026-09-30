//! Streaming CSV parsing and validation — built from scratch, zero external
//! dependencies (spec §3.5, §5.3).
//!
//! Features: quoted fields (with `""` escapes and embedded newlines), custom
//! delimiters, schema-driven cell coercion (declared property types win;
//! sampled inference only fills in for untyped columns), per-row validation
//! with error collection, and bounded memory (rows are never all buffered;
//! only the type-inference sample is).
//!
//! Leniency policy (default): content after a closing quote is accepted,
//! invalid UTF-8 decodes lossily, and blank lines are skipped entirely. Set
//! [`CsvDialect::strict`] to turn the first two into parse errors.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::engine::{check, PathCursor, ValidationOptions};
use crate::error::{ErrorCollector, ValidationError};
use crate::node::ValidationNode;
use crate::types::DataType;

/// CSV dialect settings.
#[derive(Debug, Clone)]
pub struct CsvDialect {
    /// Field separator byte. Default `b','`.
    pub delimiter: u8,
    /// Quote byte. Default `b'"'`.
    pub quote: u8,
    /// Whether the first record is a header row (default `true`). When
    /// `false`, columns are named `column_0`, `column_1`, ...
    pub has_headers: bool,
    /// Number of rows sampled for column type inference before streaming
    /// (default 1000). Inference is only used for columns whose type the
    /// schema does not declare (see [`CsvDialect::strict`]).
    pub inference_sample: usize,
    /// Strict parsing mode (default `false`, lenient). In lenient mode,
    /// content after a closing quote is accepted, invalid UTF-8 decodes
    /// lossily, and blank lines are skipped. In strict mode, content after a
    /// closing quote and invalid UTF-8 are [`CsvParseError`]s.
    pub strict: bool,
}

impl Default for CsvDialect {
    fn default() -> Self {
        Self {
            delimiter: b',',
            quote: b'"',
            has_headers: true,
            inference_sample: 1000,
            strict: false,
        }
    }
}

/// A CSV parse failure (malformed quoting, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CsvParseError {
    /// 1-based physical line where the record started.
    pub line: usize,
    /// Description of the failure.
    pub message: String,
}

impl std::fmt::Display for CsvParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CSV parse error at line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for CsvParseError {}

impl From<std::io::Error> for CsvParseError {
    fn from(e: std::io::Error) -> Self {
        Self {
            line: 0,
            message: format!("I/O error: {e}"),
        }
    }
}

/// One parsed CSV record.
#[derive(Debug, Clone)]
pub struct CsvRecord {
    /// 1-based physical line where the record starts.
    pub line: usize,
    /// Raw (unescaped) field values.
    pub fields: Vec<String>,
}

/// How a CSV cell is interpreted when converting rows to JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ColumnType {
    /// `true` / `false` (case-insensitive).
    Boolean,
    /// Integer numerals.
    Integer,
    /// Floating-point numerals (weaker than [`ColumnType::Integer`]).
    Float,
    /// Anything else.
    String,
}

/// Hand-written streaming CSV state machine (no `csv` crate).
pub struct CsvReader<R: BufRead> {
    reader: R,
    dialect: CsvDialect,
    buffer: Vec<u8>,
    pos: usize,
    eof: bool,
    physical_line: usize,
    finished: bool,
}

impl<R: BufRead> CsvReader<R> {
    /// Wrap a reader with the default dialect (comma, quote, header row).
    pub fn new(reader: R) -> Self {
        Self::with_dialect(reader, CsvDialect::default())
    }

    /// Wrap a reader with a custom dialect.
    pub fn with_dialect(reader: R, dialect: CsvDialect) -> Self {
        Self {
            reader,
            dialect,
            buffer: Vec::with_capacity(8 * 1024),
            pos: 0,
            eof: false,
            physical_line: 0,
            finished: false,
        }
    }

    /// Ensure at least `n` bytes are available at `self.pos`.
    /// Returns `false` on end of input.
    fn ensure(&mut self, n: usize) -> Result<bool, std::io::Error> {
        if self.pos + n <= self.buffer.len() {
            return Ok(true);
        }
        if self.eof {
            return Ok(false);
        }
        self.buffer.clear();
        self.pos = 0;
        let read = self.reader.read_until(b'\n', &mut self.buffer)?;
        self.eof = read == 0;
        Ok(self.pos + n <= self.buffer.len())
    }

    /// Read the next record. Returns `Ok(None)` at end of input.
    ///
    /// In strict mode ([`CsvDialect::strict`]), malformed quoting and
    /// invalid UTF-8 are [`CsvParseError`]s; lenient mode (default) accepts
    /// content after a closing quote and decodes invalid UTF-8 lossily.
    pub fn next_record(&mut self) -> Result<Option<CsvRecord>, CsvParseError> {
        loop {
            if self.finished {
                return Ok(None);
            }
            let delimiter = self.dialect.delimiter;
            let quote = self.dialect.quote;
            let strict = self.dialect.strict;

            let mut fields: Vec<Vec<u8>> = vec![Vec::new()];
            let mut in_quotes = false;
            let mut field_start = true;
            let mut saw_content = false;
            let record_line = self.physical_line + 1;
            let mut record_done = false;

            let result = (|| -> Result<(), CsvParseError> {
                while !record_done {
                    if !self.ensure(1)? {
                        if in_quotes {
                            self.finished = true;
                            return Err(CsvParseError {
                                line: record_line,
                                message: "unterminated quoted field".into(),
                            });
                        }
                        self.finished = true;
                        return Ok(());
                    }
                    let byte = self.buffer[self.pos];
                    self.pos += 1;

                    if in_quotes {
                        if byte == quote {
                            if !self.ensure(1)? {
                                // Closing quote at EOF terminates the record.
                                in_quotes = false;
                                continue;
                            }
                            if self.buffer[self.pos] == quote {
                                fields
                                    .last_mut()
                                    .expect("fields always has a current field")
                                    .push(quote);
                                self.pos += 1;
                            } else {
                                in_quotes = false;
                                if strict {
                                    let next = self.buffer[self.pos];
                                    if next != delimiter && next != b'\r' && next != b'\n' {
                                        return Err(CsvParseError {
                                            line: record_line,
                                            message: "unexpected content after closing quote"
                                                .into(),
                                        });
                                    }
                                }
                            }
                        } else {
                            fields
                                .last_mut()
                                .expect("fields always has a current field")
                                .push(byte);
                            saw_content = true;
                        }
                        continue;
                    }

                    if field_start && byte == quote {
                        in_quotes = true;
                        field_start = false;
                        saw_content = true;
                        continue;
                    }

                    match byte {
                        b if b == delimiter => {
                            fields.push(Vec::new());
                            field_start = true;
                            saw_content = true;
                        }
                        b'\r' => {
                            if self.ensure(1)? && self.buffer[self.pos] == b'\n' {
                                self.pos += 1;
                            }
                            self.physical_line += 1;
                            record_done = true;
                        }
                        b'\n' => {
                            self.physical_line += 1;
                            record_done = true;
                        }
                        b => {
                            fields
                                .last_mut()
                                .expect("fields always has a current field")
                                .push(b);
                            field_start = false;
                            saw_content = true;
                        }
                    }
                }
                Ok(())
            })();

            // A strict-mode violation or I/O failure aborts the reader.
            if let Err(e) = result {
                self.finished = true;
                return Err(e);
            }

            // End of input: flush the final record if it has content.
            if !record_done {
                self.finished = true;
                if !saw_content && fields.len() == 1 && fields[0].is_empty() {
                    return Ok(None);
                }
                return decode_fields(fields, strict, record_line).map(|fields| {
                    Some(CsvRecord {
                        line: record_line,
                        fields,
                    })
                });
            }

            // Skip blank lines entirely.
            if !saw_content && fields.len() == 1 && fields[0].is_empty() {
                continue;
            }
            return decode_fields(fields, strict, record_line).map(|fields| {
                Some(CsvRecord {
                    line: record_line,
                    fields,
                })
            });
        }
    }
}

fn decode_fields(
    fields: Vec<Vec<u8>>,
    strict: bool,
    line: usize,
) -> Result<Vec<String>, CsvParseError> {
    fields
        .into_iter()
        .map(|f| {
            if strict {
                String::from_utf8(f).map_err(|_| CsvParseError {
                    line,
                    message: "invalid UTF-8 in CSV field".into(),
                })
            } else {
                Ok(String::from_utf8_lossy(&f).into_owned())
            }
        })
        .collect()
}

/// What happened to one CSV row during streaming validation.
#[derive(Debug)]
pub enum RowOutcome {
    /// Row converted to JSON and validated clean.
    Valid {
        /// 1-based physical line of the row.
        line: usize,
        /// The converted JSON row.
        value: Value,
        /// Original raw field values, in input column order.
        fields: Vec<String>,
    },
    /// Row converted (when representable) but produced errors.
    Invalid {
        /// 1-based physical line of the row.
        line: usize,
        /// The converted JSON row.
        value: Option<Value>,
        /// All validation errors for the row.
        errors: Vec<ValidationError>,
        /// Original raw field values, in input column order.
        fields: Vec<String>,
    },
    /// The row could not be parsed as CSV at all.
    ParseError {
        /// 1-based physical line of the failure.
        line: usize,
        /// Failure description.
        message: String,
    },
}

/// Aggregate result of a streaming CSV validation run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CsvStats {
    /// Data rows seen (headers excluded).
    pub total_rows: usize,
    /// Rows that passed validation.
    pub valid_rows: usize,
    /// Rows that failed validation.
    pub invalid_rows: usize,
    /// Rows that could not be parsed as CSV.
    pub parse_errors: usize,
}

/// Stream-validate a CSV file against a schema whose object properties are
/// the column names.
///
/// Column values are coerced by the schema's declared property type
/// (`boolean` / `integer` / `number` / `string`; string-typed columns keep
/// their raw text — leading zeros, `+`-prefixed phone numbers and similar
/// are preserved). Columns with no declared type fall back to type
/// inference (`bool`/`integer`/`number`/`string`) from up to
/// [`CsvDialect::inference_sample`] rows; empty cells become `null`. Rows
/// are validated one at a time with bounded memory. `on_row` receives each
/// [`RowOutcome`]; return `false` from it to stop early.
///
/// # Examples
///
/// ```
/// use tpt_valid_core::{validate_csv_stream, ValidationNode, ValidationOptions, DataType};
/// use std::io::Cursor;
///
/// let data = "name,age\nAlice,30\nBob,-1\n";
/// let node = ValidationNode::all(vec![
///     ValidationNode::CheckType(DataType::Object),
///     ValidationNode::CheckField("age".into(), Box::new(
///         ValidationNode::all(vec![
///             ValidationNode::CheckType(DataType::Integer),
///             ValidationNode::CheckMinimum(0.0),
///         ]))),
/// ]);
/// let stats = validate_csv_stream(&node, Cursor::new(data), &Default::default(),
///     &ValidationOptions::default(), |_row| true).unwrap();
/// assert_eq!(stats.total_rows, 2);
/// assert_eq!(stats.valid_rows, 1);
/// assert_eq!(stats.invalid_rows, 1);
/// ```
pub fn validate_csv_stream<R: BufRead>(
    node: &ValidationNode,
    reader: R,
    dialect: &CsvDialect,
    opts: &ValidationOptions,
    mut on_row: impl FnMut(RowOutcome) -> bool,
) -> Result<CsvStats, CsvParseError> {
    fn process_record(
        node: &ValidationNode,
        names: &[String],
        types: &[ColumnType],
        opts: &ValidationOptions,
        record: CsvRecord,
        stats: &mut CsvStats,
        on_row: &mut dyn FnMut(RowOutcome) -> bool,
    ) -> bool {
        stats.total_rows += 1;
        let value = record_to_value(names, types, &record.fields);
        let mut collector = ErrorCollector::new(opts.fail_fast, opts.max_errors);
        let mut path = PathCursor::new();
        check(node, &value, &mut path, opts, &mut collector, 0);
        let errors = collector.into_errors();
        if errors.is_empty() {
            stats.valid_rows += 1;
            on_row(RowOutcome::Valid {
                line: record.line,
                value,
                fields: record.fields,
            })
        } else {
            stats.invalid_rows += 1;
            on_row(RowOutcome::Invalid {
                line: record.line,
                value: Some(value),
                errors,
                fields: record.fields,
            })
        }
    }

    let mut stats = CsvStats::default();
    let mut csv = CsvReader::with_dialect(reader, dialect.clone());

    // Header row.
    let headers: Vec<String> = if dialect.has_headers {
        match csv.next_record()? {
            Some(h) => h.fields,
            None => return Ok(stats),
        }
    } else {
        Vec::new()
    };

    // Sample rows for type inference (bounded memory).
    let sample_cap = dialect.inference_sample.max(1);
    let mut sample: Vec<CsvRecord> = Vec::new();
    while sample.len() < sample_cap {
        match csv.next_record()? {
            Some(record) => sample.push(record),
            None => break,
        }
    }

    let width = if dialect.has_headers {
        headers.len()
    } else {
        sample.first().map(|r| r.fields.len()).unwrap_or(0)
    };
    let names: Vec<String> = if dialect.has_headers {
        headers
    } else {
        (0..width).map(|i| format!("column_{i}")).collect()
    };
    let declared = schema_column_types(node);
    let inferred = infer_column_types(sample.iter().map(|r| r.fields.clone()), width);
    let types = resolve_column_types(&names, &declared, inferred);

    for record in sample {
        if !process_record(node, &names, &types, opts, record, &mut stats, &mut on_row) {
            return Ok(stats);
        }
    }
    while let Some(record) = csv.next_record()? {
        if !process_record(node, &names, &types, opts, record, &mut stats, &mut on_row) {
            break;
        }
    }
    Ok(stats)
}

/// Extract the schema-declared type of each top-level object property from a
/// compiled state machine, for schema-driven CSV coercion. Properties whose
/// type the schema does not state directly (absent, multi-type without a
/// string/numeric dominant, or only expressible through arbitrary
/// combinators) are omitted and fall back to inference.
pub fn schema_column_types(node: &ValidationNode) -> HashMap<String, DataType> {
    let mut out = HashMap::new();
    collect_declared_types(node, &mut out);
    out
}

fn collect_declared_types(node: &ValidationNode, out: &mut HashMap<String, DataType>) {
    match node {
        ValidationNode::CheckAll(seq) | ValidationNode::CheckAllOf(seq) => {
            for sub in seq {
                collect_declared_types(sub, out);
            }
        }
        ValidationNode::CheckObjectEx(shape) => {
            for (name, sub) in &shape.properties {
                if let Some(t) = declared_property_type(sub) {
                    out.insert(name.clone(), t);
                }
            }
        }
        ValidationNode::CheckObject(properties) => {
            for (name, sub) in properties {
                if let Some(t) = declared_property_type(sub) {
                    out.insert(name.clone(), t);
                }
            }
        }
        ValidationNode::CheckField(name, sub) => {
            // Root-level field wrapper (schema without an object node).
            if let Some(t) = declared_property_type(sub) {
                out.insert(name.clone(), t);
            }
        }
        _ => {}
    }
}

/// The type a property sub-schema declares, if unambiguous.
fn declared_property_type(node: &ValidationNode) -> Option<DataType> {
    match node {
        ValidationNode::CheckType(t) => Some(*t),
        ValidationNode::CheckAll(seq) => {
            // Conjunction: all declared types must agree.
            let mut found: Option<DataType> = None;
            for sub in seq {
                if let Some(t) = declared_property_type(sub) {
                    match found {
                        Some(prev) if prev != t => return None,
                        _ => found = Some(t),
                    }
                }
            }
            found
        }
        ValidationNode::CheckAnyOf(children) => {
            // `"type": ["string", "null"]` compiles to anyOf of type checks.
            // CSV cells are text, so a single non-null alternative decides
            // the coercion; anything else is ambiguous.
            let mut distinct: Vec<DataType> = Vec::new();
            for child in children {
                let t = declared_property_type(child)?;
                if !distinct.contains(&t) {
                    distinct.push(t);
                }
            }
            let non_null: Vec<DataType> = distinct
                .into_iter()
                .filter(|t| !matches!(t, DataType::Null))
                .collect();
            if non_null.len() == 1 {
                Some(non_null[0])
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Map a schema-declared [`DataType`] to the CSV coercion applied to cells.
/// Non-scalar types and `null` keep their raw text (they fail validation,
/// which is the correct outcome for, say, an object in a CSV cell).
fn column_type_for_datatype(t: DataType) -> ColumnType {
    match t {
        DataType::Boolean => ColumnType::Boolean,
        DataType::Integer => ColumnType::Integer,
        DataType::Number => ColumnType::Float,
        _ => ColumnType::String,
    }
}

/// Combine schema-declared and inferred column types: declared wins;
/// inference is the fallback for untyped columns.
fn resolve_column_types(
    names: &[String],
    declared: &HashMap<String, DataType>,
    inferred: Vec<ColumnType>,
) -> Vec<ColumnType> {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            declared
                .get(name)
                .map(|t| column_type_for_datatype(*t))
                .unwrap_or_else(|| inferred.get(i).copied().unwrap_or(ColumnType::String))
        })
        .collect()
}

/// Infer a [`ColumnType`] per column from raw rows.
///
/// Empty cells are ignored; a column is Integer only if every non-empty
/// sampled value parses as an integer, etc.; anything else is String.
pub fn infer_column_types<I: Iterator<Item = Vec<String>>>(
    rows: I,
    width: usize,
) -> Vec<ColumnType> {
    let mut types = vec![ColumnType::Boolean; width.max(1)];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i >= types.len() {
                break;
            }
            if cell.is_empty() {
                continue;
            }
            let t = &mut types[i];
            if *t == ColumnType::Boolean && !is_bool(cell) {
                *t = ColumnType::Integer;
            }
            if *t == ColumnType::Integer && cell.parse::<i64>().is_err() {
                *t = ColumnType::Float;
            }
            if *t == ColumnType::Float && cell.parse::<f64>().is_err() {
                *t = ColumnType::String;
            }
        }
    }
    types
}

fn is_bool(s: &str) -> bool {
    s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("false")
}

/// Convert one CSV record into a JSON object using inferred column types.
pub fn record_to_value(names: &[String], types: &[ColumnType], fields: &[String]) -> Value {
    let mut map = Map::new();
    for (i, field) in fields.iter().enumerate() {
        let name = names
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("column_{i}"));
        let value = if field.is_empty() {
            Value::Null
        } else {
            match types.get(i) {
                Some(ColumnType::Boolean) => match field.to_ascii_lowercase().as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    _ => Value::String(field.clone()),
                },
                Some(ColumnType::Integer) => field
                    .parse::<i64>()
                    .map(Value::from)
                    .unwrap_or_else(|_| Value::String(field.clone())),
                Some(ColumnType::Float) => field
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .unwrap_or_else(|| Value::String(field.clone())),
                _ => Value::String(field.clone()),
            }
        };
        map.insert(name, value);
    }
    Value::Object(map)
}

/// Per-row validation errors with the row's 1-based line number.
pub type RowErrors = Vec<(usize, Vec<ValidationError>)>;

/// Convenience helper: validate a CSV from bytes, returning stats and the
/// per-row errors for invalid rows (in encounter order).
pub fn validate_csv_bytes(
    node: &ValidationNode,
    data: &[u8],
    dialect: &CsvDialect,
    opts: &ValidationOptions,
) -> Result<(CsvStats, RowErrors), CsvParseError> {
    let mut collected: Vec<(usize, Vec<ValidationError>)> = Vec::new();
    let stats = validate_csv_stream(node, BufReader::new(data), dialect, opts, |row| {
        if let RowOutcome::Invalid { line, errors, .. } = row {
            collected.push((line, errors));
        }
        true
    })?;
    Ok((stats, collected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DataType;
    use serde_json::json;
    use std::io::Cursor;

    fn simple_schema() -> ValidationNode {
        ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckField(
                "age".into(),
                Box::new(ValidationNode::all(vec![
                    ValidationNode::CheckType(DataType::Integer),
                    ValidationNode::CheckMinimum(0.0),
                ])),
            ),
            ValidationNode::CheckField(
                "email".into(),
                Box::new(ValidationNode::CheckFormat(crate::format::Format::Email)),
            ),
        ])
    }

    #[test]
    fn csv_basic_parsing() {
        let data = "a,b,c\n1,2,3\n4,5,6\n";
        let mut r = CsvReader::new(Cursor::new(data));
        let header = r.next_record().unwrap().unwrap();
        assert_eq!(header.fields, vec!["a", "b", "c"]);
        let r1 = r.next_record().unwrap().unwrap();
        assert_eq!(r1.fields, vec!["1", "2", "3"]);
        assert_eq!(r1.line, 2);
        let r2 = r.next_record().unwrap().unwrap();
        assert_eq!(r2.fields, vec!["4", "5", "6"]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn csv_quoted_fields_and_embedded_newlines() {
        let data = "a,b\n\"hello, world\",\"say \"\"hi\"\"\"\n\"multi\nline\",2\n";
        let mut r = CsvReader::new(Cursor::new(data));
        assert_eq!(r.next_record().unwrap().unwrap().fields, vec!["a", "b"]);
        let r1 = r.next_record().unwrap().unwrap();
        assert_eq!(r1.fields, vec!["hello, world", "say \"hi\""]);
        let r2 = r.next_record().unwrap().unwrap();
        assert_eq!(r2.fields, vec!["multi\nline", "2"]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn csv_custom_delimiter_and_crlf() {
        let data = "a;b\r\n1;2\r\n";
        let mut r = CsvReader::with_dialect(
            Cursor::new(data),
            CsvDialect {
                delimiter: b';',
                ..Default::default()
            },
        );
        assert_eq!(r.next_record().unwrap().unwrap().fields, vec!["a", "b"]);
        assert_eq!(r.next_record().unwrap().unwrap().fields, vec!["1", "2"]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn csv_no_trailing_newline_and_eof_quote() {
        let data = "a,b\n1,\"two\"";
        let mut r = CsvReader::new(Cursor::new(data));
        let _ = r.next_record().unwrap().unwrap();
        let rec = r.next_record().unwrap().unwrap();
        assert_eq!(rec.fields, vec!["1", "two"]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn csv_unterminated_quote_is_error() {
        let data = "a,b\n\"oops,2\n";
        let mut r = CsvReader::new(Cursor::new(data));
        let _ = r.next_record().unwrap().unwrap();
        let err = r.next_record().unwrap_err();
        assert!(err.message.contains("unterminated"));
    }

    #[test]
    fn csv_blank_lines_skipped() {
        let data = "a\n\n1\n\n\n2\n";
        let mut r = CsvReader::new(Cursor::new(data));
        let _ = r.next_record().unwrap().unwrap();
        assert_eq!(r.next_record().unwrap().unwrap().fields, vec!["1"]);
        assert_eq!(r.next_record().unwrap().unwrap().fields, vec!["2"]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn csv_utf8_fields() {
        let data = "name\n\"Zoë 🚀\"\n";
        let mut r = CsvReader::new(Cursor::new(data));
        let _ = r.next_record().unwrap().unwrap();
        let rec = r.next_record().unwrap().unwrap();
        assert_eq!(rec.fields[0], "Zoë 🚀");
    }

    #[test]
    fn csv_many_blank_lines_no_stack_overflow() {
        let data = "a\n".to_string() + &"\n".repeat(50_000) + "1\n";
        let mut r = CsvReader::new(Cursor::new(data));
        let _ = r.next_record().unwrap().unwrap();
        assert_eq!(r.next_record().unwrap().unwrap().fields, vec!["1"]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn type_inference_priorities() {
        let rows = vec![
            vec!["true".into(), "1".into(), "1.5".into(), "x".into()],
            vec!["false".into(), "2".into(), "3".into(), "y".into()],
            vec!["".into(), "".into(), "".into(), "".into()],
        ];
        let types = infer_column_types(rows.into_iter(), 4);
        assert_eq!(
            types,
            vec![
                ColumnType::Boolean,
                ColumnType::Integer,
                ColumnType::Float,
                ColumnType::String
            ]
        );
    }

    #[test]
    fn inference_integer_column_with_float_input() {
        let rows = vec![vec!["1".into()], vec!["2".into()]];
        let types = infer_column_types(rows.into_iter(), 1);
        assert_eq!(types[0], ColumnType::Integer);
        let rows = vec![vec!["1".into()], vec!["2.5".into()]];
        let types = infer_column_types(rows.into_iter(), 1);
        assert_eq!(types[0], ColumnType::Float);
    }

    #[test]
    fn row_conversion_types() {
        let names = vec![
            "b".into(),
            "i".into(),
            "f".into(),
            "s".into(),
            "empty".into(),
        ];
        let types = vec![
            ColumnType::Boolean,
            ColumnType::Integer,
            ColumnType::Float,
            ColumnType::String,
            ColumnType::String,
        ];
        let v = record_to_value(
            &names,
            &types,
            &[
                "TRUE".into(),
                "42".into(),
                "3.25".into(),
                "hi".into(),
                "".into(),
            ],
        );
        assert_eq!(v["b"], json!(true));
        assert_eq!(v["i"], json!(42));
        assert_eq!(v["f"], json!(3.25));
        assert_eq!(v["s"], json!("hi"));
        assert_eq!(v["empty"], json!(null));
    }

    #[test]
    fn streaming_validation_end_to_end() {
        // Note: type inference requires consistent column types, so bad rows
        // here fail on *values* (format, bound), not on column typing.
        let data = "name,age,email\nAlice,30,alice@example.com\nBob,25,bob-at-example.com\nCarol,-5,carol@example.com\n";
        let node = simple_schema();
        let mut valid = 0;
        let mut invalid = 0;
        let mut first_error_line = 0;
        let stats = validate_csv_stream(
            &node,
            Cursor::new(data),
            &Default::default(),
            &ValidationOptions::default(),
            |row| {
                match row {
                    RowOutcome::Valid { .. } => valid += 1,
                    RowOutcome::Invalid { line, errors, .. } => {
                        invalid += 1;
                        if first_error_line == 0 {
                            first_error_line = line;
                        }
                        assert!(!errors.is_empty());
                    }
                    RowOutcome::ParseError { .. } => panic!("unexpected parse error"),
                }
                true
            },
        )
        .unwrap();
        assert_eq!(stats.total_rows, 3);
        assert_eq!(stats.valid_rows, 1);
        assert_eq!(stats.invalid_rows, 2);
        assert_eq!(valid, 1);
        assert_eq!(invalid, 2);
        assert_eq!(first_error_line, 3); // "Bob,x,..." row
    }

    #[test]
    fn streaming_stops_on_false() {
        let data = "age\n1\n2\n3\n";
        let node = ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckField(
                "age".into(),
                Box::new(ValidationNode::CheckType(DataType::Integer)),
            ),
        ]);
        let stats = validate_csv_stream(
            &node,
            Cursor::new(data),
            &Default::default(),
            &ValidationOptions::default(),
            |_row| false,
        )
        .unwrap();
        assert_eq!(stats.total_rows, 1);
    }

    #[test]
    fn headerless_csv_uses_generated_names() {
        let data = "1,hello\n2,world\n";
        let node = ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckField(
                "column_0".into(),
                Box::new(ValidationNode::CheckType(DataType::Integer)),
            ),
        ]);
        let stats = validate_csv_stream(
            &node,
            Cursor::new(data),
            &CsvDialect {
                has_headers: false,
                ..Default::default()
            },
            &ValidationOptions::default(),
            |_row| true,
        )
        .unwrap();
        assert_eq!(stats.total_rows, 2);
        assert_eq!(stats.valid_rows, 2);
    }

    #[test]
    fn parse_error_mid_stream_is_fatal() {
        let data = "age\n1\n\"oops\n2\n";
        let node = ValidationNode::CheckType(DataType::Object);
        let result = validate_csv_stream(
            &node,
            Cursor::new(data),
            &Default::default(),
            &ValidationOptions::default(),
            |_row| true,
        );
        assert!(result.is_err());
    }

    #[test]
    fn validate_csv_bytes_collects_row_errors() {
        let data = "age\n5\n-1\n7\n";
        let node = ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckField("age".into(), Box::new(ValidationNode::CheckMinimum(0.0))),
        ]);
        let (stats, errors) = validate_csv_bytes(
            &node,
            data.as_bytes(),
            &Default::default(),
            &ValidationOptions::default(),
        )
        .unwrap();
        assert_eq!(stats.total_rows, 3);
        assert_eq!(stats.valid_rows, 2);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, 3); // line 3
        assert_eq!(errors[0].1[0].path, "$.age");
    }

    #[test]
    fn schema_declared_string_columns_keep_raw_text() {
        // Regression: zip codes with leading zeros and +-country-code phone
        // numbers in `type: string` columns must not be coerced to numbers.
        let node = ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckObjectEx(crate::node::ObjectShape {
                properties: vec![
                    (
                        "zip".into(),
                        ValidationNode::all(vec![
                            ValidationNode::CheckType(DataType::String),
                            ValidationNode::CheckPattern(Box::new(
                                regex::Regex::new(r"^\d{5}$").unwrap(),
                            )),
                        ]),
                    ),
                    ("phone".into(), ValidationNode::CheckType(DataType::String)),
                ],
                ..Default::default()
            }),
        ]);
        let data = "zip,phone\n01234,+4155550123\n";
        let mut valid_value = None;
        let stats = validate_csv_stream(
            &node,
            Cursor::new(data),
            &Default::default(),
            &ValidationOptions::default(),
            |row| match row {
                RowOutcome::Valid { value, .. } => {
                    valid_value = Some(value);
                    true
                }
                other => panic!("expected valid row, got {other:?}"),
            },
        )
        .unwrap();
        assert_eq!(stats.valid_rows, 1);
        let v = valid_value.unwrap();
        assert_eq!(v["zip"], json!("01234"), "leading zero preserved");
        assert_eq!(v["phone"], json!("+4155550123"), "+ prefix preserved");
    }

    #[test]
    fn schema_declared_numeric_columns_coerce_and_validate() {
        let node = ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckField(
                "age".into(),
                Box::new(ValidationNode::all(vec![
                    ValidationNode::CheckType(DataType::Integer),
                    ValidationNode::CheckMinimum(0.0),
                ])),
            ),
        ]);
        // "030" coerces to integer 30 by the declared type even though the
        // first sampled row would infer Integer anyway.
        let data = "age\n030\n-1\n";
        let (stats, errors) = validate_csv_bytes(
            &node,
            data.as_bytes(),
            &Default::default(),
            &ValidationOptions::default(),
        )
        .unwrap();
        assert_eq!(stats.valid_rows, 1);
        assert_eq!(stats.invalid_rows, 1);
        assert_eq!(errors[0].1[0].path, "$.age");
    }

    #[test]
    fn schema_nullable_string_type_coerces_as_string() {
        // `"type": ["string", "null"]` → text column (empty cell → null).
        let node = ValidationNode::CheckField(
            "note".into(),
            Box::new(ValidationNode::CheckAnyOf(vec![
                ValidationNode::CheckType(DataType::String),
                ValidationNode::CheckType(DataType::Null),
            ])),
        );
        let data = "id,note\n1,0012\n2,\n";
        let mut values = Vec::new();
        validate_csv_stream(
            &node,
            Cursor::new(data),
            &Default::default(),
            &ValidationOptions::default(),
            |row| {
                if let RowOutcome::Valid { value, .. } = row {
                    values.push(value);
                }
                true
            },
        )
        .unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["note"], json!("0012"));
        assert_eq!(values[1]["note"], json!(null));
    }

    #[test]
    fn untyped_columns_still_use_inference() {
        // Schema says nothing about `n` → sampled inference applies.
        let node = ValidationNode::CheckType(DataType::Object);
        let data = "n\n1\n2\n";
        let mut values = Vec::new();
        validate_csv_stream(
            &node,
            Cursor::new(data),
            &Default::default(),
            &ValidationOptions::default(),
            |row| {
                if let RowOutcome::Valid { value, .. } = row {
                    values.push(value);
                }
                true
            },
        )
        .unwrap();
        assert_eq!(values[0]["n"], json!(1), "inferred integer");
    }

    #[test]
    fn strict_mode_rejects_content_after_closing_quote() {
        let data = "a,b\n\"oops\"garbage,2\n";
        let strict = CsvDialect {
            strict: true,
            ..Default::default()
        };
        let mut r = CsvReader::with_dialect(Cursor::new(data), strict);
        let _ = r.next_record().unwrap().unwrap(); // header parses fine
        let e = r.next_record().unwrap_err();
        assert!(e.message.contains("after closing quote"));

        // Lenient mode (default) accepts it.
        let mut lenient = CsvReader::new(Cursor::new(data));
        let _ = lenient.next_record().unwrap().unwrap();
        let rec = lenient.next_record().unwrap().unwrap();
        assert_eq!(rec.fields, vec!["oopsgarbage", "2"]);
    }

    #[test]
    fn strict_mode_rejects_invalid_utf8() {
        let data = b"a\n\"caf\xff\xe9\"\n";
        let strict = CsvDialect {
            strict: true,
            ..Default::default()
        };
        let mut r = CsvReader::with_dialect(Cursor::new(&data[..]), strict);
        let _ = r.next_record().unwrap().unwrap();
        let e = r.next_record().unwrap_err();
        assert!(e.message.contains("UTF-8"));

        // Lenient mode decodes lossily.
        let mut lenient = CsvReader::new(Cursor::new(&data[..]));
        let _ = lenient.next_record().unwrap().unwrap();
        let rec = lenient.next_record().unwrap().unwrap();
        assert_eq!(rec.fields[0], "caf\u{fffd}\u{fffd}");
    }

    #[test]
    fn strict_mode_still_accepts_normal_csv() {
        let data = "a,b\n\"hello, world\",\"say \"\"hi\"\"\"\n1,2\n";
        let strict = CsvDialect {
            strict: true,
            ..Default::default()
        };
        let mut r = CsvReader::with_dialect(Cursor::new(data), strict);
        assert_eq!(r.next_record().unwrap().unwrap().fields, vec!["a", "b"]);
        assert_eq!(
            r.next_record().unwrap().unwrap().fields,
            vec!["hello, world", "say \"hi\""]
        );
        assert_eq!(r.next_record().unwrap().unwrap().fields, vec!["1", "2"]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn strict_parse_error_is_fatal_in_stream() {
        let node = ValidationNode::CheckType(DataType::Object);
        let data = "a\n\"x\"y\n";
        let strict = CsvDialect {
            strict: true,
            ..Default::default()
        };
        let result = validate_csv_stream(
            &node,
            Cursor::new(data),
            &strict,
            &ValidationOptions::default(),
            |_row| true,
        );
        assert!(result.is_err());
    }
}
