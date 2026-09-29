//! The validation engine: traversal of the [`ValidationNode`] state machine
//! with full error collection (spec §5.2 "Validation Process", §5.5).

use serde_json::Value;

use crate::error::{ErrorCollector, ValidationError};
use crate::node::{AdditionalProperties, EnumSet, ObjectShape, ValidationNode};
use crate::types::{json_type_name, value_as_f64};

/// Options controlling a validation run.
#[derive(Debug, Clone)]
pub struct ValidationOptions {
    /// Stop at the first error (spec: "fail-fast" mode for
    /// performance-critical paths). Default: `false` — collect all errors.
    pub fail_fast: bool,
    /// Cap on collected errors per instance. Default: 1000.
    pub max_errors: usize,
}

impl Default for ValidationOptions {
    fn default() -> Self {
        Self {
            fail_fast: false,
            max_errors: crate::error::DEFAULT_MAX_ERRORS,
        }
    }
}

impl ValidationOptions {
    /// Fail-fast options.
    pub fn fail_fast() -> Self {
        Self {
            fail_fast: true,
            max_errors: 1,
        }
    }
}

/// Validate a JSON value against a state machine, returning all violations.
///
/// # Examples
///
/// ```
/// use tpt_valid_core::{validate, DataType, ValidationNode, ValidationOptions};
/// use serde_json::json;
///
/// let node = ValidationNode::CheckType(DataType::Integer);
/// let errors = validate(&node, &json!(42), &ValidationOptions::default());
/// assert!(errors.is_empty());
///
/// let errors = validate(&node, &json!("42"), &ValidationOptions::default());
/// assert_eq!(errors[0].path, "$");
/// assert_eq!(errors[0].expected, "integer");
/// ```
pub fn validate(
    node: &ValidationNode,
    value: &Value,
    opts: &ValidationOptions,
) -> Vec<ValidationError> {
    let mut collector = ErrorCollector::new(opts.fail_fast, opts.max_errors);
    let mut path = PathCursor::new();
    check(node, value, &mut path, opts, &mut collector);
    collector.into_errors()
}

/// Validate a JSON value, returning only whether it is valid (fail-fast
/// traversal, no error materialization).
pub fn validate_value(node: &ValidationNode, value: &Value, opts: &ValidationOptions) -> bool {
    let mut collector = ErrorCollector::new(true, 1);
    let mut path = PathCursor::new();
    check(node, value, &mut path, opts, &mut collector);
    collector.is_valid()
}

/// Compact JSON-path cursor: `$.tags[2].name`.
pub(crate) struct PathCursor {
    buf: String,
    marks: Vec<usize>,
}

impl PathCursor {
    pub(crate) fn new() -> Self {
        Self {
            buf: "$".to_string(),
            marks: Vec::new(),
        }
    }

    fn push_field(&mut self, key: &str) {
        self.marks.push(self.buf.len());
        self.buf.push('.');
        self.buf.push_str(key);
    }

    fn push_index(&mut self, index: usize) {
        self.marks.push(self.buf.len());
        self.buf.push('[');
        self.buf.push_str(&itoa_buf(index));
        self.buf.push(']');
    }

    fn pop(&mut self) {
        if let Some(mark) = self.marks.pop() {
            self.buf.truncate(mark);
        }
    }

    fn as_str(&self) -> &str {
        &self.buf
    }
}

fn itoa_buf(n: usize) -> String {
    n.to_string()
}

