//! # tpt-valid-py
//!
//! Python bindings for `tpt-validex` built with PyO3. Exposes the
//! `tpt_validex.Validator` class with single-object, parallel batch, and
//! streaming CSV/JSONL validation (spec §6.1).
//!
//! ```python
//! from tpt_validex import Validator
//!
//! validator = Validator({"type": "object", "required": ["age"]})
//! is_valid, errors = validator.validate({"age": 30})
//! ```
//!
//! See `LICENSE-MIT` and `LICENSE-APACHE` for licensing terms.

#![allow(unsafe_code)] // PyO3 FFI boundary

use std::fs::File;
use std::io::{BufReader, BufWriter, Sink};

use pyo3::conversion::IntoPyObject;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyDict, PyList, PyString};

use tpt_valid_core::{CsvDialect, CsvStats, ValidationError, ValidationOptions};
use tpt_valid_schema::Validator as RustValidator;

/// Convert a Python object to a `serde_json::Value`, fast-pathing through
/// Python's C-implemented `json.dumps` + the Rust SIMD parser. Objects that
/// cannot be dumped (custom classes, cycles) fall back to direct conversion.
fn py_to_value_fast(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    // Python's C-implemented json.dumps + our SIMD parser beats a manual
    // Bound-level walk for every JSON-native input shape.
    if obj.is_instance_of::<PyDict>()
        || obj.is_instance_of::<PyList>()
        || obj.is_instance_of::<pyo3::types::PyString>()
    {
        if let Ok(dumped) = py
            .import("json")
            .and_then(|json| json.call_method1("dumps", (obj,)))
        {
            if let Ok(text) = dumped.extract::<&str>() {
                if let Ok(value) = tpt_valid_parser::parse(text) {
                    return Ok(value);
                }
            }
        }
    }
    py_to_value(obj)
}

/// Convert an arbitrary Python object into a `serde_json::Value`.
fn py_to_value(obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(serde_json::Value::from(i))
    } else if let Ok(u) = obj.extract::<u64>() {
        Ok(serde_json::Value::from(u))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(tpt_valid_core::types::number_from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null))
    } else if let Ok(s) = obj.extract::<&str>() {
        Ok(serde_json::Value::String(s.to_owned()))
    } else if let Ok(list) = obj.cast::<PyList>() {
        let mut items = Vec::with_capacity(list.len());
        for item in list.iter() {
            items.push(py_to_value(&item)?);
        }
        Ok(serde_json::Value::Array(items))
    } else if let Ok(dict) = obj.cast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict.iter() {
            let key = key
                .cast::<pyo3::types::PyString>()
                .map_err(|_| PyValueError::new_err("object keys must be strings"))?
                .to_str()?
                .to_owned();
            map.insert(key, py_to_value(&value)?);
        }
        Ok(serde_json::Value::Object(map))
    } else {
        let ty = obj
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_default();
        Err(PyValueError::new_err(format!(
            "unsupported type for validation input: {ty}"
        )))
    }
}

/// Convert a `serde_json::Value` into a Python object.
fn value_to_py<'py>(py: Python<'py>, value: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    use serde_json::Value as V;
    Ok(match value {
        V::Null => py.None().into_bound(py),
        V::Bool(b) => PyBool::new(py, *b).to_owned().into_any(),
        V::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any()
            } else if let Some(u) = n.as_u64() {
                u.into_pyobject(py)?.into_any()
            } else {
                n.as_f64().unwrap_or(f64::NAN).into_pyobject(py)?.into_any()
            }
        }
        V::String(s) => PyString::new(py, s).into_any(),
        V::Array(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(value_to_py(py, item)?)?;
            }
            list.into_any()
        }
        V::Object(map) => {
            let dict = PyDict::new(py);
            for (key, val) in map {
                dict.set_item(key, value_to_py(py, val)?)?;
            }
            dict.into_any()
        }
    })
}

/// Serialize one error into a Python dict.
fn error_to_py<'py>(py: Python<'py>, error: &ValidationError) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("path", &error.path)?;
    dict.set_item("message", &error.message)?;
    dict.set_item("expected", &error.expected)?;
    dict.set_item("actual", &error.actual)?;
    if let Some(value) = &error.value {
        dict.set_item("value", value_to_py(py, value)?)?;
    } else {
        dict.set_item("value", py.None())?;
    }
    Ok(dict)
}

fn errors_list<'py>(py: Python<'py>, errors: &[ValidationError]) -> PyResult<Bound<'py, PyList>> {
    let list = PyList::empty(py);
    for error in errors {
        list.append(error_to_py(py, error)?)?;
    }
    Ok(list)
}

fn stats_dict<'py>(py: Python<'py>, stats: &CsvStats) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("total_rows", stats.total_rows)?;
    dict.set_item("valid_rows", stats.valid_rows)?;
    dict.set_item("invalid_rows", stats.invalid_rows)?;
    dict.set_item("parse_errors", stats.parse_errors)?;
    Ok(dict)
}

