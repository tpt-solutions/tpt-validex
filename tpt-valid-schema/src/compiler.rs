//! The schema compiler: AST → IR → optimized IR → [`ValidationNode`] state
//! machine (spec §4.2, §5.1), with semantic validation of the schema itself.

use std::collections::HashSet;

use regex::Regex;

use tpt_valid_core::{
    AdditionalProperties, DataType, EnumSet, Format, ObjectShape, ValidationNode,
};

use crate::ast::{AdditionalAst, ObjectAst, SchemaAst};
use crate::error::SchemaError;
use crate::ir::{IrAdditional, IrOp, IrSchema};
use crate::Warning;

/// Compile a schema AST into the validation state machine.
///
/// Returns the machine plus non-fatal warnings (unknown formats etc.).
pub fn compile(ast: &SchemaAst) -> Result<(ValidationNode, Vec<Warning>), SchemaError> {
    let mut warnings = Vec::new();
    let ir = build_ir(ast, &mut warnings)?;
    let ir = crate::ir::optimize(ir);
    Ok((lower(&ir), warnings))
}

/// Build unoptimized IR from an AST.
pub fn build_ir(ast: &SchemaAst, warnings: &mut Vec<Warning>) -> Result<IrSchema, SchemaError> {
    match ast {
        SchemaAst::Always => Ok(IrSchema::always()),
        SchemaAst::Never => Ok(IrSchema {
            ops: vec![IrOp::Never("false schema".into())],
        }),
        SchemaAst::Object(obj) => build_object_ir(obj, warnings),
    }
}

