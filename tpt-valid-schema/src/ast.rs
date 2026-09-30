//! Schema AST extracted from the parsed JSON value (Draft 2020-12 keyword
//! subset — see `docs/compliance.md` for the supported/unsupported matrix).
//!
//! `$ref` support: local `#/...` JSON-Pointer references into the same
//! document, `#anchor` fragment references (from `$anchor` / `#`-form
//! `$id`), and cross-document references through a user-supplied
//! [`SchemaRegistry`]. References are expanded inline (the compiled state
//! machine is a tree), so genuinely recursive schemas are detected and
//! rejected with a clear error instead of recursing forever.

use serde_json::Value;
use std::collections::HashMap;

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
    /// `unevaluatedProperties` (same shape; static accounting — see the
    /// compliance matrix).
    pub unevaluated_properties: Option<AdditionalAst>,
    /// `items` (single-schema form)
    pub items: Option<Box<SchemaAst>>,
    /// `items` (tuple/array form; Draft ≤07 style, mapped to `prefixItems`
    /// after any explicit `prefixItems`)
    pub tuple_items: Option<Vec<SchemaAst>>,
    /// `prefixItems`
    pub prefix_items: Vec<SchemaAst>,
    /// `unevaluatedItems`
    pub unevaluated_items: Option<Box<SchemaAst>>,
    /// `minItems`
    pub min_items: Option<usize>,
    /// `maxItems`
    pub max_items: Option<usize>,
    /// `uniqueItems`
    pub unique_items: Option<bool>,
    /// `contains`
    pub contains: Option<Box<SchemaAst>>,
    /// `minContains`
    pub min_contains: Option<usize>,
    /// `maxContains`
    pub max_contains: Option<usize>,
    /// `dependentRequired` — key → keys required alongside it.
    pub dependent_required: Vec<(String, Vec<String>)>,
    /// `dependentSchemas` — key → schema applied when the key is present.
    pub dependent_schemas: Vec<(String, SchemaAst)>,
    /// `propertyNames`
    pub property_names: Option<Box<SchemaAst>>,
    /// `minLength`
    pub min_length: Option<usize>,
    /// `maxLength`
    pub max_length: Option<usize>,
    /// `pattern`
    pub pattern: Option<String>,
    /// `format` (known formats validated; unknown formats become runtime
    /// custom-format checks and are warned about)
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

/// `additionalProperties` / `unevaluatedProperties` AST value.
#[derive(Debug, Clone, PartialEq)]
pub enum AdditionalAst {
    /// `false`
    Forbid,
    /// A schema
    Schema(SchemaAst),
}

/// Unsupported keywords that are explicitly detected and rejected with a
/// helpful message (rather than silently ignored).
pub const REJECTED_KEYWORDS: &[&str] = &["$dynamicRef", "$dynamicAnchor"];

/// User-supplied registry of external schema documents for cross-file
/// `$ref` resolution: URI → parsed schema document.
///
/// # Examples
///
/// ```
/// use tpt_valid_schema::{SchemaRegistry, Validator};
/// use serde_json::json;
///
/// let mut registry = SchemaRegistry::new();
/// registry
///     .insert("https://example.com/address.json", r#"{
///         "type": "object",
///         "properties": {"city": {"type": "string"}},
///         "required": ["city"]
///     }"#)
///     .unwrap();
///
/// let v = Validator::new_with(
///     r#"{
///         "type": "object",
///         "properties": {
///             "home": {"$ref": "https://example.com/address.json"}
///         },
///         "required": ["home"]
///     }"#,
///     &registry,
/// )
/// .unwrap();
///
/// assert!(v.validate(&json!({"home": {"city": "Berlin"}})).is_valid());
/// assert!(!v.validate(&json!({"home": {}})).is_valid());
/// ```
#[derive(Debug, Clone, Default)]
pub struct SchemaRegistry {
    docs: HashMap<String, Value>,
}

