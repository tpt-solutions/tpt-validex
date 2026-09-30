//! # tpt-valid-wasm
//!
//! WebAssembly bindings for `tpt-validex` (spec §6.2). Exposes the
//! `Validator` class for browsers, edge workers, and Node.js.
//!
//! Build:
//!
//! ```sh
//! cargo build -p tpt-valid-wasm --target wasm32-unknown-unknown --release
//! wasm-bindgen --out-dir pkg --target web \
//!     target/wasm32-unknown-unknown/release/tpt_valid_wasm.wasm
//! ```
//!
//! The JS ⇄ `serde_json::Value` conversion is hand-rolled on top of `js-sys`
//! so that schemas and results are *plain JS objects* (destructuring
//! `const { isValid, errors } = validator.validate(data)` works as
//! advertised).
//!
//! See `LICENSE-MIT` and `LICENSE-APACHE` for licensing terms.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use serde_json::Value;
use std::cell::RefCell;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

use tpt_valid_core::ValidationOptions;
use tpt_valid_schema::Validator as RustValidator;

fn to_js_error<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}

// Registry of JS custom-format callables. Wasm runs single-threaded, so a
// thread-local table plus an index captured by the (Send+Sync-safe) format
// closure keeps the engine's `FormatFn` bounds without unsafe code.
thread_local! {
    static FORMAT_FNS: RefCell<Vec<js_sys::Function>> = const { RefCell::new(Vec::new()) };
}

// ---------------------------------------------------------------------------
// JS ⇄ serde_json::Value conversion (plain objects both ways).
// ---------------------------------------------------------------------------

/// Convert a JS value into a `serde_json::Value`.
///
/// Recognizes null/undefined, booleans, numbers, strings, arrays, and plain
/// objects (own enumerable string keys). Anything else (functions, symbols,
/// class instances) is rejected with an error.
pub fn js_to_value(data: JsValue) -> Result<Value, JsValue> {
    if data.is_null() || data.is_undefined() {
        return Ok(Value::Null);
    }
    if let Some(b) = data.as_bool() {
        return Ok(Value::Bool(b));
    }
    if let Some(n) = data.as_f64() {
        return tpt_valid_core::types::number_from_f64(n)
            .map(Value::Number)
            .ok_or_else(|| JsValue::from_str("number is not JSON-representable"));
    }
    if let Some(s) = data.as_string() {
        return Ok(Value::String(s));
    }
    if data.is_array() {
        let array = js_sys::Array::from(&data);
        let mut items = Vec::with_capacity(array.length() as usize);
        for item in array.iter() {
            items.push(js_to_value(item)?);
        }
        return Ok(Value::Array(items));
    }
    if data.is_object() {
        let object = js_sys::Object::from(data);
        let mut map = serde_json::Map::new();
        for key in js_sys::Object::keys(&object).iter() {
            let key_str = key.as_string().ok_or_else(|| {
                JsValue::from_str("object keys must be strings for JSON validation")
            })?;
            let value = js_sys::Reflect::get(&object, &key)?;
            map.insert(key_str.to_string(), js_to_value(value)?);
        }
        return Ok(Value::Object(map));
    }
    Err(JsValue::from_str(&format!(
        "unsupported type for validation input: {}",
        data.js_typeof().as_string().unwrap_or_default()
    )))
}

/// Convert a `serde_json::Value` into a plain JS value.
pub fn value_to_js(value: &Value) -> JsValue {
    match value {
        Value::Null => JsValue::NULL,
        Value::Bool(b) => JsValue::from_bool(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) if i >= i32::MIN as i64 && i <= i32::MAX as i64 => JsValue::from(i as i32),
            Some(i) => JsValue::from(i),
            None => JsValue::from(n.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(s) => JsValue::from_str(s),
        Value::Array(items) => {
            let array = js_sys::Array::new_with_length(items.len() as u32);
            for (i, item) in items.iter().enumerate() {
                array.set(i as u32, value_to_js(item));
            }
            array.into()
        }
        Value::Object(map) => {
            let object = js_sys::Object::new();
            for (key, val) in map {
                let _ = js_sys::Reflect::set(&object, &JsValue::from_str(key), &value_to_js(val));
            }
            object.into()
        }
    }
}

/// Convert a slice of validation errors into a JS array of plain objects.
fn errors_to_js(errors: &[tpt_valid_core::ValidationError]) -> js_sys::Array {
    let array = js_sys::Array::new_with_length(errors.len() as u32);
    for (i, error) in errors.iter().enumerate() {
        let object = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&object, &"path".into(), &JsValue::from_str(&error.path));
        let _ = js_sys::Reflect::set(
            &object,
            &"message".into(),
            &JsValue::from_str(&error.message),
        );
        let _ = js_sys::Reflect::set(
            &object,
            &"expected".into(),
            &JsValue::from_str(&error.expected),
        );
        let _ = js_sys::Reflect::set(&object, &"actual".into(), &JsValue::from_str(&error.actual));
        let _ = js_sys::Reflect::set(
            &object,
            &"value".into(),
            &error
                .value
                .as_ref()
                .map(value_to_js)
                .unwrap_or(JsValue::NULL),
        );
        array.set(i as u32, object.into());
    }
    array
}

