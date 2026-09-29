# Changelog

All notable changes to `tpt-valid-ffi` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace.

## [Unreleased]

Nothing yet.

## [0.1.0] - 2026-09-29

Initial release of the C ABI.

### Added

- **C API** declared in `tpt_validex.h`, with full doc comments, `extern "C"`
  guards for C++ consumers, and `<stdbool.h>` inclusion:
  - `tpt_valid_create` / `tpt_valid_destroy` — compile and release a validator
    from JSON Schema text.
  - `tpt_valid_validate` / `tpt_valid_free_result` — validate one JSON document
    and release the result.
  - `tpt_valid_is_valid` — validity check; `NULL` reports `false`.
  - `tpt_valid_get_errors` — the spec §5.5 error envelope as a JSON string.
  - `tpt_valid_last_error` — thread-local reason for the most recent failed
    call.
  - `tpt_valid_version` — library version string.
- **Opaque types** `tpt_valid_handle` and `tpt_valid_result` (`#[repr(C)]`).
- Dual `crate-type` output: `cdylib` **and** `staticlib`, so consumers can link
  dynamically or statically.
- **Robustness guarantees:**
  - All entry points are wrapped in `catch_unwind`; a panic becomes a `NULL`
    return plus a `tpt_valid_last_error` message instead of unwinding across the
    FFI boundary.
  - `NULL` arguments are handled gracefully everywhere (never a crash).
  - Malformed JSON *data* produces an invalid result carrying a parse error,
    not a `NULL` return — only invalid arguments or a failed schema compile do
    that.
  - Result/error strings are kept alive in owned allocations so the returned
    pointers stay valid until the owning object is freed.
- A runnable end-to-end C example under `tests/c_example/`.

### Notes

- `#![allow(unsafe_code)]` is scoped to this crate only, since an FFI boundary
  requires it; every other workspace crate forbids `unsafe`.
- The API surface is considered **stable** for the 0.1 line: existing
  functions, signatures, and the error envelope will not change incompatibly
  within 0.1.x.
- This library is what the Go bindings (`tpt-valid-go`) link against via cgo.

[Unreleased]: https://github.com/tpt-solutions/tpt-validex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-validex/releases/tag/v0.1.0
