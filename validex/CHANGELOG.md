# Changelog

All notable changes to `validex` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace, so this
crate moves in lockstep with its siblings.

## [Unreleased]

Nothing yet.

## [0.1.0] - 2026-09-29

Initial release. The facade crate is feature-complete for the 0.1 line.

### Added

- `Validator` facade with a unified API over the engine:
  - Compile entry points: `Validator::new`, `Validator::from_json`,
    `Validator::from_value`, `Validator::cached` (process-wide cache),
    `Validator::from_ast`, and `Validator::from_node` (wrap a hand-built
    state machine with zero compilation).
  - Single-object validation: `validate`, `validate_with`, `is_valid`,
    `validate_json`.
  - Parallel batch validation: `validate_batch` (backed by `rayon`).
  - Streaming validation: `validate_csv`, `validate_csv_to`, `validate_jsonl`,
    `validate_jsonl_to`, with JSONL quarantine files carrying 1-based line
    numbers.
  - Introspection: `root()` and `warnings()`.
- The `schema!` DSL macro, re-exported from
  [`validex-macros`](../validex-macros), with compile-time syntax and semantic
  checking: unknown options, misplaced options, duplicate properties,
  `min > max`, `multiple_of <= 0`, and unknown `format` values all fail the
  build.
- The `dsl` namespace module giving `schema!`-generated code stable paths under
  the `validex` crate name.
- Broad re-exports so a single dependency is enough: `ValidationNode`,
  `ValidationOptions`, `ValidationError`, `ValidationReport`,
  `ValidationOutcome`, `ErrorCollector`, `DataType`, `Format`, `EnumSet`,
  `AdditionalProperties`, `SchemaCache`, `SchemaError`, `FlowError`, the `ast`
  module, `compile_schema_ast`, and `serde_json::json`.
- Documented JSON Schema Draft 2020-12 subset with a full
  [compliance matrix](../docs/compliance.md).

### Notes

- The DSL AST flows through the identical AST → IR → optimizer → state-machine
  pipeline as JSON Schema; a test asserts that equivalent JSON Schema and DSL
  declarations lower to byte-identical machines.
- `$ref`/`$dynamicRef` resolution and tuple validation (`prefixItems`) are
  documented non-goals for 0.1 and fail with helpful compile errors rather than
  being silently ignored.
- The crate self-references in `[dev-dependencies]` so its own tests and doctests
  can expand `schema!`.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