fn build_object_ir(obj: &ObjectAst, warnings: &mut Vec<Warning>) -> Result<IrSchema, SchemaError> {
    // Semantic cross-keyword checks.
    if let (Some(min), Some(max)) = (obj.minimum, obj.maximum) {
        if min > max {
            return Err(SchemaError::semantic(
                "minimum",
                "minimum must be <= maximum",
            ));
        }
    }
    if let (Some(min), Some(max)) = (obj.min_length, obj.max_length) {
        if min > max {
            return Err(SchemaError::semantic(
                "minLength",
                "minLength must be <= maxLength",
            ));
        }
    }
    if let (Some(min), Some(max)) = (obj.min_items, obj.max_items) {
        if min > max {
            return Err(SchemaError::semantic(
                "minItems",
                "minItems must be <= maxItems",
            ));
        }
    }

    let mut ops: Vec<IrOp> = Vec::new();

    if !obj.types.is_empty() {
        let mut types: Vec<DataType> = obj
            .types
            .iter()
            .map(|t| crate::type_keyword(t).expect("validated in AST"))
            .collect();
        types.dedup();
        ops.push(IrOp::Type(types));
    }
    if let Some(b) = obj.minimum {
        ops.push(IrOp::Minimum {
            bound: b,
            exclusive: false,
        });
    }
    if let Some(b) = obj.exclusive_minimum {
        ops.push(IrOp::Minimum {
            bound: b,
            exclusive: true,
        });
    }
    if let Some(b) = obj.maximum {
        ops.push(IrOp::Maximum {
            bound: b,
            exclusive: false,
        });
    }
    if let Some(b) = obj.exclusive_maximum {
        ops.push(IrOp::Maximum {
            bound: b,
            exclusive: true,
        });
    }
    if let Some(m) = obj.multiple_of {
        ops.push(IrOp::MultipleOf(m));
    }
    if let Some(n) = obj.min_length {
        ops.push(IrOp::MinLength(n));
    }
    if let Some(n) = obj.max_length {
        ops.push(IrOp::MaxLength(n));
    }
    if let Some(n) = obj.min_properties {
        ops.push(IrOp::MinProperties(n));
    }
    if let Some(n) = obj.max_properties {
        ops.push(IrOp::MaxProperties(n));
    }
    if let Some(p) = &obj.pattern {
        let re = Regex::new(p)
            .map_err(|e| SchemaError::semantic("pattern", format!("invalid regex: {e}")))?;
        ops.push(IrOp::Pattern(re));
    }
    if let Some(values) = &obj.enum_values {
        if values.is_empty() {
            ops.push(IrOp::Never("enum has no allowed values".into()));
        } else {
            ops.push(IrOp::Enum(EnumSet::new(values.iter().cloned())));
        }
    }
    if let Some(v) = &obj.const_value {
        ops.push(IrOp::Const(v.clone()));
    }
    if let Some(f) = &obj.format {
        match Format::from_keyword(f) {
            Some(fmt) => ops.push(IrOp::Format(fmt)),
            None => {
                // Not built-in: emitted as a runtime custom-format check so
                // applications can register an assertion for it; without a
                // registration it is a no-op (Draft 2020-12 §7.2.3).
                warnings.push(format!(
                    "format \"{f}\" is not built-in; it is validated only if a custom \
                     format function is registered at validation time"
                ));
                ops.push(IrOp::CustomFormat(f.clone()));
            }
        }
    }
    if obj.items.is_some()
        || obj.tuple_items.is_some()
        || !obj.prefix_items.is_empty()
        || obj.min_items.is_some()
        || obj.max_items.is_some()
        || obj.unevaluated_items.is_some()
    {
        let min_items = obj.min_items.unwrap_or(0);
        let max_items = obj.max_items.unwrap_or(usize::MAX);

        // Positional schemas: `prefixItems` first, then Draft ≤07 tuple-form
        // `items` entries (concatenated — documented in the compliance
        // matrix).
        let mut prefixes = Vec::new();
        for s in obj
            .prefix_items
            .iter()
            .chain(obj.tuple_items.iter().flatten())
        {
            prefixes.push(build_ir(s, warnings)?);
        }
        // Schema-form `items` applies to items beyond the prefix. Tuple-form
        // `items` IS the prefix, so it does not also constrain the rest.
        let rest = if obj.tuple_items.is_none() {
            match &obj.items {
                Some(s) => Some(Box::new(build_ir(s, warnings)?)),
                None => None,
            }
        } else {
            None
        };
        // `unevaluatedItems` constrains items beyond the prefix when no
        // `items` schema evaluates them; with schema-form `items` every item
        // is evaluated, so it is vacuous and dropped.
        let unevaluated = if rest.is_none() && obj.tuple_items.is_none() {
            match &obj.unevaluated_items {
                Some(s) => Some(Box::new(build_ir(s, warnings)?)),
                None => None,
            }
        } else {
            None
        };

        if prefixes.is_empty() && unevaluated.is_none() {
            // Plain schema-form `items` (or bounds only): one node carrying
            // schema + bounds, bounds reported first.
            let items_schema = match rest {
                Some(s) => *s,
                None => IrSchema::always(),
            };
            ops.push(IrOp::Items {
                schema: Box::new(items_schema),
                min_items,
                max_items,
            });
        } else {
            ops.push(IrOp::PrefixItems {
                prefixes,
                items: rest,
                unevaluated,
            });
            if obj.min_items.is_some() || obj.max_items.is_some() {
                ops.push(IrOp::Items {
                    schema: Box::new(IrSchema::always()),
                    min_items,
                    max_items,
                });
            }
        }
    }
    if obj.unique_items == Some(true) {
        ops.push(IrOp::UniqueItems);
    }
    if let Some(c) = &obj.contains {
        ops.push(IrOp::Contains {
            schema: Box::new(build_ir(c, warnings)?),
            min: obj.min_contains.unwrap_or(1),
            max: obj.max_contains,
        });
    }

    // Object keywords: one combined op per schema node so that
    // `required` + `properties` + `patternProperties` + `additionalProperties`
    // share exact unknown-key accounting (and allOf merging).
    if !obj.properties.is_empty()
        || !obj.pattern_properties.is_empty()
        || !obj.required.is_empty()
        || obj.additional_properties.is_some()
        || obj.unevaluated_properties.is_some()
    {
        let mut properties = Vec::with_capacity(obj.properties.len());
        for (name, sub) in &obj.properties {
            properties.push((name.clone(), build_ir(sub, warnings)?));
        }
        let mut pattern_properties = Vec::with_capacity(obj.pattern_properties.len());
        for (pat, sub) in &obj.pattern_properties {
            let re = Regex::new(pat).map_err(|e| {
                SchemaError::semantic("patternProperties", format!("invalid regex: {e}"))
            })?;
            pattern_properties.push((re, build_ir(sub, warnings)?));
        }
        let additional = match &obj.additional_properties {
            None | Some(AdditionalAst::Schema(SchemaAst::Always)) => IrAdditional::Allow,
            Some(AdditionalAst::Forbid) => IrAdditional::Forbid,
            Some(AdditionalAst::Schema(s)) => {
                IrAdditional::Schema(Box::new(build_ir(s, warnings)?))
            }
        };
        // `additionalProperties` (boolean or schema) evaluates and annotates
        // every key it applies to, so a sibling `unevaluatedProperties` in
        // the same schema object has nothing left to constrain.
        let unevaluated = if obj.additional_properties.is_some() {
            IrAdditional::Allow
        } else {
            match &obj.unevaluated_properties {
                None | Some(AdditionalAst::Schema(SchemaAst::Always)) => IrAdditional::Allow,
                Some(AdditionalAst::Forbid) => IrAdditional::Forbid,
                Some(AdditionalAst::Schema(s)) => {
                    IrAdditional::Schema(Box::new(build_ir(s, warnings)?))
                }
            }
        };
        ops.push(IrOp::Object {
            properties,
            pattern_properties,
            additional,
            unevaluated,
            required: obj.required.clone(),
        });
    }

    if !obj.dependent_required.is_empty() {
        ops.push(IrOp::DependentRequired(obj.dependent_required.clone()));
    }
    if !obj.dependent_schemas.is_empty() {
        let children = obj
            .dependent_schemas
            .iter()
            .map(|(k, s)| build_ir(s, warnings).map(|ir| (k.clone(), ir)))
            .collect::<Result<Vec<_>, _>>()?;
        ops.push(IrOp::DependentSchemas(children));
    }
    if let Some(pn) = &obj.property_names {
        ops.push(IrOp::PropertyNames(Box::new(build_ir(pn, warnings)?)));
    }

    if let Some(n) = &obj.not {
        ops.push(IrOp::Not(Box::new(build_ir(n, warnings)?)));
    }
    if !obj.any_of.is_empty() {
        let children = obj
            .any_of
            .iter()
            .map(|s| build_ir(s, warnings))
            .collect::<Result<Vec<_>, _>>()?;
        ops.push(IrOp::AnyOf(children));
    }
    if !obj.one_of.is_empty() {
        let children = obj
            .one_of
            .iter()
            .map(|s| build_ir(s, warnings))
            .collect::<Result<Vec<_>, _>>()?;
        ops.push(IrOp::OneOf(children));
    }
    if obj.if_schema.is_some() {
        let if_ = build_ir(obj.if_schema.as_deref().expect("checked"), warnings)?;
        let then_ = match &obj.then_schema {
            Some(t) => Some(Box::new(build_ir(t, warnings)?)),
            None => None,
        };
        let else_ = match &obj.else_schema {
            Some(e) => Some(Box::new(build_ir(e, warnings)?)),
            None => None,
        };
        ops.push(IrOp::IfThenElse {
            if_: Box::new(if_),
            then_,
            else_,
        });
    }

    // allOf: conjunction — fold member ops into this node, merging object
    // keywords and intersecting type sets (JSON Schema: additionalProperties
    // in one allOf member must account for properties declared in siblings).
    for member in &obj.all_of {
        let member_ir = build_ir(member, warnings)?;
        absorb_conjunct(&mut ops, member_ir);
    }

    Ok(IrSchema { ops })
}

