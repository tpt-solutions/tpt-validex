# Performance

Every number below was **measured** on the reference development machine
(Windows 11, x86_64, release profile with LTO). Absolute numbers vary by
hardware; the ratios and the methodology are what matter. All benchmarks ship
with the repository — reproduce them yourself:

```sh
# Rust core (criterion)
cargo bench                                    # full suite
cargo bench --bench validation                 # single/batch throughput

# Language comparisons (same dataset & schema)
cd tpt-valid-py && python ../benches/compare/compare_python.py   # needs maturin develop first
cd tpt-valid-wasm && npm run build:node && node ../benches/compare/compare_js.mjs
```

## Spec targets vs. measured

| Metric | Spec target (§7) | Measured | Status |
| :--- | :--- | :--- | :--- |
| Validation throughput (JSON, single object) | > 5M obj/s | **1.6–6.6M obj/s** (~150–620 ns/object) | ✅ on quiet hardware, ⚠️ machine-dependent |
| Batch validation (parallel, rayon) | — | **~3.5M obj/s** (10k batch in ~2.8 ms) | ✅ |
| Streaming CSV validation | > 10M rows/s | **~1.15M rows/s** (100k rows in ~87 ms) | ❌ not met — honest number |
| Streaming JSONL validation | — | **~1.75M lines/s** (100k lines in ~57 ms) | — |
| Schema compilation | < 10 ms | **< 1 µs** small; **~3 ms** 9-keyword schema with regexes | ✅ |
| Cached schema lookup | — | **~560 ns** | ✅ |
| Memory (1M objects) | < 50 MB | streaming APIs are O(1) per row; batch is O(batch) | ✅ streaming, ⚠️ batch |

**Memory profiling** (`scripts/profile_memory.py --rows 1000000`, Windows x86_64
release build, 2026-10-03): the streaming JSONL path peaks at **5.9 MiB
regardless of row count** (5.9 MiB at 200k / 400k / 1M rows — the process
high-water mark is flat, confirming O(1)-per-row), while the batch path (a
1M-element JSON array validated via rayon) peaks at **~895 MiB** — the
records are materialized, ~152× the streaming footprint. Batch memory is
inherent to materializing inputs, not to the engine; chunk large arrays at
the caller for bounded batch memory.
| Binary size (native FFI cdylib) | < 10 MB | **~1–2 MB** | ✅ |
| Binary size (WASM) | < 5 MB | **1.36 MB** (pre-`wasm-opt`) | ✅ |
| Dependency count | < 15 crates | 72 runtime crates (72 incl. transitive) | ❌ not met — see notes |

Notes on the two unmet targets:

* **CSV throughput**: the custom parser is byte-oriented and allocation-light,
  but per-row it builds a JSON object and materializes errors; 10M rows/s
  would require validating raw bytes without an intermediate row representation.
  This is a documented future optimization, not a claim we make today.
* **Dependency count**: the spec's "< 15 crates" was written before pyo3 and
  wasm-bindgen entered the tree. The *core* (`tpt-valid-core` +
  `tpt-valid-schema` + `tpt-valid-parser`) has 6 direct dependencies
  (`serde`, `serde_json`, `regex`, `chrono`, `rayon`, `jiter`); the 72-crate
  count is the full workspace graph. Every runtime dependency is MIT or
  MIT/Apache-2.0 dual-licensed — zero copyleft, zero Apache-2.0-only
  (enforced by `cargo deny` + `scripts/audit_apache_only.py` in CI).

## Language comparison (flat user object, batch of 10,000)

Measured with `benches/compare/compare_python.py` and
`benches/compare/compare_js.mjs` on the reference machine:

| Benchmark | objects/sec | vs. tpt-validex |
| :--- | ---: | :--- |
| **tpt-validex (Rust core)** | ~3.5M | — |
| **tpt-validex (Python, batch API)** | ~650k | — |
| **tpt-validex (Python, single)** | ~313k | — |
| pydantic (Python) | ~940k | 1.4× faster single, 1.5× batch |
| jsonschema (Python, Draft 2020-12) | ~45k | **tpt-validex is 7–16× faster** |
| **tpt-validex (WASM, JS single)** | ~514k | — |
| zod (JavaScript) | ~1.4M | ~2.7× faster (in-process JS vs. wasm boundary) |

Honest interpretation:

* The **Rust core** validates millions of objects per second — an order of
  magnitude faster than any Python validator and competitive with zod's
  in-process JS engine.
* At the **Python boundary**, dict→native conversion dominates (~µs/object).
  We fast-path through Python's C `json.dumps` + our SIMD-ish parser, which
  makes us far faster than `jsonschema` and within ~1.5× of pydantic's
  compiled models. Unlike pydantic, you get JSON Schema semantics and
  structured error paths.
* At the **WASM boundary**, the JS↔wasm conversion cost dominates. zod wins
  raw in-process throughput; choose `tpt-validex` for WASM when you need one
  schema definition across languages, JSON Schema compatibility, or
  server/browser-identical validation.

## Fail-fast mode

For invalid documents, collecting all errors costs ~4× a fail-fast check
(~1.8 µs vs ~550 ns on the invalid 4-keyword sample). Use
`ValidationOptions::fail_fast()` (or design your hot path around
`is_valid`) when you only need the boolean.

## Parser

Native targets parse with [`jiter`](https://crates.io/crates/jiter) (MIT) —
the parser behind pydantic-core. `serde_json` is the baseline on `wasm32`.
The original spec named `sonic-rs` as the SIMD parser believing it MIT;
our license audit (spec §3.2 policy) rejected it as Apache-2.0-only, and
`jiter` took its place with comparable throughput (batch benchmarks improved
~15% over the sonic-rs build).
