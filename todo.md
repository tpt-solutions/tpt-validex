# tpt-validex — Task Checklist

> Universal High-Performance Data Validator  
> TPT Solutions · Dual-licensed MIT / Apache-2.0

Status notes (2026-09-29): phases 0–6 and 8 are implemented and tested.
Deviations from the spec are annotated inline — most notably the parser
choice (sonic-rs is Apache-2.0-only, not MIT as assumed; jiter replaced it)
and the performance targets we do not meet (documented honestly in
`docs/performance.md`).

---

## Phase 0: Project Scaffolding

- [x] Initialize Cargo workspace (`Cargo.toml` with all member crates)
- [x] Create `LICENSE-MIT` (TPT Solutions, 2026)
- [x] Create `LICENSE-APACHE`
- [x] Create `deny.toml` (cargo-deny license/advisory config per spec §3.2)
- [x] Create `.gitignore` (Rust + Node + Python artifacts)
- [x] Create `README.md` (initial: name, tagline, license badges, crate links)
- [x] Create stub crates with `Cargo.toml` + `src/lib.rs`:
  - [x] `tpt-valid-core/`
  - [x] `tpt-valid-schema/`
  - [x] `tpt-valid-parser/`
  - [x] `tpt-valid-ffi/`
  - [x] `tpt-valid-py/`
  - [x] `tpt-valid-wasm/`
- [x] Create `tpt-valid-go/` stub (`go.mod` + `validex.go`)
- [ ] Set up GitHub repository *(needs repo owner/admin access — CI and publishing workflows are ready in `.github/`)*

---

## Phase 1: Core Validation Engine (Pure Rust)

### Schema Parser (`tpt-valid-schema`)
- [x] Implement JSON Schema tokenizer (custom, zero external deps)
- [x] Implement JSON Schema parser → AST
- [x] Implement semantic validation of schema (e.g. `min` ≤ `max`)

### Schema Compiler (`tpt-valid-schema`)
- [x] Compile AST → IR (intermediate representation)
- [x] IR optimizer (merge redundant checks, pre-compile regex patterns)
- [x] Compile IR → state machine (tree of `ValidationNode`)
- [x] Cache compiled state machines for reuse (`SchemaCache` + `Validator::cached`)

### Validation Engine (`tpt-valid-core`)
- [x] Implement `ValidationNode` enum (all node types from spec §5.2, plus needed extras):
  - [x] `CheckType(DataType)`
  - [x] `CheckField(String, Box<ValidationNode>)`
  - [x] `CheckRequired(Vec<String>)`
  - [x] `CheckMinimum(f64)` / `CheckMaximum(f64)` (+ exclusive/multipleOf)
  - [x] `CheckPattern(Regex)` — using `regex` crate (MIT/Apache-2.0)
  - [x] `CheckEnum(Vec<Value>)` — hash set for O(1) lookup (`EnumSet`)
  - [x] `CheckFormat(Format)`
  - [x] `CheckArray(Box<ValidationNode>, usize, usize)`
  - [x] `CheckObject(Vec<(String, ValidationNode)>)` (+ `CheckObjectEx` with additionalProperties/patternProperties)
- [x] Single-object validation (traverse state machine)
- [x] Collect all errors (not just first)
- [x] Structured error output with JSON path (spec §5.5 error format)

### SIMD JSON Parser (`tpt-valid-parser`)
- [x] ~~Integrate `sonic-rs` (MIT)~~ — **rejected by the license audit**: sonic-rs is Apache-2.0-only. Replaced with `jiter` (MIT, the pydantic-core parser) as primary.
- [x] `jiter` (MIT) primary parser (native targets)
- [x] `serde_json` (MIT/Apache-2.0) guaranteed-compatible baseline fallback (wasm32 + others)

### License Compliance
- [x] Run `cargo deny check licenses` — zero violations
- [x] Verify zero Apache-2.0-only dependencies in full runtime transitive tree (`scripts/audit_apache_only.py`)
- [ ] Verify total crate count < 15 — **not met**: 72 runtime crates (6 direct core deps; pyo3/wasm-bindgen/criterion account for the bulk). Documented in `docs/performance.md`.

### Tests
- [x] Unit tests: schema parsing (valid and invalid schemas)
- [x] Unit tests: state machine compilation
- [x] Unit tests: basic validation (type, required, min/max)

---

## Phase 2: Advanced Validation Features

### Array & Object Keywords
- [x] `items` validation
- [x] `minItems` / `maxItems`
- [x] `uniqueItems`
- [x] `additionalProperties`
- [x] `patternProperties`

