//! # tpt-valid-ffi
//!
//! Stable C ABI for `tpt-validex`: opaque handles, error codes and simple
//! C types (spec §6.4). Consumed by the Go wrapper (`tpt-valid-go`), C/C++
//! users, and any language with a C FFI. Declarations live in
//! `tpt_validex.h`.
//!
//! # API contract
//!
//! * All functions are thread-safe unless documented otherwise.
//! * Handles are created by [`tpt_valid_create`] and destroyed by
//!   [`tpt_valid_destroy`]; results by [`tpt_valid_validate`], freed by
//!   [`tpt_valid_free_result`].
//! * NULL return means "invalid arguments" or "schema failed to compile";
//!   the reason is available via [`tpt_valid_last_error`] (thread-local).
//! * Strings returned by `get_errors` / `last_error` are owned by their
//!   result/handle (or thread-local storage) and stay valid until the
//!   owning object is freed or the same thread calls the API again.
//!
//! See `LICENSE-MIT` and `LICENSE-APACHE` for licensing terms.

#![allow(unsafe_code)] // FFI boundary requires unsafe by definition

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::panic::catch_unwind;
use std::sync::OnceLock;

use tpt_valid_schema::Validator;

/// Opaque validator handle (see `tpt_validex.h`).
#[repr(C)]
pub struct tpt_valid_handle {
    _private: [u8; 0],
}

/// Opaque validation result (see `tpt_validex.h`).
#[repr(C)]
pub struct tpt_valid_result {
    _private: [u8; 0],
}

struct ValidatorBox {
    validator: Validator,
}

struct ResultBox {
    valid: bool,
    /// `{"errors": [...]}` per spec §5.5.
    errors_json: CString,
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error(message: impl Into<String>) {
    let c = CString::new(message.into())
        .unwrap_or_else(|_| CString::new("invalid error text").unwrap());
    LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(c));
}

fn take_last_error_cstr() -> *const c_char {
    LAST_ERROR.with(|slot| {
        let mut borrow = slot.borrow_mut();
        match borrow.as_ref() {
            Some(c) => c.as_ptr(),
            None => {
                let empty = CString::new(String::new()).unwrap();
                let ptr = empty.as_ptr();
                // Keep an owned copy alive in the slot for pointer stability.
                *borrow = Some(empty);
                ptr
            }
        }
    })
}

const NULL_MSG: &str = "argument pointer was NULL";

/// Create a validator from a JSON Schema string.
///
/// Returns NULL when `schema` is NULL or fails to compile (see
/// `tpt_valid_last_error`). Free with `tpt_valid_destroy`.
///
/// # Safety
/// `schema` must be a valid NUL-terminated C string pointer or NULL.
#[no_mangle]
pub unsafe extern "C" fn tpt_valid_create(schema: *const c_char) -> *mut tpt_valid_handle {
    let out = catch_unwind(|| {
        if schema.is_null() {
            set_last_error(NULL_MSG);
            return std::ptr::null_mut();
        }
        let schema_str = match CStr::from_ptr(schema).to_str() {
            Ok(s) => s,
            Err(e) => {
                set_last_error(format!("schema is not valid UTF-8: {e}"));
                return std::ptr::null_mut();
            }
        };
        match Validator::new(schema_str) {
            Ok(v) => {
                Box::into_raw(Box::new(ValidatorBox { validator: v })) as *mut tpt_valid_handle
            }
            Err(e) => {
                set_last_error(e.to_string());
                std::ptr::null_mut()
            }
        }
    });
    out.unwrap_or_else(|_| {
        set_last_error("internal panic while compiling schema");
        std::ptr::null_mut()
    })
}

/// Validate a JSON document against the validator's schema.
///
/// Returns a result describing validity and (when invalid) all errors as a
/// JSON object `{"errors": [...]}`. Malformed JSON data yields an invalid
/// result with a parse error, not NULL. Returns NULL only for NULL args.
/// Free with `tpt_valid_free_result`.
///
/// # Safety
/// `handle` must come from `tpt_valid_create` (not yet destroyed); `data`
/// must be a valid NUL-terminated C string pointer or NULL.
#[no_mangle]
pub unsafe extern "C" fn tpt_valid_validate(
    handle: *mut tpt_valid_handle,
    data: *const c_char,
) -> *mut tpt_valid_result {
    let out = catch_unwind(|| {
        if handle.is_null() || data.is_null() {
            set_last_error(NULL_MSG);
            return std::ptr::null_mut();
        }
        let validator = &(*(handle as *const ValidatorBox)).validator;
        let data_str = match CStr::from_ptr(data).to_str() {
            Ok(s) => s,
            Err(e) => {
                set_last_error(format!("data is not valid UTF-8: {e}"));
                return std::ptr::null_mut();
            }
        };
        let (valid, errors_json) = match tpt_valid_parser::parse(data_str) {
            Ok(value) => {
                let report = validator.validate(&value);
                (report.is_valid(), report.to_json_string())
            }
            Err(e) => {
                let report = serde_json::json!({
                    "errors": [{
                        "path": "$",
                        "message": format!("invalid JSON data: {e}"),
                        "expected": "valid JSON",
                        "actual": "parse error",
                    }]
                });
                (false, report.to_string())
            }
        };
        let errors_json =
            CString::new(errors_json).unwrap_or_else(|_| CString::new("{\"errors\":[]}").unwrap());
        Box::into_raw(Box::new(ResultBox { valid, errors_json })) as *mut tpt_valid_result
    });
    out.unwrap_or_else(|_| {
        set_last_error("internal panic during validation");
        std::ptr::null_mut()
    })
}

