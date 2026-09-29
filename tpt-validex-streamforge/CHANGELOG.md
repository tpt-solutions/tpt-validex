# Changelog

All notable changes to `tpt-validex-streamforge` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace.

## [Unreleased]

Nothing yet.

## [0.1.0] - 2026-09-29

Initial release of the streamforge pipeline integration.

### Added

- **`Validate` stage** — a `tpt_stream_core::pipeline::PipelineStage` that
  validates every row of a `RecordBatch` against a compiled `Validator`,
  drops invalid rows, and forwards the valid ones downstream.
- **`ErrorMode`** with three configurable behaviors (spec §7):
  - `Skip` *(default)* — drop invalid rows and continue.
  - `Log` — drop and log one summary line per bad row to stderr.
  - `Abort` — fail the pipeline immediately with `Error::DataQuality`, naming
    the row number and the offending JSON path.
- **`ValidateConfig`** fluent builder: `new`, `on_invalid(ErrorMode)`,
  `errors_to(path)` for JSONL quarantine, and `max_log_lines(n)` (default 100)
  to cap `Log` output.
- **Quarantine file** (`errors.jsonl`): one JSON object per invalid row —
  `{"line": N, "row": {...}, "errors": [...]}` — where `line` is the 1-based
  data-row number (header excluded), stable across streamed chunks.
- **`ValidateStats`** — `rows_in`, `valid`, `invalid` counters with
  `all_valid()`.
- **`PipelineExt` extension trait** — `validate(&Validator)` and
  `validate_with(&Validator, ValidateConfig)`, so pipelines read as
  `read_csv(…).validate(…).write_csv(…)`.
- **`value_to_json`** — maps streamforge's typed `Value` (including
  `Date(days)` → `"YYYY-MM-DD"` and `Timestamp(micros)` → the canonical
  timestamp string, so `format: "date"` applies naturally) to JSON. Public so
  the same mapping can be reused.
- Seven end-to-end integration tests: CSV → validate → valid.csv + errors.jsonl
  (with line numbers and paths), abort mode failing on `$.age`, skip and log
  modes, all-valid pass-through, date-column mapping, and DSL/AST-built
  validators.

### Notes

- The stage tracks its own `rows_in`/`valid`/`invalid` counters; the pipeline's
  own `PipelineStats` counts source rows.
- For CSV sources, rows whose raw text cannot fit a column's streamforge-inferred
  type are a source-level concern handled by streamforge's `ErrorPolicy`; this
  stage validates the typed values against the schema.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
