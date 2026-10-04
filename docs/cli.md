# The `validex` CLI

`tpt-valid-cli` ships the `validex` command: validate data files against a
JSON Schema (Draft 2020-12 subset), infer starter schemas from sample data,
diff schema versions for breaking changes, and continuously validate on file
changes. Zero runtime dependencies beyond the Rust core.

## Install

```sh
cargo install tpt-valid-cli
# or download a prebuilt binary from a GitHub release (linux/macOS/Windows)
```

## `validex check`

```sh
validex check schema.json data.json            # single document or array
validex check schema.json records.jsonl        # newline-delimited JSON
validex check schema.json table.csv            # streaming CSV
```

Exit codes: `0` all records valid · `1` validation failures · `2` usage,
I/O or schema errors.

Options:

| Flag | Effect |
| :--- | :--- |
| `--errors <file>` | invalid records as JSONL: `{"line", "row", "errors"}` |
| `--valid <file>` | valid records (JSONL for json/jsonl; re-quoted CSV for csv) |
| `--fail-fast` | stop at the first failing record |
| `--max-errors <N>` | cap errors collected per record (default 1000) |
| `--format text\|json\|junit\|sarif` | output format (`sarif` annotates PRs on GitHub) |
| `--delimiter <c>` / `--no-headers` | CSV dialect |
| `--registry <file>` | JSON object mapping URIs → schema documents for cross-file `$ref` |
| `--quiet` | summary line only |

A top-level JSON **array** is validated item-by-item (labels carry the
index), so `validex check user.schema.json users.json` reports which array
elements fail.

## `validex infer`

Generate a starter schema from sample data:

```sh
validex infer sample.csv > schema.json
validex infer records.jsonl --output schema.json
```

Scalar types are inferred per key/column (integer/number/string/boolean,
`null` alternatives for columns with empty cells), nested objects are merged
recursively, and keys present in every sampled record become `required`.

## `validex diff`

Compatibility report between two schema versions:

```sh
validex diff v1.schema.json v2.schema.json            # text
validex diff v1.schema.json v2.schema.json --format json
```

Reports `BREAKING` changes — required keys added, type narrowed, bounds
tightened, `enum` shrunk, `const` changed, `additionalProperties: false`
added — plus informational notes (new optional properties, loosened bounds).
Exit `1` when any breaking change is detected, so it can gate a CI pipeline
(data-contract workflow).

## `validex watch`

Continuously validate files mapped to schemas by a `.validex.toml` config:

```toml
interval_ms = 500

[[watch]]
schema = "schemas/user.schema.json"
files = ["data/users/*.jsonl", "data/users/*.csv"]
```

```sh
validex watch                 # reads .validex.toml in the current directory
validex watch --config other.toml
```

Globs support `*` (within a path component), `**` (across components) and
`?`; polling is mtime-based with no external dependencies.

## Data-contract workflow

Treat schema files as versioned contracts and gate changes in CI:

```yaml
# .github/workflows/contract.yml
- uses: actions/checkout@v4
- run: cargo install tpt-valid-cli
- name: Check the contract for breaking changes
  run: validex diff schemas/v1.json schemas/v2.json --format text
- name: Validate sample data against the new contract
  run: validex check schemas/v2.json samples/records.jsonl --format sarif > results.sarif
```

`validex diff` exits `1` on breaking changes, blocking the PR; the SARIF
output annotates failing records directly in the GitHub UI when uploaded
with `github/codeql-action/upload-sarif`.

## LLM structured-output repair

`tpt-validex.repair_prompt(schema, data, errors)` (Python) and
`tpt_valid_core::repair_prompt` (Rust) render a failed validation into a
ready-to-send prompt: the schema, the rejected document, every error with
its JSON path, and any "did you mean" hints — so a model can emit a
corrected document in one shot.

```python
from tpt_validex import Validator, repair_prompt

is_valid, errors = validator.validate(tool_call_arguments)
if not is_valid:
    prompt = repair_prompt(schema_text, raw_text, errors)
    fixed = llm.complete(prompt)   # -> corrected JSON
```

