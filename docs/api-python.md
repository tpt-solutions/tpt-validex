# Python API reference (`tpt-validex` on PyPI)

Install:

```sh
pip install tpt-validex
```

Wheels are built for Linux (x86_64, aarch64), macOS (x86_64, arm64), and
Windows x86_64. Python ≥ 3.8. Fully typed (`py.typed`).

## `Validator`

```python
from tpt_validex import Validator

validator = Validator(schema)          # dict / bool / JSON string
validator = Validator.from_json(text)  # JSON string explicitly
```

Compiles the JSON Schema (Draft 2020-12 subset — see
[compliance.md](compliance.md)) once. A `Validator` is thread-safe; compile
it at import time and share it.

### `validate(data) -> (is_valid: bool, errors: list[dict])`

Validates one JSON-like value (dict / list / str / int / float / bool / None).
Errors are dicts with keys `path`, `message`, `expected`, `actual`, `value`
(spec §5.5):

```python
is_valid, errors = validator.validate({"name": "", "age": 200})
# is_valid == False
# errors == [
#   {"path": "$.name", "message": "Expected string length >= 1, got 0", ...},
#   {"path": "$.age",  "message": "Expected value <= 150, got 200", ...},
# ]
```

Performance note: dict/list inputs take a fast path through Python's C
`json.dumps` + the Rust parser; the streaming APIs are the highest-throughput
options for bulk data.

### `is_valid(data) -> bool`

Boolean-only validation (fail-fast, no error materialization). Slightly
faster than `validate` when you don't need the errors.

### `validate_batch(batch: list) -> list[ValidationResult]`

Validates all items **in parallel** (rayon). Returns one `ValidationResult`
per item, in input order:

```python
results = validator.validate_batch(records)
for r in results:
    if not r.is_valid:
        print(r.index, r.errors())
```

`ValidationResult.index: int`, `.is_valid: bool`, `.errors() -> list[dict]`.

### `validate_csv(input_path, valid_output=None, errors_output=None, *, delimiter=",", has_headers=True) -> dict`

Stream-validates a CSV file with **bounded memory** (rows are never all
buffered). Column values are type-inferred (`bool`/`integer`/`number`/
`string`; empty cells → `null`) from the first 1000 rows.

* `valid_output`: when given, valid rows are written as CSV (input column
  order preserved, re-quoted as needed).
* `errors_output`: when given, invalid rows are appended as JSONL:
  `{"line": <1-based row>, "row": {...}, "errors": [...]}`.
* Returns `{"total_rows", "valid_rows", "invalid_rows", "parse_errors"}`.

```python
stats = validator.validate_csv("input.csv", valid_output="valid.csv",
                               errors_output="errors.jsonl")
```

CSV schemas map column names to object properties. Type inference means a
column with any non-numeric value becomes `string` — write your schema to
match (or clean the data).

### `validate_jsonl(input_path, errors_output=None) -> dict`

Stream-validates newline-delimited JSON. Each line is independent; blank
lines are skipped; parse failures are reported as invalid lines. Errors file
lines: `{"line": N, "row": ..., "errors": [...]}` (parse errors:
`{"line": N, "error": "..."}`).

### `warnings() -> list[str]`

Non-fatal schema-compilation warnings (e.g. unknown `format` keywords,
which JSON Schema says to ignore).

## Complete examples

### FastAPI request validation (spec §13 Use Case 1)

```python
from fastapi import FastAPI, HTTPException
from tpt_validex import Validator

app = FastAPI()

user_schema = Validator({
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age": {"type": "integer", "minimum": 0, "maximum": 150},
        "email": {"type": "string", "format": "email"},
    },
    "required": ["name", "age"],
})

@app.post("/users")
async def create_user(request: dict):
    is_valid, errors = user_schema.validate(request)
    if not is_valid:
        raise HTTPException(status_code=422, detail={"errors": errors})
    # ... create the user
    return {"ok": True}
```

### ETL pipeline with error quarantine (spec §13 Use Case 2)

```python
from tpt_validex import Validator

row_schema = Validator({
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age": {"type": "integer", "minimum": 0},
    },
    "required": ["name", "age"],
})

stats = row_schema.validate_csv(
    "users.csv",
    valid_output="valid_users.csv",
    errors_output="invalid_users.jsonl",
)
print(f"{stats['valid_rows']}/{stats['total_rows']} rows kept")
```

## Building from source

```sh
cd tpt-valid-py
python -m venv .venv && .venv/Scripts/pip install maturin pytest  # (activate first)
maturin develop --release
pytest
```
