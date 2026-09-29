# tpt-validex-go

[![Go Reference](https://img.shields.io/badge/go.dev-reference-007d9c)](https://pkg.go.dev/github.com/tpt-solutions/tpt-validex-go)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

Go bindings for [tpt-validex](../README.md) — validate millions of records per
second with the same JSON Schema used by the Python, JavaScript, Rust, and C
builds.

**Keywords:** validation · json-schema · validator · cgo · ffi
**Categories:** API bindings · Data validation · Foreign function interface

```go
import "github.com/tpt-solutions/tpt-validex-go"

validator, err := validex.NewValidator(`{
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age":  {"type": "integer", "minimum": 0, "maximum": 150}
    },
    "required": ["name", "age"]
}`)
if err != nil {
    panic(err)
}
defer validator.Close()

valid, errs, err := validator.Validate(map[string]interface{}{
    "name": "Alice",
    "age":  30,
})
```

## API

| Function | Description |
| :--- | :--- |
| `NewValidator(schema string) (*Validator, error)` | Compile a JSON Schema (Draft 2020-12 subset). |
| `(*Validator) Validate(data any) (bool, []ValidationError, error)` | Validate one JSON-marshalable value (map, struct, slice, ...). |
| `(*Validator) ValidateBatch(batch []any) ([]Result, error)` | Validate a batch in parallel; results keep input order. |
| `(*Validator) Close() error` | Release native resources (safe to call twice). |
| `Version() string` | Native library version. |

A `Validator` is safe for concurrent use. `ValidationError` carries
`Path`, `Message`, `Expected`, `Actual` (spec §5.5) and implements `error`.

Errors use the same envelope as every other tpt-validex binding:

```json
{
  "errors": [
    {
      "path": "$.age",
      "message": "Expected value <= 150, got 200",
      "expected": "maximum 150",
      "actual": "200",
      "value": 200
    }
  ]
}
```

## Building

This package uses cgo. Build the native library first:

```sh
cargo build --release -p tpt-valid-ffi
```

Then run tests / build with the flags pointing at the header and library:

```sh
# Linux / macOS
CGO_CFLAGS="-I../tpt-valid-ffi" CGO_LDFLAGS="-L../target/release" go test ./...

# Windows (MSVC or MinGW)
CGO_CFLAGS="-I..\tpt-valid-ffi" CGO_LDFLAGS="-L..\target\release" go test ./...
```

On Windows with MinGW, GNU ld may pick up the MSVC import library
(`tpt_valid_ffi.lib`) and fail. Generate a GNU import library from the DLL
once:

```sh
cd target/release
{ echo "LIBRARY tpt_valid_ffi.dll"; echo "EXPORTS";
  objdump -p tpt_valid_ffi.dll | sed -n '/Ordinal\/Name Pointer] Table/,$p' \
    | awk '$NF ~ /^tpt_valid_[a-z_]+$/ {print $NF}'; } > tpt_valid_ffi.def
dlltool -d tpt_valid_ffi.def -D tpt_valid_ffi.dll -l libtpt_valid_ffi.dll.a
```

And make sure `tpt_valid_ffi.dll` is on `PATH` (or next to the binary) at
runtime.

## Publishing

The module path is `github.com/tpt-solutions/tpt-validex-go`; tag releases as
`vX.Y.Z` (Go modules require the tag on the repository root even though the
code lives in `tpt-valid-go/` — use a `vX.Y.Z` tag combined with the
`//go:build` free layout above, or move this directory to a dedicated
repository when publishing publicly).

## Documentation

- [Go API reference](../docs/api-go.md)
- [Compliance matrix](../docs/compliance.md)
- [C API reference](../docs/api-c.md) — the ABI these bindings wrap
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or
[Apache-2.0](../LICENSE-APACHE), at your option.

