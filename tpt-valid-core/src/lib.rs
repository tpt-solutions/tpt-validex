//! # tpt-valid-core
//!
//! Core validation engine for `tpt-validex`: the validation state machine
//! ([`ValidationNode`]), structured error reporting, built-in format
//! validators, and batch / streaming (CSV, JSONL) validation.
//!
//! This crate is intentionally low-level. Most users should consume the
//! [`tpt-valid-schema`](https://docs.rs/tpt-valid-schema) front-end (JSON
//! Schema / DSL compilation) or the [`validex`](https://docs.rs/validex)
//! facade instead.
//!
//! # Design notes (spec §5.2, §5.5)
//!
//! * Schemas are compiled to a tree of [`ValidationNode`]s once; validation
//!   then traverses the tree with O(1) work per field.
//! * All errors are collected (not just the first) unless
//!   [`ValidationOptions::fail_fast`] is set.
//! * Regex patterns are pre-compiled; enum checks use hash sets for O(1)
//!   lookup.
//!
//! See `LICENSE-MIT` and `LICENSE-APACHE` for licensing terms.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod batch;
pub mod csv;
pub mod engine;
pub mod error;
pub mod format;
pub mod jsonl;
pub mod node;
pub mod types;

pub use batch::{validate_batch, ValidationOutcome};
pub use csv::{
    infer_column_types, validate_csv_bytes, validate_csv_stream, ColumnType, CsvDialect,
    CsvParseError, CsvReader, CsvRecord, CsvStats, RowErrors, RowOutcome,
};
pub use engine::{validate, validate_value, CustomFormats, FormatFn, ValidationOptions};
pub use error::{ErrorCollector, ValidationError, ValidationReport};
pub use format::Format;
pub use jsonl::{validate_jsonl_stream, JsonlRowOutcome, JsonlStats};
pub use node::{AdditionalProperties, EnumSet, ObjectShape, ValidationNode};
pub use types::{json_type_name, DataType};

/// Parse a JSON document with the platform's fastest backend, flattening
/// backend errors into a plain message. Used by streaming validation and
/// available to callers who want the same fallback chain.
pub fn parse_value(input: &str) -> Result<serde_json::Value, String> {
    tpt_valid_parser::parse(input).map_err(|e| e.to_string())
}
