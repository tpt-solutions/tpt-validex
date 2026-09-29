# tpt-validex-benches

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

**The criterion benchmark suite for tpt-validex, plus cross-language comparison scripts.**

This crate (`publish = false`) holds the reproducible benchmarks behind the
numbers in [../docs/performance.md](../docs/performance.md). It benchmarks the
Rust engine directly — single-object throughput, parallel batch, fail-fast vs.
collect-all, schema compilation, and streaming CSV/JSONL.

## Running the benchmarks

```sh
# Full suite
cargo bench

# Individual benchmarks
cargo bench --bench validation        # single-object and batch throughput
cargo bench --bench schema_compile    # schema compilation cost
cargo bench --bench streaming         # streaming CSV and JSONL
```

All three targets use `harness = false` and drive
[criterion](https://github.com/bheisler/criterion.rs) directly. Criterion writes
a `target/criterion/` directory with per-benchmark baselines, so you can compare
runs across commits.

## What is measured

| Criterion group | What it exercises |
| :--- | :--- |
| `validate/single` → `user_schema` | Per-object validation cost on a fixed, realistic object. |
| `validate/batch` → `10k_objects_parallel` | 10,000-object batch through the `rayon` parallel path. |
| `validate/invalid_document` → `collect_all_errors` | The cost of collecting every error versus stopping at the first. |
| `schema_compile` → `small_schema` and larger variants | Compilation latency per schema shape, plus cached lookup. |
| `streaming` | 100,000-row CSV and 100,000-line JSONL, with `Throughput::Elements` reported. |

## Cross-language comparison

The scripts under `compare/` run the *same dataset and schema* through the
Python and JavaScript bindings, so the numbers are directly comparable:

```sh
# Python: needs `maturin develop --release` in tpt-valid-py first
cd tpt-valid-py && python ../benches/compare/compare_python.py

# JavaScript: needs `npm run build:node` in tpt-valid-wasm first
cd tpt-valid-wasm && npm run build:node && node ../benches/compare/compare_js.mjs
```

Both compare against the relevant in-language incumbents —
[`jsonschema`](https://pypi.org/project/jsonschema/) and
[`pydantic`](https://pypi.org/project/pydantic/) in Python,
[`zod`](https://www.npmjs.com/package/zod) in JavaScript.

## Interpreting the results

Absolute numbers vary by hardware; the ratios and the methodology are what
matter. The reference machine was Windows 11 x86_64 with the release profile
(LTO, `codegen-units = 1`). Two spec targets are **not** met and are documented
honestly rather than papered over: streaming CSV throughput (~1.15M rows/s
against a 10M target) and the total transitive dependency count. See
[../docs/performance.md](../docs/performance.md) for the full analysis.

## Notes

- This crate is never published to crates.io (`publish = false`).
- Benchmark results are also produced in CI as a smoke run; comparing against
  stored baselines is future work.
- Always benchmark with `--release` semantics — the workspace `release` and
  `bench` profiles both use `opt-level = 3`, `lto = true`, and
  `codegen-units = 1`.

## Related crates

| Crate | What is benchmarked |
| :--- | :--- |
| [`tpt-valid-core`](../tpt-valid-core) | The engine: state machine, batch, streaming. |
| [`tpt-valid-schema`](../tpt-valid-schema) | Schema compilation latency. |
| [`tpt-valid-py`](../tpt-valid-py) · [`tpt-valid-wasm`](../tpt-valid-wasm) | The language-boundary comparisons. |

## Documentation

- [Performance](../docs/performance.md)
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option.