/// Fold a conjunction operand's ops into the parent op list.
fn absorb_conjunct(ops: &mut Vec<IrOp>, member: IrSchema) {
    for op in member.ops {
        match op {
            IrOp::Never(reason) => {
                ops.clear();
                ops.push(IrOp::Never(reason));
            }
            IrOp::Type(types) => {
                // Intersect with an existing type constraint, if any.
                if let Some(existing_pos) = ops.iter().position(|o| matches!(o, IrOp::Type(_))) {
                    if let IrOp::Type(existing) = &mut ops[existing_pos] {
                        let keep: Vec<DataType> = existing
                            .iter()
                            .filter(|t| types.contains(t))
                            .cloned()
                            .collect();
                        if keep.is_empty() {
                            ops.clear();
                            ops.push(IrOp::Never(
                                "allOf members have disjoint `type` sets".into(),
                            ));
                            return;
                        }
                        *existing = keep;
                    }
                } else {
                    ops.push(IrOp::Type(types));
                }
            }
            IrOp::Object {
                properties,
                pattern_properties,
                additional,
                unevaluated,
                required,
            } => {
                merge_object_op(
                    ops,
                    properties,
                    pattern_properties,
                    additional,
                    unevaluated,
                    required,
                );
            }
            other => ops.push(other),
        }
    }
}

