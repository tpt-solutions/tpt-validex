# tpt-streamforge pipeline integration (`tpt-validex-streamforge`)

The `validate` transformation stage for [tpt-streamforge](https://github.com/tpt-solutions/tpt-streamforge)
data pipelines — spec §5.6 and §13 Use Case 2. Every row is checked against a
compiled `tpt-validex` schema; valid rows flow downstream, invalid rows are
handled per the configured mode and (optionally) quarantined to an
`errors.jsonl` file with 1-based data-row numbers.

```toml
# Cargo.toml
[dependencies]
tpt-validex-streamforge = { path = "tpt-validex-streamforge", version = "0.1.0" }
# or, once published alongside its git dependency:
# tpt-validex-streamforge = "0.1"
```

## Usage (spec §5.6)

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

The [`PipelineExt`] extension trait adds `.validate(&schema)` (default
configuration) and `.validate_with(&schema, config)` to any streamforge
`Pipeline`, so the stage reads exactly like the spec's
`read_csv → validate → write_csv` sketch.

## Error handling modes

| Mode | Invalid rows | Errors file |
| :--- | :--- | :--- |
| [`ErrorMode::Skip`] (default) | Dropped, counted in stats | Written if configured |
| [`ErrorMode::Log`] | Dropped + one stderr line each (capped via `max_log_lines`, default 100) | Written if configured |
| [`ErrorMode::Abort`] | Pipeline fails immediately with `Error::DataQuality`, naming the row number and offending path | — |

Quarantined rows are JSONL objects:

```json
{"line": 2, "row": {"name": null, "age": 200}, "errors": [
  {"path": "$.name", "message": "Missing required property \"name\"", "expected": "required", "actual": "missing"},
  {"path": "$.age", "message": "Expected value <= 150, got 200", "expected": "maximum 150", "actual": "200", "value": 200}
]}
```

`line` is the 1-based data-row number (header excluded), stable across
streamed chunks.

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

Note on CSV sources: streamforge's CSV reader infers **column types** (a
column that is entirely numeric becomes `Int64`, etc.). Rows whose raw text
cannot fit a column's inferred type are a *source*-level concern — configure
streamforge's `ErrorPolicy` for those; this stage handles schema-level
validation of the typed values.

## Stats

The stage tracks `rows_in` / `valid` / `invalid` counters
(`Validate::stats()`); the pipeline's own `PipelineStats` counts source rows.

## Tests

`cargo test -p tpt-validex-streamforge` — end-to-end pipeline tests:
CSV → validate → valid.csv + errors.jsonl (with line numbers and paths),
abort mode failing with `$.age`, skip/log modes, all-valid pass-through,
date-column mapping, and DSL/AST-built validators.
