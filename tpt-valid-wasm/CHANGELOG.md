# Changelog

All notable changes to `tpt-valid-wasm` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace; the
published npm package is `tpt-validex`.

## [Unreleased]

Nothing yet.

## [0.1.0] - 2026-09-29

Initial release of the WebAssembly bindings.

### Added

- **`Validator` class** (`wasm-bindgen`) for browsers, edge workers, and Node:
  - `new Validator(schema)` compiling from a plain JS object or boolean schema,
    plus the static `Validator.fromJson(text)` alternative.
  - `validate(data)` returning a **plain object** `{ isValid, errors }`.
  - `isValid(data)` — fail-fast boolean, no error materialization.
  - `validateBatch(batch)` returning one plain object
    `{ index, isValid, errors }` per input item, in order.
  - `warnings()` — non-fatal schema-compile warnings (e.g. unknown formats).
- **`version()`** — the library version string, e.g. `"0.1.0"`.
- **Hand-rolled JS ⇄ `serde_json::Value` conversion** built on `js-sys`, so
  schemas and results are plain JS objects rather than wrapper types:
  `js_to_value` and `value_to_js`. Recognizes `null`/`undefined`, booleans,
  numbers, strings, arrays, and plain objects (own enumerable string keys);
  functions, symbols, and class instances are rejected with a descriptive error
  because they have no JSON representation.
- **Ergonomic TypeScript declarations** in `tpt-validex.d.ts` describing the
  intended public API (`ValidationError`, `ValidateResult`, `BatchResult`,
  `JsonSchema`, `ValidationStats`, `Validator`, `version`, `init`).
- **npm packaging** (`package.json`): `pkg/` + the `.d.ts` + README published;
  `build` (web) and `build:node` (Node) scripts; Node ≥ 18 engine requirement.
- **Build pinning:** `wasm-bindgen` pinned to `=0.2.129` in the workspace
  manifest so the generated glue always matches the installed
  `wasm-bindgen-cli`.
- A Node test suite (`tests/validator.test.mjs`, 8 tests).

### Notes

- The module is 1.36 MB before `wasm-opt`, under the 5 MB budget, with a CI size
  check on releases.
- On the JS boundary this binding reaches ~514k objects/s; the JS ⇄ wasm
  conversion cost dominates, so in-process engines are faster on raw throughput.
  See `docs/performance.md` for the full, honest comparison.
- `tpt-valid-parser` falls back to `serde_json` on `wasm32`, so the WASM build
  never pulls in `jiter`.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
