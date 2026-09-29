//! The validation state machine: a tree of [`ValidationNode`]s (spec §5.2).

use std::collections::HashSet;

use regex::Regex;
use serde_json::Value;

use crate::format::Format;
use crate::types::DataType;

/// Pre-compiled regular expression for `pattern` / `patternProperties`.
pub type CompiledPattern = Box<Regex>;

/// Hash-set-backed enum store for O(1) membership tests (spec §5.2:
/// "Enum checks use hash sets for O(1) lookup").
///
/// Values are *number-normalized* before insertion and lookup, so `1` and
/// `1.0` compare equal per JSON Schema's numeric equality.
#[derive(Debug, Clone)]
pub struct EnumSet {
    values: HashSet<Value>,
    display: Vec<Value>,
}

impl EnumSet {
    /// Build an enum set from candidate values (number-normalized).
    pub fn new(values: impl IntoIterator<Item = Value>) -> Self {
        let values: HashSet<Value> = values.into_iter().map(normalize_numbers).collect();
        let mut display: Vec<Value> = values.iter().cloned().collect();
        display.sort_by_key(|v| v.to_string());
        Self { values, display }
    }

    /// O(1) membership test.
    pub fn contains(&self, value: &Value) -> bool {
        self.values.contains(&normalize_numbers_ref(value))
    }

    /// The allowed values (for error messages), sorted for stable output.
    pub fn allowed(&self) -> &[Value] {
        &self.display
    }

    /// Number of allowed values.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the set has no allowed values (matches nothing).
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Replace integral floats with their integer representation so that `1`
/// and `1.0` hash equal (JSON Schema numeric equality).
pub fn normalize_numbers(value: Value) -> Value {
    match value {
        Value::Number(n) => {
            if let Some(m) = integral_float_to_number(&n) {
                Value::Number(m)
            } else {
                Value::Number(n)
            }
        }
        Value::Array(a) => Value::Array(a.into_iter().map(normalize_numbers).collect()),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| (k, normalize_numbers(v)))
                .collect(),
        ),
        other => other,
    }
}

/// If `n` is a float with a zero fractional part, return it re-backed by the
/// smallest matching integer type.
fn integral_float_to_number(n: &serde_json::Number) -> Option<serde_json::Number> {
    if !n.is_f64() {
        return None;
    }
    let f = n.as_f64()?;
    if !f.is_finite() || f.fract() != 0.0 {
        return None;
    }
    if f >= i64::MIN as f64 && f < i64::MAX as f64 {
        Some(serde_json::Number::from(f as i64))
    } else if f >= 0.0 && f < u64::MAX as f64 {
        Some(serde_json::Number::from(f as u64))
    } else {
        None
    }
}

fn normalize_numbers_ref(value: &Value) -> Value {
    normalize_numbers(value.clone())
}

/// How `additionalProperties` treats keys not covered by `properties` /
/// `patternProperties`.
#[derive(Debug, Clone, Default)]
pub enum AdditionalProperties {
    /// `true` (or absent): unconstrained.
    #[default]
    Allow,
    /// `false`: unknown keys are rejected.
    Forbid,
    /// A schema every unknown key's value must satisfy.
    Schema(Box<ValidationNode>),
}

/// Full object validation shape emitted by the schema compiler:
/// `properties` + `patternProperties` + `additionalProperties` + `required`
/// combined so unknown-key accounting is exact.
#[derive(Debug, Clone, Default)]
pub struct ObjectShape {
    /// Property-name → schema.
    pub properties: Vec<(String, ValidationNode)>,
    /// Pattern → schema, applied to every matching key.
    pub pattern_properties: Vec<(CompiledPattern, ValidationNode)>,
    /// Rule for keys not covered above.
    pub additional: AdditionalProperties,
    /// Keys that must be present.
    pub required: Vec<String>,
}

