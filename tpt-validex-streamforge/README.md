# tpt-validex-streamforge

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

**The `validate` stage for [tpt-streamforge](https://github.com/tpt-solutions/tpt-streamforge) data pipelines.**

Drop a validation stage into any streamforge pipeline: every row is checked
against a compiled tpt-validex schema, valid rows flow downstream, and invalid
rows are handled per the configured error mode and optionally quarantined to an
`errors.jsonl` file with line numbers.

```
read_csv → validate → write_csv
                 ↘ errors.jsonl
```

## Install

```toml
# Cargo.toml
[dependencies]
tpt-validex-streamforge = { path = "tpt-validex-streamforge", version = "0.1.0" }
```

```sh
cargo add tpt-validex-streamforge
```

## Usage

```rust
use tpt_stream_core::Pipeline;
use tpt_valid_schema::Validator;
use tpt_validex_streamforge::{ErrorMode, PipelineExt, ValidateConfig};

let schema = Validator::new(r#"{
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age":  {"type": "integer", "minimum": 0}
    },
    "required": ["name", "age"]
}"#).map_err(|e| tpt_stream_core::Error::Config(e.to_string()))?;

let mut pipeline = Pipeline::new();
pipeline
    .read_csv("input.csv")
    .validate_with(
        &schema,
        ValidateConfig::new()
            .on_invalid(ErrorMode::Skip)
            .errors_to("errors.jsonl"),
    )
    .write_csv("valid_output.csv");
let stats = pipeline.execute().await?;
```

The `PipelineExt` extension trait adds two methods to any streamforge
`Pipeline`:

| Method | Behavior |
| :--- | :--- |
| `validate(&Validator)` | Add the stage with the default configuration (`ErrorMode::Skip`). |
| `validate_with(&Validator, ValidateConfig)` | Add the stage with explicit configuration. |

You can also construct `Validate::new` / `Validate::with_config` directly and
insert it with `Pipeline::stage`.

## Error handling modes

| Mode | Invalid rows | Errors file |
| :--- | :--- | :--- |
| `ErrorMode::Skip` *(default)* | Dropped, counted in stats | Written if configured |
| `ErrorMode::Log` | Dropped + one stderr line each (capped via `max_log_lines`, default 100) | Written if configured |
| `ErrorMode::Abort` | Pipeline fails immediately with `Error::DataQuality`, naming the row number and offending path | — |

Quarantined rows are JSONL objects. `line` is the 1-based **data-row** number
(header excluded), stable across streamed chunks:

```json
{"line": 2, "row": {"name": null, "age": 200}, "errors": [
  {"path": "$.name", "message": "Missing required property \"name\"", "expected": "required", "actual": "missing"},
  {"path": "$.age",  "message": "Expected value <= 150, got 200", "expected": "maximum 150", "actual": "200", "value": 200}
]}
```

## `ValidateConfig`

Built with a fluent builder:

| Method | Default | Description |
| :--- | :--- | :--- |
| `ValidateConfig::new()` | — | `Skip` mode, no errors file. |
| `.on_invalid(ErrorMode)` | `Skip` | Choose `Skip`, `Log`, or `Abort`. |
| `.errors_to(path)` | none | Quarantine every invalid row to a JSONL file. |
| `.max_log_lines(n)` | 100 | Cap stderr logging in `Log` mode; extra rows are still counted. |

## Stats

`Validate::stats()` returns a `ValidateStats` with `rows_in`, `valid`, and
`invalid` counters, plus `all_valid()`. The pipeline's own `PipelineStats`
counts source rows.

## Row mapping

Streamforge's typed columns map to JSON before validation:

| streamforge `Value` | JSON |
| :--- | :--- |
| `Int32` / `Int64` | number |
| `Float32` / `Float64` | number |
| `Utf8` | string |
| `Bool` | boolean |
| `Date(days)` | `"YYYY-MM-DD"` string (so `format: "date"` applies) |
| `Timestamp(micros)` | canonical timestamp string |
| `Null` | `null` |

The `value_to_json` function is public if you need the same mapping elsewhere.

Note on CSV sources: streamforge's CSV reader infers **column types** (a column
that is entirely numeric becomes `Int64`, etc.). Rows whose raw text cannot fit
a column's inferred type are a *source*-level concern — configure streamforge's
`ErrorPolicy` for those. This stage handles schema-level validation of the
typed values.

## Testing

```sh
cargo test -p tpt-validex-streamforge
```

Seven end-to-end pipeline tests: CSV → validate → valid.csv + errors.jsonl
(with line numbers and paths), abort mode failing on `$.age`, skip and log
modes, all-valid pass-through, date-column mapping, and DSL/AST-built
validators.

## Related crates

| Crate | Relationship |
| :--- | :--- |
| [`tpt-valid-core`](../tpt-valid-core) | The validation engine. |
| [`tpt-valid-schema`](../tpt-valid-schema) | Provides the `Validator` compiled from a JSON Schema. |
| [`tpt-streamforge`](https://github.com/tpt-solutions/tpt-streamforge) | The pipeline framework this stage plugs into. |

## Documentation

- [Streamforge API reference](../docs/api-streamforge.md)
- [Compliance matrix](../docs/compliance.md)
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option.
