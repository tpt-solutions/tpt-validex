# validex

[![Crates.io](https://img.shields.io/crates/v/validex.svg)](https://crates.io/crates/validex)
[![Docs.rs](https://img.shields.io/docsrs/validex.svg)](https://docs.rs/validex)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

**One schema, every language. Validate millions of records per second.**

`validex` is the user-facing Rust facade for the
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) engine. It re-exports
the whole engine in a single crate, so you get:

- **JSON Schema (Draft 2020-12 subset)** — a documented, tested
  [compliance matrix](../docs/compliance.md).
- **An ergonomic Rust DSL** — the [`schema!`](#the-dsl) macro, fully checked at
  **compile time**.
- **Identical behavior** from Python, JavaScript/WASM, Go, C/C++, and Rust.
- **Batch and streaming** — parallel batches via `rayon`, plus bounded-memory
  CSV and JSONL streaming with quarantine files.
- **Clean licensing** — dual-licensed MIT / Apache-2.0, with **zero
  Apache-2.0-only and zero copyleft dependencies**.

Schemas compile once to a validation state machine; validation is then O(1) per
field. This is the crate to reach for unless you need the lower-level building
blocks (see [Related crates](#related-crates)).

## Install

```sh
cargo add validex
```

| Requirement | Value |
| :--- | :--- |
| Rust edition | 2021 |
| Minimum Rust | 1.75 |
| License | MIT OR Apache-2.0 |

## Quick start

```rust
use validex::{Validator, schema};
use serde_json::json;

// From JSON Schema text
let validator = Validator::new(r#"{
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age":  {"type": "integer", "minimum": 0, "maximum": 150}
    },
    "required": ["name", "age"]
}"#).unwrap();

let report = validator.validate(&json!({"name": "Alice", "age": 30}));
assert!(report.is_valid());

// Errors carry a JSON path and are all collected, not just the first
let report = validator.validate(&json!({"name": "", "age": 200}));
assert_eq!(report.errors.len(), 2);
assert!(report.errors.iter().any(|e| e.path == "$.name"));
assert!(report.errors.iter().any(|e| e.path == "$.age"));
```

> **Note on error order.** Errors are collected in the order the compiled state
> machine visits fields, which is an optimizer decision — not the order keys
> appear in your document. Match on `path` rather than indexing when you care
> about which error came first.

## The DSL

The `schema!` macro declares schemas with compile-time syntax *and* semantic
checking. The DSL AST flows through the **same** AST → IR → optimizer →
state-machine pipeline as JSON Schema, so equivalent schemas produce
identical machines (asserted by a test in this crate).

```rust
use validex::{schema, json};

let v = schema! {
    object {
        required "name"  => string(min_length = 1, max_length = 100),
        required "age"   => integer(min = 0, max = 150),
        optional "email" => string(format = "email"),
        optional "tags"  => array(items = string(), min_items = 0, max_items = 10),
        optional "meta"  => object {
            optional "source" => string(enum = ["web", "api"]),
            optional "score"  => number(exclusive_min = 0.0, multiple_of = 0.5),
        },
    }
};

assert!(v.validate(&json!({"name": "Bob", "age": 25, "tags": ["a"]})).is_valid());
assert!(!v.validate(&json!({"name": "", "age": 999})).is_valid());
```

Mistakes fail the **build**, not the runtime: malformed syntax, unknown
options, options on types that don't take them, duplicate properties,
`min > max`, `multiple_of <= 0`, and unknown `format` values.

Full grammar and option tables: [../docs/dsl.md](../docs/dsl.md).

## `Validator` API

| Method | Returns | Notes |
| :--- | :--- | :--- |
| `Validator::new(&str)` | `Result<Validator, SchemaError>` | Compile JSON Schema text. |
| `Validator::from_json(&str)` *(alias)* | `Result<Validator, SchemaError>` | Explicitly named alias. |

## Batch and streaming

```rust
use validex::{schema, json};
use std::io::Cursor;

let v = schema! { object { required "id" => integer(min = 0) } };

// Parallel batch
let outcomes = v.validate_batch(&[json!({"id": 1}), json!({"id": -1})]);
assert!(outcomes[0].valid);
assert!(!outcomes[1].valid);
assert_eq!(outcomes[1].errors[0].path, "$.id");

// Streaming CSV: valid rows out, invalid rows quarantined as JSONL
let input = "id\n1\n-5\n";
let mut valid_out = Vec::new();
let mut errors_out = Vec::new();
let stats = v.validate_csv_to(
    Cursor::new(input),
    &mut valid_out,
    &mut errors_out,
    &Default::default(),
    &Default::default(),
).unwrap();
assert_eq!(stats.invalid_rows, 1);
let quarantined = String::from_utf8(errors_out).unwrap();
// `line` is the 1-based physical line in the input, header included.
assert!(quarantined.contains(r#""line":3"#));
assert!(quarantined.contains(r#""path":"$.id""#));
```

## Error format

All language bindings emit the same envelope (spec §5.5):

```json
{
  "errors": [
    {
      "path": "$.age",
      "message": "Expected value <= 150, got 200",
      "expected": "maximum 150",
      "actual": "200",
      "value": 200
    }
  ]
}
```

`ValidationReport::to_json_string()` produces exactly this; `value` is omitted
when the offending value is not JSON-representable.

**Error order.** Errors are collected in the order the compiled state machine
visits fields — an optimizer decision, not document key order. Match on `path`
rather than indexing when order matters.

**Streaming line numbers.** In `validate_csv_to` / `validate_jsonl_to` the
`line` field of a quarantined record is the **1-based physical line** in the
input, so a header row counts as line 1. (The streamforge integration in
[`tpt-validex-streamforge`](../tpt-validex-streamforge) instead reports
1-based *data-row* numbers, header excluded.)

## Performance

Measured on the reference machine (Windows 11 x86_64, release + LTO). See
[../docs/performance.md](../docs/performance.md) for methodology and honest
accounting of the targets we do not hit.

| Benchmark | Result |
| :--- | ---: |
| Single-object validation | 1.6–6.6M objects/s |
| Parallel batch (`rayon`) | ~3.5M objects/s |
| Streaming CSV / JSONL | ~1.15M rows/s · ~1.75M lines/s |
| Schema compilation | < 1 µs (small) · ~3 ms (with regexes) |
| Cached schema lookup | ~560 ns |
| Native binary | ~1–2 MB |

## Re-exports

This crate re-exports the engine, so a single dependency is usually enough:

- From `tpt_valid_core`: `ValidationNode`, `ValidationOptions`, `ValidationError`,
  `ValidationReport`, `ValidationOutcome`, `ErrorCollector`, `DataType`, `Format`,
  `EnumSet`, `AdditionalProperties`, `json_type_name`, `core_validate_batch`.
- From `tpt_valid_schema`: `Validator`, `SchemaCache`, `SchemaError`, `FlowError`,
  `ast`, `compile_schema_ast`.
- From `validex_macros`: the `schema!` macro and the `dsl` namespace.
- `serde_json::json` for convenience.

Lower-level pieces — `CsvDialect`, `RowOutcome`, `JsonlStats`,
`validate_jsonl_stream` — are reachable through the sibling crates directly.

## Testing

```sh
cargo test -p validex              # unit tests + doctests (incl. DSL round-trip)
cargo clippy -p validex --all-targets -- -D warnings
cargo fmt --check
cargo doc -p validex --open
```

## Related crates

| Crate | Purpose |
| :--- | :--- |
| [`validex-macros`](../validex-macros) | The `schema!` proc macro (this crate's dependency). |
| [`tpt-valid-schema`](../tpt-valid-schema) | JSON Schema tokenizer → AST → IR → compiler + cache. |
| [`tpt-valid-core`](../tpt-valid-core) | Validation state machine, formats, errors, batch, CSV/JSONL streaming. |
| [`tpt-valid-parser`](../tpt-valid-parser) | JSON parsing front-end (`jiter` primary, `serde_json` baseline). |
| [`tpt-valid-ffi`](../tpt-valid-ffi) | C ABI + `tpt_validex.h` (used by the Go bindings). |
| [`tpt-valid-py`](../tpt-valid-py) · [`tpt-valid-wasm`](../tpt-valid-wasm) | Python (PyO3) and JavaScript (wasm-bindgen) bindings. |

## Documentation

- [Rust API reference](../docs/api-rust.md)
- [DSL reference](../docs/dsl.md)
- [Compliance matrix](../docs/compliance.md)
- [Migration guide](../docs/migration.md) — from `pydantic`, `zod`, `jsonschema`
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option. Every runtime dependency is MIT-only or MIT/Apache-2.0
dual-licensed — no copyleft, no Apache-2.0-only crates. See the
[dependency policy](../README.md#licensing--dependency-policy).

| `Validator::from_value(&Value)` | `Result<Validator, SchemaError>` | Compile from a parsed schema. |
| `Validator::cached(&str)` | `Result<Arc<Validator>, SchemaError>` | Process-wide schema cache. |
| `Validator::from_node(ValidationNode)` | `Validator` | Wrap a hand-built machine. |
| `validate(&Value)` | `ValidationReport` | Collects **all** errors. |
| `validate_with(&Value, &ValidationOptions)` | `ValidationReport` | Fail-fast / error caps. |
| `is_valid(&Value)` | `bool` | Fail-fast, no error allocation. |
| `validate_json(&str)` | `Result<ValidationReport, ParseError>` | Parse + validate. |
| `validate_batch(&[Value])` | `Vec<ValidationOutcome>` | Parallel via `rayon`. |
| `validate_csv(...)` | `(CsvStats, RowErrors)` | Streaming, type inference. |
| `validate_csv_to(...)` | `CsvStats` | Valid rows out + JSONL errors. |
| `validate_jsonl(...)` / `validate_jsonl_to(...)` | `JsonlStats` | Streaming JSONL. |
| `root()` | `&ValidationNode` | The compiled state machine. |
| `warnings()` | `&[String]` | Non-fatal compile warnings. |