/// Whether the validated document passed the schema.
///
/// # Safety
/// `result` must come from `tpt_valid_validate` (not yet freed).
#[no_mangle]
pub unsafe extern "C" fn tpt_valid_is_valid(result: *const tpt_valid_result) -> bool {
    if result.is_null() {
        return false;
    }
    (*(result as *const ResultBox)).valid
}

/// The error report as a JSON string (`{"errors": [...]}`).
///
/// The pointer stays valid until `tpt_valid_free_result(result)`.
///
/// # Safety
/// `result` must come from `tpt_valid_validate` (not yet freed).
#[no_mangle]
pub unsafe extern "C" fn tpt_valid_get_errors(result: *const tpt_valid_result) -> *const c_char {
    static EMPTY_ERRORS: OnceLock<CString> = OnceLock::new();
    if result.is_null() {
        return EMPTY_ERRORS
            .get_or_init(|| CString::new("{\"errors\":[]}").unwrap())
            .as_ptr();
    }
    (*(result as *const ResultBox)).errors_json.as_ptr()
}

/// Thread-local description of the most recent failed call (NULL arguments,
/// schema compile failure, ...). The pointer is valid until the next
/// tpt-validex call on the same thread.
#[no_mangle]
pub extern "C" fn tpt_valid_last_error() -> *const c_char {
    take_last_error_cstr()
}

/// Library version string, e.g. "0.1.0".
#[no_mangle]
pub extern "C" fn tpt_valid_version() -> *const c_char {
    static VERSION: OnceLock<CString> = OnceLock::new();
    VERSION
        .get_or_init(|| CString::new(env!("CARGO_PKG_VERSION")).unwrap())
        .as_ptr()
}

/// Free a result returned by `tpt_valid_validate`. NULL is ignored.
///
/// # Safety
/// `result` must come from `tpt_valid_validate` and must not be freed twice.
#[no_mangle]
pub unsafe extern "C" fn tpt_valid_free_result(result: *mut tpt_valid_result) {
    if !result.is_null() {
        drop(Box::from_raw(result as *mut ResultBox));
    }
}

/// Destroy a validator created by `tpt_valid_create`. NULL is ignored.
///
/// # Safety
/// `handle` must come from `tpt_valid_create` and must not be destroyed twice.
#[no_mangle]
pub unsafe extern "C" fn tpt_valid_destroy(handle: *mut tpt_valid_handle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle as *mut ValidatorBox));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "age": {"type": "integer", "minimum": 0}
        },
        "required": ["age"]
    }"#;

    fn schema_cstr() -> CString {
        CString::new(SCHEMA).unwrap()
    }

    #[test]
    fn create_validate_destroy_roundtrip() {
        let schema = schema_cstr();
        let handle = unsafe { tpt_valid_create(schema.as_ptr()) };
        assert!(!handle.is_null());

        let good = CString::new(r#"{"age": 25}"#).unwrap();
        let result = unsafe { tpt_valid_validate(handle, good.as_ptr()) };
        assert!(!result.is_null());
        assert!(unsafe { tpt_valid_is_valid(result) });
        let errors = unsafe { CStr::from_ptr(tpt_valid_get_errors(result)) };
        assert_eq!(errors.to_str().unwrap(), "{\"errors\":[]}");
        unsafe { tpt_valid_free_result(result) };

        let bad = CString::new(r#"{"age": "25"}"#).unwrap();
        let result = unsafe { tpt_valid_validate(handle, bad.as_ptr()) };
        assert!(!unsafe { tpt_valid_is_valid(result) });
        let errors = unsafe { CStr::from_ptr(tpt_valid_get_errors(result)) }
            .to_str()
            .unwrap()
            .to_string();
        assert!(errors.contains(r#""path":"$.age""#));
        unsafe { tpt_valid_free_result(result) };

        unsafe { tpt_valid_destroy(handle) };
    }

    #[test]
    fn invalid_data_json_yields_invalid_result() {
        let schema = schema_cstr();
        let handle = unsafe { tpt_valid_create(schema.as_ptr()) };
        let bad_json = CString::new("{oops").unwrap();
        let result = unsafe { tpt_valid_validate(handle, bad_json.as_ptr()) };
        assert!(!result.is_null());
        assert!(!unsafe { tpt_valid_is_valid(result) });
        let errors = unsafe { CStr::from_ptr(tpt_valid_get_errors(result)) }
            .to_str()
            .unwrap();
        assert!(errors.contains("invalid JSON data"));
        unsafe { tpt_valid_free_result(result) };
        unsafe { tpt_valid_destroy(handle) };
    }

    #[test]
    fn invalid_schema_returns_null_and_error() {
        let bad_schema = CString::new("{\"minimum\": 10, \"maximum\": 5}").unwrap();
        let handle = unsafe { tpt_valid_create(bad_schema.as_ptr()) };
        assert!(handle.is_null());
        let err = unsafe { CStr::from_ptr(tpt_valid_last_error()) }
            .to_str()
            .unwrap();
        assert!(
            err.contains("minimum"),
            "last error mentions the problem: {err}"
        );
    }

    #[test]
    fn null_arguments_are_safe() {
        assert!(unsafe { tpt_valid_create(std::ptr::null()) }.is_null());
        assert!(unsafe { tpt_valid_validate(std::ptr::null_mut(), std::ptr::null()) }.is_null());
        assert!(!unsafe { tpt_valid_is_valid(std::ptr::null()) });
        unsafe {
            tpt_valid_free_result(std::ptr::null_mut());
            tpt_valid_destroy(std::ptr::null_mut());
        }
        assert!(!tpt_valid_version().is_null());
    }
}
