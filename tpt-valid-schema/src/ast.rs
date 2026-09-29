//! Schema AST extracted from the parsed JSON value (Draft 2020-12 keyword
//! subset — see `docs/compliance.md` for the supported/unsupported matrix).

use serde_json::Value;

use crate::error::SchemaError;

/// A schema node: either a JSON boolean schema (`true`/`false`) or an object
/// of keywords.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaAst {
    /// The boolean `true` schema (always matches).
    Always,
    /// The boolean `false` schema (never matches).
    Never,
    /// An object of keywords.
    Object(Box<ObjectAst>),
}

/// Object-form schema keywords supported by this compiler.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObjectAst {
    /// `type`: one keyword or a list of keywords.
    pub types: Vec<String>,
    /// `properties`
    pub properties: Vec<(String, SchemaAst)>,
    /// `required`
    pub required: Vec<String>,
    /// `patternProperties` (pattern text; compiled later)
    pub pattern_properties: Vec<(String, SchemaAst)>,
    /// `additionalProperties`: `None` = absent, `Some(false)` = forbid,
    /// `Some(schema)` = sub-schema.
    pub additional_properties: Option<AdditionalAst>,
    /// `items` (single-schema form)
    pub items: Option<Box<SchemaAst>>,
    /// `minItems`
    pub min_items: Option<usize>,
    /// `maxItems`
    pub max_items: Option<usize>,
    /// `uniqueItems`
    pub unique_items: Option<bool>,
    /// `contains`
    pub contains: Option<Box<SchemaAst>>,
    /// `minLength`
    pub min_length: Option<usize>,
    /// `maxLength`
    pub max_length: Option<usize>,
    /// `pattern`
    pub pattern: Option<String>,
    /// `format` (known formats validated; unknown formats warned + ignored
    /// per Draft 2020-12 §7.2.3)
    pub format: Option<String>,
    /// `minimum`
    pub minimum: Option<f64>,
    /// `maximum`
    pub maximum: Option<f64>,
    /// `exclusiveMinimum` (numeric form)
    pub exclusive_minimum: Option<f64>,
    /// `exclusiveMaximum` (numeric form)
    pub exclusive_maximum: Option<f64>,
    /// `multipleOf`
    pub multiple_of: Option<f64>,
    /// `enum`
    pub enum_values: Option<Vec<Value>>,
    /// `const`
    pub const_value: Option<Value>,
    /// `allOf`
    pub all_of: Vec<SchemaAst>,
    /// `anyOf`
    pub any_of: Vec<SchemaAst>,
    /// `oneOf`
    pub one_of: Vec<SchemaAst>,
    /// `not`
    pub not: Option<Box<SchemaAst>>,
    /// `if`
    pub if_schema: Option<Box<SchemaAst>>,
    /// `then`
    pub then_schema: Option<Box<SchemaAst>>,
    /// `else`
    pub else_schema: Option<Box<SchemaAst>>,
}

/// `additionalProperties` AST value.
#[derive(Debug, Clone, PartialEq)]
pub enum AdditionalAst {
    /// `false`
    Forbid,
    /// A schema
    Schema(SchemaAst),
}

/// Unsupported keywords that are explicitly detected and rejected with a
/// helpful message (rather than silently ignored).
pub const REJECTED_KEYWORDS: &[&str] = &["$ref", "$dynamicRef", "prefixItems", "dependsRequired"];

/// Extract an AST from a parsed schema JSON value.
///
/// Unknown keywords are ignored (JSON Schema extensibility) and surfaced as
/// warnings by the compiler. Rejected keywords (see [`REJECTED_KEYWORDS`])
/// produce [`SchemaError::Unsupported`].
pub fn parse_schema(value: &Value) -> Result<SchemaAst, SchemaError> {
    check_rejected_keywords(value)?;
    match value {
        Value::Bool(true) => Ok(SchemaAst::Always),
        Value::Bool(false) => Ok(SchemaAst::Never),
        Value::Object(map) => Ok(SchemaAst::Object(Box::new(parse_object(map)?))),
        other => Err(SchemaError::Syntax(format!(
            "schema must be an object or boolean, got {}",
            crate::type_name(other)
        ))),
    }
}