/// Merge another object keyword set into the parent's (creating one if
/// absent), so that `additionalProperties` accounting sees the union.
#[allow(clippy::too_many_arguments)]
fn merge_object_op(
    ops: &mut Vec<IrOp>,
    properties: Vec<(String, IrSchema)>,
    pattern_properties: Vec<(Regex, IrSchema)>,
    additional: IrAdditional,
    unevaluated: IrAdditional,
    required: Vec<String>,
) {
    let existing = ops.iter_mut().find(|o| matches!(o, IrOp::Object { .. }));
    if let Some(IrOp::Object {
        properties: p,
        pattern_properties: pp,
        additional: a,
        unevaluated: u,
        required: r,
    }) = existing
    {
        for (name, schema) in properties {
            if let Some((_, slot)) = p.iter_mut().find(|(n, _)| *n == name) {
                // Same property in both: conjunction of both sub-schemas.
                let mut ops = std::mem::take(&mut slot.ops);
                ops.extend(schema.ops);
                *slot = IrSchema { ops };
            } else {
                p.push((name, schema));
            }
        }
        pp.extend(pattern_properties);
        r.extend(required);
        *a = merge_additional(a.clone(), additional);
        *u = merge_additional(u.clone(), unevaluated);
    } else {
        ops.push(IrOp::Object {
            properties,
            pattern_properties,
            additional,
            unevaluated,
            required,
        });
    }
}

/// `additionalProperties` under conjunction: `Forbid` dominates; schemas
/// combine into a conjunction; `Allow` is the identity.
fn merge_additional(a: IrAdditional, b: IrAdditional) -> IrAdditional {
    use IrAdditional::*;
    match (a, b) {
        (Forbid, _) | (_, Forbid) => Forbid,
        (Schema(x), Schema(y)) => {
            let mut ops = x.ops;
            ops.extend(y.ops);
            Schema(Box::new(IrSchema { ops }))
        }
        (Schema(x), Allow) => Schema(x),
        (Allow, Schema(y)) => Schema(y),
        (Allow, Allow) => Allow,
    }
}

/// Lower optimized IR to the validation state machine.
pub fn lower(ir: &IrSchema) -> ValidationNode {
    if ir.ops.is_empty() {
        return ValidationNode::Always;
    }
    // Order: type first (cheap, often decisive), then everything else in
    // declaration order.
    let mut nodes: Vec<ValidationNode> = Vec::with_capacity(ir.ops.len());
    for op in &ir.ops {
        nodes.push(lower_op(op));
    }
    nodes.sort_by_key(rank);
    ValidationNode::all(nodes)
}

