# Changelog

All notable changes to `tpt-validex-benches` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace.

This crate is `publish = false` — it is an internal benchmarking tool, never
published to crates.io.

## [Unreleased]

Nothing yet.

## [0.1.0] - 2026-09-29

Initial benchmark suite.

### Added

- **Criterion benchmark crate** with three `harness = false` targets:
  - `validation` — `validate/single` (per-object cost on a fixed realistic
    object), `validate/batch` (`10k_objects_parallel` through the `rayon`
    path), and `validate/invalid_document` (collecting all errors versus
    fail-fast).
  - `schema_compile` — compilation latency for small, medium, and
    regex-bearing schemas, plus cached lookup.
  - `streaming` — 100,000-row CSV and 100,000-line JSONL validation, with
    `Throughput::Elements` reported.
- **Cross-language comparison scripts** under `compare/` running the same
  dataset and schema through the Python and JavaScript bindings:
  - `compare_python.py` — against `jsonschema` (Draft 2020-12) and `pydantic`.
  - `compare_js.mjs` — against `zod`.
- Documented methodology and results in `docs/performance.md`, including an
  honest accounting of the two spec targets that are **not** met (CSV
  streaming throughput and total transitive dependency count).

### Notes

- Benchmarks must be run with release semantics; the workspace `release` and
  `bench` profiles both use `opt-level = 3`, `lto = true`, and
  `codegen-units = 1`.
- CI runs the suite as a smoke test and stores criterion artifacts; comparing
  against stored baselines is future work.
- Reference machine for the published numbers: Windows 11 x86_64.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
