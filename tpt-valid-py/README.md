# tpt-valid-py

[![PyPI](https://img.shields.io/pypi/v/tpt-validex.svg)](https://pypi.org/project/tpt-validex/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

**Python bindings for tpt-validex, built with PyO3.**

This crate builds the native extension module published to PyPI as
`tpt-validex`. It exposes the same engine, the same JSON Schema, and the same
error format as every other tpt-validex binding — one schema, every language.

Measured at the Python boundary, this is **7–16× faster than `jsonschema`** and
within ~1.5× of `pydantic`'s compiled models, while giving you real JSON Schema
semantics and structured error paths. See
[../docs/performance.md](../docs/performance.md).

## Install

```sh
pip install tpt-validex
```

Wheels are published for Linux (x86_64, aarch64), macOS (x86_64, arm64), and
Windows x86_64. Requires Python ≥ 3.8. Fully typed (`py.typed`).

## Usage

```python
from tpt_validex import Validator

validator = Validator({
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age":  {"type": "integer", "minimum": 0, "maximum": 150},
        "email": {"type": "string", "format": "email"},
    },
    "required": ["name", "age"],
})

is_valid, errors = validator.validate({"name": "Alice", "age": 30,
                                       "email": "alice@example.com"})
assert is_valid

_, errors = validator.validate({"name": "", "age": 200})
for e in errors:
    print(e["path"], e["message"])     # $.age  Expected value <= 150, got 200
```

## API

| Member | Returns | Notes |
| :--- | :--- | :--- |
| `Validator(schema)` | `Validator` | Compile from a dict or a JSON string. |
| `Validator.from_json(text)` | `Validator` | Compile from schema JSON text explicitly. |
| `validate(data)` | `(is_valid: bool, errors: list[dict])` | Collects **all** errors. |
| `is_valid(data)` | `bool` | Fail-fast boolean only — fastest when you don't need errors. |
| `validate_batch(batch)` | `list[BatchResult]` | **Parallel** (rayon); one result per item, in input order. |
| `validate_csv(input_path, valid_output=None, errors_output=None, *, delimiter=",", has_headers=True)` | `stats: dict` | Streaming CSV with bounded memory. |
| `validate_jsonl(input_path, errors_output=None)` | `stats: dict` | Streaming JSONL. |
| `warnings()` | `list[str]` | Non-fatal schema-compile warnings (e.g. unknown formats). |

A `Validator` is thread-safe — compile once at import time and share it.

`BatchResult` exposes `.is_valid`, `.index`, and `.errors()`.

CSV stats dicts contain `total_rows`, `valid_rows`, `invalid_rows`, and
`parse_errors`; JSONL stats dicts contain `total_lines`, `valid_lines`,
`invalid_lines`, and `parse_errors`.

### Errors

`validate` returns errors as dicts with keys `path`, `message`, `expected`,
`actual`, and `value` (spec §5.5):

```python
{
    "path": "$.age",
    "message": "Expected value <= 150, got 200",
    "expected": "maximum 150",
    "actual": "200",
    "value": 200,
}
```

### Batch and streaming

```python
# Parallel batch
results = validator.validate_batch(list_of_records)
for r in results:
    if not r.is_valid:
        print(r.index, r.errors())

# Streaming CSV: valid rows out, invalid rows quarantined as JSONL
stats = validator.validate_csv(
    "input.csv",
    valid_output="valid.csv",
    errors_output="errors.jsonl",
)
print(f"{stats['valid_rows']}/{stats['total_rows']} rows valid")
```

## Performance note

dict/list inputs take a fast path through Python's C `json.dumps` plus the Rust
parser, which is what makes this binding competitive. For bulk data, prefer the
streaming APIs (`validate_csv`, `validate_jsonl`) — they stay in Rust and avoid
per-object boundary crossings entirely.

## Development

Build the extension in place with [maturin](https://github.com/PyO3/maturin):

```sh
cd tpt-valid-py
maturin develop --release      # or: pip install -e .
pytest                         # 17 tests
```

`pyproject.toml` declares the `extension-module` feature for wheel builds. The
feature is **off by default** so `cargo test` links against libpython and can
run in-process tests.

Key files:

| Path | Purpose |
| :--- | :--- |
| `src/lib.rs` | The PyO3 bindings (`tpt_validex.Validator`). |
| `pyproject.toml` | maturin build config, metadata, `py.typed`. |
| `tpt_validex.pyi` | Type stubs shipped in the sdist and wheel. |
| `tests/test_validator.py` | pytest suite. |

## Related crates

| Crate | Relationship |
| :--- | :--- |
| [`tpt-valid-core`](../tpt-valid-core) | The engine behind these bindings. |
| [`tpt-valid-schema`](../tpt-valid-schema) | Schema compilation used by `Validator`. |
| [`tpt-valid-parser`](../tpt-valid-parser) | JSON parsing on the Python boundary. |

## Documentation

- [Python API reference](../docs/api-python.md) — incl. FastAPI and ETL examples
- [Migration guide](../docs/migration.md) — from `pydantic` / `jsonschema`
- [Performance](../docs/performance.md)
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option.
