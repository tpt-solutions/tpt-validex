# tpt-valid-parser

[![Crates.io](https://img.shields.io/crates/v/tpt-valid-parser.svg)](https://crates.io/crates/tpt-valid-parser)
[![Docs.rs](https://img.shields.io/docsrs/tpt-valid-parser.svg)](https://docs.rs/tpt-valid-parser)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

**The JSON parsing front-end: `jiter` on native, `serde_json` on wasm — one API, strict licensing.**

This is the smallest crate in the
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace: it picks
the fastest JSON backend available for the target and normalizes it behind one
uniform API, so callers never see which parser handled a document.

| Target | Backend | License |
| :--- | :--- | :--- |
| Native (Linux, macOS, Windows) | [`jiter`](https://crates.io/crates/jiter) — the high-performance iterative parser behind `pydantic-core` | MIT |
| `wasm32` and other non-native | [`serde_json`](https://crates.io/crates/serde_json) — the guaranteed-compatible baseline | MIT / Apache-2.0 |

## Why `jiter` and not `sonic-rs`?

The original spec called for `sonic-rs` and assumed it was MIT. Our dependency
audit found it is **Apache-2.0-only**, which this project rejects. `jiter` (MIT)
replaced it and is the parser behind `pydantic-core` — the same engine that
makes the Python comparison benchmarks competitive. This is enforced by
`cargo deny` plus `scripts/audit_apache_only.py` on every PR.

## Install

```sh
cargo add tpt-valid-parser
```

| Requirement | Value |
| :--- | :--- |
| Rust edition | 2021 |
| Minimum Rust | 1.75 |
| License | MIT OR Apache-2.0 |
| `unsafe` code | Forbidden (`#![forbid(unsafe_code)]`) |

## Usage

```rust
let v = tpt_valid_parser::parse(r#"{"name": "Alice", "age": 30}"#).unwrap();
assert_eq!(v["name"], "Alice");
assert_eq!(v["age"], 30);

// From bytes
let v = tpt_valid_parser::parse_bytes(br#"{"ok": true}"#).unwrap();
assert_eq!(v["ok"], true);

// Diagnostics: which backend handled this build?
let (v, backend) = tpt_valid_parser::parse_verbose("[]").unwrap();
assert!(backend == "jiter" || backend == "serde_json");
assert_eq!(v, serde_json::json!([]));
```

Invalid input returns a `ParseError` with position information where the
backend provides it:

```rust
let err = tpt_valid_parser::parse("{invalid}").unwrap_err();
assert!(!err.message.is_empty());
// Display includes position when known: "expected ... at line 1 column 2"
```

## API

| Item | Description |
| :--- | :--- |
| `parse(&str)` | Parse a document, choosing the platform's primary backend. |
| `parse_bytes(&[u8])` | Same, from raw bytes. |
| `parse_verbose(&str)` | Parse and also return the backend name — for tests and diagnostics. |
| `backend_name()` | The backend compiled into this build (`"jiter"` or `"serde_json"`). |
| `ParseError { message, line, column }` | Unified error across all backends. `line`/`column` are 1-based, `0` when unknown. `Display` appends the position when known. |

Every backend produces `serde_json::Value`, so the rest of the engine is
backend-agnostic.

## Design notes

- **Fallback is a compile-time decision, not a runtime one.** The `jiter`
  dependency is gated behind `cfg(not(target_arch = "wasm32"))`, so wasm builds
  never pull it in.
- **Errors are normalized.** `ParseError` erases backend-specific error types so
  callers get one shape; `line`/`column` are best-effort.
- **Strict licensing.** Both backends are MIT or MIT/Apache-2.0 dual-licensed —
  no Apache-2.0-only and no copyleft crates enter the tree.

## Performance

JSON parsing is the front half of the validation pipeline. See
[../docs/performance.md](../docs/performance.md) for the end-to-end numbers
(single-object validation runs 1.6–6.6M objects/s including parse time on the
native `jiter` backend).

## Testing

```sh
cargo test -p tpt-valid-parser
cargo clippy -p tpt-valid-parser --all-targets -- -D warnings
cargo fmt --check
```

## Related crates

| Crate | Relationship |
| :--- | :--- |
| [`tpt-valid-core`](../tpt-valid-core) | Consumes this crate's output in its streaming validators. |
| [`tpt-valid-schema`](../tpt-valid-schema) | Uses this parser for data documents (schema documents use their own reader). |
| [`validex`](../validex) | Facade re-exporting the engine. |

## Documentation

- [Performance](../docs/performance.md)
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option. Dependencies: `serde_json` (always), `jiter` (native targets only).
