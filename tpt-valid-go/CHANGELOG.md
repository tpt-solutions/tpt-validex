# Changelog

All notable changes to the Go bindings (`tpt-valid-go`) are documented in this
file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace. The Go
module is published as `github.com/tpt-solutions/tpt-validex-go`.

## [Unreleased]

Nothing yet.

## [0.1.0] - 2026-09-29

Initial release of the Go bindings.

### Added

- **cgo bindings** over the native [`tpt-valid-ffi`](../tpt-valid-ffi) C ABI
  (`libtpt_valid_ffi`), with `tpt_validex.h` vendored alongside the source.
- **API:**
  - `NewValidator(schema string) (*Validator, error)` — compile a JSON Schema
    (Draft 2020-12 subset).
  - `(*Validator) Validate(data any) (bool, []ValidationError, error)` —
    validate one JSON-marshalable value (map, struct, slice, ...).
  - `(*Validator) ValidateBatch(batch []any) ([]Result, error)` — validate a
    batch in parallel; results keep input order.
  - `(*Validator) Close() error` — release native resources; safe to call
    twice.
  - `Version() string` — the native library version.
- **`ValidationError`** carrying `Path`, `Message`, `Expected`, and `Actual`
  (spec §5.5), implementing the `error` interface.
- A `Validator` is safe for concurrent use.
- A Go test suite (`validex_test.go`).
- Build instructions covering Linux, macOS, and Windows, including the
  MinGW-specific step of generating a GNU import library from the MSVC-built
  DLL.

### Notes

- **cgo is required.** Build the native library first with
  `cargo build --release -p tpt-valid-ffi`, then point the compiler at the
  header and library via `CGO_CFLAGS` and `CGO_LDFLAGS`. At runtime the shared
  library must be on `PATH` (or next to the binary).
- On Windows with MinGW, GNU `ld` may pick up the MSVC import library
  (`tpt_valid_ffi.lib`) and fail; generate a GNU import library once with
  `objdump` + `dlltool` as documented in the README.
- The module path is `github.com/tpt-solutions/tpt-validex-go`. Go requires the
  version tag on the repository root even though the code lives in
  `tpt-valid-go/`; either tag `vX.Y.Z` on the root, or move this directory to a
  dedicated repository when publishing publicly.
- Requires Go 1.22 or later.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
