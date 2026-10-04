<div align="center">

# tpt-validex

**Validate millions of records per second. One schema, every language. Zero Apache-2.0-only dependencies.**

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE-APACHE)
[![Crates.io](https://img.shields.io/crates/v/validex.svg)](https://crates.io/crates/validex)
[![PyPI](https://img.shields.io/pypi/v/tpt-validex.svg)](https://pypi.org/project/tpt-validex/)
[![npm](https://img.shields.io/npm/v/tpt-validex.svg)](https://www.npmjs.com/package/tpt-validex)
[![Go Reference](https://img.shields.io/badge/go.dev-reference-007d9c)](https://pkg.go.dev/github.com/tpt-solutions/tpt-validex-go)

</div>

`tpt-validex` is a blazing-fast, universal, embeddable data validation engine
written in pure Rust, with native bindings for **Python**, **JavaScript
(WASM)**, **Go**, **C/C++**, and Rust.

- ⚡ **Fast** — schemas compile to validation state machines; ~1.6–6.6M objects/s on the Rust core, parallel batch via rayon.
- 🌍 **Universal** — one JSON Schema, identical behavior across Python, JavaScript, Go, C++, and Rust.
- 📜 **JSON Schema (Draft 2020-12 subset)** — documented [compliance matrix](docs/compliance.md), plus an ergonomic compile-time-checked [Rust DSL](docs/dsl.md).
- 🌊 **Streaming** — validate CSV and JSONL with bounded memory; quarantined rows written to an errors file with line numbers.
- 🛠️ **A real CLI** — `validex check/infer/diff/watch/profile` with text, JSON, JUnit and SARIF output ([docs/cli.md](docs/cli.md)).
- 🧭 **Friendly errors** — "did you mean" hints for typos, top failure-pattern clustering for large files, opt-in coercion with a change report, and an LLM repair-prompt helper.
- 🪶 **Embeddable** — 1.36 MB WASM, ~1–2 MB native cdylib.
- ⚖️ **Clean licensing** — dual-licensed MIT / Apache-2.0; **no copyleft and no Apache-2.0-only dependencies**, enforced by `cargo deny` + a dedicated audit on every PR.

## Install

```sh
cargo add validex                 # Rust
pip install tpt-validex           # Python
npm install tpt-validex           # JavaScript / WASM
go get github.com/tpt-solutions/tpt-validex-go   # Go (cgo — see below)
cargo install tpt-valid-cli       # the `validex` CLI
```

## Quick start

### CLI (60 seconds)

```sh
validex check user.schema.json users.jsonl        # structured errors, exit 1 on failure
validex check user.schema.json table.csv --format sarif   # GitHub PR annotations
validex infer sample.csv > schema.json            # starter schema from sample data
validex diff v1.schema.json v2.schema.json        # breaking-change report (CI gate)
validex profile table.csv                         # null rates, distinct counts, hints
```

See [docs/cli.md](docs/cli.md) for the full surface (watch mode, error
sinks, JUnit output) and [examples/](examples) for runnable projects.

Schema used in every example:

```json
{
  "type": "object",
  "properties": {
    "name": {"type": "string", "minLength": 1},
    "age":  {"type": "integer", "minimum": 0, "maximum": 150},
    "email": {"type": "string", "format": "email"}
  },
  "required": ["name", "age"]
}
```

### Python

```python
from tpt_validex import Validator

validator = Validator({
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age": {"type": "integer", "minimum": 0, "maximum": 150},
        "email": {"type": "string", "format": "email"},
    },
    "required": ["name", "age"],
})

is_valid, errors = validator.validate({"name": "Alice", "age": 30, "email": "alice@example.com"})
assert is_valid

_, errors = validator.validate({"name": "", "age": 200})
for e in errors:
    print(e["path"], e["message"])     # $.age  Expected value <= 150, got 200

results = validator.validate_batch(list_of_records)      # parallel
validator.validate_csv("input.csv", valid_output="valid.csv",
                       errors_output="errors.jsonl")      # streaming
```

Full API + FastAPI/ETL examples: [docs/api-python.md](docs/api-python.md).

### JavaScript / TypeScript (WASM)

```js
import init, { Validator } from 'tpt-validex';
await init();

const validator = new Validator(schema);
const { isValid, errors } = validator.validate(data);   // plain objects in/out
const results = validator.validateBatch(batch);
```

Full API + browser form validation: [docs/api-javascript.md](docs/api-javascript.md).

### Rust (JSON Schema or the `schema!` DSL)

```rust
use validex::{Validator, schema};
use serde_json::json;

let validator = Validator::new(SCHEMA_TEXT)?;     // or Validator::cached(text)

let report = validator.validate(&json!({"name": "Alice", "age": 30}));
assert!(report.is_valid());

// The DSL: compile-time-checked, same engine as JSON Schema
let v = schema! {
    object {
        required "name" => string(min_length = 1, max_length = 100),
        required "age"  => integer(min = 0, max = 150),
        optional "email" => string(format = "email"),
    }
};
```

Full API: [docs/api-rust.md](docs/api-rust.md) · [docs/dsl.md](docs/dsl.md).

### Streamforge pipelines (Rust ETL)

```rust
use tpt_validex_streamforge::{ErrorMode, PipelineExt, ValidateConfig};

let mut pipeline = tpt_stream_core::Pipeline::new();
pipeline
    .read_csv("input.csv")
    .validate_with(
        &validator,
        ValidateConfig::new()
            .on_invalid(ErrorMode::Skip)
            .errors_to("errors.jsonl"),
    )
    .write_csv("valid_output.csv");
pipeline.execute().await?;
```

Full API: [docs/api-streamforge.md](docs/api-streamforge.md).

### Go

```go
validator, err := validex.NewValidator(schema)
if err != nil {
    return err
}
defer validator.Close()

valid, errs, err := validator.Validate(map[string]any{"name": "Alice", "age": 30})
results, err := validator.ValidateBatch(batch)   // parallel
```

Requires the native library — build and flag setup:
[docs/api-go.md](docs/api-go.md) / [tpt-valid-go/README.md](tpt-valid-go/README.md).

### C / C++

```c
#include "tpt_validex.h"

tpt_valid_handle* v = tpt_valid_create(schema_json);
tpt_valid_result* r = tpt_valid_validate(v, data_json);
if (!tpt_valid_is_valid(r)) printf("%s\n", tpt_valid_get_errors(r));
tpt_valid_free_result(r);
tpt_valid_destroy(v);
```

Full API: [docs/api-c.md](docs/api-c.md) · runnable example in
[tests/c_example/](tests/c_example/).

## Error format (spec §5.5)

All errors are collected (fail-fast optional), each with a JSON path:

```json
{
  "errors": [
    {
      "path": "$.age",
      "message": "Expected integer, got string",
      "expected": "integer",
      "actual": "string",
      "value": "25"
    }
  ]
}
```

## Performance

Measured, reproducible numbers (and an honest accounting of which spec
targets we hit): [docs/performance.md](docs/performance.md).

| Benchmark | Result |
| :--- | ---: |
| Single-object validation (Rust core) | 1.6–6.6M objects/s |
| Parallel batch (rayon) | ~3.5M objects/s |
| Streaming CSV / JSONL | ~1.15M rows/s · ~1.75M lines/s |
| Schema compilation | < 1 µs (small) · ~3 ms (with regexes) |
| WASM binary | 1.36 MB |
| vs `jsonschema` (Python) | **7–16× faster** |
| vs `pydantic` / `zod` | within ~1.5–2.7× at their language boundaries; core is an order of magnitude faster |

## Licensing & dependency policy

- `tpt-validex` is dual-licensed under [MIT](LICENSE-MIT) or
  [Apache-2.0](LICENSE-APACHE), at your option.
- Dependencies must be MIT-only or MIT/Apache-2.0 dual-licensed. Copyleft
  licenses are rejected by `deny.toml`; Apache-2.0-only crates are rejected
  by `scripts/audit_apache_only.py` (this audit is why we ship
  [jiter](https://crates.io/crates/jiter) instead of the sonic-rs the
  original spec assumed was MIT).

## Repository layout

Every crate has its own README, CHANGELOG, and crates.io keywords/categories.

| Crate | Purpose |
| :--- | :--- |
| [`validex`](validex/) | **Start here.** Rust facade: `Validator` + `schema!` DSL + re-exports |
| [`tpt-valid-core`](tpt-valid-core/) | Validation state machine, formats, structured errors, batch, CSV/JSONL streaming |
| [`tpt-valid-schema`](tpt-valid-schema/) | JSON Schema tokenizer → AST → IR → compiler + cache |
| [`tpt-valid-parser`](tpt-valid-parser/) | JSON parsing (jiter primary, serde_json baseline) |
| [`tpt-valid-cli`](tpt-valid-cli/) | the `validex` command (check / infer / diff / watch / profile) |
| [`tpt-valid-ffi`](tpt-valid-ffi/) | C ABI + `tpt_validex.h` |
| [`tpt-valid-py`](tpt-valid-py/) | Python bindings (PyO3 + maturin) + tests |
| [`tpt-valid-wasm`](tpt-valid-wasm/) | WASM bindings (wasm-bindgen) + package.json + JS tests |
| [`tpt-valid-go`](tpt-valid-go/) | Go bindings (cgo) + tests |
| [`tpt-validex-streamforge`](tpt-validex-streamforge/) | `validate` stage for tpt-streamforge pipelines |
| [`validex-macros`](validex-macros/) | `schema!` proc macro |
| [`benches`](benches/) | Criterion benchmarks + language comparison scripts |

```text
validex/                Rust facade (Validator + schema! DSL)
validex-macros/         schema! proc macro
tpt-valid-core/         validation state machine, formats, streaming, batch
tpt-valid-schema/       JSON Schema tokenizer → AST → IR → compiler + cache
tpt-valid-parser/       JSON parsing (jiter / serde_json)
tpt-valid-cli/          the validex command
tpt-valid-ffi/          C ABI + tpt_validex.h
tpt-valid-py/           Python bindings (PyO3 + maturin) + tests
tpt-valid-wasm/         WASM bindings (wasm-bindgen) + package.json + JS tests
tpt-valid-go/           Go bindings (cgo) + tests
tpt-validex-streamforge/ validate stage for tpt-streamforge pipelines
benches/                criterion benchmarks + language comparison scripts
docs/                   API references, compliance matrix, performance, migration
tests/c_example/        runnable C example
```

## Development

```sh
cargo test --workspace            # Rust: 127 tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo deny check licenses         # license audit
python scripts/audit_apache_only.py
cd tpt-valid-py && maturin develop --release && pytest      # Python: 17 tests
cd tpt-valid-wasm && npm test                               # JS: 8 tests
cd tpt-valid-go && go test ./...                            # Go (needs native lib)
```

CI runs all of the above across Linux/macOS/Windows plus WASM; releases
publish wheels to PyPI and the npm package on tags.

## Status

🚧 v0.1.0 — core engine, all language bindings, DSL, benchmarks, and CI are
implemented; `$ref` resolution and tuple validation are documented
non-goals for now. See [todo.md](todo.md) for the roadmap.
