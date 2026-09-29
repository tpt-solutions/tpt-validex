//! # tpt-validex-streamforge
//!
//! The `validate` transformation stage for [tpt-streamforge] pipelines
//! (spec §5.6, §13 Use Case 2): every row is checked against a compiled
//! `tpt-validex` schema; valid rows flow downstream, invalid rows are
//! handled per the configured [`ErrorMode`] and optionally quarantined to
//! an `errors.jsonl` file with line numbers.
//!
//! ```no_run
//! use tpt_stream_core::Pipeline;
//! use tpt_valid_schema::Validator;
//! use tpt_validex_streamforge::{ErrorMode, PipelineExt, ValidateConfig};
//!
//! # async fn example() -> tpt_stream_core::Result<()> {
//! let schema = Validator::new(r#"{
//!     "type": "object",
//!     "properties": {
//!         "name": {"type": "string", "minLength": 1},
//!         "age":  {"type": "integer", "minimum": 0}
//!     },
//!     "required": ["name", "age"]
//! }"#).map_err(|e| tpt_stream_core::Error::Config(e.to_string()))?;
//!
//! let mut pipeline = Pipeline::new();
//! pipeline
//!     .read_csv("input.csv")
//!     .validate_with(
//!         &schema,
//!         ValidateConfig::new()
//!             .on_invalid(ErrorMode::Skip)
//!             .errors_to("errors.jsonl"),
//!     )
//!     .write_csv("valid_output.csv");
//! let stats = pipeline.execute().await?;
//! # Ok(())
//! # }
//! ```
//!
//! [tpt-streamforge]: https://github.com/tpt-solutions/tpt-streamforge
//!
//! See `LICENSE-MIT` and `LICENSE-APACHE` for licensing terms.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tpt_stream_core::pipeline::PipelineStage;
use tpt_stream_core::table::RecordBatch;
use tpt_stream_core::{Error, Pipeline};

use tpt_valid_schema::Validator;

/// What happens to a row that fails validation.
///
/// Spec §7: configurable error handling modes — `skip` / `abort` / `log`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorMode {
    /// Drop invalid rows and continue (default). Counts are kept in
    /// [`Validate::stats`]; the errors file (if configured) still records
    /// every dropped row.
    #[default]
    Skip,
    /// Drop invalid rows, continue, and log a summary line per bad row to
    /// stderr. The errors file (if configured) still records every row.
    Log,
    /// Stop the pipeline with a [`tpt_stream_core::Error::DataQuality`]
    /// error on the first invalid row.
    Abort,
}

/// Configuration for the [`Validate`] stage.
#[derive(Debug, Clone, Default)]
pub struct ValidateConfig {
    mode: ErrorMode,
    errors_path: Option<PathBuf>,
    max_log_lines: usize,
}

impl ValidateConfig {
    /// Default configuration: [`ErrorMode::Skip`], no errors file.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the invalid-row mode.
    pub fn on_invalid(mut self, mode: ErrorMode) -> Self {
        self.mode = mode;
        self
    }

    /// Quarantine every invalid row to a JSONL file. Each line is
    /// `{"line": <1-based data row number>, "row": {...}, "errors": [...]}`.
    pub fn errors_to(mut self, path: impl Into<PathBuf>) -> Self {
        self.errors_path = Some(path.into());
        self
    }

    /// Cap stderr logging in [`ErrorMode::Log`] (default 100; extra invalid
    /// rows are counted but not printed).
    pub fn max_log_lines(mut self, n: usize) -> Self {
        self.max_log_lines = n;
        self
    }
}

/// Running counters for a [`Validate`] stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ValidateStats {
    /// Rows seen (valid + invalid).
    pub rows_in: usize,
    /// Rows that passed the schema.
    pub valid: usize,
    /// Rows that failed the schema (or, in `Abort` mode, the row that
    /// stopped the pipeline).
    pub invalid: usize,
}

impl ValidateStats {
    /// Whether no invalid rows have been seen.
    pub fn all_valid(&self) -> bool {
        self.invalid == 0
    }
}

/// The `validate` pipeline stage (spec §5.6).
///
/// Construct with [`PipelineExt::validate`] / [`PipelineExt::validate_with`]
/// or directly and add with `Pipeline::stage`.
pub struct Validate {
    validator: Arc<Validator>,
    config: ValidateConfig,
    errors_writer: Option<BufWriter<File>>,
    stats: ValidateStats,
    /// 1-based data-row counter (header excluded), stable across batches.
    next_row: usize,
    log_lines: usize,
    aborted: Option<String>,
}

impl fmt::Debug for Validate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Validate")
            .field("mode", &self.config.mode)
            .field("errors_path", &self.config.errors_path)
            .field("stats", &self.stats)
            .finish()
    }
}

impl Validate {
    /// Validate with the default configuration ([`ErrorMode::Skip`]).
    pub fn new(validator: &Validator) -> Self {
        Self::with_config(validator, ValidateConfig::default())
    }

    /// Validate with explicit configuration.
    pub fn with_config(validator: &Validator, config: ValidateConfig) -> Self {
        Validate {
            validator: Arc::new(validator.clone()),
            config,
            errors_writer: None,
            stats: ValidateStats::default(),
            next_row: 1,
            log_lines: 0,
            aborted: None,
        }
    }

    /// Running row counters.
    pub fn stats(&self) -> ValidateStats {
        self.stats
    }

