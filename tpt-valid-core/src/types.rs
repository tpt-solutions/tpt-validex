//! JSON Schema data types (`type` keyword, Draft 2020-12 §6.1.1).

use serde_json::Value;

/// The JSON Schema primitive types.
///
/// Per Draft 2020-12, `integer` also matches floats with a zero fractional
/// part (`1.0` is an integer), and `number` matches both integers and floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    /// `null`
    Null,
    /// `boolean`
    Boolean,
    /// `object`
    Object,
    /// `array`
    Array,
    /// `number` (integers included)
    Number,
    /// `integer` (whole floats included)
    Integer,
    /// `string`
    String,
}

impl DataType {
    /// Map a JSON Schema type keyword to a [`DataType`].
    pub fn from_keyword(keyword: &str) -> Option<DataType> {
        match keyword {
            "null" => Some(DataType::Null),
            "boolean" => Some(DataType::Boolean),
            "object" => Some(DataType::Object),
            "array" => Some(DataType::Array),
            "number" => Some(DataType::Number),
            "integer" => Some(DataType::Integer),
            "string" => Some(DataType::String),
            _ => None,
        }
    }

    /// The keyword name of this type.
    pub fn keyword(&self) -> &'static str {
        match self {
            DataType::Null => "null",
            DataType::Boolean => "boolean",
            DataType::Object => "object",
            DataType::Array => "array",
            DataType::Number => "number",
            DataType::Integer => "integer",
            DataType::String => "string",
        }
    }

    /// Whether a JSON value is an instance of this type.
    pub fn matches(&self, value: &Value) -> bool {
        match self {
            DataType::Null => value.is_null(),
            DataType::Boolean => value.is_boolean(),
            DataType::Object => value.is_object(),
            DataType::Array => value.is_array(),
            DataType::Number => value.is_number(),
            DataType::Integer => is_integer_value(value),
            DataType::String => value.is_string(),
        }
    }
}

/// JSON Schema integer semantics: `1` and `1.0` are both integers;
/// `1.5` and non-finite floats are not.
pub fn is_integer_value(value: &Value) -> bool {
    match value {
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                true
            } else if let Some(f) = n.as_f64() {
                f.is_finite() && f.fract() == 0.0
            } else {
                false
            }
        }
        _ => false,
    }
}

/// The JSON type name of a value ("string", "integer", "number", ...).
///
/// Integers report as `"integer"`, other numbers as `"number"`, matching the
/// error style of the spec's §5.5 example.
pub fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Convert any JSON number to `f64` (`None` for non-numbers).
pub fn value_as_f64(value: &Value) -> Option<f64> {
    value.as_f64()
}

/// Build a JSON number from an `f64`, preferring an integer representation
/// when the value is integral, so that `1.0` from dynamically-typed hosts
/// (JS numbers, CSV cells) reads back as `1` for `integer`-type keywords.
///
/// Returns `None` for non-finite values (which JSON cannot represent).
pub fn number_from_f64(f: f64) -> Option<serde_json::Number> {
    let number = serde_json::Number::from_f64(f)?;
    if f.is_finite() && f.fract() == 0.0 {
        if f >= i64::MIN as f64 && f < i64::MAX as f64 {
            return Some(serde_json::Number::from(f as i64));
        }
        if f >= 0.0 && f < u64::MAX as f64 {
            return Some(serde_json::Number::from(f as u64));
        }
    }
    Some(number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keyword_roundtrip() {
        for kw in [
            "null", "boolean", "object", "array", "number", "integer", "string",
        ] {
            let t = DataType::from_keyword(kw).unwrap();
            assert_eq!(t.keyword(), kw);
        }
        assert!(DataType::from_keyword("unknown").is_none());
    }

    #[test]
    fn integer_includes_whole_floats() {
        assert!(DataType::Integer.matches(&json!(1)));
        assert!(DataType::Integer.matches(&json!(1.0)));
        assert!(!DataType::Integer.matches(&json!(1.5)));
        assert!(DataType::Number.matches(&json!(1.5)));
        assert!(DataType::Number.matches(&json!(-3)));
    }

    #[test]
    fn type_matches() {
        assert!(DataType::Null.matches(&Value::Null));
        assert!(DataType::Boolean.matches(&json!(true)));
        assert!(DataType::Object.matches(&json!({})));
        assert!(DataType::Array.matches(&json!([])));
        assert!(DataType::String.matches(&json!("x")));
    }

    #[test]
    fn type_names() {
        assert_eq!(json_type_name(&json!(2)), "integer");
        assert_eq!(json_type_name(&json!(2.5)), "number");
        assert_eq!(json_type_name(&json!("s")), "string");
    }
}