/// Main recursive traversal.
pub(crate) fn check(
    node: &ValidationNode,
    value: &Value,
    path: &mut PathCursor,
    opts: &ValidationOptions,
    out: &mut ErrorCollector,
) {
    if out.stopped() {
        return;
    }
    match node {
        ValidationNode::Always => {}
        ValidationNode::Never(reason) => {
            out.push(
                ValidationError::new(
                    path.as_str(),
                    reason.clone(),
                    "valid instance",
                    json_type_name(value),
                )
                .with_value(value.clone()),
            );
        }
        ValidationNode::CheckAll(seq) => {
            for sub in seq {
                if out.stopped() {
                    return;
                }
                check(sub, value, path, opts, out);
            }
        }
        ValidationNode::CheckType(expected) => {
            if !expected.matches(value) {
                out.push(
                    ValidationError::new(
                        path.as_str(),
                        format!(
                            "Expected {}, got {}",
                            expected.keyword(),
                            json_type_name(value)
                        ),
                        expected.keyword(),
                        json_type_name(value),
                    )
                    .with_value(value.clone()),
                );
            }
        }
        ValidationNode::CheckField(name, inner) => {
            if let Value::Object(map) = value {
                if let Some(v) = map.get(name) {
                    path.push_field(name);
                    check(inner, v, path, opts, out);
                    path.pop();
                }
            }
        }
        ValidationNode::CheckRequired(required) => {
            if let Value::Object(map) = value {
                for key in required {
                    if out.stopped() {
                        return;
                    }
                    if !map.contains_key(key) {
                        path.push_field(key);
                        out.push(ValidationError::new(
                            path.as_str(),
                            format!("Missing required property \"{key}\""),
                            "required",
                            "missing",
                        ));
                        path.pop();
                    }
                }
            }
        }
        ValidationNode::CheckMinimum(bound) => {
            if let Some(v) = value_as_f64(value) {
                if v < *bound {
                    number_bound_error(out, path, value, *bound, "minimum", ">=");
                }
            }
        }
        ValidationNode::CheckMaximum(bound) => {
            if let Some(v) = value_as_f64(value) {
                if v > *bound {
                    number_bound_error(out, path, value, *bound, "maximum", "<=");
                }
            }
        }
        ValidationNode::CheckExclusiveMinimum(bound) => {
            if let Some(v) = value_as_f64(value) {
                if v <= *bound {
                    number_bound_error(out, path, value, *bound, "exclusiveMinimum", ">");
                }
            }
        }
        ValidationNode::CheckExclusiveMaximum(bound) => {
            if let Some(v) = value_as_f64(value) {
                if v >= *bound {
                    number_bound_error(out, path, value, *bound, "exclusiveMaximum", "<");
                }
            }
        }
        ValidationNode::CheckMultipleOf(divisor) => {
            if let Some(v) = value_as_f64(value) {
                if !is_multiple_of(value, v, *divisor) {
                    out.push(
                        ValidationError::new(
                            path.as_str(),
                            format!("{v} is not a multiple of {divisor}"),
                            format!("multiple of {divisor}"),
                            v.to_string(),
                        )
                        .with_value(value.clone()),
                    );
                }
            }
        }
        ValidationNode::CheckMinLength(bound) => {
            if let Value::String(s) = value {
                let len = s.chars().count();
                if len < *bound {
                    out.push(
                        ValidationError::new(
                            path.as_str(),
                            format!("Expected string length >= {bound}, got {len}"),
                            format!("minLength {bound}"),
                            len.to_string(),
                        )
                        .with_value(value.clone()),
                    );
                }
            }
        }
        ValidationNode::CheckMaxLength(bound) => {
            if let Value::String(s) = value {
                let len = s.chars().count();
                if len > *bound {
                    out.push(
                        ValidationError::new(
                            path.as_str(),
                            format!("Expected string length <= {bound}, got {len}"),
                            format!("maxLength {bound}"),
                            len.to_string(),
                        )
                        .with_value(value.clone()),
                    );
                }
            }
        }
        ValidationNode::CheckPattern(pattern) => {
            if let Value::String(s) = value {
                if !pattern.is_match(s) {
                    out.push(
                        ValidationError::new(
                            path.as_str(),
                            format!("String does not match pattern \"{}\"", pattern.as_str()),
                            format!("pattern {}", pattern.as_str()),
                            "string",
                        )
                        .with_value(value.clone()),
                    );
                }
            }
        }
        ValidationNode::CheckEnum(set) => {
            if !set.contains(value) {
                out.push(
                    ValidationError::new(
                        path.as_str(),
                        format!("Value must be one of {}", format_allowed(set)),
                        "enum",
                        json_type_name(value),
                    )
                    .with_value(value.clone()),
                );
            }
        }
        ValidationNode::CheckConst(expected) => {
            if !crate::node::normalize_numbers(value.clone())
                .eq(&crate::node::normalize_numbers(expected.clone()))
            {
                out.push(
                    ValidationError::new(
                        path.as_str(),
                        format!("Value must be equal to constant {expected}"),
                        "const",
                        json_type_name(value),
                    )
                    .with_value(value.clone()),
                );
            }
        }
        ValidationNode::CheckFormat(format) => {
            if let Value::String(s) = value {
                if !format.is_valid(s) {
                    out.push(
                        ValidationError::new(
                            path.as_str(),
                            format!("Invalid {} format", format.keyword()),
                            format.keyword(),
                            truncate_display(s, 48),
                        )
                        .with_value(value.clone()),
                    );
                }
            }
        }
        ValidationNode::CheckArray(items, min_items, max_items) => {
            if let Value::Array(arr) = value {
                if arr.len() < *min_items {
                    out.push(
                        ValidationError::new(
                            path.as_str(),
                            format!("Expected at least {min_items} items, got {}", arr.len()),
                            format!("minItems {min_items}"),
                            arr.len().to_string(),
                        )
                        .with_value(value.clone()),
                    );
                }
                if arr.len() > *max_items {
                    out.push(
                        ValidationError::new(
                            path.as_str(),
                            format!("Expected at most {max_items} items, got {}", arr.len()),
                            format!("maxItems {max_items}"),
                            arr.len().to_string(),
                        )
                        .with_value(value.clone()),
                    );
                }
                for (i, item) in arr.iter().enumerate() {
                    if out.stopped() {
                        return;
                    }
                    path.push_index(i);
                    check(items, item, path, opts, out);
                    path.pop();
                }
            }
        }
        ValidationNode::CheckUniqueItems(unique) => {
            if *unique {
                if let Value::Array(arr) = value {
                    if let Some(dup) = find_duplicate(arr) {
                        path.push_index(dup);
                        out.push(ValidationError::new(
                            path.as_str(),
                            format!("Array items are not unique (duplicate at index {dup})"),
                            "uniqueItems",
                            "duplicate",
                        ));
                        path.pop();
                    }
                }
            }
        }
        ValidationNode::CheckContains(contains) => {
            if let Value::Array(arr) = value {
                let matched = arr.iter().any(|item| {
                    let mut trial = ErrorCollector::new(true, 1);
                    check(contains, item, path, opts, &mut trial);
                    trial.is_valid()
                });
                if !matched {
                    out.push(
                        ValidationError::new(
                            path.as_str(),
                            "Expected at least one array item to match the `contains` schema"
                                .to_string(),
                            "contains",
                            json_type_name(value),
                        )
                        .with_value(value.clone()),
                    );
                }
            }
        }
        ValidationNode::CheckObject(properties) => {
            if let Value::Object(map) = value {
                for (name, sub) in properties {
                    if out.stopped() {
                        return;
                    }
                    if let Some(v) = map.get(name) {
                        path.push_field(name);
                        check(sub, v, path, opts, out);
                        path.pop();
                    }
                }
            }
        }
        ValidationNode::CheckObjectEx(shape) => check_object_ex(shape, value, path, opts, out),
        ValidationNode::CheckIfThenElse(if_node, then_node, else_node) => {
            let cond_matched = {
                let mut trial = ErrorCollector::fail_fast();
                check(if_node, value, path, opts, &mut trial);
                trial.is_valid()
            };
            if cond_matched {
                if let Some(t) = then_node {
                    check(t, value, path, opts, out);
                }
            } else if let Some(e) = else_node {
                check(e, value, path, opts, out);
            }
        }
        ValidationNode::CheckAllOf(schemas) => {
            for sub in schemas {
                if out.stopped() {
                    return;
                }
                check(sub, value, path, opts, out);
            }
        }
        ValidationNode::CheckAnyOf(schemas) => {
            let any = schemas.iter().any(|sub| {
                let mut trial = ErrorCollector::fail_fast();
                check(sub, value, path, opts, &mut trial);
                trial.is_valid()
            });
            if !any {
                out.push(
                    ValidationError::new(
                        path.as_str(),
                        format!(
                            "Expected to match at least one of {} schemas",
                            schemas.len()
                        ),
                        "anyOf",
                        json_type_name(value),
                    )
                    .with_value(value.clone()),
                );
            }
        }
        ValidationNode::CheckOneOf(schemas) => {
            let matched = schemas
                .iter()
                .filter(|sub| {
                    let mut trial = ErrorCollector::fail_fast();
                    check(sub, value, path, opts, &mut trial);
                    trial.is_valid()
                })
                .count();
            if matched != 1 {
                let detail = if matched == 0 {
                    format!("none of {} schemas matched", schemas.len())
                } else {
                    format!("{matched} of {} schemas matched", schemas.len())
                };
                out.push(
                    ValidationError::new(
                        path.as_str(),
                        format!("Expected exactly one schema to match ({detail})"),
                        "oneOf",
                        detail,
                    )
                    .with_value(value.clone()),
                );
            }
        }
        ValidationNode::CheckNot(inner) => {
            let mut trial = ErrorCollector::fail_fast();
            check(inner, value, path, opts, &mut trial);
            if trial.is_valid() {
                out.push(
                    ValidationError::new(
                        path.as_str(),
                        "Value must not match the `not` schema".to_string(),
                        "not",
                        json_type_name(value),
                    )
                    .with_value(value.clone()),
                );
            }
        }
    }
}

