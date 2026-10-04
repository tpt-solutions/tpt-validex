# Changelog

All notable changes to `tpt-valid-cli` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers are shared across the whole
[tpt-validex](https://github.com/tpt-solutions/tpt-validex) workspace.

## [Unreleased]

### Added

- Initial `validex` CLI:
  - `check` — validate `json` / `jsonl` / `csv` with text, JSON, JUnit and
    SARIF output; `--errors` / `--valid` record sinks, `--fail-fast`,
    `--max-errors`, `--delimiter` / `--no-headers`, `--registry` for
    cross-file `$ref`, `--coerce` (opt-in value coercion before
    validation), `--top-errors` (failure-pattern clustering).
  - `infer` — starter schema from sample data (JSON/JSONL/CSV).
  - `diff` — schema compatibility / breaking-change report (CI gate).
  - `profile` — column null rates, distinct counts, type mixes and
    tighter-schema suggestions.
  - `watch` — `.validex.toml`-driven continuous validation (mtime polling,
    dependency-free glob matching).
  - `memcheck` — internal helper reporting the process peak RSS for
    `scripts/profile_memory.py`.