/// Lowering order: type (0), scalars (1), structural (2), combinators (3).
fn rank(n: &ValidationNode) -> u8 {
    match n {
        ValidationNode::CheckType(_) => 0,
        ValidationNode::CheckObject(_)
        | ValidationNode::CheckObjectEx(_)
        | ValidationNode::CheckRequired(_)
        | ValidationNode::CheckArray(_, _, _)
        | ValidationNode::CheckPrefixItems(_, _, _)
        | ValidationNode::CheckUniqueItems(_)
        | ValidationNode::CheckContains(_, _, _)
        | ValidationNode::CheckDependentRequired(_)
        | ValidationNode::CheckDependentSchemas(_)
        | ValidationNode::CheckPropertyNames(_)
        | ValidationNode::CheckField(_, _) => 2,
        ValidationNode::CheckIfThenElse(_, _, _)
        | ValidationNode::CheckAllOf(_)
        | ValidationNode::CheckAnyOf(_)
        | ValidationNode::CheckOneOf(_)
        | ValidationNode::CheckNot(_) => 3,
        _ => 1,
    }
}

fn lower_op(op: &IrOp) -> ValidationNode {
    match op {
        IrOp::Type(types) => {
            if types.len() == 1 {
                ValidationNode::CheckType(types[0])
            } else {
                ValidationNode::CheckAnyOf(
                    types
                        .iter()
                        .map(|t| ValidationNode::CheckType(*t))
                        .collect(),
                )
            }
        }
        IrOp::Minimum { bound, exclusive } => {
            if *exclusive {
                ValidationNode::CheckExclusiveMinimum(*bound)
            } else {
                ValidationNode::CheckMinimum(*bound)
            }
        }
        IrOp::Maximum { bound, exclusive } => {
            if *exclusive {
                ValidationNode::CheckExclusiveMaximum(*bound)
            } else {
                ValidationNode::CheckMaximum(*bound)
            }
        }
        IrOp::MultipleOf(m) => ValidationNode::CheckMultipleOf(*m),
        IrOp::MinLength(n) => ValidationNode::CheckMinLength(*n),
        IrOp::MaxLength(n) => ValidationNode::CheckMaxLength(*n),
        IrOp::MinProperties(n) => ValidationNode::CheckMinProperties(*n),
        IrOp::MaxProperties(n) => ValidationNode::CheckMaxProperties(*n),
        IrOp::Pattern(re) => ValidationNode::CheckPattern(Box::new(re.clone())),
        IrOp::Enum(set) => ValidationNode::CheckEnum(set.clone()),
        IrOp::Const(v) => ValidationNode::CheckConst(v.clone()),
        IrOp::Format(f) => ValidationNode::CheckFormat(*f),
        IrOp::CustomFormat(name) => ValidationNode::CheckCustomFormat(name.clone()),
        IrOp::Items {
            schema,
            min_items,
            max_items,
        } => ValidationNode::CheckArray(Box::new(lower(schema)), *min_items, *max_items),
        IrOp::PrefixItems {
            prefixes,
            items,
            unevaluated,
        } => ValidationNode::CheckPrefixItems(
            prefixes.iter().map(lower).collect(),
            items.as_deref().map(|s| Box::new(lower(s))),
            unevaluated.as_deref().map(|s| Box::new(lower(s))),
        ),
        IrOp::UniqueItems => ValidationNode::CheckUniqueItems(true),
        IrOp::Contains { schema, min, max } => {
            ValidationNode::CheckContains(Box::new(lower(schema)), *min, *max)
        }
        IrOp::Object {
            properties,
            pattern_properties,
            additional,
            unevaluated,
            required,
        } => ValidationNode::CheckObjectEx(ObjectShape {
            properties: properties
                .iter()
                .map(|(n, s)| (n.clone(), lower(s)))
                .collect(),
            pattern_properties: pattern_properties
                .iter()
                .map(|(re, s)| (Box::new(re.clone()), lower(s)))
                .collect(),
            additional: match additional {
                IrAdditional::Allow => AdditionalProperties::Allow,
                IrAdditional::Forbid => AdditionalProperties::Forbid,
                IrAdditional::Schema(s) => AdditionalProperties::Schema(Box::new(lower(s))),
            },
            unevaluated: match unevaluated {
                IrAdditional::Allow => None,
                IrAdditional::Forbid => Some(AdditionalProperties::Forbid),
                IrAdditional::Schema(s) => Some(AdditionalProperties::Schema(Box::new(lower(s)))),
            },
            required: required.clone(),
        }),
        IrOp::DependentRequired(deps) => ValidationNode::CheckDependentRequired(deps.clone()),
        IrOp::DependentSchemas(deps) => ValidationNode::CheckDependentSchemas(
            deps.iter()
                .map(|(k, s)| (k.clone(), Box::new(lower(s))))
                .collect(),
        ),
        IrOp::PropertyNames(s) => ValidationNode::CheckPropertyNames(Box::new(lower(s))),
        IrOp::IfThenElse { if_, then_, else_ } => ValidationNode::CheckIfThenElse(
            Box::new(lower(if_)),
            then_.as_deref().map(|t| Box::new(lower(t))),
            else_.as_deref().map(|e| Box::new(lower(e))),
        ),
        IrOp::AnyOf(children) => ValidationNode::CheckAnyOf(children.iter().map(lower).collect()),
        IrOp::OneOf(children) => ValidationNode::CheckOneOf(children.iter().map(lower).collect()),
        IrOp::Not(child) => ValidationNode::CheckNot(Box::new(lower(child))),
        IrOp::Never(reason) => ValidationNode::Never(reason.clone()),
    }
}