/// A compiled validator for one JSON Schema (Draft 2020-12 subset).
///
/// Compile once, validate millions of records.
///
/// ```js
/// import init, { Validator } from './pkg/tpt_valid_wasm.js';
/// await init();
/// const v = new Validator({ type: 'object', required: ['age'] });
/// const { isValid, errors } = v.validate({ age: 30 });
/// ```
#[wasm_bindgen]
pub struct Validator {
    inner: RustValidator,
    /// `(format name, index into FORMAT_FNS)` for registered callables.
    format_ids: Vec<(String, usize)>,
}

impl Validator {
    /// Build validation options carrying the registered format callables.
    fn build_opts(&self) -> ValidationOptions {
        let mut opts = ValidationOptions::default();
        for (name, id) in &self.format_ids {
            let id = *id;
            let f: tpt_valid_core::FormatFn = Arc::new(move |s: &str| {
                FORMAT_FNS.with(|slot| {
                    let slot = slot.borrow();
                    match slot.get(id) {
                        Some(f) => {
                            let arg = JsValue::from_str(s);
                            f.call1(&JsValue::NULL, &arg)
                                .map(|r| r.is_truthy())
                                .unwrap_or(false)
                        }
                        None => false,
                    }
                })
            });
            opts.custom_formats.insert(name.clone(), f);
        }
        opts
    }
}

#[wasm_bindgen]
impl Validator {
    /// Compile a JSON Schema from a JS object or a JSON string.
    #[wasm_bindgen(constructor)]
    pub fn new(schema: JsValue) -> Result<Validator, JsValue> {
        let schema_value = js_to_value(schema)?;
        let inner = RustValidator::from_value(&schema_value).map_err(to_js_error)?;
        Ok(Validator {
            inner,
            format_ids: Vec::new(),
        })
    }

    /// Compile from a schema JSON string.
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(schema: &str) -> Result<Validator, JsValue> {
        let inner = RustValidator::new(schema).map_err(to_js_error)?;
        Ok(Validator {
            inner,
            format_ids: Vec::new(),
        })
    }

    /// Register a custom format assertion for a non-built-in `format` name.
    /// The function receives the string under test and its truthiness
    /// decides validity. Pass `undefined` to unregister. Unregistered
    /// custom formats are ignored, matching JSON Schema annotation
    /// semantics.
    #[wasm_bindgen(js_name = registerFormat)]
    pub fn register_format(&mut self, name: &str, f: Option<js_sys::Function>) {
        self.format_ids.retain(|(n, _)| n != name);
        if let Some(f) = f {
            let id = FORMAT_FNS.with(|slot| {
                let mut slot = slot.borrow_mut();
                slot.push(f);
                slot.len() - 1
            });
            self.format_ids.push((name.to_string(), id));
        }
    }

    /// Validate a single value. Returns `{ isValid, errors }` (spec §6.2).
    pub fn validate(&self, data: JsValue) -> Result<JsValue, JsValue> {
        let value = js_to_value(data)?;
        let opts = self.build_opts();
        let report = self.inner.validate_with(&value, &opts);
        let result = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &result,
            &"isValid".into(),
            &JsValue::from_bool(report.is_valid()),
        );
        let _ = js_sys::Reflect::set(&result, &"errors".into(), &errors_to_js(&report.errors));
        Ok(result.into())
    }

    /// Boolean-only validation (fail-fast, no error materialization).
    #[wasm_bindgen(js_name = isValid)]
    pub fn is_valid(&self, data: JsValue) -> Result<bool, JsValue> {
        let value = js_to_value(data)?;
        let opts = self.build_opts();
        Ok(tpt_valid_core::validate_value(self.inner.root(), &value, &opts))
    }

    /// Validate an array of values (parallel batch mode). Returns an array
    /// of `{ index, isValid, errors }`.
    #[wasm_bindgen(js_name = validateBatch)]
    pub fn validate_batch(&self, batch: JsValue) -> Result<JsValue, JsValue> {
        let values = js_to_value(batch)?;
        let Value::Array(values) = values else {
            return Err(JsValue::from_str("validateBatch expects an array"));
        };
        let opts = self.build_opts();
        let outcomes = tpt_valid_core::validate_batch(self.inner.root(), &values, &opts);
        let results = js_sys::Array::new_with_length(outcomes.len() as u32);
        for (i, outcome) in outcomes.iter().enumerate() {
            let object = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &object,
                &"index".into(),
                &JsValue::from(outcome.index as u32),
            );
            let _ = js_sys::Reflect::set(
                &object,
                &"isValid".into(),
                &JsValue::from_bool(outcome.valid),
            );
            let _ = js_sys::Reflect::set(&object, &"errors".into(), &errors_to_js(&outcome.errors));
            results.set(i as u32, object.into());
        }
        Ok(results.into())
    }

    /// Warnings collected during schema compilation (e.g. unknown formats).
    #[wasm_bindgen(js_name = warnings)]
    pub fn warnings(&self) -> js_sys::Array {
        let array = js_sys::Array::new_with_length(self.inner.warnings().len() as u32);
        for (i, warning) in self.inner.warnings().iter().enumerate() {
            array.set(i as u32, JsValue::from_str(warning));
        }
        array
    }
}

/// Library version, e.g. "0.1.0".
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