/// One node of the validation state machine (spec §5.2).
///
/// Nodes form a tree; the engine traverses it per instance and collects all
/// violations. Type-specific keywords only apply to instances of their type
/// (JSON Schema semantics) — e.g. [`ValidationNode::CheckMinimum`] silently
/// accepts non-numbers, which a sibling [`ValidationNode::CheckType`] flags.
#[derive(Debug, Clone)]
pub enum ValidationNode {
    /// `type` — instance must be of this JSON type.
    CheckType(DataType),
    /// Named property must satisfy the sub-node (no-op when absent).
    CheckField(String, Box<ValidationNode>),
    /// `required` — all listed keys must be present on an object.
    CheckRequired(Vec<String>),
    /// `minimum`
    CheckMinimum(f64),
    /// `maximum`
    CheckMaximum(f64),
    /// `exclusiveMinimum` (numeric form, Draft 2020-12)
    CheckExclusiveMinimum(f64),
    /// `exclusiveMaximum` (numeric form, Draft 2020-12)
    CheckExclusiveMaximum(f64),
    /// `multipleOf`
    CheckMultipleOf(f64),
    /// `minLength` (in characters)
    CheckMinLength(usize),
    /// `maxLength` (in characters)
    CheckMaxLength(usize),
    /// `pattern` — pre-compiled regex.
    CheckPattern(CompiledPattern),
    /// `enum` — hash-set membership.
    CheckEnum(EnumSet),
    /// `const` — deep equality with numeric normalization.
    CheckConst(Value),
    /// `format` — built-in format validators.
    CheckFormat(Format),
    /// Array constraints: `items` schema plus `minItems` / `maxItems`
    /// (`usize::MAX` = unbounded).
    CheckArray(Box<ValidationNode>, usize, usize),
    /// `uniqueItems`
    CheckUniqueItems(bool),
    /// `contains` — at least one array item must match.
    CheckContains(Box<ValidationNode>),
    /// `properties` (name → schema).
    CheckObject(Vec<(String, ValidationNode)>),
    /// Compiled object shape: `properties` + `patternProperties` +
    /// `additionalProperties` + `required` in one node.
    CheckObjectEx(ObjectShape),
    /// `if` / `then` / `else` — conditional validation.
    CheckIfThenElse(
        Box<ValidationNode>,
        Option<Box<ValidationNode>>,
        Option<Box<ValidationNode>>,
    ),
    /// `allOf` — every sub-schema must match.
    CheckAllOf(Vec<ValidationNode>),
    /// `anyOf` — at least one sub-schema must match.
    CheckAnyOf(Vec<ValidationNode>),
    /// `oneOf` — exactly one sub-schema must match.
    CheckOneOf(Vec<ValidationNode>),
    /// `not` — the sub-schema must not match.
    CheckNot(Box<ValidationNode>),
    /// Optimized sequence of checks applied to the same instance (compiler
    /// output for a schema with several keywords).
    CheckAll(Vec<ValidationNode>),
    /// The boolean `true` schema / empty schema: always matches.
    Always,
    /// The boolean `false` schema: never matches.
    Never(String),
}

impl ValidationNode {
    /// Convenience constructor for `CheckType`.
    pub fn type_of(t: DataType) -> ValidationNode {
        ValidationNode::CheckType(t)
    }

    /// Convenience constructor for `CheckAll` (flattens nested sequences and
    /// drops no-op `Always` nodes).
    pub fn all(nodes: Vec<ValidationNode>) -> ValidationNode {
        let mut flat = Vec::with_capacity(nodes.len());
        for n in nodes {
            match n {
                ValidationNode::CheckAll(inner) => flat.extend(inner),
                ValidationNode::Always => {}
                other => flat.push(other),
            }
        }
        match flat.len() {
            0 => ValidationNode::Always,
            1 => flat.into_iter().next().unwrap(),
            _ => ValidationNode::CheckAll(flat),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enum_set_numeric_equality() {
        let set = EnumSet::new(vec![json!(1.0), json!("a"), json!([2.5])]);
        assert!(set.contains(&json!(1))); // 1 == 1.0
        assert!(set.contains(&json!("a")));
        assert!(set.contains(&json!([2.5])));
        assert!(!set.contains(&json!(2)));
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn normalize_deep() {
        let v = normalize_numbers(json!({"a": [1.0, {"b": 2.0}], "c": 1.5}));
        assert_eq!(v, json!({"a": [1, {"b": 2}], "c": 1.5}));
    }

    #[test]
    fn all_flattens_and_simplifies() {
        let n = ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::String),
            ValidationNode::Always,
            ValidationNode::all(vec![
                ValidationNode::CheckMinLength(1),
                ValidationNode::CheckMaxLength(5),
            ]),
        ]);
        match n {
            ValidationNode::CheckAll(v) => assert_eq!(v.len(), 3),
            other => panic!("expected CheckAll, got {other:?}"),
        }
        let single = ValidationNode::all(vec![ValidationNode::Always]);
        assert!(matches!(single, ValidationNode::Always));
    }
}