    fn ensure_writer(&mut self) -> Result<&mut BufWriter<File>, Error> {
        if self.errors_writer.is_none() {
            let path = self
                .config
                .errors_path
                .clone()
                .ok_or_else(|| Error::Config("no errors path configured".into()))?;
            let file = File::create(&path)?;
            self.errors_writer = Some(BufWriter::new(file));
        }
        Ok(self.errors_writer.as_mut().expect("just set"))
    }

    fn quarantine(
        &mut self,
        row_number: usize,
        row: &Value,
        errors: &[tpt_valid_core::ValidationError],
    ) -> Result<(), Error> {
        let record = serde_json::json!({
            "line": row_number,
            "row": row,
            "errors": errors,
        });
        self.ensure_writer()?
            .write_all(format!("{record}\n").as_bytes())?;
        Ok(())
    }
}

#[async_trait]
impl PipelineStage for Validate {
    fn name(&self) -> &'static str {
        "validate"
    }

    async fn process(&mut self, mut batch: RecordBatch) -> Result<Vec<RecordBatch>, Error> {
        if self.aborted.is_some() {
            return Err(Error::DataQuality(self.aborted.clone().expect("checked")));
        }
        let num_rows = batch.num_rows();
        if num_rows == 0 {
            return Ok(vec![batch]);
        }

        let columns: Vec<String> = batch.column_names().into_iter().map(String::from).collect();
        let mut keep = vec![true; num_rows];
        let mut any_dropped = false;

        for (i, keep_i) in keep.iter_mut().enumerate() {
            self.stats.rows_in += 1;
            let row_number = self.next_row;
            self.next_row += 1;

            let mut map = serde_json::Map::with_capacity(columns.len());
            for name in &columns {
                let cell = batch
                    .cell(i, name)
                    .unwrap_or(tpt_stream_core::value::Value::Null);
                map.insert(name.clone(), value_to_json(cell));
            }
            let row_json = Value::Object(map);

            let report = self.validator.validate(&row_json);
            if report.is_valid() {
                self.stats.valid += 1;
                continue;
            }

            self.stats.invalid += 1;
            match self.config.mode {
                ErrorMode::Abort => {
                    let first = report.errors.first();
                    let detail = match first {
                        Some(e) => format!("row {row_number}: {} ({})", e.path, e.message),
                        None => format!("row {row_number}: invalid"),
                    };
                    let message = format!("validation failed: {detail}");
                    self.aborted = Some(message.clone());
                    return Err(Error::DataQuality(message));
                }
                ErrorMode::Log => {
                    if self.log_lines < self.config.max_log_lines {
                        let first = report
                            .errors
                            .first()
                            .map(|e| format!("{}: {}", e.path, e.message))
                            .unwrap_or_else(|| "invalid".to_string());
                        eprintln!("[tpt-validex] row {row_number} invalid: {first}");
                        self.log_lines += 1;
                    }
                }
                ErrorMode::Skip => {}
            }

            if self.config.errors_path.is_some() {
                self.quarantine(row_number, &row_json, &report.errors)?;
            }
            *keep_i = false;
            any_dropped = true;
        }

        if num_rows > 0 && any_dropped {
            for column in batch.columns_mut() {
                column.retain_rows(&keep);
            }
            batch.recompute_row_count();
        }

        Ok(vec![batch])
    }

    async fn finish(&mut self) -> Result<Vec<RecordBatch>, Error> {
        if let Some(writer) = self.errors_writer.as_mut() {
            writer.flush()?;
        }
        Ok(Vec::new())
    }
}

/// Convert a streamforge [`tpt_stream_core::value::Value`] cell into JSON.
///
/// Dates render as `YYYY-MM-DD` and timestamps as
/// `YYYY-MM-DD HH:MM:SS` strings (streamforge's canonical text forms), so
/// schema `format: "date"` / string constraints apply naturally.
pub fn value_to_json(value: tpt_stream_core::value::Value) -> Value {
    use tpt_stream_core::value::{date_to_string, timestamp_to_string, Value as Sv};
    match value {
        Sv::Int32(v) => Value::from(v as i64),
        Sv::Int64(v) => Value::from(v),
        Sv::Float32(v) => serde_json::Number::from_f64(v as f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Sv::Float64(v) => serde_json::Number::from_f64(v)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Sv::Utf8(s) => Value::String(s),
        Sv::Bool(b) => Value::Bool(b),
        Sv::Date(days) => Value::String(date_to_string(days)),
        Sv::Timestamp(micros) => Value::String(timestamp_to_string(micros)),
        Sv::Null => Value::Null,
    }
}

/// Extension methods mirroring the spec §5.6 pipeline shape:
/// `read_csv(...).validate(&schema).write_csv(...)` with invalid rows
/// quarantined to `errors.jsonl`.
pub trait PipelineExt {
    /// Add a [`Validate`] stage with the default configuration
    /// ([`ErrorMode::Skip`]).
    fn validate(&mut self, validator: &Validator) -> &mut Pipeline;

    /// Add a [`Validate`] stage with explicit configuration.
    fn validate_with(&mut self, validator: &Validator, config: ValidateConfig) -> &mut Pipeline;
}

impl PipelineExt for Pipeline {
    fn validate(&mut self, validator: &Validator) -> &mut Pipeline {
        self.stage(Validate::new(validator))
    }

    fn validate_with(&mut self, validator: &Validator, config: ValidateConfig) -> &mut Pipeline {
        self.stage(Validate::with_config(validator, config))
    }
}

#[cfg(test)]
mod tests;
