# Changelog

All notable changes to `tpt-valid-core` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace.

## [Unreleased]

Nothing yet.

## [0.1.0] - 2026-09-29

Initial release of the core validation engine.

### Added

- **State machine** (`node`): `ValidationNode` covering the full supported
  keyword set — `CheckType`, `CheckField`, `CheckRequired`, `CheckMinimum` /
  `CheckMaximum` (plus exclusive and `multipleOf` forms), `CheckMinLength` /
  `CheckMaxLength`, `CheckPattern` (pre-compiled `regex`), `CheckEnum`,
  `CheckConst`, `CheckFormat`, `CheckArray`, `CheckUniqueItems`,
  `CheckContains`, `CheckObject`, `CheckObjectEx`, `CheckIfThenElse`,
  `CheckAllOf` / `CheckAnyOf` / `CheckOneOf` / `CheckNot`, `CheckAll`,
  `Always`, and `Never`.
  - `ValidationNode::type_of` and `ValidationNode::all` constructors; `all`
    flattens nested sequences and drops no-op `Always` nodes.
  - `EnumSet` with hash-set O(1) lookup and number normalization so `1` ≡ `1.0`.
  - `ObjectShape` fusing `properties` + `patternProperties` +
    `additionalProperties` + `required` for exact unknown-key accounting.
  - `AdditionalProperties::{Allow, Forbid, Schema}`.
- **Engine** (`engine`): `validate` (collects all errors) and `validate_value`
  (fail-fast boolean-only), with `ValidationOptions { fail_fast, max_errors }`.
- **Errors** (`error`): structured `ValidationError` with JSON `path`, `message`,
  `expected`, `actual`, and optional `value`; `ValidationReport` serializing to
  the spec §5.5 envelope via `to_json_string` / `to_json_string_pretty`;
  `ErrorCollector` with `collect_all`, `fail_fast`, and `new` constructors.
- **Types** (`types`): `DataType` with Draft 2020-12 integer semantics (`1.0` is
  an integer), `is_integer_value`, `json_type_name`, `value_as_f64`, and
  `number_from_f64`.
- **Formats** (`format`): `email`, `uri` (aliases `url`, `iri`), `date`,
  `date-time` (alias `datetime`), `uuid`, `ipv4`, `ipv6`, `hostname` (alias
  `idn-hostname`) — all hand-written in pure Rust, with `chrono` used only for
  calendar correctness.
- **Batch** (`batch`): `validate_batch` with parallel execution via `rayon`,
  returning input-ordered `ValidationOutcome { index, valid, errors }`.
- **Streaming CSV** (`csv`): hand-written CSV state machine with quoted-field
  and escape support, custom dialects, per-column type inference
  (`infer_column_types`, `record_to_value`), `CsvReader`, and
  `validate_csv_stream` / `validate_csv_bytes` with bounded memory.
- **Streaming JSONL** (`jsonl`): `validate_jsonl_stream` with
  `JsonlRowOutcome` (valid / invalid / parse error, each with a 1-based line
  number) and `JsonlStats`.
- `parse_value` helper exposing the platform's fastest JSON backend with errors
  flattened to a message.

### Notes

- `#![forbid(unsafe_code)]` is enforced crate-wide; `#![warn(missing_docs)]` is
  treated as an error in CI.
- The crate has 6 direct dependencies, all MIT-only or MIT/Apache-2.0
  dual-licensed: `serde`, `serde_json`, `regex`, `chrono`, `rayon`, and
  `tpt-valid-parser`.
- `multipleOf` uses exact integer arithmetic for integral operands and an
  epsilon-tolerant float path; the documented quirk is that `35 / 1.4` is
  accepted.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
