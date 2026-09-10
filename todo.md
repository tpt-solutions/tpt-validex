# tpt-validex — Task Checklist

> Universal High-Performance Data Validator  
> TPT Solutions · Dual-licensed MIT / Apache-2.0

---

## Phase 0: Project Scaffolding

- [ ] Initialize Cargo workspace (`Cargo.toml` with all member crates)
- [ ] Create `LICENSE-MIT` (TPT Solutions, 2026)
- [ ] Create `LICENSE-APACHE`
- [ ] Create `deny.toml` (cargo-deny license/advisory config per spec §3.2)
- [ ] Create `.gitignore` (Rust + Node + Python artifacts)
- [ ] Create `README.md` (initial: name, tagline, license badges, crate links)
- [ ] Create stub crates with `Cargo.toml` + `src/lib.rs`:
  - [ ] `tpt-valid-core/`
  - [ ] `tpt-valid-schema/`
  - [ ] `tpt-valid-parser/`
  - [ ] `tpt-valid-ffi/`
  - [ ] `tpt-valid-py/`
  - [ ] `tpt-valid-wasm/`
- [ ] Create `tpt-valid-go/` stub (`go.mod` + `validex.go`)
- [ ] Set up GitHub repository

---

## Phase 1: Core Validation Engine (Pure Rust)

### Schema Parser (`tpt-valid-schema`)
- [ ] Implement JSON Schema tokenizer (custom, zero external deps)
- [ ] Implement JSON Schema parser → AST
- [ ] Implement semantic validation of schema (e.g. `min` ≤ `max`)

### Schema Compiler (`tpt-valid-schema`)
- [ ] Compile AST → IR (intermediate representation)
- [ ] IR optimizer (merge redundant checks, pre-compile regex patterns)
- [ ] Compile IR → state machine (tree of `ValidationNode`)
- [ ] Cache compiled state machines for reuse

### Validation Engine (`tpt-valid-core`)
- [ ] Implement `ValidationNode` enum (all node types from spec §5.2):
  - [ ] `CheckType(DataType)`
  - [ ] `CheckField(String, Box<ValidationNode>)`
  - [ ] `CheckRequired(Vec<String>)`
  - [ ] `CheckMinimum(f64)` / `CheckMaximum(f64)`
  - [ ] `CheckPattern(Regex)` — using `regex` crate (MIT/Apache-2.0)
  - [ ] `CheckEnum(Vec<Value>)` — hash set for O(1) lookup
  - [ ] `CheckFormat(Format)`
  - [ ] `CheckArray(Box<ValidationNode>, usize, usize)`
  - [ ] `CheckObject(Vec<(String, ValidationNode)>)`
- [ ] Single-object validation (traverse state machine)
- [ ] Collect all errors (not just first)
- [ ] Structured error output with JSON path (spec §5.5 error format)

### SIMD JSON Parser (`tpt-valid-parser`)
- [ ] Integrate `sonic-rs` (MIT) as primary parser
- [ ] `jiter` (MIT) fallback
- [ ] `serde_json` (MIT/Apache-2.0) guaranteed-compatible baseline fallback

### License Compliance
- [ ] Run `cargo deny check licenses` — zero violations
- [ ] Verify zero Apache-2.0-only dependencies in full transitive tree
- [ ] Verify total crate count < 15

### Tests
- [ ] Unit tests: schema parsing (valid and invalid schemas)
- [ ] Unit tests: state machine compilation
- [ ] Unit tests: basic validation (type, required, min/max)

---

## Phase 2: Advanced Validation Features

### Array & Object Keywords
- [ ] `items` validation
- [ ] `minItems` / `maxItems`
- [ ] `uniqueItems`
- [ ] `additionalProperties`
- [ ] `patternProperties`

### Format Validators (all built from scratch, zero external deps)
- [ ] Email — custom RFC 5322 validator
- [ ] URI/URL — custom RFC 3986 validator
- [ ] Date — ISO 8601 (using `chrono`, MIT/Apache-2.0)
- [ ] Date-Time — ISO 8601 (using `chrono`)
- [ ] UUID — custom RFC 4122 validator
- [ ] IPv4 — custom validator
- [ ] IPv6 — custom validator

### Enum & Logical Keywords
- [ ] `enum` validation (hash set, O(1) lookup)
- [ ] `if` / `then` / `else` conditional validation
- [ ] `allOf`
- [ ] `anyOf`
- [ ] `oneOf`
- [ ] `not`

### Compliance
- [ ] JSON Schema Draft 2020-12 compliance test suite
- [ ] Document supported vs. unsupported keywords (compliance matrix)

---

## Phase 3: Streaming & Batch Validation

- [ ] Batch validation — parallel via `rayon` (MIT/Apache-2.0)
- [ ] Custom streaming CSV parser (scratch, zero external deps):
  - [ ] Quoted fields
  - [ ] Custom delimiters
  - [ ] Type inference per column
  - [ ] Per-row validation with error collection
- [ ] Custom streaming JSONL parser (scratch, zero external deps):
  - [ ] Line-by-line validation
  - [ ] Error reporting with line numbers
- [ ] "Fail-fast" mode (stop on first error, for performance-critical paths)
- [ ] Tests: batch validation correctness and parallelism
- [ ] Tests: streaming CSV and JSONL validation

---

## Phase 4: FFI & Python Wrapper

### C ABI (`tpt-valid-ffi`)
- [ ] `tpt_valid_create(schema: *const c_char) -> *mut tpt_valid_handle`
- [ ] `tpt_valid_validate(handle, data) -> *mut tpt_valid_result`
- [ ] `tpt_valid_is_valid(result) -> bool`
- [ ] `tpt_valid_get_errors(result) -> *const c_char`
- [ ] `tpt_valid_free_result(result)`
- [ ] `tpt_valid_destroy(handle)`
- [ ] `tpt_validex.h` header file

