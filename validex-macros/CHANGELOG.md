# Changelog

All notable changes to `validex-macros` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace.

## [Unreleased]

Nothing yet.

## [0.1.0] - 2026-09-29

Initial release of the `schema!` proc macro.

### Added

- **`schema!` proc macro** — a compile-time-checked DSL for declaring validation
  schemas, evaluating to a compiled `validex::Validator`.
- **DSL grammar** (spec §5.1):
  - Roots may be any type: `object { … }`, `array(…)`,
    `string(…)` / `integer(…)` / `number(…)` / `boolean(…)` / `null(…)` /
    `any(…)`.
  - Object entries are `required "name" => <type>` or
    `optional "name" => <type>`, with optional trailing commas; unmarked
    properties are `optional` (the JSON Schema default).
  - Options are `IDENT = value`, accepting strings, integers, floats, booleans,
    literal lists, and nested type expressions (for `items`).
- **Options per type:** `min_length`, `max_length`, `pattern`, `format`, and
  `enum` for `string`; `min`, `max`, `exclusive_min`, `exclusive_max`,
  `multiple_of`, and `enum` for `integer`/`number`; `items`, `min_items`,
  `max_items`, and `unique` for `array`; block entries only for `object`.
- **Compile-time semantic validation** — the following are rejected with
  `compile_error!` during expansion, not at runtime:
  - Syntax errors: missing `=>`, unknown type names, trailing tokens.
  - Unknown options, and options passed to types that don't accept them.
  - Duplicate options and duplicate properties.
  - Inverted bounds: `min > max`, `min_length > max_length`,
    `min_items > max_items`.
  - `multiple_of <= 0`.
  - Unknown `format` values (strict where JSON Schema would ignore them).
- **Shared compilation pipeline:** the emitted code builds
  `tpt_valid_schema::ast::SchemaAst`, so DSL and JSON Schema declarations lower
  through an identical AST → IR → optimizer → state-machine path. A test in the
  `validex` crate asserts that equivalent declarations produce byte-identical
  machines.

### Notes

- **Zero dependencies** beyond `proc_macro` — the DSL tokenizer, parser, and
  code emitter are all hand-written in this crate.
- Generated code addresses items via the `::validex::` path.
- Regex *syntax* is the one thing not checked at compile time, since the macro
  has no regex engine; an invalid `pattern` panics at first use with a clear
  message.
- Use the macro through `validex::schema!`; this crate is an implementation
  detail of the `validex` facade.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