fn io_err(e: std::io::Error) -> PyErr {
    PyValueError::new_err(format!("I/O error: {e}"))
}

fn flow_err(e: tpt_valid_schema::FlowError) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// A compiled validator for one JSON Schema (Draft 2020-12).
///
/// Compile once, validate millions of records.
#[pyclass]
struct Validator {
    inner: RustValidator,
    /// Custom format callables registered for non-built-in `format` names.
    formats: std::collections::HashMap<String, pyo3::Py<PyAny>>,
}

/// Wrap one Python callable as a core [`FormatFn`]: the callable receives
/// the string under test and its truthiness decides validity.
fn make_format_fn(func: pyo3::Py<PyAny>) -> tpt_valid_core::FormatFn {
    std::sync::Arc::new(move |value: &str| {
        pyo3::Python::attach(|py| {
            // Cheap per-call refcount bump: the callable itself is shared.
            let callable = func.clone_ref(py);
            callable
                .bind(py)
                .call1((value,))
                .ok()
                .and_then(|r| r.is_truthy().ok())
                .unwrap_or(false)
        })
    })
}

impl Validator {
    /// Build validation options carrying the registered format callables.
    /// Callables are invoked with the string under test; a falsy return
    /// rejects it.
    fn build_opts(&self, py: Python<'_>) -> ValidationOptions {
        let mut opts = ValidationOptions::default();
        for (name, func) in &self.formats {
            let f = make_format_fn(func.clone_ref(py));
            opts.custom_formats.insert(name.clone(), f);
        }
        opts
    }
}

#[pymethods]
impl Validator {
    /// Compile a JSON Schema from a dict or a JSON string.
    ///
    /// `formats` optionally maps non-built-in `format` names to Python
    /// callables; each callable receives the string under test and returns
    /// whether it is valid. Unregistered custom formats are ignored,
    /// matching JSON Schema annotation semantics.
    #[new]
    #[pyo3(signature = (schema, formats=None))]
    fn new(
        py: Python<'_>,
        schema: &Bound<'_, PyAny>,
        formats: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        let schema_value = py_to_value(schema)?;
        let inner = RustValidator::from_value(&schema_value)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        let mut registered = std::collections::HashMap::new();
        if let Some(formats) = formats {
            for (key, value) in formats.iter() {
                let name = key
                    .cast::<PyString>()
                    .map_err(|_| PyValueError::new_err("format names must be strings"))?
                    .to_str()?
                    .to_owned();
                if !value.is_callable() {
                    return Err(PyValueError::new_err(format!(
                        "format \"{name}\" must be callable"
                    )));
                }
                registered.insert(name, value.clone().unbind());
            }
        }
        let _ = py;
        Ok(Validator {
            inner,
            formats: registered,
        })
    }

