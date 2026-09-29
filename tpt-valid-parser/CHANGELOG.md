# Changelog

All notable changes to `tpt-valid-parser` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace.

## [Unreleased]

Nothing yet.

## [0.1.0] - 2026-09-29

Initial release of the JSON parsing front-end.

### Added

- **Dual-backend parsing** behind one uniform API, always producing
  `serde_json::Value`:
  - `jiter` (MIT) as the primary parser on all native targets — the
    high-performance iterative parser behind `pydantic-core`.
  - `serde_json` (MIT / Apache-2.0) as the guaranteed-compatible baseline on
    `wasm32` and other non-native targets.
- `parse(&str)` — parse a document, automatically selecting the platform's
  primary parser.
- `parse_bytes(&[u8])` — the same, from raw bytes.
- `parse_verbose(&str)` — parse and additionally return the backend used, for
  tests and diagnostics.
- `backend_name()` — report the backend compiled into this build.
- `ParseError { message, line, column }` — a unified error type across all
  backends, with `Display` appending position information when known,
  `std::error::Error` support, and a `From<serde_json::Error>` conversion.

### Changed

- The `jiter` dependency is gated behind `cfg(not(target_arch = "wasm32"))`, so
  WebAssembly builds never pull it in.

### Notes

- **`sonic-rs` was rejected** during dependency auditing: it is Apache-2.0-only,
  not MIT as the original spec assumed. `jiter` (MIT) replaced it. This is
  enforced by `cargo deny check licenses` and `scripts/audit_apache_only.py` on
  every PR.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
