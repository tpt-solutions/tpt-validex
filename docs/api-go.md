# Go API reference (`github.com/tpt-solutions/tpt-validex-go`)

cgo bindings over the C ABI. See [README in tpt-valid-go/](../tpt-valid-go/README.md)
for build prerequisites (a native `tpt_valid_ffi` library is required).

## `NewValidator(schema string) (*Validator, error)`

Compiles a JSON Schema (Draft 2020-12 subset):

```go
validator, err := validex.NewValidator(`{
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age":  {"type": "integer", "minimum": 0, "maximum": 150}
    },
    "required": ["name", "age"]
}`)
if err != nil {
    return err // includes the schema error from the engine
}
defer validator.Close()
```

## `(*Validator) Validate(data any) (bool, []ValidationError, error)`

Validates one JSON-marshalable value — maps, structs, slices, scalars —
(serialized with `encoding/json`):

```go
type User struct {
    Name string `json:"name"`
    Age  int    `json:"age"`
}

valid, errs, err := validator.Validate(User{Name: "Alice", Age: 30})
```

* `valid` — whether the document passed.
* `errs` — all violations (spec §5.5): each has `Path` (`$.age`), `Message`,
  `Expected`, `Actual`, and implements `error`.
* `err` — non-nil only for infrastructure failures (closed validator,
  marshal errors). Malformed JSON *data* yields `valid == false` with a
  parse error in `errs`, not an `err`.

## `(*Validator) ValidateBatch(batch []any) ([]Result, error)`

Parallel batch validation; results keep input order:

```go
results, err := validator.ValidateBatch([]any{user1, user2, user3})
for _, r := range results {
    if !r.IsValid {
        log.Printf("item %d: %v", r.Index, r.Errors)
    }
}
```

## `(*Validator) Close() error`

Releases native resources. Safe to call twice; use of a closed validator
returns an error rather than crashing.

## Concurrency

A `Validator` is safe for concurrent use by multiple goroutines — the native
engine is thread-safe and lock-free during validation. Compile once at
startup and share it.

## Version

`validex.Version()` returns the native library version string.
