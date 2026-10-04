//! Opt-in data coercion (Phase 13): light, lossless-cleanable fixes applied
//! before validation — trimming whitespace, thousands separators, text-form
//! booleans, US-style date normalization, and empty-string → null. Every
//! change is reported as a [`CoerceChange`] so the caller can show (or
//! reverse) what was touched.
//!
//! Coercion is deliberately conservative: it never changes a value that
//! already parses as its target type, and it never mutates numbers or
//! booleans in place.

use serde::Serialize;
use serde_json::Value;

/// Which coercions to apply. All rules default to `false` — coercion is
/// opt-in — except recursion, which is on by default.
#[derive(Debug, Clone)]
pub struct CoerceOptions {
    /// Strip leading/trailing whitespace from strings.
    pub trim: bool,
    /// `"1,234"` → `1234` (thousands separators removed; the result must be
    /// a clean number).
    pub numeric_strings: bool,
    /// `"yes"/"no"/"on"/"off"/"true"/"false"` (any case) → booleans.
    pub booleans: bool,
    /// `M/D/YYYY` and `M/D/YY` (US order) → `YYYY-MM-DD`.
    pub dates: bool,
    /// Empty strings become `null`.
    pub empty_string_as_null: bool,
    /// Recurse into arrays and objects (default `true`).
    pub recurse: bool,
}

impl Default for CoerceOptions {
    fn default() -> Self {
        Self {
            trim: false,
            numeric_strings: false,
            booleans: false,
            dates: false,
            empty_string_as_null: false,
            recurse: true,
        }
    }
}

/// One applied coercion.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoerceChange {
    /// JSON path of the coerced value.
    pub path: String,
    /// The original value.
    pub from: Value,
    /// The coerced value.
    pub to: Value,
    /// Which rule fired.
    pub rule: &'static str,
}

/// Coerce `value` in place, returning the list of applied changes.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use tpt_valid_core::{coerce_value, CoerceOptions};
///
/// let mut v = json!({"name": "  Alice ", "amount": "1,234", "active": "yes"});
/// let changes = coerce_value(&mut v, &CoerceOptions {
///     trim: true,
///     numeric_strings: true,
///     booleans: true,
///     ..Default::default()
/// });
/// assert_eq!(v["name"], "Alice");
/// assert_eq!(v["amount"], 1234);
/// assert_eq!(v["active"], true);
/// assert_eq!(changes.len(), 3);
/// ```
pub fn coerce_value(value: &mut Value, opts: &CoerceOptions) -> Vec<CoerceChange> {
    let mut changes = Vec::new();
    let mut path = String::from("$");
    coerce_at(value, opts, &mut path, &mut changes);
    changes
}

fn coerce_at(
    value: &mut Value,
    opts: &CoerceOptions,
    path: &mut String,
    changes: &mut Vec<CoerceChange>,
) {
    match value {
        Value::String(_) => {
            if let Some((rule, to)) = coerce_string(value, opts) {
                let from = std::mem::replace(value, to);
                changes.push(CoerceChange {
                    path: path.clone(),
                    from,
                    to: value.clone(),
                    rule,
                });
            }
        }
        Value::Array(items) if opts.recurse => {
            for (i, item) in items.iter_mut().enumerate() {
                let mark = path.len();
                path.push_str(&format!("[{i}]"));
                coerce_at(item, opts, path, changes);
                path.truncate(mark);
            }
        }
        Value::Object(map) if opts.recurse => {
            for (key, item) in map.iter_mut() {
                let mark = path.len();
                path.push('.');
                path.push_str(key);
                coerce_at(item, opts, path, changes);
                path.truncate(mark);
            }
        }
        _ => {}
    }
}

