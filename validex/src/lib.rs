//! # validex
//!
//! The user-facing facade for **tpt-validex** — a blazing-fast, universal,
//! embeddable data validation engine. One schema, every language.
//!
//! - Validate millions of records per second (SIMD-optimized JSON parsing).
//! - JSON Schema (Draft 2020-12 subset) **and** an ergonomic Rust DSL.
//! - Identical behavior from Python, JavaScript/WASM, Go, C/C++, and Rust.
//! - Minimal dependency tree; zero Apache-2.0-only dependencies.
//!
//! # Quick start (Rust)
//!
//! ```
//! use validex::{Validator, schema};
//! use serde_json::json;
//!
//! // From JSON Schema
//! let validator = Validator::new(r#"{
//!     "type": "object",
//!     "properties": {
//!         "name": {"type": "string", "minLength": 1},
//!         "age":  {"type": "integer", "minimum": 0, "maximum": 150}
//!     },
//!     "required": ["name", "age"]
//! }"#).unwrap();
//!
//! assert!(validator.validate(&json!({"name": "Alice", "age": 30})).is_valid());
//!
//! // Or from the DSL — compiled through the same pipeline, with the DSL
//! // grammar validated at compile time.
//! let v = schema! {
//!     object {
//!         required "name" => string(min_length = 1, max_length = 100),
//!         required "age"  => integer(min = 0, max = 150),
//!         optional "email" => string(format = "email"),
//!         optional "tags" => array(items = string(), min_items = 0, max_items = 10),
//!     }
//! };
//!
//! assert!(v.validate(&json!({"name": "Bob", "age": 25, "tags": ["a"]})).is_valid());
//! assert!(!v.validate(&json!({"name": "", "age": 999})).is_valid());
//! ```
//!
//! # Batch and streaming
//!
//! ```
//! use validex::{Validator, schema};
//! use serde_json::json;
//!
//! let v = schema! { object { required "id" => integer(min = 0) } };
//! let outcomes = v.validate_batch(&[json!({"id": 1}), json!({"id": -1})]);
//! assert_eq!(outcomes.len(), 2);
//! assert!(outcomes[0].valid);
//! assert!(!outcomes[1].valid);
//! ```
//!
//! See `LICENSE-MIT` and `LICENSE-APACHE` for licensing terms.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use serde_json::json;
pub use tpt_valid_core::{
    json_type_name, validate_batch as core_validate_batch, AdditionalProperties, DataType, EnumSet,
    ErrorCollector, Format, ValidationError, ValidationNode, ValidationOptions, ValidationOutcome,
    ValidationReport,
};
pub use tpt_valid_schema::{
    ast, compile as compile_schema_ast, FlowError, SchemaCache, SchemaError, Validator,
};
pub use validex_macros::schema;

/// The DSL builder namespace (see the [`schema!`] macro). All types live in
/// [`ast`]; this module exists so `schema!`'s generated code has stable paths
/// under the `validex` crate name.
pub mod dsl {
    pub use super::schema;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_schema_and_dsl_agree() {
        let from_json = Validator::new(
            r#"{
                "type": "object",
                "properties": {
                    "name": {"type": "string", "minLength": 1},
                    "age": {"type": "integer", "minimum": 0, "maximum": 150}
                },
                "required": ["name", "age"]
            }"#,
        )
        .unwrap();

        let from_dsl = schema! {
            object {
                required "name" => string(min_length = 1),
                required "age" => integer(min = 0, max = 150),
            }
        };

        let good = json!({"name": "Alice", "age": 30});
        let bad = json!({"name": "", "age": 200});
        assert!(from_json.validate(&good).is_valid());
        assert!(from_dsl.validate(&good).is_valid());
        assert_eq!(from_json.validate(&bad).errors.len(), 2);
        assert_eq!(from_dsl.validate(&bad).errors.len(), 2);
    }

    #[test]
    fn dsl_arrays_formats_and_enums() {
        let v = schema! {
            object {
                required "status" => string(enum = ["active", "paused"]),
                optional "email" => string(format = "email"),
                optional "scores" => array(items = integer(min = 0, max = 100), min_items = 1, max_items = 5),
            }
        };
        assert!(v
            .validate(&json!({"status": "active", "email": "a@b.c", "scores": [10, 99]}))
            .is_valid());
        assert!(!v.validate(&json!({"status": "deleted"})).is_valid());
        assert!(!v
            .validate(&json!({"status": "active", "scores": []}))
            .is_valid());
        assert!(!v
            .validate(&json!({"status": "active", "scores": [101]}))
            .is_valid());
        assert!(!v
            .validate(&json!({"status": "active", "email": "bad"}))
            .is_valid());
    }

    #[test]
    fn dsl_nested_objects() {
        let v = schema! {
            object {
                required "user" => object {
                    required "name" => string(min_length = 1),
                    optional "age" => integer(min = 0),
                },
            }
        };
        assert!(v.validate(&json!({"user": {"name": "A"}})).is_valid());
        assert!(!v.validate(&json!({"user": {"name": ""}})).is_valid());
        assert!(
            !v.validate(&json!({"user": {}})).is_valid(),
            "nested required applies"
        );
    }

    #[test]
    fn dsl_root_scalar() {
        let v = schema! { integer(min = 1, max = 10) };
        assert!(v.validate(&json!(5)).is_valid());
        assert!(!v.validate(&json!(50)).is_valid());
    }

    #[test]
    fn dsl_and_json_schema_compile_to_identical_machines() {
        // Round-trip proof: the DSL AST flows through the exact same
        // AST → IR → optimizer → state machine pipeline as JSON Schema, so
        // equivalent schemas must lower to byte-identical machines.
        let from_json = Validator::new(
            r#"{
                "type": "object",
                "properties": {
                    "name": {"type": "string", "minLength": 1},
                    "tags": {"type": "array", "items": {"type": "string"}, "maxItems": 10}
                },
                "required": ["name"]
            }"#,
        )
        .unwrap();
        let from_dsl = schema! {
            object {
                required "name" => string(min_length = 1),
                optional "tags" => array(items = string(), max_items = 10),
            }
        };
        assert_eq!(
            format!("{:?}", from_json.root()),
            format!("{:?}", from_dsl.root()),
            "equivalent schemas must produce identical state machines"
        );
    }

    #[test]
    fn error_report_matches_spec_envelope() {
        let v = schema! { object { required "age" => integer(min = 0) } };
        let report = v.validate(&json!({}));
        let rendered = report.to_json_string();
        assert!(rendered.contains(r#""path":"$.age""#));
        assert!(rendered.contains(r#""expected":"required""#));
    }
}