fn check_object_ex(
    shape: &ObjectShape,
    value: &Value,
    path: &mut PathCursor,
    opts: &ValidationOptions,
    out: &mut ErrorCollector,
) {
    let Value::Object(map) = value else { return };

    for key in &shape.required {
        if out.stopped() {
            return;
        }
        if !map.contains_key(key) {
            path.push_field(key);
            out.push(ValidationError::new(
                path.as_str(),
                format!("Missing required property \"{key}\""),
                "required",
                "missing",
            ));
            path.pop();
        }
    }

    for (name, sub) in &shape.properties {
        if out.stopped() {
            return;
        }
        if let Some(v) = map.get(name) {
            path.push_field(name);
            check(sub, v, path, opts, out);
            path.pop();
        }
    }

    for (pattern, sub) in &shape.pattern_properties {
        if out.stopped() {
            return;
        }
        for (key, v) in map {
            if pattern.is_match(key) {
                path.push_field(key);
                check(sub, v, path, opts, out);
                path.pop();
                if out.stopped() {
                    return;
                }
            }
        }
    }

    if !matches!(shape.additional, AdditionalProperties::Allow) {
        for (key, v) in map {
            if out.stopped() {
                return;
            }
            let covered = shape.properties.iter().any(|(n, _)| n == key)
                || shape
                    .pattern_properties
                    .iter()
                    .any(|(p, _)| p.is_match(key));
            if covered {
                continue;
            }
            match &shape.additional {
                AdditionalProperties::Allow => unreachable!(),
                AdditionalProperties::Forbid => {
                    path.push_field(key);
                    out.push(
                        ValidationError::new(
                            path.as_str(),
                            format!("Unknown property \"{key}\" is not allowed"),
                            "additionalProperties",
                            "unknown property",
                        )
                        .with_value(v.clone()),
                    );
                    path.pop();
                }
                AdditionalProperties::Schema(sub) => {
                    path.push_field(key);
                    check(sub, v, path, opts, out);
                    path.pop();
                }
            }
        }
    }
}

