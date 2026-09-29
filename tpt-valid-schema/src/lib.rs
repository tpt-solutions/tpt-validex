//! # tpt-valid-schema
//!
//! Schema front-end for `tpt-validex`: parses JSON Schema (Draft 2020-12),
//! validates schema semantics, optimizes an intermediate representation and
//! compiles it to the [`tpt_valid_core::ValidationNode`] state machine.
//!
//! Pipeline (spec §4.2):
//!
//! ```text
//! schema text ──▶ tokenizer ──▶ JSON value ──▶ AST ──▶ semantic checks
//!            ──▶ IR ──▶ IR optimizer ──▶ ValidationNode state machine ──▶ cache
//! ```
//!
//! The tokenizer and JSON reader for schema documents are custom with zero
//! external dependencies (spec §3.5). Compilation happens once per schema;
//! validation afterwards is O(1) per field.
//!
//! See `LICENSE-MIT` and `LICENSE-APACHE` for licensing terms.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod ast;
pub mod cache;
pub mod compiler;
pub mod error;
pub mod ir;
pub mod json;
pub mod tokenizer;
pub mod validator;

pub use ast::{ObjectAst, SchemaAst};
pub use cache::SchemaCache;
pub use compiler::compile;
pub use error::SchemaError;
pub use validator::{FlowError, Validator};

/// Semantic warning collected while compiling a schema (unknown keywords,
/// ignored-but-noticed constructs). Non-fatal by design; surfaced through
/// [`Validator::warnings`].
pub type Warning = String;

/// Map a JSON Schema type keyword to a [`DataType`].
pub(crate) fn type_keyword(keyword: &str) -> Option<DataType> {
    DataType::from_keyword(keyword)
}

/// The JSON type name of a value (for schema error messages).
pub(crate) fn type_name(value: &serde_json::Value) -> &'static str {
    tpt_valid_core::json_type_name(value)
}

use tpt_valid_core::DataType;