fn check_rejected_keywords(value: &Value) -> Result<(), SchemaError> {
    match value {
        Value::Object(map) => {
            for kw in REJECTED_KEYWORDS {
                if map.contains_key(*kw) {
                    return Err(SchemaError::unsupported(
                        *kw,
                        match *kw {
                            "$ref" | "$dynamicRef" => {
                                "cross-reference resolution is not implemented; \
                                 inline the referenced schema instead"
                            }
                            "prefixItems" => {
                                "tuple validation is not implemented; use `items` with a \
                                 single schema instead"
                            }
                            _ => "not implemented",
                        },
                    ));
                }
            }
            for v in map.values() {
                check_rejected_keywords(v)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for v in items {
                check_rejected_keywords(v)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn parse_object(map: &serde_json::Map<String, Value>) -> Result<ObjectAst, SchemaError> {
    let mut ast = ObjectAst::default();

    // type: string or array of strings
    if let Some(v) = map.get("type") {
        match v {
            Value::String(s) => {
                validate_type_keyword(s)?;
                ast.types.push(s.clone());
            }
            Value::Array(items) => {
                for item in items {
                    let s = item.as_str().ok_or_else(|| {
                        SchemaError::semantic("type", "array entries must be strings")
                    })?;
                    validate_type_keyword(s)?;
                    ast.types.push(s.to_string());
                }
            }
            _ => {
                return Err(SchemaError::semantic(
                    "type",
                    "must be a string or array of strings",
                ))
            }
        }
    }

    fn sub(node: &Value, keyword: &str) -> Result<SchemaAst, SchemaError> {
        parse_schema(node).map_err(|e| context(e, keyword))
    }

    if let Some(v) = map.get("properties") {
        let m = v
            .as_object()
            .ok_or_else(|| SchemaError::semantic("properties", "must be an object"))?;
        for (k, sub_v) in m {
            ast.properties.push((k.clone(), sub(sub_v, "properties")?));
        }
    }
    if let Some(v) = map.get("required") {
        let a = v
            .as_array()
            .ok_or_else(|| SchemaError::semantic("required", "must be an array"))?;
        for item in a {
            let s = item
                .as_str()
                .ok_or_else(|| SchemaError::semantic("required", "entries must be strings"))?;
            ast.required.push(s.to_string());
        }
    }
    if let Some(v) = map.get("patternProperties") {
        let m = v
            .as_object()
            .ok_or_else(|| SchemaError::semantic("patternProperties", "must be an object"))?;
        for (k, sub_v) in m {
            ast.pattern_properties
                .push((k.clone(), sub(sub_v, "patternProperties")?));
        }
    }
    if let Some(v) = map.get("additionalProperties") {
        ast.additional_properties = Some(match v {
            Value::Bool(false) => AdditionalAst::Forbid,
            other => AdditionalAst::Schema(sub(other, "additionalProperties")?),
        });
    }
    if let Some(v) = map.get("items") {
        if v.is_array() {
            return Err(SchemaError::unsupported(
                "items",
                "tuple form (array) is not supported; use prefixItems instead — which is \
                 also unsupported; flatten to a single `items` schema",
            ));
        }
        ast.items = Some(Box::new(sub(v, "items")?));
    }
    if let Some(v) = map.get("minItems") {
        ast.min_items = Some(non_negative_usize(v, "minItems")?);
    }
    if let Some(v) = map.get("maxItems") {
        ast.max_items = Some(non_negative_usize(v, "maxItems")?);
    }
    if let Some(v) = map.get("uniqueItems") {
        ast.unique_items = Some(
            v.as_bool()
                .ok_or_else(|| SchemaError::semantic("uniqueItems", "must be a boolean"))?,
        );
    }
    if let Some(v) = map.get("contains") {
        ast.contains = Some(Box::new(sub(v, "contains")?));
    }
    if let Some(v) = map.get("minLength") {
        ast.min_length = Some(non_negative_usize(v, "minLength")?);
    }
    if let Some(v) = map.get("maxLength") {
        ast.max_length = Some(non_negative_usize(v, "maxLength")?);
    }
    if let Some(v) = map.get("pattern") {
        ast.pattern = Some(
            v.as_str()
                .ok_or_else(|| SchemaError::semantic("pattern", "must be a string"))?
                .to_string(),
        );
    }
    if let Some(v) = map.get("format") {
        ast.format = Some(
            v.as_str()
                .ok_or_else(|| SchemaError::semantic("format", "must be a string"))?
                .to_string(),
        );
    }
    if let Some(v) = map.get("minimum") {
        ast.minimum = Some(number(v, "minimum")?);
    }
    if let Some(v) = map.get("maximum") {
        ast.maximum = Some(number(v, "maximum")?);
    }
    if let Some(v) = map.get("exclusiveMinimum") {
        ast.exclusive_minimum = Some(number(v, "exclusiveMinimum")?);
    }
    if let Some(v) = map.get("exclusiveMaximum") {
        ast.exclusive_maximum = Some(number(v, "exclusiveMaximum")?);
    }
    if let Some(v) = map.get("multipleOf") {
        let m = number(v, "multipleOf")?;
        if m <= 0.0 {
            return Err(SchemaError::semantic("multipleOf", "must be > 0"));
        }
        ast.multiple_of = Some(m);
    }
    if let Some(v) = map.get("enum") {
        let a = v
            .as_array()
            .ok_or_else(|| SchemaError::semantic("enum", "must be an array"))?;
        ast.enum_values = Some(a.clone());
    }
    if let Some(v) = map.get("const") {
        ast.const_value = Some(v.clone());
    }
    if let Some(v) = map.get("allOf") {
        let a = v
            .as_array()
            .ok_or_else(|| SchemaError::semantic("allOf", "must be an array"))?;
        for item in a {
            ast.all_of.push(sub(item, "allOf")?);
        }
    }
    if let Some(v) = map.get("anyOf") {
        let a = v
            .as_array()
            .ok_or_else(|| SchemaError::semantic("anyOf", "must be an array"))?;
        for item in a {
            ast.any_of.push(sub(item, "anyOf")?);
        }
    }
    if let Some(v) = map.get("oneOf") {
        let a = v
            .as_array()
            .ok_or_else(|| SchemaError::semantic("oneOf", "must be an array"))?;
        for item in a {
            ast.one_of.push(sub(item, "oneOf")?);
        }
    }
    if let Some(v) = map.get("not") {
        ast.not = Some(Box::new(sub(v, "not")?));
    }
    if let Some(v) = map.get("if") {
        ast.if_schema = Some(Box::new(sub(v, "if")?));
    }
    if let Some(v) = map.get("then") {
        ast.then_schema = Some(Box::new(sub(v, "then")?));
    }
    if let Some(v) = map.get("else") {
        ast.else_schema = Some(Box::new(sub(v, "else")?));
    }

    Ok(ast)
}

fn validate_type_keyword(s: &str) -> Result<(), SchemaError> {
    if crate::type_keyword(s).is_none() {
        return Err(SchemaError::semantic(
            "type",
            format!(
                "unknown type \"{s}\" (expected null|boolean|object|array|number|integer|string)"
            ),
        ));
    }
    Ok(())
}

fn non_negative_usize(v: &Value, keyword: &str) -> Result<usize, SchemaError> {
    let n = v
        .as_u64()
        .ok_or_else(|| SchemaError::semantic(keyword, "must be a non-negative integer"))?;
    usize::try_from(n).map_err(|_| SchemaError::semantic(keyword, "value too large"))
}

fn number(v: &Value, keyword: &str) -> Result<f64, SchemaError> {
    v.as_f64()
        .ok_or_else(|| SchemaError::semantic(keyword, "must be a number"))
}

/// Attach parent keyword context to nested errors.
fn context(e: SchemaError, keyword: &str) -> SchemaError {
    match e {
        SchemaError::Syntax(m) => SchemaError::Syntax(format!("{keyword}: {m}")),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn boolean_schemas() {
        assert_eq!(parse_schema(&json!(true)).unwrap(), SchemaAst::Always);
        assert_eq!(parse_schema(&json!(false)).unwrap(), SchemaAst::Never);
    }

    #[test]
    fn object_with_keywords() {
        let ast = parse_schema(&json!({
            "type": ["integer", "null"],
            "properties": {"a": {"type": "string"}},
            "required": ["a"],
            "additionalProperties": false,
            "minimum": 0,
            "enum": [1, 2]
        }))
        .unwrap();
        match ast {
            SchemaAst::Object(o) => {
                assert_eq!(o.types, vec!["integer", "null"]);
                assert_eq!(o.required, vec!["a"]);
                assert_eq!(o.additional_properties, Some(AdditionalAst::Forbid));
                assert_eq!(o.minimum, Some(0.0));
            }
            _ => panic!("expected object ast"),
        }
    }

    #[test]
    fn unknown_types_rejected() {
        assert!(parse_schema(&json!({"type": "strin"})).is_err());
        assert!(parse_schema(&json!({"type": 5})).is_err());
    }

    #[test]
    fn rejected_keywords_surface_as_unsupported() {
        let e = parse_schema(&json!({"$ref": "#/definitions/x"})).unwrap_err();
        assert!(matches!(e, SchemaError::Unsupported { .. }));
        let e = parse_schema(&json!({"properties": {"a": {"prefixItems": []}}})).unwrap_err();
        assert!(matches!(e, SchemaError::Unsupported { .. }));
    }

    #[test]
    fn non_object_schema_rejected() {
        assert!(parse_schema(&json!("schema")).is_err());
        assert!(parse_schema(&json!(5)).is_err());
    }
}
