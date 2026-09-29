# tpt-valid-ffi

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

**The stable C ABI: opaque handles, error codes, plain C types.**

`tpt-valid-ffi` is the C boundary of
[tpt-validex](https://github.com/tpt-solutions/tpt-validex). It exposes the
engine through opaque handles and a small, stable, thread-safe C API declared in
[`tpt_validex.h`](tpt_validex.h). This is what the Go bindings
([`tpt-valid-go`](../tpt-valid-go)) link against, and it is the way to embed the
validator in any language with a C FFI.

It builds as both a `cdylib` and a `staticlib`, so you can link it dynamically
or statically.

## Build

```sh
cargo build --release -p tpt-valid-ffi
```

This produces (in `target/release`):

| Platform | Artifacts |
| :--- | :--- |
| Linux | `libtpt_valid_ffi.so` |
| macOS | `libtpt_valid_ffi.dylib` |
| Windows (MSVC) | `tpt_valid_ffi.dll` + `tpt_valid_ffi.dll.lib` |

Include [`tpt_validex.h`](tpt_validex.h) in your project (a copy also ships in
[`tests/c_example/`](../tests/c_example/)).

## Usage

```c
#include "tpt_validex.h"
#include <stdio.h>

int main(void) {
    const char* schema =
        "{\"type\": \"object\","
        " \"properties\": {\"age\": {\"type\": \"integer\", \"minimum\": 0}},"
        " \"required\": [\"age\"]}";

    tpt_valid_handle* v = tpt_valid_create(schema);
    if (v == NULL) {
        fprintf(stderr, "schema error: %s\n", tpt_valid_last_error());
        return 1;
    }

    tpt_valid_result* r = tpt_valid_validate(v, "{\"age\": 25}");
    if (tpt_valid_is_valid(r)) {
        printf("Valid!\n");
    } else {
        printf("%s\n", tpt_valid_get_errors(r));   // {"errors":[...]}
    }

    tpt_valid_free_result(r);
    tpt_valid_destroy(v);
    return 0;
}
```

A runnable end-to-end example lives in
[`tests/c_example/`](../tests/c_example/) — see its
[README](../tests/c_example/README.md) for per-platform build commands.

## API

| Function | Behavior |
| :--- | :--- |
| `tpt_valid_create(const char* schema)` | Compiles a JSON Schema (Draft 2020-12 subset). Returns `NULL` on failure — call `tpt_valid_last_error()` for the reason. Free with `tpt_valid_destroy`. |
| `tpt_valid_validate(handle, const char* data)` | Validates one JSON document. Malformed JSON yields an **invalid result** carrying a parse error (never `NULL` unless an argument is `NULL`). Free with `tpt_valid_free_result`. |
| `tpt_valid_is_valid(result)` | `true` when the document passed. `NULL` → `false`. |
| `tpt_valid_get_errors(result)` | The error report as a JSON string: `{"errors":[{"path","message","expected","actual","value"}, ...]}` — an empty array when valid. |
| `tpt_valid_last_error()` | Thread-local description of the most recent failed call (NULL argument, schema compile failure, encoding error). |
| `tpt_valid_version()` | Library version string, e.g. `"0.1.0"`. |
| `tpt_valid_free_result(result)` | Frees a result. `NULL` is ignored. |
| `tpt_valid_destroy(handle)` | Frees a validator. `NULL` is ignored. |

Two opaque types are declared for C: `tpt_valid_handle` (a compiled validator)
and `tpt_valid_result` (one validation outcome).

## API contract

- **Thread safety.** All functions are thread-safe. A single `tpt_valid_handle`
  may be shared across threads.
- **Lifetime.** Handles are created by `tpt_valid_create` and destroyed by
  `tpt_valid_destroy`; results by `tpt_valid_validate`, freed by
  `tpt_valid_free_result`. Never free either twice.
- **Error reporting.** A `NULL` return means "invalid argument" or "the schema
  failed to compile". The reason is available from `tpt_valid_last_error()`,
  which is thread-local.
- **String ownership.** Strings returned by `tpt_valid_get_errors` and
  `tpt_valid_last_error` are owned by their result/handle (or thread-local
  storage) and stay valid until the owning object is freed, or until the same
  thread makes another tpt-validex call.
- **Null-safety.** Passing `NULL` for any argument is handled gracefully: it
  yields a `NULL` return or `false`, never a crash.

## Error format

Errors use the same envelope as every other tpt-validex binding (spec §5.5):

```json
{
  "errors": [
    {
      "path": "$.age",
      "message": "Expected integer, got string",
      "expected": "integer",
      "actual": "string",
      "value": "25"
    }
  ]
}
```

## Consuming from other languages

- **Go** — see [`tpt-valid-go`](../tpt-valid-go) (cgo) for a ready-made wrapper.
- **C++** — the header is `extern "C"`-guarded, so it works unchanged.
- **Anything else** — link the staticlib and declare the prototypes, or generate
  bindings from the header.

## Testing

```sh
cargo test -p tpt-valid-ffi
cargo clippy -p tpt-valid-ffi --all-targets -- -D warnings
cargo fmt --check
```

Rust unit tests exercise the create/validate/destroy round-trip, invalid schema
handling, malformed-JSON data, and null-argument safety through the C surface.

## Related crates

| Crate | Relationship |
| :--- | :--- |
| [`tpt-valid-core`](../tpt-valid-core) | The engine behind this boundary. |
| [`tpt-valid-schema`](../tpt-valid-schema) | Schema compilation used by `tpt_valid_create`. |
| [`tpt-valid-parser`](../tpt-valid-parser) | JSON parsing for the data passed to `tpt_valid_validate`. |
| [`tpt-valid-go`](../tpt-valid-go) | Go (cgo) bindings that link this library. |

## Documentation

- [C API reference](../docs/api-c.md)
- [Runnable C example](../tests/c_example/)
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option. `#![allow(unsafe_code)]` is scoped to this crate only — the FFI
boundary requires it by definition, and every other crate in the workspace
forbids `unsafe`.
