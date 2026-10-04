# Contributing to tpt-validex

Thanks for helping build the universal high-performance data validator!

## Development setup

```sh
git clone --recurse-submodules https://github.com/tpt-solutions/tpt-validex
cd tpt-validex
cargo test --workspace        # all Rust tests (incl. the official JSON-Schema-Test-Suite)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Language bindings:

```sh
cargo test -p tpt-valid-py && (cd tpt-valid-py && python -m pytest tests -q)   # Python (needs maturin develop)
cargo build -p tpt-valid-wasm --target wasm32-unknown-unknown                  # WASM
(cd tpt-valid-wasm/tests && node --test)                                       # JS tests (needs wasm-pack build first)
CGO_CFLAGS="-I tpt-valid-ffi" CGO_LDFLAGS="-L target/debug" go test ./...      # Go (from tpt-valid-go)
```

## Ground rules

* **Licensing**: every dependency must be MIT or MIT/Apache-2.0 dual-licensed.
  `cargo deny check licenses` and `scripts/audit_apache_only.py` run in CI —
  Apache-2.0-only deps are rejected (this is why sonic-rs was replaced by
  jiter).
* **No unsafe** outside the FFI boundary (`tpt-valid-ffi`, `tpt-valid-py`,
  `tpt-valid-wasm` glue). Core crates carry `#![forbid(unsafe_code)]`.
* **Documented subset**: schema features that are not implemented must fail
  loudly (compile error) or be listed with their status in
  `docs/compliance.md`. The official JSON-Schema-Test-Suite runs from the
  `third-party/JSON-Schema-Test-Suite` submodule; new skips need a reason in
  the allow-list (`tpt-valid-schema/tests/jsonschema_suite.rs`).
* **Performance**: hot paths are allocation-light and dependency-free by
  design (custom CSV/JSONL parsers, custom tokenizer). Benchmarks live in
  `benches/`; add one for user-visible changes.

## Pull requests

1. Fork, branch (`feat/...` or `fix/...`).
2. Add or update tests for the change (unit tests per crate; suite cases for
   schema features).
3. Keep `cargo fmt` + `cargo clippy -D warnings` + the full test suite green.
4. Update `docs/` when behavior or the compliance matrix changes, and the
   relevant `CHANGELOG.md`.

## Issues

Use the issue templates. Good first issues are labeled `good first issue` —
they keep to one crate and include a failing test or a clear acceptance
criterion.

## License

By contributing you agree your work is dual-licensed MIT / Apache-2.0.
