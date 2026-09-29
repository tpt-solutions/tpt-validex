# tpt-valid-schema

[![Crates.io](https://img.shields.io/crates/v/tpt-valid-schema.svg)](https://crates.io/crates/tpt-valid-schema)
[![Docs.rs](https://img.shields.io/docsrs/tpt-valid-schema.svg)](https://docs.rs/tpt-valid-schema)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

**JSON Schema front-end: tokenizer → AST → IR → optimized IR → validation state machine, with a schema cache.**

`tpt-valid-schema` turns JSON Schema (a documented
[Draft 2020-12 subset](../docs/compliance.md)) into the `ValidationNode` state
machines that [`tpt-valid-core`](../tpt-valid-core) executes. It is the crate
that makes `schema!`-written and JSON-Schema-written schemas behave
identically — the DSL produces the same AST type and flows through this exact
pipeline.

```
schema text ──▶ tokenizer ──▶ JSON reader ──▶ AST ──▶ semantic checks
           ──▶ IR ──▶ IR optimizer ──▶ ValidationNode state machine ──▶ cache
```

The tokenizer and JSON reader for schema documents are **custom, with zero
external dependencies**, so syntax errors carry precise line/column positions.

## Install

```sh
cargo add tpt-valid-schema
```

| Requirement | Value |
| :--- | :--- |
| Rust edition | 2021 |
| Minimum Rust | 1.75 |
| License | MIT OR Apache-2.0 |
| `unsafe` code | Forbidden (`#![forbid(unsafe_code)]`) |

## Quick start

```rust
use tpt_valid_schema::Validator;
use serde_json::json;

let v = Validator::new(r#"{
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age":  {"type": "integer", "minimum": 0, "maximum": 150}
    },
    "required": ["name", "age"]
}"#).unwrap();

assert!(v.validate(&json!({"name": "Alice", "age": 30})).is_valid());

let report = v.validate(&json!({"name": "", "age": 200}));
assert_eq!(report.errors.len(), 2);
assert!(report.errors.iter().any(|e| e.path == "$.age"));
```

Compile once, then validate millions of records. `Validator` is `Clone` and
cheap to share across threads (it holds an `Arc<ValidationNode>`).

## Compilation pipeline

| Stage | Module | What happens |
| :--- | :--- | :--- |
| Tokenize | `tokenizer` | Custom JSON tokenizer producing positioned `Token`s; errors carry line/column. |
| Read | `json` | Recursive-descent JSON reader over the tokens → `serde_json::Value`. |
| Extract | `ast` | `parse_schema` → `SchemaAst` (`Always` / `Never` / `Object`), rejecting unsupported keywords. |
| Semantic checks | `ast` | Cross-keyword invariants (`minimum` ≤ `maximum`, valid types, non-negative counts). |
| Lower to IR | `ir`, `compiler::build_ir` | Flat per-node conjunction of `IrOp`s; `allOf` folded in with object-keyword merging. |
| Optimize | `ir` | Prune checks that cannot apply to the node's type set; merge `minimum`/`exclusiveMinimum` into one bound. |
| Lower to machine | `compiler::lower` | Emit the `ValidationNode` state machine. |
| Cache | `cache` | Reuse compiled machines across calls via `SchemaCache` / `Validator::cached`. |

Public compiler entry points: `compile(&SchemaAst) -> (ValidationNode, Vec<Warning>)`,
`build_ir`, and `lower`.


## `Validator` API

| Method | Returns | Notes |
| :--- | :--- | :--- |
| `new(&str)` | `Result<Validator, SchemaError>` | Compile from schema text. |
| `from_value(&Value)` | `Result<Validator, SchemaError>` | Compile from an already-parsed schema. |
| `cached(&str)` | `Result<Arc<Validator>, SchemaError>` | Reuse the process-wide cache. |
| `from_ast(&SchemaAst)` | `Result<Validator, SchemaError>` | Compile a hand-built or DSL-produced AST. |
| `from_node(ValidationNode)` | `Validator` | Wrap a pre-built machine, zero compilation. |
| `validate(&Value)` | `ValidationReport` | Collects all errors. |
| `validate_with(&Value, &ValidationOptions)` | `ValidationReport` | Fail-fast / caps. |
| `validate_json(&str)` | `Result<ValidationReport, ParseError>` | Parse + validate. |
| `is_valid(&Value)` | `bool` | Fail-fast boolean. |
| `validate_batch(&[Value])` | `Vec<ValidationOutcome>` | Parallel via `rayon`. |
| `validate_csv(...)` | `(CsvStats, RowErrors)` | Streaming, in-memory errors. |
| `validate_csv_to(...)` | `CsvStats` | Valid rows out + JSONL quarantine. |
| `validate_jsonl(...)` / `validate_jsonl_to(...)` | `JsonlStats` | Streaming JSONL. |
| `root()` | `&ValidationNode` | The compiled machine. |
| `warnings()` | `&[Warning]` | Non-fatal compile warnings (e.g. unknown `format`). |

### Schema caching

`Validator::cached` uses a process-wide `SchemaCache` keyed by schema text. When
the capacity is exceeded the cache is cleared (simple amortized policy). You can
manage your own cache for finer control:

```rust
use tpt_valid_schema::SchemaCache;

let cache = SchemaCache::new(64);
let a = cache.get_or_compile(r#"{"type": "integer"}"#).unwrap();
let b = cache.get_or_compile(r#"{"type": "integer"}"#).unwrap();
assert!(std::sync::Arc::ptr_eq(&a, &b));   // same compiled machine
assert_eq!(cache.len(), 1);
```

## Errors and warnings

`SchemaError` distinguishes three failure classes:

| Variant | Meaning | Example |
| :--- | :--- | :--- |
| `Syntax(String)` | Not valid JSON, or not an object/boolean. | Trailing content after the top-level value. |
| `Semantic { keyword, message }` | A keyword's value breaks an invariant. | `minimum: 10` with `maximum: 5`. |
| `Unsupported { keyword, message }` | A keyword we deliberately don't support yet. | `$ref`, `$dynamicRef`, `prefixItems`, `dependsRequired`. |

Unknown keywords are **ignored** per Draft 2020-12 extensibility and surfaced as
non-fatal `Warning`s via `Validator::warnings()` — so typos in *supported*
keywords fail loudly, while genuinely unknown keywords don't break the build.

## Supported keywords

See the full [compliance matrix](../docs/compliance.md). In short: the core
validation keywords are supported (including `patternProperties`,
`additionalProperties` with exact unknown-key accounting, `contains`,
`uniqueItems`, and the `allOf` / `anyOf` / `oneOf` / `not` / `if-then-else`
applicators), with `items` limited to the single-schema form. `$ref` and tuple
validation are documented non-goals for 0.1.

## Interoperating with the DSL

Because the DSL produces the same `SchemaAst`, you can mix the two freely:

```rust
use tpt_valid_schema::Validator;

let ast = tpt_valid_schema::ast::parse_schema(&serde_json::json!({
    "type": "object",
    "properties": {"extra": {"type": "boolean"}}
})).unwrap();
let v = Validator::from_ast(&ast).unwrap();
assert!(v.validate(&serde_json::json!({"extra": true})).is_valid());
```

## Performance

| Benchmark | Result |
| :--- | ---: |
| Schema compilation (small) | < 1 µs |
| Schema compilation (9-keyword, with regexes) | ~3 ms |
| Cached schema lookup | ~560 ns |

Compilation happens once per schema; the machine is then shared read-only. See
[../docs/performance.md](../docs/performance.md).

## Testing

```sh
cargo test -p tpt-valid-schema
cargo clippy -p tpt-valid-schema --all-targets -- -D warnings
cargo fmt --check
cargo doc -p tpt-valid-schema --open
```

## Related crates

| Crate | Relationship |
| :--- | :--- |
| [`tpt-valid-core`](../tpt-valid-core) | Executes the state machines this crate produces. |
| [`tpt-valid-parser`](../tpt-valid-parser) | JSON parsing used for data (schema documents use the built-in reader). |
| [`validex-macros`](../validex-macros) | The `schema!` DSL, whose AST flows through this compiler. |
| [`validex`](../validex) | Facade re-exporting this crate — **start here** for most users. |

## Documentation

- [Rust API reference](../docs/api-rust.md)
- [DSL reference](../docs/dsl.md)
- [Compliance matrix](../docs/compliance.md)
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option. Dependencies are `tpt-valid-core`, `tpt-valid-parser`, `serde_json`,
and `regex` — all MIT-only or MIT/Apache-2.0 dual-licensed.

## Modules

| Module | Contents |
| :--- | :--- |
| `ast` | `SchemaAst`, `ObjectAst`, `AdditionalAst`, `parse_schema`, `REJECTED_KEYWORDS`. |
| `tokenizer` | `tokenize`, `Token`, `TokenKind`, `TokenError`. |
| `json` | `from_str` — the schema-document JSON reader. |
| `ir` | `IrSchema`, `IrOp`, `IrAdditional`, `IrConstraint` — the optimizer's view. |
| `compiler` | `compile`, `build_ir`, `lower`. |
| `cache` | `SchemaCache`, `global_cache`, `Validator::cached`. |
| `validator` | `Validator` — the compiled-schema façade. |
| `error` | `SchemaError` — `Syntax`, `Semantic`, `Unsupported`. |
