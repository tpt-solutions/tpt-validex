# Changelog

All notable changes to `tpt-valid-py` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace; the
published Python distribution is `tpt-validex` on
[PyPI](https://pypi.org/project/tpt-validex/).

## [Unreleased]

### Added
- `Validator(schema, formats={...})` — Python callables as custom format
assertions.
- `repair_prompt(schema, data, errors)` — LLM repair-prompt helper.


## [0.1.0] - 2026-09-29

Initial release of the Python bindings.

### Added

- **`tpt_validex.Validator`**, a PyO3 class wrapping the compiled engine:
  - `Validator(schema)` compiling from a dict or JSON string, plus the explicit
    `Validator.from_json(text)` alternative.
  - `validate(data)` returning `(is_valid, errors)` with **all** errors
    collected, each a dict with `path`, `message`, `expected`, `actual`, and
    `value` (spec §5.5).
  - `is_valid(data)` for a fail-fast boolean check with no error
    materialization.
  - `validate_batch(batch)` validating a list **in parallel** via `rayon`,
    returning `BatchResult` objects (`.is_valid`, `.index`, `.errors()`) in
    input order.
  - `validate_csv(input_path, valid_output=None, errors_output=None, *,
    delimiter=",", has_headers=True)` — streaming CSV with bounded memory,
    writing valid rows out and quarantining invalid rows as JSONL
    (`{"line": N, "row": ..., "errors": [...]}`).
  - `validate_jsonl(input_path, errors_output=None)` — streaming JSONL.
  - `warnings()` — non-fatal schema-compile warnings.
- **Conversion fast paths:** dict/list/str inputs go through Python's C-implemented
  `json.dumps` plus the Rust parser, which is what makes this binding 7–16×
  faster than `jsonschema` and within ~1.5× of `pydantic`. Batch mode dumps the
  whole batch once and parses it in a single pass. Non-JSON-native objects
  (custom classes) fall back to a direct recursive conversion.
- **Packaging** via maturin (`pyproject.toml`): the `extension-module` feature is
  enabled for wheel builds and left off by default so `cargo test` links against
  libpython for in-process testing.
- **Typing:** `tpt_validex.pyi` stubs and `py.typed` shipped in both the sdist
  and the wheel.
- **Platform wheels** for Linux (x86_64, aarch64), macOS (x86_64, arm64), and
  Windows x86_64; requires Python ≥ 3.8.
- A pytest suite (`tests/test_validator.py`, 17 tests) covering the Python API
  and the spec §5.5 error format.

### Notes

- The Python `Validator` is thread-safe; compile it at import time and share it.
- At the WASM/FFI boundary the dict→native conversion dominates (~µs/object);
  the streaming APIs avoid per-object crossings and are the highest-throughput
  option for bulk data.
- Measured numbers and the honest comparison against `pydantic` / `jsonschema`
  are in `docs/performance.md`.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
