# tpt-streamforge pipeline with a validate stage

Reads a CSV stream, validates every row against a compiled schema, and
routes invalid rows to a JSONL reject file — all streaming, O(1) memory per
row.

## Run

```sh
cargo run -p tpt-validex-streamforge --example pipeline   # from the repo root
```

The stage itself (`Validate` over `RecordBatch`es, `ErrorMode::Skip` /
`Abort` / `Log`) is documented in `docs/api-streamforge.md`.