impl SchemaRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse `schema_text` and register it under `uri`.
    pub fn insert(&mut self, uri: impl Into<String>, schema_text: &str) -> Result<(), SchemaError> {
        let value = crate::json::from_str(schema_text)?;
        self.docs.insert(uri.into(), value);
        Ok(())
    }

    /// Register an already-parsed document under `uri`.
    pub fn insert_value(&mut self, uri: impl Into<String>, value: Value) {
        self.docs.insert(uri.into(), value);
    }

    /// Look up a registered document.
    pub fn get(&self, uri: &str) -> Option<&Value> {
        self.docs.get(uri)
    }

    /// Number of registered documents.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }
}

/// Maximum nesting depth accepted when parsing a schema [`Value`] into an
/// AST. Matches the schema reader's [`crate::json::DEFAULT_MAX_DEPTH`], so
/// text-parsed schemas always fit; this guards the `from_value` path, where
/// a programmatically-built deep value would otherwise overflow the
/// recursive parser. It also bounds `$ref` expansion: a chain of refs
/// nesting deeper than this (including genuinely recursive schemas) is
/// rejected as cyclic.
pub const MAX_SCHEMA_DEPTH: usize = 128;

/// Extract an AST from a parsed schema JSON value.
///
/// Unknown keywords are ignored (JSON Schema extensibility) and surfaced as
/// warnings by the compiler. Rejected keywords (see [`REJECTED_KEYWORDS`])
/// produce [`SchemaError::Unsupported`].
pub fn parse_schema(value: &Value) -> Result<SchemaAst, SchemaError> {
    parse_schema_with(value, &SchemaRegistry::new())
}

/// Extract an AST with cross-document `$ref` resolution through `registry`.
pub fn parse_schema_with(
    value: &Value,
    registry: &SchemaRegistry,
) -> Result<SchemaAst, SchemaError> {
    let mut ctx = RefCtx {
        root: value,
        anchors: HashMap::new(),
        registry,
        in_flight: Vec::new(),
    };
    collect_anchors(value, &mut ctx, 0)?;
    check_rejected_keywords(value, 0)?;
    parse_schema_at(value, &mut ctx, 0)
}

/// Reference-resolution context shared across one schema parse.
struct RefCtx<'a> {
    root: &'a Value,
    anchors: HashMap<String, &'a Value>,
    registry: &'a SchemaRegistry,
    /// `$ref` strings currently being expanded (cycle detection).
    in_flight: Vec<String>,
}

impl RefCtx<'_> {
    /// Resolve a `$ref` string to the subschema it points at. The target is
    /// returned as an owned clone so it can be parsed without aliasing the
    /// context (ref targets are usually small; cloning happens once per
    /// expansion site).
    fn resolve(&self, reference: &str) -> Result<Value, SchemaError> {
        if reference.starts_with("#/") {
            return resolve_pointer(self.root, &reference[1..], reference).cloned();
        }
        if let Some(target) = self.anchors.get(reference) {
            return Ok((*target).clone());
        }
        // External document: everything before the '#' (or the whole string
        // when there is no fragment).
        let (uri, fragment) = match reference.split_once('#') {
            Some((uri, frag)) => (uri, Some(frag)),
            None => (reference, None),
        };
        if uri.is_empty() {
            return Err(SchemaError::unsupported(
                "$ref",
                format!("cannot resolve reference \"{reference}\": no such local anchor"),
            ));
        }
        let doc = self.registry.get(uri).ok_or_else(|| {
            SchemaError::unsupported(
                "$ref",
                format!(
                    "external reference \"{uri}\" is not in the schema registry; \
                     register it via Validator::new_with / parse_schema_with"
                ),
            )
        })?;
        match fragment {
            None | Some("") => Ok(doc.clone()),
            Some(frag) => resolve_pointer(doc, frag, reference).cloned(),
        }
    }
}