/// Decide the coercion for one string value: `Some((rule, replacement))`.
fn coerce_string(value: &Value, opts: &CoerceOptions) -> Option<(&'static str, Value)> {
    let raw = value.as_str()?;

    if opts.empty_string_as_null && raw.is_empty() {
        return Some(("empty_string_as_null", Value::Null));
    }
    if opts.booleans {
        match raw.to_ascii_lowercase().as_str() {
            "yes" | "on" | "true" => return Some(("boolean", Value::Bool(true))),
            "no" | "off" | "false" => return Some(("boolean", Value::Bool(false))),
            _ => {}
        }
    }
    if opts.numeric_strings {
        let cleaned = raw.replace(',', "");
        if cleaned != raw && !cleaned.trim().is_empty() {
            if let Ok(i) = cleaned.trim().parse::<i64>() {
                return Some(("numeric_string", Value::from(i)));
            }
            if let Ok(f) = cleaned.trim().parse::<f64>() {
                if let Some(n) = crate::types::number_from_f64(f) {
                    return Some(("numeric_string", Value::Number(n)));
                }
            }
        }
    }
    if opts.dates {
        if let Some(normalized) = normalize_us_date(raw) {
            return Some(("date", Value::String(normalized)));
        }
    }
    if opts.trim {
        let trimmed = raw.trim();
        if trimmed != raw {
            return Some(("trim", Value::String(trimmed.to_string())));
        }
    }
    None
}

/// `M/D/YYYY` / `M/D/YY` → `YYYY-MM-DD` (zero-padded), `None` otherwise.
fn normalize_us_date(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let month: u32 = parts[0].parse().ok()?;
    let day: u32 = parts[1].parse().ok()?;
    let year_raw = parts[2];
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = if year_raw.len() == 2 {
        format!("20{year_raw}")
    } else if year_raw.len() == 4 {
        year_raw.to_string()
    } else {
        return None;
    };
    Some(format!("{year}-{month:02}-{day:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn trims_and_coerces_numerals_and_booleans() {
        let mut v = json!({
            "name": "  Alice ",
            "amount": "1,234",
            "active": "Yes",
            "count": " 42 "
        });
        let changes = coerce_value(
            &mut v,
            &CoerceOptions {
                trim: true,
                numeric_strings: true,
                booleans: true,
                ..Default::default()
            },
        );
        assert_eq!(v["name"], "Alice");
        assert_eq!(v["amount"], 1234);
        assert_eq!(v["active"], true);
        // " 42 " has no thousands separator, so only the trim rule fires
        // (the string stays a string).
        assert_eq!(v["count"], "42");
        assert!(changes.iter().any(|c| c.rule == "trim"));
        assert!(changes.iter().any(|c| c.rule == "numeric_string"));
        assert!(changes.iter().any(|c| c.rule == "boolean"));
        assert!(changes.iter().any(|c| c.path == "$.name"));
    }

    #[test]
    fn empty_string_and_dates() {
        let mut v = json!({"note": "", "when": "1/5/2024", "big": "12/31/99"});
        let changes = coerce_value(
            &mut v,
            &CoerceOptions {
                empty_string_as_null: true,
                dates: true,
                ..Default::default()
            },
        );
        assert_eq!(v["note"], Value::Null);
        assert_eq!(v["when"], "2024-01-05");
        assert_eq!(v["big"], "2099-12-31");
        assert_eq!(changes.len(), 3);
    }

    #[test]
    fn recursion_into_arrays_and_paths() {
        let mut v = json!({"rows": [{"amount": "2,000"}, {"amount": "3,5"}]});
        let changes = coerce_value(
            &mut v,
            &CoerceOptions {
                numeric_strings: true,
                ..Default::default()
            },
        );
        assert_eq!(v["rows"][0]["amount"], 2000);
        assert_eq!(changes[0].path, "$.rows[0].amount");
    }

    #[test]
    fn leaves_non_strings_alone() {
        let mut v = json!({"n": 5, "flag": true, "list": [1, 2]});
        let changes = coerce_value(
            &mut v,
            &CoerceOptions {
                trim: true,
                numeric_strings: true,
                booleans: true,
                dates: true,
                empty_string_as_null: true,
                ..Default::default()
            },
        );
        assert_eq!(v, json!({"n": 5, "flag": true, "list": [1, 2]}));
        assert!(changes.is_empty());
    }

    #[test]
    fn us_date_rejects_invalid() {
        assert_eq!(normalize_us_date("13/40/2024"), None);
        assert_eq!(normalize_us_date("2024-01-05"), None);
        assert_eq!(normalize_us_date("1/5/12345"), None);
    }
}