    /// Compile from a schema JSON string.
    #[staticmethod]
    fn from_json(schema: &str) -> PyResult<Self> {
        let inner = RustValidator::new(schema).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Validator {
            inner,
            formats: std::collections::HashMap::new(),
        })
    }

    /// Validate a single object. Returns `(is_valid, errors)` where `errors`
    /// is a list of dicts with keys `path`, `message`, `expected`, `actual`,
    /// `value`.
    fn validate<'py>(
        &self,
        py: Python<'py>,
        data: &Bound<'py, PyAny>,
    ) -> PyResult<(bool, Bound<'py, PyList>)> {
        let value = py_to_value_fast(py, data)?;
        let opts = self.build_opts(py);
        let report = self.inner.validate_with(&value, &opts);
        let errors = errors_list(py, &report.errors)?;
        Ok((report.is_valid(), errors))
    }

    /// Boolean-only validation (fail-fast, no error materialization).
    fn is_valid(&self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<bool> {
        let value = py_to_value_fast(py, data)?;
        let opts = self.build_opts(py);
        Ok(tpt_valid_core::validate_value(self.inner.root(), &value, &opts))
    }

    /// Validate a list of objects in parallel. Returns a list of
    /// `ValidationResult` objects with `.is_valid`, `.errors`, `.index`.
    fn validate_batch(
        &self,
        py: Python<'_>,
        batch: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<BatchResult>> {
        let list = batch
            .cast::<PyList>()
            .map_err(|_| PyValueError::new_err("batch must be a list"))?;

        // Fast path: dump the whole batch with Python's C encoder once, then
        // parse with the SIMD parser in one shot.
        let dumped = py
            .import("json")
            .and_then(|json| json.call_method1("dumps", (batch,)))
            .and_then(|s| s.extract::<String>())
            .ok()
            .and_then(|text| tpt_valid_parser::parse(&text).ok());
        let values = match dumped {
            Some(serde_json::Value::Array(values)) if values.len() == list.len() => values,
            _ => {
                let mut values = Vec::with_capacity(list.len());
                for item in list.iter() {
                    values.push(py_to_value(&item)?);
                }
                values
            }
        };
        let opts = self.build_opts(py);
        let outcomes = py.detach(move || {
            tpt_valid_core::validate_batch(self.inner.root(), &values, &opts)
        });
        Ok(outcomes
            .into_iter()
            .map(|outcome| BatchResult {
                index: outcome.index,
                is_valid: outcome.valid,
                errors: outcome.errors,
            })
            .collect())
    }

    /// Stream-validate a CSV file. Valid rows are written to `valid_output`
    /// (when given); invalid rows are written to `errors_output` as JSONL
    /// (`{"line": ..., "row": ..., "errors": [...]}`) when given. Returns a
    /// stats dict with `total_rows`, `valid_rows`, `invalid_rows`,
    /// `parse_errors`.
    #[pyo3(signature = (input_path, valid_output=None, errors_output=None, *, delimiter=",", has_headers=true))]
    fn validate_csv<'py>(
        &self,
        py: Python<'py>,
        input_path: &str,
        valid_output: Option<&str>,
        errors_output: Option<&str>,
        delimiter: &str,
        has_headers: bool,
    ) -> PyResult<Bound<'py, PyDict>> {
        let delimiter = delimiter
            .bytes()
            .next()
            .ok_or_else(|| PyValueError::new_err("delimiter must be a non-empty string"))?;
        let dialect = CsvDialect {
            delimiter,
            has_headers,
            ..CsvDialect::default()
        };
        let opts = self.build_opts(py);

        let input = File::open(input_path)
            .map_err(|e| PyValueError::new_err(format!("cannot open input: {e}")))?;
        let reader = BufReader::with_capacity(64 * 1024, input);

        let stats: CsvStats = match (valid_output, errors_output) {
            (Some(v), Some(e)) => {
                let mut valid_out = BufWriter::new(File::create(v).map_err(io_err)?);
                let mut errors_out = BufWriter::new(File::create(e).map_err(io_err)?);
                self.inner
                    .validate_csv_to(
                        reader,
                        &mut valid_out,
                        &mut errors_out,
                        &dialect,
                        &opts,
                    )
                    .map_err(flow_err)?
            }
            (Some(v), None) => {
                let mut valid_out = BufWriter::new(File::create(v).map_err(io_err)?);
                let sink: Sink = std::io::sink();
                self.inner
                    .validate_csv_to(reader, &mut valid_out, sink, &dialect, &opts)
                    .map_err(flow_err)?
            }
            (None, Some(e)) => {
                let mut errors_out = BufWriter::new(File::create(e).map_err(io_err)?);
                self.inner
                    .validate_csv_to(reader, std::io::sink(), &mut errors_out, &dialect, &opts)
                    .map_err(flow_err)?
            }
            (None, None) => {
                let (stats, _errors) = self
                    .inner
                    .validate_csv(reader, &dialect, &opts)
                    .map_err(flow_err)?;
                stats
            }
        };
        stats_dict(py, &stats)
    }

    /// Stream-validate a newline-delimited JSON file. Invalid lines are
    /// written to `errors_output` (when given) as JSONL. Returns a stats dict
    /// with `total_lines`, `valid_lines`, `invalid_lines`, `parse_errors`.
    #[pyo3(signature = (input_path, errors_output=None))]
    fn validate_jsonl<'py>(
        &self,
        py: Python<'py>,
        input_path: &str,
        errors_output: Option<&str>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let input = File::open(input_path)
            .map_err(|e| PyValueError::new_err(format!("cannot open input: {e}")))?;
        let reader = BufReader::with_capacity(64 * 1024, input);
        let opts = self.build_opts(py);
        let stats = match errors_output {
            Some(e) => {
                let mut errors_out = BufWriter::new(File::create(e).map_err(io_err)?);
                self.inner
                    .validate_jsonl_to(reader, &mut errors_out, &opts)
                    .map_err(io_err)?
            }
            None => self
                .inner
                .validate_jsonl(reader, &opts)
                .map_err(io_err)?,
        };
        let dict = PyDict::new(py);
        dict.set_item("total_lines", stats.total_lines)?;
        dict.set_item("valid_lines", stats.valid_lines)?;
        dict.set_item("invalid_lines", stats.invalid_lines)?;
        dict.set_item("parse_errors", stats.parse_errors)?;
        Ok(dict)
    }

    /// Warnings collected during schema compilation (e.g. unknown formats).
    fn warnings(&self) -> Vec<String> {
        self.inner.warnings().to_vec()
    }
}

/// Result of one item in a `validate_batch` call.
#[pyclass]
struct BatchResult {
    #[pyo3(get)]
    index: usize,
    #[pyo3(get)]
    is_valid: bool,
    errors: Vec<ValidationError>,
}

#[pymethods]
impl BatchResult {
    /// List of error dicts (empty when valid).
    fn errors<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        errors_list(py, &self.errors)
    }

    fn __repr__(&self) -> String {
        format!(
            "ValidationResult(index={}, is_valid={}, errors={})",
            self.index,
            self.is_valid,
            self.errors.len()
        )
    }
}

/// tpt-validex: validate millions of records per second, in every language.
#[pymodule]
fn tpt_validex(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Validator>()?;
    m.add_class::<BatchResult>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