/// Convenience for tests and the DSL: the set of properties declared at the
/// root object, if any.
#[allow(dead_code)]
pub(crate) fn root_property_names(ir: &IrSchema) -> HashSet<&str> {
    let mut out = HashSet::new();
    for op in &ir.ops {
        if let IrOp::Object { properties, .. } = op {
            for (name, _) in properties {
                out.insert(name.as_str());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parse_schema;
    use serde_json::{json, Value};
    use tpt_valid_core::{validate, ValidationOptions};

    fn compiled(schema: Value) -> ValidationNode {
        let ast = parse_schema(&schema).unwrap();
        compile(&ast).unwrap().0
    }

    fn ok(schema: &Value, instance: Value) -> bool {
        let node = compiled(schema.clone());
        validate(&node, &instance, &ValidationOptions::default()).is_empty()
    }

    #[test]
    fn compiles_basic_types() {
        assert!(ok(&json!({"type": "integer"}), json!(5)));
        assert!(!ok(&json!({"type": "integer"}), json!(5.5)));
        assert!(ok(&json!({"type": ["string", "null"]}), json!(null)));
        assert!(!ok(&json!({"type": ["string", "null"]}), json!(1)));
    }

    #[test]
    fn numeric_bounds_and_optimizer_merge() {
        // minimum 5 + exclusiveMinimum 5 → exclusiveMinimum(5) after merge.
        let node = compiled(json!({
            "allOf": [
                {"minimum": 5},
                {"exclusiveMinimum": 5}
            ]
        }));
        assert_eq!(
            validate(&node, &json!(5), &ValidationOptions::default()).len(),
            1
        );
        assert!(validate(&node, &json!(6), &ValidationOptions::default()).is_empty());
    }

    #[test]
    fn all_of_merges_object_properties() {
        // additionalProperties:false in the parent must not reject keys that
        // a sibling allOf member declares.
        let schema = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "required": ["a"],
            "additionalProperties": false,
            "allOf": [
                {"properties": {"b": {"type": "integer"}}}
            ]
        });
        assert!(ok(&schema, json!({"a": "x", "b": 1})));
        let schema = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "additionalProperties": false,
            "allOf": [
                {"properties": {"b": {"type": "integer"}}}
            ]
        });
        assert!(
            !ok(&schema, json!({"a": "x", "c": true})),
            "c is unknown to both"
        );
    }

    #[test]
    fn all_of_intersects_types() {
        let schema = json!({
            "allOf": [
                {"type": ["string", "integer"]},
                {"type": ["integer", "null"]}
            ]
        });
        assert!(ok(&schema, json!(3)));
        assert!(!ok(&schema, json!("x")));
        assert!(!ok(&schema, json!(null)));
    }

    #[test]
    fn disjoint_all_of_types_become_never() {
        let schema = json!({
            "allOf": [{"type": "string"}, {"type": "integer"}]
        });
        assert!(!ok(&schema, json!(3)));
        assert!(!ok(&schema, json!("x")));
    }

    #[test]
    fn type_pruning_drops_inapplicable_checks() {
        // "pattern" on a number-typed node is pruned (no-op either way, but
        // the machine should not even carry it).
        let ast = parse_schema(&json!({"type": "integer", "pattern": "^a", "minimum": 1})).unwrap();
        let (node, warnings) = compile(&ast).unwrap();
        assert!(warnings.is_empty());
        match &node {
            ValidationNode::CheckAll(ops) => {
                assert_eq!(ops.len(), 2, "pattern pruned; type + minimum kept");
                assert!(matches!(ops[0], ValidationNode::CheckType(_)));
                assert!(matches!(ops[1], ValidationNode::CheckMinimum(_)));
            }
            other => panic!("unexpected node {other:?}"),
        }
    }

    #[test]
    fn unknown_format_warns() {
        let ast = parse_schema(&json!({"type": "string", "format": "color"})).unwrap();
        let (_, warnings) = compile(&ast).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("color"));
    }

    #[test]
    fn semantic_errors() {
        let e = parse_schema(&json!({"minimum": 10, "maximum": 5})).unwrap();
        let err = compile(&e).unwrap_err();
        assert!(matches!(err, SchemaError::Semantic { .. }));

        let ast = parse_schema(&json!({"minLength": 5, "maxLength": 2})).unwrap();
        assert!(compile(&ast).is_err());

        let ast = parse_schema(&json!({"minItems": 3, "maxItems": 2})).unwrap();
        assert!(compile(&ast).is_err());

        let ast = parse_schema(&json!({"pattern": "["})).unwrap();
        assert!(compile(&ast).is_err());

        let ast = parse_schema(&json!({"multipleOf": 0}));
        assert!(ast.is_err(), "multipleOf <= 0 rejected at parse time");
    }

    #[test]
    fn one_of_with_vacuous_branches() {
        // Sanity check that compiled oneOf matches the raw engine semantics.
        let schema = json!({
            "oneOf": [
                {"type": "number", "minimum": 5},
                {"type": "integer"}
            ]
        });
        assert!(ok(&schema.clone(), json!(3)));
        assert!(!ok(&schema.clone(), json!(6)));
        assert!(!ok(&schema, json!(2.5)));
    }

    #[test]
    fn prefix_items_and_tuple_items() {
        let schema = json!({
            "type": "array",
            "prefixItems": [{"type": "string"}, {"type": "integer"}],
            "items": {"type": "boolean"}
        });
        assert!(ok(&schema, json!(["a", 1, true])));
        assert!(!ok(&schema, json!(["a", 1, 2])));

        let tuple = json!({
            "type": "array",
            "items": [{"type": "string"}, {"type": "integer"}]
        });
        assert!(ok(&tuple, json!(["a", 1])));
        assert!(ok(&tuple, json!(["a", 1, "anything"])));
        assert!(!ok(&tuple, json!([1, 2])));
    }

    #[test]
    fn contains_min_max() {
        let schema = json!({
            "type": "array",
            "contains": {"type": "integer"},
            "minContains": 2,
            "maxContains": 3
        });
        assert!(ok(&schema, json!([1, 2])));
        assert!(ok(&schema, json!(["x", 1, 2, 3])));
        assert!(!ok(&schema, json!([1])));
        assert!(!ok(&schema, json!([1, 2, 3, 4])));
    }

    #[test]
    fn dependent_required_and_schemas() {
        let dr = json!({
            "type": "object",
            "properties": {
                "credit_card": {"type": "string"},
                "billing_address": {"type": "string"}
            },
            "dependentRequired": {"credit_card": ["billing_address"]}
        });
        assert!(ok(&dr, json!({"credit_card": "1", "billing_address": "x"})));
        assert!(ok(&dr, json!({"name": "x"})));
        assert!(!ok(&dr, json!({"credit_card": "1"})));

        let ds = json!({
            "type": "object",
            "properties": {"a": {"type": "integer"}},
            "dependentSchemas": {
                "a": {"required": ["b"], "properties": {"b": {"type": "string"}}}
            }
        });
        assert!(ok(&ds, json!({"a": 1, "b": "x"})));
        assert!(ok(&ds, json!({"c": 1})));
        assert!(!ok(&ds, json!({"a": 1})));
        assert!(!ok(&ds, json!({"a": 1, "b": 2})));
    }

    #[test]
    fn property_names() {
        let schema = json!({
            "type": "object",
            "propertyNames": {"pattern": "^[a-z]+$"}
        });
        assert!(ok(&schema, json!({"abc": 1})));
        assert!(!ok(&schema, json!({"ABC": 1})));
    }

    #[test]
    fn unevaluated_properties_and_items() {
        let schema = json!({
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "patternProperties": {"^S_": {"type": "string"}},
            "unevaluatedProperties": false
        });
        assert!(ok(&schema, json!({"name": "x", "S_a": "y"})));
        assert!(!ok(&schema, json!({"name": "x", "other": 1})));

        let items_schema = json!({
            "type": "array",
            "prefixItems": [{"type": "integer"}],
            "unevaluatedItems": {"type": "string"}
        });
        assert!(ok(&items_schema, json!([1, "a"])));
        assert!(!ok(&items_schema, json!([1, 2])));
    }

    #[test]
    fn refs_end_to_end() {
        let schema = json!({
            "type": "object",
            "properties": {
                "billing": {"$ref": "#/$defs/address"},
                "shipping": {"$ref": "#/$defs/address"}
            },
            "required": ["billing"],
            "$defs": {
                "address": {
                    "type": "object",
                    "properties": {"city": {"type": "string"}},
                    "required": ["city"]
                }
            }
        });
        assert!(ok(&schema, json!({"billing": {"city": "Berlin"}})));
        assert!(!ok(&schema, json!({"billing": {}})));
        assert!(!ok(&schema, json!({"billing": {"city": 1}})));

        // Anchors:
        let anchored = json!({
            "type": "object",
            "properties": {"a": {"$ref": "#nonneg"}},
            "$defs": {"x": {"$anchor": "nonneg", "type": "integer", "minimum": 0}}
        });
        assert!(ok(&anchored, json!({"a": 3})));
        assert!(!ok(&anchored, json!({"a": -1})));
    }

    #[test]
    fn custom_formats_pass_through_to_runtime() {
        let ast = parse_schema(&json!({"type": "string", "format": "internal-id"})).unwrap();
        let (node, warnings) = compile(&ast).unwrap();
        assert_eq!(warnings.len(), 1, "unknown format warns");
        assert!(warnings[0].contains("internal-id"));
        // Unregistered: no-op (annotation semantics).
        assert!(validate(&node, &json!("anything"), &ValidationOptions::default()).is_empty());
        // Registered: enforced.
        let opts =
            ValidationOptions::default().with_format("internal-id", |s| s.starts_with("ID-"));
        assert!(validate(&node, &json!("ID-1234"), &opts).is_empty());
        assert_eq!(validate(&node, &json!("nope"), &opts).len(), 1);
        // Non-strings are exempt (format applies to strings only).
        let ast = parse_schema(&json!({"format": "internal-id"})).unwrap();
        let (node, _) = compile(&ast).unwrap();
        assert!(validate(&node, &json!(42), &opts).is_empty());
    }
}