fn number_bound_error(
    out: &mut ErrorCollector,
    path: &PathCursor,
    value: &Value,
    bound: f64,
    keyword: &str,
    op: &str,
) {
    let v = value_as_f64(value).unwrap_or(f64::NAN);
    out.push(
        ValidationError::new(
            path.as_str(),
            format!("Expected value {op} {bound}, got {v}"),
            format!("{keyword} {bound}"),
            v.to_string(),
        )
        .with_value(value.clone()),
    );
}

/// Numeric-aware multiple-of check. Integral pairs use exact integer
/// arithmetic; floats use an epsilon-tolerant quotient (documented in the
/// compliance matrix).
fn is_multiple_of(raw: &Value, v: f64, divisor: f64) -> bool {
    if divisor == 0.0 {
        return false;
    }
    let integral_value = match raw {
        Value::Number(n) if n.is_i64() => Some(n.as_i64().unwrap() as i128),
        Value::Number(n) if n.is_u64() => Some(n.as_u64().unwrap() as i128),
        _ => None,
    };
    if let Some(iv) = integral_value {
        if divisor.fract() == 0.0 && divisor.abs() <= 9.0e15 {
            return iv % (divisor as i128) == 0;
        }
    }
    let q = v / divisor;
    let tolerance = 1e-9 * q.abs().max(1.0);
    (q - q.round()).abs() <= tolerance
}

