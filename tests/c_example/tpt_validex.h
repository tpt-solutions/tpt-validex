/*
 * tpt_validex.h — C API for tpt-validex, the universal high-performance
 * data validation engine.
 *
 * Build: link against libtpt_valid_ffi (static or dynamic).
 * Thread safety: all functions are thread-safe.
 *
 * Example:
 *     const char* schema = "{\"type\": \"object\", \"properties\": "
 *                          "{\"age\": {\"type\": \"integer\"}}}";
 *     tpt_valid_handle* v = tpt_valid_create(schema);
 *     tpt_valid_result* r = tpt_valid_validate(v, "{\"age\": 25}");
 *     if (tpt_valid_is_valid(r)) { printf("Valid!\n"); }
 *     else { printf("%s\n", tpt_valid_get_errors(r)); }
 *     tpt_valid_free_result(r);
 *     tpt_valid_destroy(v);
 *
 * Licensed under MIT OR Apache-2.0.
 */

#ifndef TPT_VALIDEX_H
#define TPT_VALIDEX_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Opaque compiled validator. Create with tpt_valid_create. */
typedef struct tpt_valid_handle tpt_valid_handle;

/** Opaque result of one validation. Create with tpt_valid_validate. */
typedef struct tpt_valid_result tpt_valid_result;

/**
 * Custom format assertion callback. Return non-zero when `value` is valid.
 * Must be thread-safe: it may be invoked concurrently from batch and
 * streaming validation runs.
 */
typedef int (*tpt_valid_format_cb)(void* user_data, const char* value);

/**
 * Compile a JSON Schema (Draft 2020-12 subset).
 *
 * @param schema  NUL-terminated UTF-8 JSON Schema text; must not be NULL.
 * @return New validator handle, or NULL on failure (see
 *         tpt_valid_last_error). Free with tpt_valid_destroy.
 */
tpt_valid_handle* tpt_valid_create(const char* schema);

/**
 * Register a custom format assertion on a validator.
 *
 * From then on, strings in fields whose schema carries `"format": name`
 * (and which is not a built-in format) are validated by calling
 * `callback(user_data, value)`; a non-zero return accepts the value.
 * Pass NULL as `callback` to unregister `name`. Unregistered custom
 * formats are ignored (annotation semantics).
 *
 * @return 0 on success, 1 on invalid arguments.
 */
int tpt_valid_register_format(tpt_valid_handle* handle,
                              const char* name,
                              tpt_valid_format_cb callback,
                              void* user_data);

/**
 * Validate a JSON document.
 *
 * @param handle Validator from tpt_valid_create; must not be NULL.
 * @param data   NUL-terminated UTF-8 JSON text; must not be NULL.
 * @return Result handle; malformed JSON yields an invalid result with a
 *         parse error (never NULL unless an argument is NULL). Free with
 *         tpt_valid_free_result.
 */
tpt_valid_result* tpt_valid_validate(tpt_valid_handle* handle, const char* data);

/**
 * Whether the validated document passed the schema.
 * A NULL result reports false.
 */
bool tpt_valid_is_valid(const tpt_valid_result* result);

/**
 * Error report as a NUL-terminated JSON string:
 *   {"errors": [{"path": "$.age", "message": "...", "expected": "...",
 *                "actual": "...", "value": ...}, ...]}
 * Empty array when valid. The pointer is owned by the result and remains
 * valid until tpt_valid_free_result.
 */
const char* tpt_valid_get_errors(const tpt_valid_result* result);

/**
 * Thread-local description of the most recent failed call (NULL argument,
 * schema compile failure, encoding error). Valid until the next
 * tpt-validex call on the same thread.
 */
const char* tpt_valid_last_error(void);

/** Library version string, e.g. "0.1.0". */
const char* tpt_valid_version(void);

/** Free a result from tpt_valid_validate. NULL is ignored. */
void tpt_valid_free_result(tpt_valid_result* result);

/** Destroy a validator from tpt_valid_create. NULL is ignored. */
void tpt_valid_destroy(tpt_valid_handle* handle);

#ifdef __cplusplus
}
#endif

#endif /* TPT_VALIDEX_H */