### Format Validators (all built from scratch, zero external deps)
- [x] Email — custom RFC 5322 validator
- [x] URI/URL — custom RFC 3986 validator
- [x] Date — ISO 8601 (using `chrono`, MIT/Apache-2.0)
- [x] Date-Time — ISO 8601 (using `chrono`)
- [x] UUID — custom RFC 4122 validator
- [x] IPv4 — custom validator
- [x] IPv6 — custom validator

### Enum & Logical Keywords
- [x] `enum` validation (hash set, O(1) lookup)
- [x] `if` / `then` / `else` conditional validation
- [x] `allOf`
- [x] `anyOf`
- [x] `oneOf`
- [x] `not`

### Compliance
- [x] JSON Schema Draft 2020-12 compliance test suite (keyword-by-keyword tests; suite subset)
- [x] Document supported vs. unsupported keywords (compliance matrix — `docs/compliance.md`)

---

## Phase 3: Streaming & Batch Validation

- [x] Batch validation — parallel via `rayon` (MIT/Apache-2.0)
- [x] Custom streaming CSV parser (scratch, zero external deps):
  - [x] Quoted fields
  - [x] Custom delimiters
  - [x] Type inference per column
  - [x] Per-row validation with error collection
- [x] Custom streaming JSONL parser (scratch, zero external deps):
  - [x] Line-by-line validation
  - [x] Error reporting with line numbers
- [x] "Fail-fast" mode (stop on first error, for performance-critical paths)
- [x] Tests: batch validation correctness and parallelism
- [x] Tests: streaming CSV and JSONL validation

---

## Phase 4: FFI & Python Wrapper

### C ABI (`tpt-valid-ffi`)
- [x] `tpt_valid_create(schema: *const c_char) -> *mut tpt_valid_handle`
- [x] `tpt_valid_validate(handle, data) -> *mut tpt_valid_result`
- [x] `tpt_valid_is_valid(result) -> bool`
- [x] `tpt_valid_get_errors(result) -> *const c_char`
- [x] `tpt_valid_free_result(result)`
- [x] `tpt_valid_destroy(handle)`
- [x] `tpt_validex.h` header file (verified with a compiled C example, `tests/c_example/`)

### Python Bindings (`tpt-valid-py`, PyO3)
- [x] `Validator` class
- [x] `validate(data) -> (bool, list[dict])` — single object
- [x] `validate_batch(batch) -> list[Result]` — parallel
- [x] `validate_csv(path, valid_output, errors_output)` — streaming
- [x] `maturin` build configuration (`pyproject.toml` + `.pyi` stubs, fully typed)

### Publishing & CI
- [x] GitHub Actions: build Python wheels for:
  - [x] Linux x86_64 (manylinux)
  - [x] Linux aarch64 (manylinux)
  - [x] macOS x86_64
  - [x] macOS arm64 (Apple Silicon)
  - [x] Windows x86_64
- [x] Publish wheels to PyPI on release tag (`.github/workflows/release.yml`; actual publishing needs repo + PyPI trusted publishing)

### Tests & Docs
- [x] Python tests (`pytest`) — validate, validate_batch, validate_csv (17 tests)
- [x] Python API documentation (`docs/api-python.md` + `.pyi`)
- [x] Python usage examples (FastAPI, ETL pipeline)

---

## Phase 5: WASM Wrapper

- [x] `tpt-valid-wasm` (wasm-bindgen, MIT/Apache-2.0):
  - [x] `Validator` class
  - [x] `validate(data) -> { isValid, errors }`
  - [x] `validateBatch(batch) -> Result[]`
- [x] Optimize binary size — **1.36 MB** (already well under the 5 MB target; `wasm-opt -Oz` runs best-effort in the release workflow)
- [x] TypeScript type definitions (`tpt-validex.d.ts` + generated bindings)
- [x] `package.json` for npm package `tpt-validex`
- [x] GitHub Actions: build WASM + publish to npm on release tag
- [x] JS/TS tests (8 Node tests)
- [x] JavaScript documentation and examples (Node.js, browser form validation)

---

## Phase 6: Go Wrapper

- [x] `tpt-valid-go` (cgo):
  - [x] `NewValidator(schema string) (*Validator, error)`
  - [x] `Validate(data any) (bool, []ValidationError, error)`
  - [x] `ValidateBatch(batch []any) ([]Result, error)`
- [x] `go.mod` (module: `github.com/tpt-solutions/tpt-validex-go`)
- [ ] Publish to Go module registry *(tag-based; requires the GitHub repository first)*
- [x] Go tests (9 tests incl. concurrency + use-after-close)
- [x] Go documentation and examples

---

## Phase 7: tpt-streamforge Integration

