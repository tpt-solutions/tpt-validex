# tpt-valid-core

[![Crates.io](https://img.shields.io/crates/v/tpt-valid-core.svg)](https://crates.io/crates/tpt-valid-core)
[![Docs.rs](https://img.shields.io/docsrs/tpt-valid-core.svg)](https://docs.rs/tpt-valid-core)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

**The core validation engine: state machines, format validators, structured errors, batch and streaming.**

`tpt-valid-core` is the low-level heart of
[tpt-validex](https://github.com/tpt-solutions/tpt-validex). It contains no schema
parsing — it consumes an already-compiled [`ValidationNode`] state machine and
executes it. Everything else in the workspace is built on this crate:

```
tpt-valid-parser   →  tpt-valid-core  →  tpt-valid-schema  →  validex (facade)
                          (this crate)         │                  ├─ tpt-valid-py
                                                └──────────────────┼─ tpt-valid-wasm
                                                                   └─ tpt-valid-ffi
```

**Most users should depend on [`validex`](../validex) or
[`tpt-valid-schema`](../tpt-valid-schema) instead.** Reach for this crate
directly when you want to build state machines by hand, implement a custom
validation backend, or reuse the streaming CSV/JSONL readers on their own.

## Install

```sh
cargo add tpt-valid-core
```

| Requirement | Value |
| :--- | :--- |
| Rust edition | 2021 |
| Minimum Rust | 1.75 |
| License | MIT OR Apache-2.0 |
| `unsafe` code | Forbidden (`#![forbid(unsafe_code)]`) |

## Design guarantees

- **Compile once, validate many.** Schemas become a tree of `ValidationNode`s;
  validation is O(1) work per field afterwards.
- **All errors are collected** by default, each with a JSON path. Fail-fast is
  opt-in via [`ValidationOptions::fail_fast`].
- **Regexes are pre-compiled** at schema-compile time; enum checks use hash sets
  for O(1) lookup.
- **No `unsafe`**, enforced crate-wide with `#![forbid(unsafe_code)]`.
- **`missing_docs` is a warning-as-error** in CI, so every public item is
  documented.

## Quick start

```rust
use tpt_valid_core::{validate, DataType, ValidationNode, ValidationOptions};
use serde_json::json;

// Build a state machine by hand
let node = ValidationNode::all(vec![
    ValidationNode::CheckType(DataType::Object),
    ValidationNode::CheckRequired(vec!["age".into()]),
    ValidationNode::CheckField(
        "age".into(),
        Box::new(ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Integer),
            ValidationNode::CheckMinimum(0.0),
            ValidationNode::CheckMaximum(150.0),
        ])),
    ),
]);

let errors = validate(&node, &json!({"age": 30}), &ValidationOptions::default());
assert!(errors.is_empty());

let errors = validate(&node, &json!({"age": 200}), &ValidationOptions::default());
assert_eq!(errors[0].path, "$.age");
assert_eq!(errors[0].expected, "maximum 150");
```

## Modules

| Module | Contents |
| :--- | :--- |
| `node` | `ValidationNode` state machine, `EnumSet`, `ObjectShape`, `AdditionalProperties`. |
| `engine` | Traversal: `validate`, `validate_value`, `ValidationOptions`. |
| `error` | `ValidationError`, `ValidationReport`, `ErrorCollector`. |
| `format` | `Format` — `email`, `uri`, `date`, `date-time`, `uuid`, `ipv4`, `ipv6`, `hostname`. |
| `types` | `DataType`, `is_integer_value`, `json_type_name`, `number_from_f64`. |
| `batch` | `validate_batch` — parallel validation via `rayon`. |
| `csv` | Streaming CSV parser + validator (`CsvReader`, `validate_csv_stream`). |
| `jsonl` | Streaming JSONL validator (`validate_jsonl_stream`). |

## The state machine

`ValidationNode` mirrors the JSON Schema keyword set. Type-specific keywords
only apply to instances of their type, per JSON Schema semantics — e.g.
`CheckMinimum` silently accepts a non-number, which a sibling `CheckType` flags.

| Variant | Keyword | Notes |
| :--- | :--- | :--- |
| `CheckType(DataType)` | `type` | `integer` matches whole floats (`1.0`). |
| `CheckField(String, Box<..>)` | `properties` | No-op when the key is absent. |
| `CheckRequired(Vec<String>)` | `required` | **All** missing keys reported, not just the first. |
| `CheckObject(Vec<..>)` | `properties` | Simple property map. |
| `CheckObjectEx(ObjectShape)` | `properties` + `patternProperties` + `additionalProperties` + `required` | Fused shape so unknown-key accounting is exact. |
| `CheckMinimum(f64)` / `CheckMaximum(f64)` | `minimum` / `maximum` | |
| `CheckExclusiveMinimum(f64)` / `CheckExclusiveMaximum(f64)` | `exclusiveMinimum` / `exclusiveMaximum` | Draft 2020-12 numeric form. |
| `CheckMultipleOf(f64)` | `multipleOf` | Exact integer arithmetic; epsilon-tolerant float path. |
| `CheckMinLength(usize)` / `CheckMaxLength(usize)` | `minLength` / `maxLength` | Counted in characters, not bytes. |
| `CheckPattern(Box<Regex>)` | `pattern` | Pre-compiled. |
| `CheckEnum(EnumSet)` | `enum` | Hash set, O(1); numeric normalization (`1` ≡ `1.0`). |
| `CheckConst(Value)` | `const` | Deep equality with numeric normalization. |
| `CheckFormat(Format)` | `format` | Built-in validators. |
| `CheckArray(Box<..>, usize, usize)` | `items` + `minItems` / `maxItems` | `usize::MAX` = unbounded. |
| `CheckUniqueItems(bool)` | `uniqueItems` | Numeric-equality aware. |
| `CheckContains(Box<..>)` | `contains` | At least one item must match. |
| `CheckIfThenElse(..)` | `if` / `then` / `else` | |
| `CheckAllOf` / `CheckAnyOf` / `CheckOneOf` / `CheckNot` | `allOf` / `anyOf` / `oneOf` / `not` | |
| `CheckAll(Vec<..>)` | *(compiler output)* | Optimized sequence of checks on one instance. |
| `Always` / `Never(String)` | `true` / `false` | Boolean schemas. |

Helpers: `ValidationNode::type_of(t)` and `ValidationNode::all(nodes)` — the
latter flattens nested sequences and drops no-op `Always` nodes.


## Error format

`ValidationError` is structured (spec §5.5); `ValidationReport` serializes to the
canonical envelope shared by every language binding.

```rust
use tpt_valid_core::ErrorCollector;

let mut collector = ErrorCollector::collect_all();
collector.push(
    tpt_valid_core::ValidationError::new(
        "$.age",
        "Expected value <= 150, got 200",
        "maximum 150",
        "200",
    ).with_value(serde_json::json!(200)),
);

let report = collector.into_report();
assert!(!report.is_valid());
assert!(report.to_json_string().contains(r#""path":"$.age""#));
```

`ErrorCollector` variants:

| Constructor | Behavior |
| :--- | :--- |
| `collect_all()` | Collect everything, capped at `DEFAULT_MAX_ERRORS` (1000). |
| `fail_fast()` | Stop after the first error. |
| `new(fail_fast, max_errors)` | Explicit control. |

`stopped()` tells the engine to prune the rest of the traversal; `into_report()`
finishes the run.

## Batch validation

`validate_batch` runs across all cores with `rayon`. The state machine is shared
read-only; only per-item collectors are thread-local. Results are returned in
input order, each tagged with its `index`.

```rust
use tpt_valid_core::{validate_batch, DataType, ValidationNode, ValidationOptions};

let node = ValidationNode::CheckType(DataType::Integer);
let batch = vec![serde_json::json!(1), serde_json::json!("x"), serde_json::json!(3)];
let outcomes = validate_batch(&node, &batch, &ValidationOptions::default());

assert_eq!(outcomes.len(), 3);
assert!(outcomes[0].valid);
assert!(!outcomes[1].valid);
assert_eq!(outcomes[1].index, 1);
```

## Streaming CSV

A hand-written, allocation-light CSV state machine — no `csv` crate dependency.
Supports quoted fields (with `""` escapes and embedded newlines), custom
delimiters, per-column type inference, and bounded memory: rows are never all
buffered, only the type-inference sample is.

```rust
use tpt_valid_core::{validate_csv_stream, CsvDialect, RowOutcome, ValidationNode, DataType};
use std::io::Cursor;

let node = ValidationNode::all(vec![
    ValidationNode::CheckType(DataType::Object),
    ValidationNode::CheckField("id".into(), Box::new(ValidationNode::all(vec![
        ValidationNode::CheckType(DataType::Integer),
        ValidationNode::CheckMinimum(0.0),
    ]))),
]);

let input = "id,name\n1,Alice\n-5,Bob\n";
let stats = validate_csv_stream(
    &node,
    Cursor::new(input),
    &CsvDialect::default(),
    &ValidationOptions::default(),
    |row| {
        if let RowOutcome::Invalid { line, errors, .. } = row {
            println!("line {line}: {} errors", errors.len());
        }
        true // return false to stop early
    },
).unwrap();

assert_eq!(stats.total_rows, 2);
assert_eq!(stats.invalid_rows, 1);
```

`CsvDialect` defaults to comma delimiter, `"` quote, header row present, and a
1000-row inference sample. Leniency policy: content after a closing quote is
accepted, invalid UTF-8 decodes lossily, blank lines are skipped.

Column types are inferred from the sample — a column is `Integer` only if every
non-empty sampled value parses as one, otherwise `Float`, `Boolean`, or
`String`. `infer_column_types` and `record_to_value` are exposed if you want to
control that yourself.

> **Empty cells become `null`, not a type error.** An empty CSV cell converts to
> JSON `null`, and `required` only asserts that a *key is present* — so a row
> with a blank `id` still passes a `required: ["id"]` check. To reject blanks,
> assert on the field's type (as above) or add `"type": ["integer", "null"]`
> handling explicitly in your schema.

## Streaming JSONL

Each line is parsed and validated independently; errors carry 1-based line
numbers and blank lines are skipped (but still counted for line numbering).

```rust
use tpt_valid_core::{validate_jsonl_stream, JsonlRowOutcome, ValidationNode, DataType};
use std::io::Cursor;

let node = ValidationNode::all(vec![
    ValidationNode::CheckType(DataType::Object),

## String formats

All formats are implemented from scratch in pure Rust — no external
format-validation dependencies. Date and date-time parsing use `chrono`
(MIT / Apache-2.0) for calendar correctness; everything else is hand-written.

| Format | Aliases | Implementation |
| :--- | :--- | :--- |
| `email` | — | Practical RFC 5322 subset: dot-atom local part, DNS-style domain, 254-char limit. |
| `uri` | `url`, `iri` | RFC 3986 absolute URI. |
| `date` | — | ISO 8601 full-date `YYYY-MM-DD`. |
| `date-time` | `datetime` | RFC 3339. |
| `uuid` | — | RFC 4122 textual, 8-4-4-4-12 hex, either case. |
| `ipv4` | — | Dotted quad; leading zeros rejected. |
| `ipv6` | — | RFC 4294, `::` compression, IPv4 tails; zone IDs rejected. |
| `hostname` | `idn-hostname` | RFC 1123; max 253 chars. |

Format assertions apply only to strings — `Format::Email.validate_value(&json!(42))`
is `true`, per Draft 2020-12.

## Performance

Measured on the reference machine (Windows 11 x86_64, release + LTO). Full
methodology and honest accounting of unmet targets in
[../docs/performance.md](../docs/performance.md).

| Benchmark | Result |
| :--- | ---: |
| Single-object validation | 1.6–6.6M objects/s |
| Parallel batch (`rayon`) | ~3.5M objects/s |
| Streaming CSV | ~1.15M rows/s |
| Streaming JSONL | ~1.75M lines/s |
| Fail-fast vs. collect-all | ~550 ns vs. ~1.8 µs on an invalid 4-keyword sample |

Collecting all errors costs roughly 4× a fail-fast check. Use
`ValidationOptions::fail_fast()` (or `Validator::is_valid`) on hot paths where
you only need the boolean.

## Testing

```sh
cargo test -p tpt-valid-core
cargo clippy -p tpt-valid-core --all-targets -- -D warnings
cargo fmt --check
cargo doc -p tpt-valid-core --open
```

## Related crates

| Crate | Relationship |
| :--- | :--- |
| [`tpt-valid-parser`](../tpt-valid-parser) | Provides the JSON parsing used by streaming validators. |
| [`tpt-valid-schema`](../tpt-valid-schema) | Compiles JSON Schema into the `ValidationNode` trees this crate executes. |
| [`validex`](../validex) | Facade re-exporting this crate — **start here** if you don't need the low-level API. |
| [`tpt-valid-ffi`](../tpt-valid-ffi) | C ABI over the schema + core crates. |
| [`tpt-valid-py`](../tpt-valid-py) · [`tpt-valid-wasm`](../tpt-valid-wasm) | Python and JavaScript bindings. |

## Documentation

- [Rust API reference](../docs/api-rust.md)
- [Compliance matrix](../docs/compliance.md) — which keyword maps to which node
- [Performance](../docs/performance.md)
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option. Direct dependencies — `serde`, `serde_json`, `regex`, `chrono`,
`rayon`, `tpt-valid-parser` — are all MIT-only or MIT/Apache-2.0 dual-licensed.

    ValidationNode::CheckField("age".into(), Box::new(ValidationNode::CheckType(DataType::Integer))),
]);

let stats = validate_jsonl_stream(
    &node,
    Cursor::new("{\"age\": 1}\n{\"age\": \"x\"}\nnot json\n"),
    &ValidationOptions::default(),
    |row| {
        match row {
            JsonlRowOutcome::Valid { line, .. } => println!("{line}: ok"),
            JsonlRowOutcome::Invalid { line, errors, .. } => println!("{line}: {}", errors[0].message),
            JsonlRowOutcome::ParseError { line, message } => println!("{line}: {message}"),
        }
        true
    },
).unwrap();

assert_eq!(stats.total_lines, 3);
assert_eq!(stats.valid_lines, 1);
assert_eq!(stats.invalid_lines, 1);
assert_eq!(stats.parse_errors, 1);
```
