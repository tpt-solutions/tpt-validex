# C API reference (`tpt_validex.h`)

Header: [`tpt-valid-ffi/tpt_validex.h`](../tpt-valid-ffi/tpt_validex.h)
(also shipped in [tests/c_example/](../tests/c_example/)). Link against
`libtpt_valid_ffi` (built with `cargo build --release -p tpt-valid-ffi`).

All functions are thread-safe. Strings returned by `tpt_valid_get_errors`
and `tpt_valid_last_error` are owned by their result / thread-local storage
and remain valid until the owning object is freed or the same thread makes
another tpt-validex call.

## Lifecycle

```c
tpt_valid_handle* tpt_valid_create(const char* schema);
void tpt_valid_destroy(tpt_valid_handle* handle);

tpt_valid_result* tpt_valid_validate(tpt_valid_handle* handle, const char* data);
bool tpt_valid_is_valid(const tpt_valid_result* result);
const char* tpt_valid_get_errors(const tpt_valid_result* result);
void tpt_valid_free_result(tpt_valid_result* result);
```

| Function | Behavior |
| :--- | :--- |
| `tpt_valid_create` | Compiles a JSON Schema (Draft 2020-12 subset) from NUL-terminated UTF-8 text. Returns NULL on failure — call `tpt_valid_last_error()` for the reason. |
| `tpt_valid_validate` | Validates one JSON document. Malformed JSON yields an **invalid result** carrying a parse error (never NULL unless an argument is NULL). |
| `tpt_valid_is_valid` | True when the document passed. NULL → false. |
| `tpt_valid_get_errors` | Error report as JSON: `{"errors":[{"path","message","expected","actual","value"}]}` — empty array when valid. |
| `tpt_valid_free_result` | Frees a result. NULL ignored. |
| `tpt_valid_destroy` | Frees a validator. NULL ignored. |

## Diagnostics

```c
const char* tpt_valid_last_error(void); // thread-local reason for the last failed call
const char* tpt_valid_version(void);    // e.g. "0.1.0"
```

## Minimal example

```c
#include "tpt_validex.h"
#include <stdio.h>

int main(void) {
    tpt_valid_handle* v = tpt_valid_create(
        "{\"type\": \"object\", \"properties\": {\"age\": {\"type\": \"integer\"}}}");
    if (!v) { fprintf(stderr, "%s\n", tpt_valid_last_error()); return 1; }

    tpt_valid_result* r = tpt_valid_validate(v, "{\"age\": 25}");
    if (tpt_valid_is_valid(r)) {
        printf("Valid!\n");
    } else {
        printf("Invalid: %s\n", tpt_valid_get_errors(r));
    }
    tpt_valid_free_result(r);
    tpt_valid_destroy(v);
    return 0;
}
```

Compile-and-run instructions for Linux/macOS/Windows:
[tests/c_example/README.md](../tests/c_example/README.md).

## Error-handling contract

* NULL arguments never crash: they return NULL/false and set the
  thread-local last error.
* Panic safety: the Rust side catches panics at the FFI boundary and
  reports them through `tpt_valid_last_error` instead of unwinding into C.