> **Unblocked (2026-09-29):** `tpt-streamforge` is live at
> `github.com/tpt-solutions/tpt-streamforge`. Integration shipped in the
> `tpt-validex-streamforge` crate (git dependency on branch `master`).

- [x] Define integration interface (trait/API boundary) between `tpt-validex` and `tpt-streamforge` (`PipelineStage` impl + `PipelineExt` extension trait on streamforge's `Pipeline`)
- [x] Implement `validate` transformation stage for `tpt-streamforge` pipelines (`Validate` stage over columnar `RecordBatch`es)
- [x] Error output: invalid rows written to separate `.jsonl` file (`ValidateConfig::errors_to` → `{"line", "row", "errors"}` with 1-based data-row numbers)
- [x] Configurable error handling modes: `skip` / `abort` / `log` (`ErrorMode`)
- [x] Integration tests (pipeline: read CSV → validate → write valid + errors; plus JSONL, abort, log, all-valid pass-through, date-column mapping — 7 tests)
- [x] Pipeline usage examples (spec §5.6 and §13 Use Case 2 — crate docs + `docs/api-streamforge.md`)

---

## Phase 8: Custom DSL (`validex::schema!` macro)

- [x] Design DSL syntax (per spec §5.1 example)
- [x] DSL tokenizer (proc-macro token parser)
- [x] DSL parser → AST
- [x] Implement proc macro crate (`validex-macros`; zero deps beyond `proc_macro`)
- [x] DSL AST → shared IR (same compiler pipeline as JSON Schema; identical-machine round-trip test)
- [x] `validex::schema!` macro usable from Rust consumer crates
- [x] DSL tests (round-trip: DSL → state machine → validation; compile-time semantic errors)
- [x] DSL documentation and examples (`docs/dsl.md`)

---

## Benchmarks

- [x] Set up `criterion` benchmark suite (`benches/` in workspace)
- [x] Benchmark: single-object validation throughput (measured 1.6–6.6M obj/s; target > 5M obj/s — machine-dependent)
- [x] Benchmark: batch validation throughput (parallel; ~3.5M obj/s on 10k batch)
- [x] Benchmark: streaming CSV validation throughput (measured ~1.15M rows/s — **target > 10M rows/s not met**, documented)
- [x] Benchmark: schema compilation time (measured < 1 µs small, ~3 ms with regexes; target < 10 ms ✅)
- [x] Comparison benchmark vs `pydantic` (Python) — same dataset (`benches/compare/compare_python.py`; 7–16× vs jsonschema, ~1.4× behind pydantic at the Python boundary)
- [x] Comparison benchmark vs `zod` (JavaScript) — same dataset (`benches/compare/compare_js.mjs`; zod faster in-process, wasm boundary documented)
- [x] Comparison benchmark vs `jsonschema` (Python) — same dataset
- [ ] Memory usage profiling (streaming APIs are O(1) per row by construction; explicit 1M-object RSS profiling not yet scripted)
- [x] Binary size tracking — native (~1–2 MB < 10 MB ✅), WASM (1.36 MB < 5 MB ✅, CI budget check)

---

## Documentation

- [x] `README.md`: installation, quick start, all-language examples, license badges
- [x] `rustdoc` for all public APIs in all crates (`#![warn(missing_docs)]` enforced, clippy-clean)
- [x] Python API reference (`docs/api-python.md`)
- [x] JavaScript / TypeScript API reference (`docs/api-javascript.md`)
- [x] Go API reference (`docs/api-go.md`)
- [x] C API reference (`docs/api-c.md`, `tpt_validex.h` doc comments)
- [x] JSON Schema Draft 2020-12 compliance matrix (`docs/compliance.md`)
- [x] Performance comparison page (`docs/performance.md`)
- [x] Migration guide: from `pydantic` / `zod` / `jsonschema` to `tpt-validex` (`docs/migration.md`)

---

## CI/CD

- [x] GitHub Actions: `cargo test` — all crates, all platforms (linux/macos/windows)
- [x] GitHub Actions: `cargo deny check licenses` — runs on every PR
- [x] GitHub Actions: `cargo deny check advisories` — security audit
- [x] GitHub Actions: `cargo clippy -- -D warnings`
- [x] GitHub Actions: `cargo fmt --check`
- [x] Cross-compilation matrix: Linux (x86_64, aarch64), macOS (x86_64, arm64), Windows (x86_64), WASM (release workflow + wasm CI job)
- [x] Publish Python wheels to PyPI (triggered by release tag)
- [x] Publish WASM package to npm (triggered by release tag)
- [x] Benchmark CI: run benchmarks and compare against stored baseline (smoke run + criterion artifacts; stored-baseline comparison is future work)
- [x] Dependabot: automated dependency updates gated by `cargo deny check`

---

## Phase 9: Bugs & Hardening (from 2026-09-30 review)

- [ ] **CSV: schema-driven coercion** — coerce cells by the schema's declared property type instead of sampled inference (fixes `01234` zip / `+4155550123` phone in `type: string` columns being turned into integers); inference only as fallback for untyped properties
- [ ] Add max nesting depth (`max_depth`, default ~128) to the schema reader (`tpt-valid-schema/src/json.rs`) and the validation engine; regression tests with 100k-deep input; verify FFI/WASM cannot stack-overflow
- [ ] `SchemaCache`: replace clear-on-overflow with LRU; key by hash of schema text
- [ ] Remove committed build artifacts (`tests/c_example/example.exe`, `tpt-valid-wasm/pkg/*`) and add to `.gitignore`
- [ ] Document invariants (or replace with `?`/`expect`) for non-test `unwrap()` at `format.rs:125`, `node.rs:217`
- [ ] CSV `strict` option (reject content after closing quote and invalid UTF-8; currently lenient)
- [ ] Fix `dependsRequired` → `dependentRequired` naming in `docs/compliance.md`

## Phase 10: JSON Schema Coverage

- [ ] `$ref` / `$defs` / `definitions` — local `#/...` refs with cycle detection for recursive schemas
- [ ] `$anchor` and `$id`-relative refs; optional user-supplied schema registry for cross-file refs
- [ ] `prefixItems` and tuple-form `items`
- [ ] `dependentRequired` / `dependentSchemas`
- [ ] `minContains` / `maxContains`
- [ ] `propertyNames`
- [ ] `unevaluatedProperties` / `unevaluatedItems`
- [ ] Vendor the official JSON-Schema-Test-Suite (submodule), run in CI with an allow-list of known skips; publish the pass rate in `docs/compliance.md`
- [ ] Additional formats: `phone`, `currency`, `iban`, `country-code`, `semver`, `regex`, `json-pointer`, `duration`, `time`
- [ ] Custom format registration API (Rust, Python, JS, Go)

## Phase 11: CLI (`validex`)

- [ ] New `tpt-valid-cli` crate: `validex check <schema> <data.{json,jsonl,csv}>` with summary output and non-zero exit on failure
- [ ] `--errors <file>`, `--valid <file>`, `--fail-fast`, `--max-errors`, `--format text|json|junit|sarif`
- [ ] `validex infer <data>` — generate a starter schema from sample data (reuse CSV inference)
- [ ] `validex diff old.json new.json` — schema compatibility / breaking-change report
- [ ] `validex watch` mode and `.validex.toml` config mapping globs to schemas
- [ ] Distribution: prebuilt release binaries, `cargo install`, `pipx`, `npx`, Homebrew, Scoop

## Phase 12: Adoption — Examples, Templates, Playground

- [ ] `examples/` with runnable projects (Makefile/`just run` each): FastAPI request validation, Express middleware, pandas/Polars ETL step, Go HTTP handler, browser form validation (WASM), Streamforge pipeline
- [ ] `templates/` schema library with good/bad sample data and README each: contact/CRM import, e-commerce orders, invoices, GeoJSON, log lines, IoT events, OpenAPI request bodies
- [ ] Online playground (static WASM page on GitHub Pages): schema + data panes, live errors, shareable links
- [ ] Official GitHub Action that validates files in a repo and annotates PRs (SARIF)
- [ ] `pre-commit` hook definition
- [ ] Publish schema/meta-schema for editor autocomplete (SchemaStore submission)
- [ ] README: "60-second start" per language; benchmark table vs `jsonschema`, `fastjsonschema`, `ajv` (extend `benches/compare`)
- [ ] Migration guide: side-by-side pydantic / zod / ajv snippets; optional `validex convert --from pydantic|zod|ajv`
- [ ] `CONTRIBUTING.md`, issue templates, `good first issue` labels

## Phase 13: Innovation / Differentiators

- [ ] Friendlier errors: "did you mean" key suggestions, failing schema path, human-readable summaries
- [ ] Error clustering in streaming mode: top failure patterns with counts and example rows
- [ ] Opt-in `coerce` mode (trim, `"1,234"`→`1234`, `yes`→`true`, date normalization) with change report and cleaned-output file
- [ ] Column profiling (null rates, distinct counts) and tighter-schema suggestions
- [ ] LLM structured-output validation helper: validate tool-call JSON and generate repair prompts from errors
- [ ] Data-contract workflow: versioned schemas, compatibility gates in CI

## Existing open items (carried over)

- [ ] Set up GitHub repository and publish Go module (see Phase 0 / Phase 6)
- [ ] Memory usage profiling script for 1M-object RSS (see Benchmarks)