fn find_duplicate(arr: &[Value]) -> Option<usize> {
    let mut seen: std::collections::HashSet<String> =
        std::collections::HashSet::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let key = serde_json::to_string(&crate::node::normalize_numbers(item.clone())).ok()?;
        if !seen.insert(key) {
            return Some(i);
        }
    }
    None
}

fn format_allowed(set: &EnumSet) -> String {
    let items: Vec<String> = set
        .allowed()
        .iter()
        .map(|v| truncate_display(&v.to_string(), 24))
        .collect();
    if items.len() > 8 {
        format!("[{}, ... {} values]", items[..8].join(", "), items.len())
    } else {
        format!("[{}]", items.join(", "))
    }
}

fn truncate_display(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}...")
    }
}

// Re-exported for downstream error formatting convenience.
pub use crate::types::json_type_name as value_type_name;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Format;
    use crate::types::DataType;
    use serde_json::json;

    fn node_errs(node: &ValidationNode, v: Value) -> Vec<ValidationError> {
        validate(node, &v, &ValidationOptions::default())
    }

    #[test]
    fn type_check_with_path() {
        let node = ValidationNode::CheckType(DataType::Integer);
        let errs = node_errs(&node, json!("25"));
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "$");
        assert_eq!(errs[0].expected, "integer");
        assert_eq!(errs[0].actual, "string");
        assert_eq!(errs[0].value, Some(json!("25")));
    }

    #[test]
    fn minimum_maximum() {
        let node = ValidationNode::all(vec![
            ValidationNode::CheckMinimum(0.0),
            ValidationNode::CheckMaximum(150.0),
        ]);
        assert!(node_errs(&node, json!(75)).is_empty());
        assert!(node_errs(&node, json!(-1)).len() == 1);
        assert!(node_errs(&node, json!(151)).len() == 1);
        // Non-numbers are skipped (type keyword's job).
        assert!(node_errs(&node, json!("x")).is_empty());
        assert!(node_errs(&node, json!(0.0)).is_empty());
        assert!(node_errs(&node, json!(150.0)).is_empty());
    }

    #[test]
    fn exclusive_bounds_and_multiple_of() {
        let node = ValidationNode::all(vec![
            ValidationNode::CheckExclusiveMinimum(0.0),
            ValidationNode::CheckExclusiveMaximum(10.0),
            ValidationNode::CheckMultipleOf(2.0),
        ]);
        assert!(node_errs(&node, json!(4)).is_empty());
        assert_eq!(node_errs(&node, json!(0)).len(), 1);
        assert_eq!(node_errs(&node, json!(10)).len(), 1);
        assert_eq!(node_errs(&node, json!(7)).len(), 1, "only multipleOf fails");
        assert!(node_errs(&node, json!(8)).is_empty());
        // Exact integer path for large i64 values:
        let big = ValidationNode::CheckMultipleOf(2.0);
        assert!(node_errs(&big, json!(1_000_000_000_000i64)).is_empty());
        let big_odd = ValidationNode::CheckMultipleOf(3.0);
        assert_eq!(node_errs(&big_odd, json!(1_000_000_000_001i64)).len(), 1);
    }

    #[test]
    fn string_length_and_pattern() {
        let node = ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::String),
            ValidationNode::CheckMinLength(2),
            ValidationNode::CheckMaxLength(5),
            ValidationNode::CheckPattern(Box::new(regex::Regex::new("^[a-z]+$").unwrap())),
        ]);
        assert!(node_errs(&node, json!("abc")).is_empty());
        assert!(node_errs(&node, json!("a")).len() == 1);
        assert!(node_errs(&node, json!("abcdef")).len() == 1);
        assert!(node_errs(&node, json!("ABC")).len() == 1);
        // Length counts chars, not bytes ("äöü" = 3 chars, 6 bytes): passes
        // both length bounds, fails the ASCII-only pattern.
        assert_eq!(node_errs(&node, json!("äöü")).len(), 1);
        let lengths_only = ValidationNode::all(vec![
            ValidationNode::CheckMinLength(2),
            ValidationNode::CheckMaxLength(5),
        ]);
        assert!(node_errs(&lengths_only, json!("äöü")).is_empty());
        assert!(
            node_errs(&lengths_only, json!("äöüäöü")).len() == 1,
            "6 chars > 5"
        );
    }

    #[test]
    fn enum_and_const() {
        let set = EnumSet::new(vec![json!("red"), json!("green"), json!("blue")]);
        let node = ValidationNode::CheckEnum(set);
        assert!(node_errs(&node, json!("red")).is_empty());
        let errs = node_errs(&node, json!("pink"));
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("red"));

        let cnode = ValidationNode::CheckConst(json!(1.0));
        assert!(node_errs(&cnode, json!(1)).is_empty());
        assert_eq!(node_errs(&cnode, json!(2)).len(), 1);
    }

    #[test]
    fn collect_all_errors_not_just_first() {
        let node = ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckField(
                "age".into(),
                Box::new(ValidationNode::CheckType(DataType::Integer)),
            ),
            ValidationNode::CheckField(
                "email".into(),
                Box::new(ValidationNode::CheckFormat(Format::Email)),
            ),
            ValidationNode::CheckRequired(vec!["name".into(), "age".into()]),
        ]);
        let errs = node_errs(&node, json!({"age": "25", "email": "bad"}));
        assert_eq!(errs.len(), 3, "type error + format error + missing name");
        assert_eq!(errs[0].path, "$.age");
        assert_eq!(errs[1].path, "$.email");
        assert_eq!(errs[2].path, "$.name");
    }

    #[test]
    fn fail_fast_stops_early() {
        let node = ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckRequired(vec!["a".into(), "b".into(), "c".into()]),
        ]);
        let opts = ValidationOptions::fail_fast();
        let errs = validate(&node, &json!({}), &opts);
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn array_items_and_bounds() {
        let node = ValidationNode::CheckArray(
            Box::new(ValidationNode::CheckType(DataType::Integer)),
            1,
            3,
        );
        assert!(node_errs(&node, json!([1, 2])).is_empty());
        let errs = node_errs(&node, json!([1, "x", 3, 4]));
        assert_eq!(errs.len(), 2, "type error at [1] + maxItems");
        assert_eq!(errs[0].path, "$", "bounds are reported before item errors");
        assert_eq!(errs[1].path, "$[1]");
    }

    #[test]
    fn unique_items() {
        let node = ValidationNode::CheckUniqueItems(true);
        assert!(node_errs(&node, json!([1, 2, 3])).is_empty());
        assert!(node_errs(&node, json!([])).is_empty());
        // numeric equality: 1 and 1.0 are duplicates
        let errs = node_errs(&node, json!([1, 1.0]));
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn object_shape_unknown_and_pattern_keys() {
        use crate::node::AdditionalProperties;
        let shape = ObjectShape {
            properties: vec![(
                "name".to_string(),
                ValidationNode::CheckType(DataType::String),
            )],
            pattern_properties: vec![(
                Box::new(regex::Regex::new("^S_").unwrap()),
                ValidationNode::CheckType(DataType::String),
            )],
            additional: AdditionalProperties::Forbid,
            required: vec!["name".into()],
        };
        let node = ValidationNode::CheckObjectEx(shape);
        assert!(node_errs(&node, json!({"name": "a", "S_key": "v"})).is_empty());
        let errs = node_errs(&node, json!({"name": "a", "extra": 1}));
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "$.extra");
        let errs = node_errs(&node, json!({"S_key": 5}));
        assert_eq!(
            errs.len(),
            2,
            "missing required + type error on pattern prop"
        );
        assert_eq!(errs[1].path, "$.S_key");
    }

    #[test]
    fn if_then_else() {
        let node = ValidationNode::CheckIfThenElse(
            Box::new(ValidationNode::CheckField(
                "country".into(),
                Box::new(ValidationNode::CheckConst(json!("US"))),
            )),
            Some(Box::new(ValidationNode::CheckRequired(vec!["zip".into()]))),
            Some(Box::new(ValidationNode::CheckRequired(vec![
                "postcode".into()
            ]))),
        );
        assert!(node_errs(&node, json!({"country": "US", "zip": "94103"})).is_empty());
        assert!(node_errs(&node, json!({"country": "DE", "postcode": "10115"})).is_empty());
        assert_eq!(node_errs(&node, json!({"country": "US"})).len(), 1);
        assert_eq!(node_errs(&node, json!({"country": "DE"})).len(), 1);
        // JSON Schema subtlety: `{}` matches the `if` schema vacuously
        // (properties only constrain present keys), so `then` applies.
        assert_eq!(node_errs(&node, json!({})).len(), 1);
    }

    #[test]
    fn combinators() {
        let all_of = ValidationNode::CheckAllOf(vec![
            ValidationNode::CheckMinimum(0.0),
            ValidationNode::CheckMultipleOf(2.0),
        ]);
        assert!(node_errs(&all_of, json!(4)).is_empty());
        assert_eq!(node_errs(&all_of, json!(-1)).len(), 2);

        let any_of = ValidationNode::CheckAnyOf(vec![
            ValidationNode::CheckType(DataType::String),
            ValidationNode::CheckType(DataType::Integer),
        ]);
        assert!(node_errs(&any_of, json!("s")).is_empty());
        assert_eq!(node_errs(&any_of, json!(1.5)).len(), 1);

        let one_of = ValidationNode::CheckOneOf(vec![
            ValidationNode::all(vec![
                ValidationNode::CheckType(DataType::Number),
                ValidationNode::CheckMinimum(5.0),
            ]),
            ValidationNode::CheckType(DataType::Integer),
        ]);
        // 3: first branch fails (3 < 5), second matches -> exactly 1 -> valid
        assert!(node_errs(&one_of, json!(3)).is_empty());
        // 6 matches both branches -> invalid
        assert_eq!(node_errs(&one_of, json!(6)).len(), 1);
        // 2.5 matches neither (first: < 5; second: not integer) -> invalid
        assert_eq!(node_errs(&one_of, json!(2.5)).len(), 1);
        // "x" matches neither -> invalid
        assert_eq!(node_errs(&one_of, json!("x")).len(), 1);

        let not = ValidationNode::CheckNot(Box::new(ValidationNode::CheckType(DataType::String)));
        assert!(node_errs(&not, json!(1)).is_empty());
        assert_eq!(node_errs(&not, json!("s")).len(), 1);
    }

    #[test]
    fn never_and_always() {
        assert_eq!(
            node_errs(&ValidationNode::Never("false schema".into()), json!(1)).len(),
            1
        );
        assert!(node_errs(&ValidationNode::Always, json!(1)).is_empty());
    }

    #[test]
    fn contains_keyword() {
        let node = ValidationNode::CheckContains(Box::new(ValidationNode::CheckMinimum(5.0)));
        assert!(node_errs(&node, json!([1, 6])).is_empty());
        assert_eq!(node_errs(&node, json!([1, 2])).len(), 1);
        // applies to arrays only
        assert!(node_errs(&node, json!(7)).is_empty());
    }

    #[test]
    fn deep_paths() {
        let node = ValidationNode::CheckArray(
            Box::new(ValidationNode::all(vec![
                ValidationNode::CheckType(DataType::Object),
                ValidationNode::CheckField(
                    "zip".into(),
                    Box::new(ValidationNode::CheckType(DataType::String)),
                ),
            ])),
            0,
            usize::MAX,
        );
        let errs = node_errs(&node, json!([{"zip": 1}, {"zip": "ok"}, {"zip": 2}]));
        assert_eq!(errs.len(), 2);
        assert_eq!(errs[0].path, "$[0].zip");
        assert_eq!(errs[1].path, "$[2].zip");
    }

    #[test]
    fn validate_value_short_circuits() {
        let node = ValidationNode::CheckType(DataType::Integer);
        assert!(validate_value(
            &node,
            &json!(1),
            &ValidationOptions::default()
        ));
        assert!(!validate_value(
            &node,
            &json!("1"),
            &ValidationOptions::default()
        ));
    }
}