### Python Bindings (`tpt-valid-py`, PyO3)
- [ ] `Validator` class
- [ ] `validate(data) -> (bool, list[dict])` — single object
- [ ] `validate_batch(batch) -> list[Result]` — parallel
- [ ] `validate_csv(path, valid_output, errors_output)` — streaming
- [ ] `maturin` build configuration (`pyproject.toml`)

### Publishing & CI
- [ ] GitHub Actions: build Python wheels for:
  - [ ] Linux x86_64 (manylinux)
  - [ ] Linux aarch64 (manylinux)
  - [ ] macOS x86_64
  - [ ] macOS arm64 (Apple Silicon)
  - [ ] Windows x86_64
- [ ] Publish wheels to PyPI on release tag

### Tests & Docs
- [ ] Python tests (`pytest`) — validate, validate_batch, validate_csv
- [ ] Python API documentation
- [ ] Python usage examples (FastAPI, ETL pipeline)

---

## Phase 5: WASM Wrapper

- [ ] `tpt-valid-wasm` (wasm-bindgen, MIT/Apache-2.0):
  - [ ] `Validator` class
  - [ ] `validate(data) -> { isValid, errors }`
  - [ ] `validateBatch(batch) -> Result[]`
- [ ] Optimize binary size with `wasm-opt -Oz` (target < 5 MB)
- [ ] TypeScript type definitions (`.d.ts`)
- [ ] `package.json` for npm package `tpt-validex`
- [ ] GitHub Actions: build WASM + publish to npm on release tag
- [ ] JS/TS tests
- [ ] JavaScript documentation and examples (Node.js, browser form validation)

---

## Phase 6: Go Wrapper

- [ ] `tpt-valid-go` (cgo):
  - [ ] `NewValidator(schema string) (*Validator, error)`
  - [ ] `Validate(data map[string]interface{}) (bool, []ValidationError)`
  - [ ] `ValidateBatch(batch []map[string]interface{}) []Result`
- [ ] `go.mod` (module: `github.com/tpt-solutions/tpt-validex-go`)
- [ ] Publish to Go module registry
- [ ] Go tests
- [ ] Go documentation and examples

---

## Phase 7: tpt-streamforge Integration

> **Blocked:** `tpt-streamforge` does not exist yet. These tasks are gated on that project being built.

- [ ] Define integration interface (trait/API boundary) between `tpt-validex` and `tpt-streamforge`
- [ ] Implement `validate` transformation stage for `tpt-streamforge` pipelines
- [ ] Error output: invalid rows written to separate `.jsonl` file
- [ ] Configurable error handling modes: `skip` / `abort` / `log`
- [ ] Integration tests (pipeline: read CSV → validate → write valid + errors)
- [ ] Pipeline usage examples (spec §5.6 and §13 Use Case 2)

---

## Phase 8: Custom DSL (`validex::schema!` macro)

- [ ] Design DSL syntax (per spec §5.1 example)
- [ ] DSL tokenizer
- [ ] DSL parser → AST
- [ ] Implement proc macro crate (`validex-macros` or inline in `tpt-valid-schema`)
- [ ] DSL AST → shared IR (same compiler pipeline as JSON Schema)
- [ ] `validex::schema!` macro usable from Rust consumer crates
- [ ] DSL tests (round-trip: DSL → state machine → validation)
- [ ] DSL documentation and examples

---

## Benchmarks

- [ ] Set up `criterion` benchmark suite (`benches/` in workspace)
- [ ] Benchmark: single-object validation throughput (target: > 5M obj/sec)
- [ ] Benchmark: batch validation throughput (parallel)
- [ ] Benchmark: streaming CSV validation throughput (target: > 10M rows/sec)
- [ ] Benchmark: schema compilation time (target: < 10 ms)
- [ ] Comparison benchmark vs `pydantic` (Python) — same dataset
- [ ] Comparison benchmark vs `zod` (JavaScript) — same dataset
- [ ] Comparison benchmark vs `jsonschema` (Python) — same dataset
- [ ] Memory usage profiling (target: < 50 MB for 1M objects)
- [ ] Binary size tracking — native (target: < 10 MB), WASM (target: < 5 MB)

---

## Documentation

- [ ] `README.md`: installation, quick start, all-language examples, license badges
- [ ] `rustdoc` for all public APIs in all crates
- [ ] Python API reference
- [ ] JavaScript / TypeScript API reference
- [ ] Go API reference
- [ ] C API reference (`tpt_validex.h` doc comments)
- [ ] JSON Schema Draft 2020-12 compliance matrix (supported/unsupported keywords)
- [ ] Performance comparison page (vs pydantic, zod, jsonschema, ajv)
- [ ] Migration guide: from `pydantic` / `zod` / `jsonschema` to `tpt-validex`

---

## CI/CD

- [ ] GitHub Actions: `cargo test` — all crates, all platforms
- [ ] GitHub Actions: `cargo deny check licenses` — runs on every PR
- [ ] GitHub Actions: `cargo deny check advisories` — security audit
- [ ] GitHub Actions: `cargo clippy -- -D warnings`
- [ ] GitHub Actions: `cargo fmt --check`
- [ ] Cross-compilation matrix: Linux (x86_64, aarch64), macOS (x86_64, arm64), Windows (x86_64), WASM
- [ ] Publish Python wheels to PyPI (triggered by release tag)
- [ ] Publish WASM package to npm (triggered by release tag)
- [ ] Benchmark CI: run benchmarks and compare against stored baseline
- [ ] Dependabot: automated dependency updates gated by `cargo deny check`
