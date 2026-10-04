# Changelog

All notable changes to `tpt-valid-schema` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace.

## [Unreleased]

### Added
- `$ref` support: local `#/...` JSON-Pointers (percent-decoded), `#anchor`
fragments (`$anchor`, `#`-form `$id`), and cross-document refs via
`SchemaRegistry` (`Validator::new_with`). Recursive refs are detected and
rejected.
- `SchemaCache`: LRU eviction (was clear-on-overflow), keys hashed.
- Depth limits: `json::from_str_with_limit` / `MAX_SCHEMA_DEPTH` guard the
reader and AST parser against stack exhaustion (100k-deep regression
tests).
### Fixed
- `non_negative_usize` accepts integral floats (`minItems: 1.0`).
- `minContains > maxContains` is an unsatisfiable schema, not a compile
error (per spec).
### Changed
- Unknown `format` keywords now compile to runtime custom-format checks
(no-op unless registered) with an informative warning.


## [0.1.0] - 2026-09-29

Initial release of the JSON Schema front-end.

### Added

- **Custom tokenizer** (`tokenizer`): zero-dependency JSON tokenizer producing
  position-annotated `Token`s, with `TokenError` carrying 1-based line and
  column.
- **Custom JSON reader** (`json`): recursive-descent reader over the tokenizer
  output, producing `serde_json::Value` for schema documents. Duplicate object
  keys keep the last occurrence; trailing content after the top-level value is
  rejected.
- **AST** (`ast`): `SchemaAst` (`Always` / `Never` / `Object`), `ObjectAst` with
  every supported keyword, and `AdditionalAst`. `parse_schema` performs
  cross-keyword semantic validation and rejects unsupported keywords via
  `REJECTED_KEYWORDS` (`$ref`, `$dynamicRef`, `prefixItems`, `dependsRequired`)
  with helpful `SchemaError::Unsupported` messages.
- **IR and optimizer** (`ir`, `compiler`): `IrSchema`, `IrOp`, `IrAdditional`;
  `build_ir` folds `allOf` conjunctions with object-keyword merging (so
  `additionalProperties: false` in a parent does not reject keys declared in a
  sibling `allOf` member), and the optimizer prunes inapplicable checks and
  merges inclusive/exclusive numeric bounds.
- **Compiler** (`compiler`): `compile`, `build_ir`, and `lower` producing a
  `ValidationNode` state machine plus non-fatal `Warning`s.
- **Cache** (`cache`): `SchemaCache` with `new(capacity)`, `get_or_compile`,
  `len`, `is_empty`, `clear`; `global_cache()` backing `Validator::cached`.
  Capacity overflow clears the cache (simple amortized policy).
- **`Validator`** (`validator`): compile-once façade with `new`, `from_value`,
  `cached`, `from_ast`, `from_node`, `validate`, `validate_with`,
  `validate_json`, `is_valid`, `validate_batch`, `validate_csv`,
  `validate_csv_to`, `validate_jsonl`, `validate_jsonl_to`, `root`, and
  `warnings`.
- **`FlowError`**: distinguishes CSV parse failures from I/O errors in
  streaming flows.
- **`SchemaError`** (`error`): `Syntax`, `Semantic { keyword, message }`, and
  `Unsupported { keyword, message }`, with `semantic` and `unsupported`
  constructors and conversions from tokenizer errors.

### Notes

- Unknown keywords are ignored per Draft 2020-12 §6.5 extensibility and surfaced
  as warnings, so typos in supported keywords still fail loudly.
- The `schema!` DSL (in `validex-macros`) emits this crate's `SchemaAst`, so DSL
  and JSON Schema declarations lower through an identical pipeline.
- `#![forbid(unsafe_code)]` is enforced; `#![warn(missing_docs)]` is treated as
  an error in CI.
- `$ref`/`$dynamicRef` resolution and tuple validation are documented non-goals
  for 0.1 and fail loudly rather than being silently ignored.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
