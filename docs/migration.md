# Migrating to tpt-validex

One schema, every language. This guide maps the mental models and APIs of
`pydantic`, `zod`, and `jsonschema` to `tpt-validex`.

## From `jsonschema` (Python)

The closest match — you already have JSON Schema documents:

```python
# Before
from jsonschema import Draft202012Validator
validator = Draft202012Validator(schema)
errors = list(validator.iter_errors(data))          # ~45k obj/s

# After
from tpt_validex import Validator
validator = Validator(schema)                        # compiled once
is_valid, errors = validator.validate(data)          # ~313k obj/s single, ~650k batch
```

Differences to know:

* **Errors are flat dicts**, not `ValidationError` trees:
  `{"path": "$.address.zip", "message": "...", "expected": "...", "actual": "...", "value": ...}`.
* The instance must be **JSON-shaped** (dicts/lists/str/int/float/bool/None) —
  which is what `json.load` produces anyway.
* `$ref` is not supported — inline your definitions (see
  [docs/compliance.md](compliance.md)).
* Keyword coverage is a documented subset; unsupported constructs are
  rejected loudly at compile time rather than silently misbehaving.

## From `pydantic` (Python)

pydantic models *describe a class*; tpt-validex validates *documents against
a schema*. Where pydantic wins (typed models, single fast objects), keep it.
Switch to tpt-validex when you need:

* **Bulk validation** of records from JSON files, queues, or ETL pipelines
  (`validate_batch`, `validate_csv`, `validate_jsonl`).
* **The same schema in several languages** (Go microservice + Python ETL +
  browser form).
* **Structured error paths** for arbitrary documents, not just your models.

```python
# Before (pydantic)
from pydantic import BaseModel
class User(BaseModel):
    name: str
    age: int

User.model_validate(data)

# After (tpt-validex)
from tpt_validex import Validator
user_validator = Validator({
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age": {"type": "integer", "minimum": 0},
    },
    "required": ["name", "age"],
})
is_valid, errors = user_validator.validate(data)
```

Note tpt-validex does **not** coerce or transform data ("25" stays a string
and fails `type: integer`); it validates, it doesn't parse. Feed it
`json.loads` output, not untrusted strings you expected parsed.

## From `zod` (JavaScript)

```js
// Before (zod)
import { z } from 'zod';
const User = z.object({
  name: z.string().min(1),
  age: z.number().int().min(0),
});
const result = User.safeParse(data);   // result.success, result.error

// After (tpt-validex, WASM)
import init, { Validator } from 'tpt-validex';
await init();
const userValidator = new Validator({
  type: 'object',
  properties: {
    name: { type: 'string', minLength: 1 },
    age: { type: 'integer', minimum: 0 },
  },
  required: ['name', 'age'],
});
const { isValid, errors } = userValidator.validate(data);
```

Trade-offs: zod is faster for pure in-process JS; tpt-validex gives you the
*same schema* as your Python/Go/Rust services, JSON Schema compatibility, and
a 1.36 MB wasm bundle. Raw JS objects in, plain objects out — no schema
object chaining required.

If you already have zod schemas, they can be expressed in tpt-validex
mechanically: `z.string().min(1)` → `{"type": "string", "minLength": 1}`,
`z.number().int().min(0).max(150)` → `{"type": "integer", "minimum": 0,
"maximum": 150}`, `.optional()` → leave the property out of `required`.

## From `valico` / `jsonschema` (Rust)

```rust
// Before (a JSON Schema crate with a large dependency tree)
let schema = jsonschema::JSONSchema::compile(&schema_json)?;
let result = schema.validate(&instance);

// After
use validex::Validator;
let validator = Validator::new(schema_text)?;       // or Validator::cached(...)
let report = validator.validate(&instance);          // Vec<ValidationError>
assert!(report.is_valid());
```

Plus what no JSON Schema crate here offers:

```rust
// Compile-time DSL with the same engine
let v = validex::schema! {
    object {
        required "name" => string(min_length = 1),
        required "age"  => integer(min = 0, max = 150),
    }
};

// Streaming validation of CSV/JSONL without loading the file
let stats = validator.validate_csv(reader, &Default::default(), &Default::default())?;
```

## Common concepts map

| Concept | pydantic | zod | jsonschema | tpt-validex |
| :--- | :--- | :--- | :--- | :--- |
| Define schema | class + type hints | `z.object({...})` | JSON document | JSON document or `schema!` DSL |
| Validate one | `Model(...)` | `.safeParse()` | `.iter_errors()` | `.validate()` |
| Validate batch | loop | loop | loop | `.validate_batch()` (parallel) |
| Validate stream | — | — | — | `.validate_csv()` / `.validate_jsonl()` |
| Error shape | exception list | `ZodError` | nested tree | flat dicts with JSON paths |
| Coercion | yes (`str→int`) | opt-in | no | no |