/// Resolve an RFC 6901 JSON Pointer (without the leading '#') in `doc`.
fn resolve_pointer<'a>(
    doc: &'a Value,
    pointer: &str,
    reference: &str,
) -> Result<&'a Value, SchemaError> {
    if pointer.is_empty() {
        return Ok(doc);
    }
    let mut cur = doc;
    for raw in pointer.split('/').skip(1) {
        // RFC 6901: ~1 → '/' first, then ~0 → '~'.
        let token = raw.replace("~1", "/").replace("~0", "~");
        cur = match cur {
            Value::Object(map) => map.get(&token).ok_or_else(|| {
                SchemaError::unsupported(
                    "$ref",
                    format!("unresolved reference \"{reference}\": no key \"{token}\""),
                )
            })?,
            Value::Array(items) => {
                let idx: usize = token.parse().map_err(|_| {
                    SchemaError::unsupported(
                        "$ref",
                        format!("unresolved reference \"{reference}\": \"{token}\" is not an index"),
                    )
                })?;
                items.get(idx).ok_or_else(|| {
                    SchemaError::unsupported(
                        "$ref",
                        format!("unresolved reference \"{reference}\": index {idx} out of bounds"),
                    )
                })?
            }
            _ => {
                return Err(SchemaError::unsupported(
                    "$ref",
                    format!("unresolved reference \"{reference}\": \"{token}\" into a scalar"),
                ))
            }
        };
    }
    Ok(cur)
}

