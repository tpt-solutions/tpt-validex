# Rust API reference

The `validex` crate is the user-facing facade; the underlying crates are
usable individually.

## Crate layout

| Crate | Purpose |
| :--- | :--- |
| [`validex`](https://crates.io/crates/validex) | Facade: `Validator`, `schema!`, re-exports. **Start here.** |
| `tpt-valid-schema` | JSON Schema tokenizer → AST → IR → state-machine compiler + cache |
| `tpt-valid-core` | Validation state machine, formats, structured errors, batch, CSV/JSONL streaming |
| `tpt-valid-parser` | JSON parsing (jiter primary, serde_json baseline) |
| `tpt-valid-ffi` | C ABI (`cdylib` + `staticlib`) |
| `validex-macros` | The `schema!` proc macro |

## `validex::Validator`

```rust
use validex::{Validator, schema, ValidationOptions};
use serde_json::json;

// From JSON Schema text (or a parsed Value with Validator::from_value)
let v = Validator::new(r#"{"type": "integer", "minimum": 0}"#)?;
let v = Validator::cached(schema_text)?;          // process-wide cache (Arc<Validator>)
let v = Validator::from_value(&json!({"type": "integer"}))?;
let v = schema! { integer(min = 0) };             // DSL — see docs/dsl.md
let v = Validator::from_node(node);               // wrap a pre-built state machine
```

### Methods

| Method | Returns | Notes |
| :--- | :--- | :--- |
| `validate(&Value)` | `ValidationReport` | collects **all** errors |
| `validate_with(&Value, &ValidationOptions)` | `ValidationReport` | fail-fast / error caps |
| `is_valid(&Value)` | `bool` | fail-fast traversal, no error allocation |
| `validate_json(&str)` | `Result<ValidationReport, ParseError>` | parse + validate |
| `validate_batch(&[Value])` | `Vec<ValidationOutcome>` | parallel via rayon |
| `validate_csv(reader, &CsvDialect, &ValidationOptions)` | `(CsvStats, RowErrors)` | streaming, type inference |
| `validate_csv_to(reader, valid_out, errors_out, ...)` | `CsvStats` | writes valid rows + JSONL errors |
| `validate_jsonl(reader, &ValidationOptions)` / `validate_jsonl_to` | `JsonlStats` | streaming |
| `root()` | `&ValidationNode` | the compiled state machine |
| `warnings()` | `&[String]` | compile-time warnings |

`ValidationReport` serializes to the spec §5.5 envelope via
`.to_json_string()` → `{"errors":[{"path","message","expected","actual","value"}]}`.

## The DSL

```rust
let v = validex::schema! {
    object {
        required "name" => string(min_length = 1, max_length = 100),
        optional "tags" => array(items = string(), max_items = 10),
    }
};
```

Compile-time syntax + semantic checking; see [docs/dsl.md](dsl.md).

## Batch and streaming

```rust
// Parallel batch
let outcomes = v.validate_batch(&values);
assert_eq!(outcomes[0].index, 0);

// Streaming CSV (bounded memory; per-column type inference)
let stats = v.validate_csv(File::open("input.csv")?, &Default::default(), &Default::default())?;
println!("{}/{} rows valid", stats.0.valid_rows, stats.0.total_rows);

// Writing quarantined errors while streaming
let mut errors_jsonl = File::create("errors.jsonl")?;
let stats = v.validate_jsonl_to(File::open("data.jsonl")?, &mut errors_jsonl, &Default::default())?;
```

## Lower-level crates

* `tpt_valid_core::{ValidationNode, DataType, ValidationOptions, ValidationError}` —
  build state machines by hand; `tpt_valid_core::validate(&node, &value, &opts)`.
* `tpt_valid_schema::{SchemaCache, compile}` — compile ASTs yourself,
  share a cache; `Validator::cached` uses `SchemaCache::global_cache()`.
* `tpt_valid_parser::parse` — the jiter/serde_json JSON front-end.

Rustdoc for all public APIs: `cargo doc --workspace --open`.