/// Collect `#anchor` references: `$anchor: "name"` and `$id: "#name"`.
fn collect_anchors<'a>(
    value: &'a Value,
    ctx: &mut RefCtx<'a>,
    depth: usize,
) -> Result<(), SchemaError> {
    if depth > 512 {
        // The anchor walk must stay bounded even for programmatic values;
        // real anchor tables live near the document root.
        return Ok(());
    }
    match value {
        Value::Object(map) => {
            if let Some(Value::String(name)) = map.get("$anchor") {
                ctx.anchors.insert(format!("#{name}"), value);
            }
            if let Some(Value::String(id)) = map.get("$id") {
                if id.starts_with('#') {
                    ctx.anchors.insert(id.clone(), value);
                }
            }
            for v in map.values() {
                collect_anchors(v, ctx, depth + 1)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for v in items {
                collect_anchors(v, ctx, depth + 1)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn check_rejected_keywords(value: &Value, depth: usize) -> Result<(), SchemaError> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(SchemaError::Syntax(format!(
            "schema nesting exceeds the maximum depth of {MAX_SCHEMA_DEPTH} levels"
        )));
    }
    match value {
        Value::Object(map) => {
            for kw in REJECTED_KEYWORDS {
                if map.contains_key(*kw) {
                    return Err(SchemaError::unsupported(
                        *kw,
                        "dynamic references are not implemented; use plain `$ref` \
                         with `$defs` / `$anchor` instead",
                    ));
                }
            }
            for v in map.values() {
                check_rejected_keywords(v, depth + 1)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for v in items {
                check_rejected_keywords(v, depth + 1)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn parse_schema_at(
    value: &Value,
    ctx: &mut RefCtx<'_>,
    depth: usize,
) -> Result<SchemaAst, SchemaError> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(SchemaError::Syntax(format!(
            "schema nesting exceeds the maximum depth of {MAX_SCHEMA_DEPTH} levels \
             (recursive `$ref`?)"
        )));
    }
    match value {
        Value::Bool(true) => Ok(SchemaAst::Always),
        Value::Bool(false) => Ok(SchemaAst::Never),
        Value::Object(map) => {
            let mut ast = parse_object(map, ctx, depth)?;
            if let Some(Value::String(reference)) = map.get("$ref") {
                if ctx.in_flight.iter().any(|r| r == reference) {
                    return Err(SchemaError::unsupported(
                        "$ref",
                        format!(
                            "recursive reference \"{reference}\" cannot be expanded; \
                             inline or restructure the schema"
                        ),
                    ));
                }
                let resolved = ctx.resolve(reference)?;
                ctx.in_flight.push(reference.clone());
                let target =
                    parse_schema_at(&resolved, ctx, depth + 1).map_err(|e| ref_context(e, reference))?;
                ctx.in_flight.pop();
                // `$ref` composes with sibling keywords as a conjunction
                // (Draft 2020-12); the compiler folds `allOf` members.
                ast.all_of.push(target);
            }
            Ok(SchemaAst::Object(Box::new(ast)))
        }
        other => Err(SchemaError::Syntax(format!(
            "schema must be an object or boolean, got {}",
            crate::type_name(other)
        ))),
    }
}

fn parse_object(
    map: &serde_json::Map<String, Value>,
    ctx: &mut RefCtx<'_>,
    depth: usize,
) -> Result<ObjectAst, SchemaError> {
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

    let mut sub = |node: &Value, keyword: &str| -> Result<SchemaAst, SchemaError> {
        parse_schema_at(node, ctx, depth + 1).map_err(|e| context(e, keyword))
    };

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
        ast.additional_properties = Some(additional_ast(v, &mut sub, "additionalProperties")?);
    }
    if let Some(v) = map.get("unevaluatedProperties") {
        ast.unevaluated_properties = Some(additional_ast(v, &mut sub, "unevaluatedProperties")?);
    }
    if let Some(v) = map.get("items") {
        match v {
            Value::Array(tuple) => {
                let mut parsed = Vec::with_capacity(tuple.len());
                for item in tuple.iter() {
                    parsed.push(sub(item, "items")?);
                }
                ast.tuple_items = Some(parsed);
            }
            other => {
                ast.items = Some(Box::new(sub(other, "items")?));
            }
        }
    }
    if let Some(v) = map.get("prefixItems") {
        let a = v
            .as_array()
            .ok_or_else(|| SchemaError::semantic("prefixItems", "must be an array"))?;
        for item in a.iter() {
            ast.prefix_items.push(sub(item, "prefixItems")?);
        }
    }
    if let Some(v) = map.get("unevaluatedItems") {
        ast.unevaluated_items = Some(Box::new(sub(v, "unevaluatedItems")?));
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
    if let Some(v) = map.get("minContains") {
        ast.min_contains = Some(non_negative_usize(v, "minContains")?);
    }
    if let Some(v) = map.get("maxContains") {
        ast.max_contains = Some(non_negative_usize(v, "maxContains")?);
    }
    if let (Some(min), Some(max)) = (ast.min_contains, ast.max_contains) {
        if min > max {
            return Err(SchemaError::semantic(
                "minContains",
                "minContains must be <= maxContains",
            ));
        }
    }
    if let Some(v) = map.get("dependentRequired") {
        let m = v
            .as_object()
            .ok_or_else(|| SchemaError::semantic("dependentRequired", "must be an object"))?;
        for (k, deps) in m {
            let list = deps
                .as_array()
                .ok_or_else(|| SchemaError::semantic("dependentRequired", "must map to arrays"))?;
            let mut names = Vec::with_capacity(list.len());
            for d in list {
                names.push(
                    d.as_str()
                        .ok_or_else(|| {
                            SchemaError::semantic("dependentRequired", "entries must be strings")
                        })?
                        .to_string(),
                );
            }
            ast.dependent_required.push((k.clone(), names));
        }
    }
    if let Some(v) = map.get("dependentSchemas") {
        let m = v
            .as_object()
            .ok_or_else(|| SchemaError::semantic("dependentSchemas", "must be an object"))?;
        for (k, sub_v) in m {
            ast.dependent_schemas
                .push((k.clone(), sub(sub_v, "dependentSchemas")?));
        }
    }
    if let Some(v) = map.get("propertyNames") {
        ast.property_names = Some(Box::new(sub(v, "propertyNames")?));
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

fn additional_ast(
    v: &Value,
    sub: &mut dyn FnMut(&Value, &str) -> Result<SchemaAst, SchemaError>,
    keyword: &str,
) -> Result<AdditionalAst, SchemaError> {
    match v {
        Value::Bool(false) => Ok(AdditionalAst::Forbid),
        other => Ok(AdditionalAst::Schema(sub(other, keyword)?)),
    }
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

/// Attach the offending `$ref` to resolution errors.
fn ref_context(e: SchemaError, reference: &str) -> SchemaError {
    match e {
        SchemaError::Syntax(m) => SchemaError::Syntax(format!("$ref \"{reference}\": {m}")),
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
        let e = parse_schema(&json!({"$dynamicRef": "#x"})).unwrap_err();
        assert!(matches!(e, SchemaError::Unsupported { .. }));
    }

    #[test]
    fn non_object_schema_rejected() {
        assert!(parse_schema(&json!("schema")).is_err());
        assert!(parse_schema(&json!(5)).is_err());
    }

    #[test]
    fn local_refs_resolve() {
        let ast = parse_schema(&json!({
            "type": "object",
            "properties": {
                "a": {"$ref": "#/$defs/string-or-null"}
            },
            "$defs": {
                "string-or-null": {"type": ["string", "null"]}
            }
        }))
        .unwrap();
        let SchemaAst::Object(o) = ast else { panic!("object") };
        assert_eq!(o.properties.len(), 1);
        // The ref target became a conjunction member of the property schema.
        let SchemaAst::Object(inner) = &o.properties[0].1 else {
            panic!("object property")
        };
        assert_eq!(inner.all_of.len(), 1);
    }

    #[test]
    fn recursive_refs_are_detected() {
        let err = parse_schema(&json!({
            "type": "object",
            "properties": {
                "child": {"$ref": "#/$defs/node"}
            },
            "$defs": {
                "node": {
                    "type": "object",
                    "properties": {"next": {"$ref": "#/$defs/node"}}
                }
            }
        }))
        .unwrap_err();
        assert!(matches!(err, SchemaError::Unsupported { .. }));
        assert!(err.to_string().contains("recursive"));
    }

    #[test]
    fn anchor_refs_resolve() {
        let ast = parse_schema(&json!({
            "type": "object",
            "properties": {"a": {"$ref": "#positive"}},
            "$defs": {
                "n": {"$anchor": "positive", "type": "integer", "minimum": 0}
            }
        }))
        .unwrap();
        let SchemaAst::Object(o) = ast else { panic!("object") };
        assert_eq!(o.properties[0].0, "a");
    }

    #[test]
    fn external_refs_use_registry() {
        let mut registry = SchemaRegistry::new();
        registry
            .insert("https://example.com/x.json", r#"{"type": "string"}"#)
            .unwrap();
        let ast = parse_schema_with(&json!({"$ref": "https://example.com/x.json"}), &registry)
            .unwrap();
        let SchemaAst::Object(o) = ast else { panic!("object") };
        // The ref target is a conjunction member of the root schema.
        assert!(o.types.is_empty());
        assert_eq!(o.all_of.len(), 1);
        let SchemaAst::Object(target) = &o.all_of[0] else { panic!("target object") };
        assert_eq!(target.types, vec!["string".to_string()]);

        // Unknown external ref errors helpfully.
        let err = parse_schema(&json!({"$ref": "https://example.com/missing.json"})).unwrap_err();
        assert!(err.to_string().contains("registry"));
    }

    #[test]
    fn depth_bounded() {
        let mut deep = json!(true);
        for _ in 0..(MAX_SCHEMA_DEPTH + 10) {
            deep = json!({"properties": {"a": deep}});
        }
        let err = parse_schema(&deep).unwrap_err();
        assert!(err.to_string().contains("depth"));
    }
}
