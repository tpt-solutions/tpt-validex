//! Intermediate representation produced from the AST, plus the optimizer
//! passes (spec §4.2 steps 3–5).
//!
//! The IR is a flat list of checks per schema node. `allOf` conjunctions are
//! folded into a single node by the compiler (with object-keyword merging),
//! then the optimizer:
//!
//! 1. Prunes checks that cannot apply given the node's `type` set
//!    (e.g. numeric bounds on a string-typed node).
//! 2. Merges `minimum`/`exclusiveMinimum` (and upper) into single bounds.
//!
//! The result is lowered to a [`ValidationNode`] state machine by the
//! compiler. The DSL (`schema!` macro) shares this exact pipeline.

use regex::Regex;
use serde_json::Value;

use tpt_valid_core::{EnumSet, Format};

/// One compiled schema node in IR form: a conjunction of ops.
#[derive(Debug, Clone)]
pub struct IrSchema {
    /// Checks that all apply to the same instance.
    pub ops: Vec<IrOp>,
}

impl IrSchema {
    /// An IR node that always matches.
    pub fn always() -> IrSchema {
        IrSchema { ops: Vec::new() }
    }

    /// Whether this node has no checks (always matches).
    pub fn is_always(&self) -> bool {
        self.ops.is_empty()
    }
}

/// `additionalProperties` in IR form.
#[derive(Debug, Clone)]
pub enum IrAdditional {
    /// Unconstrained.
    Allow,
    /// Rejected.
    Forbid,
    /// Validated with a sub-schema.
    Schema(Box<IrSchema>),
}

/// A single check in the IR.
#[derive(Debug, Clone)]
#[allow(missing_docs)] // variant fields are named after their schema keywords
pub enum IrOp {
    /// `type` — instance type must be in this set.
    Type(Vec<tpt_valid_core::DataType>),
    /// Merged lower numeric bound (`exclusive` from `exclusiveMinimum`).
    Minimum { bound: f64, exclusive: bool },
    /// Merged upper numeric bound (`exclusive` from `exclusiveMaximum`).
    Maximum { bound: f64, exclusive: bool },
    /// `multipleOf`
    MultipleOf(f64),
    /// `minLength`
    MinLength(usize),
    /// `maxLength`
    MaxLength(usize),
    /// `pattern` (pre-compiled — spec §4.2: "pre-compile regex")
    Pattern(Regex),
    /// `enum`
    Enum(EnumSet),
    /// `const`
    Const(Value),
    /// `format` (built-in)
    Format(Format),
    /// `format` that is not a built-in (validated by a runtime-registered
    /// custom format function, if any).
    CustomFormat(String),
    /// `items` + `minItems` + `maxItems`
    Items {
        schema: Box<IrSchema>,
        min_items: usize,
        max_items: usize,
    },
    /// `prefixItems` + (optional) `items` for the remainder + (optional)
    /// `unevaluatedItems` for the remainder when no `items` applies.
    PrefixItems {
        prefixes: Vec<IrSchema>,
        items: Option<Box<IrSchema>>,
        unevaluated: Option<Box<IrSchema>>,
    },
    /// `uniqueItems: true`
    UniqueItems,
    /// `contains` + `minContains` (default 1) + `maxContains`
    Contains {
        schema: Box<IrSchema>,
        min: usize,
        max: Option<usize>,
    },
    /// Object keywords combined (exact unknown-key accounting).
    Object {
        properties: Vec<(String, IrSchema)>,
        pattern_properties: Vec<(Regex, IrSchema)>,
        additional: IrAdditional,
        unevaluated: IrAdditional,
        required: Vec<String>,
    },
    /// `dependentRequired` — key → keys required alongside it.
    DependentRequired(Vec<(String, Vec<String>)>),
    /// `dependentSchemas` — key → schema applied when present.
    DependentSchemas(Vec<(String, IrSchema)>),
    /// `propertyNames`
    PropertyNames(Box<IrSchema>),
    /// `if` / `then` / `else`
    IfThenElse {
        if_: Box<IrSchema>,
        then_: Option<Box<IrSchema>>,
        else_: Option<Box<IrSchema>>,
    },
    /// `anyOf`
    AnyOf(Vec<IrSchema>),
    /// `oneOf`
    OneOf(Vec<IrSchema>),
    /// `not`
    Not(Box<IrSchema>),
    /// The boolean `false` schema (or an empty `enum`).
    Never(String),
}

/// Run the optimizer passes over a freshly built IR tree.
pub fn optimize(mut root: IrSchema) -> IrSchema {
    prune_by_type(&mut root);
    merge_bounds(&mut root);
    root
}

/// Pass 2: drop ops that cannot fire given the node's own `type` set.
fn prune_by_type(node: &mut IrSchema) {
    let types = node.ops.iter().find_map(|op| match op {
        IrOp::Type(types) => Some(types.clone()),
        _ => None,
    });

    if let Some(types) = types {
        let allows = |t: tpt_valid_core::DataType| types.contains(&t);
        let numeric =
            allows(tpt_valid_core::DataType::Number) || allows(tpt_valid_core::DataType::Integer);
        let string = allows(tpt_valid_core::DataType::String);
        let array = allows(tpt_valid_core::DataType::Array);
        let object = allows(tpt_valid_core::DataType::Object);

        node.ops.retain(|op| match op {
            IrOp::Minimum { .. } | IrOp::Maximum { .. } | IrOp::MultipleOf(_) => numeric,
            IrOp::MinLength(_)
            | IrOp::MaxLength(_)
            | IrOp::Pattern(_)
            | IrOp::Format(_)
            | IrOp::CustomFormat(_) => string,
            IrOp::Items { .. }
            | IrOp::PrefixItems { .. }
            | IrOp::UniqueItems
            | IrOp::Contains { .. } => array,
            IrOp::Object { .. }
            | IrOp::DependentRequired(_)
            | IrOp::DependentSchemas(_)
            | IrOp::PropertyNames(_) => object,
            _ => true,
        });
    }

    // Recurse into children.
    for op in &mut node.ops {
        match op {
            IrOp::Items { schema, .. } => prune_by_type(schema),
            IrOp::PrefixItems {
                prefixes,
                items,
                unevaluated,
            } => {
                for p in prefixes {
                    prune_by_type(p);
                }
                if let Some(s) = items {
                    prune_by_type(s);
                }
                if let Some(s) = unevaluated {
                    prune_by_type(s);
                }
            }
            IrOp::Contains { schema, .. } => prune_by_type(schema),
            IrOp::Object {
                properties,
                pattern_properties,
                additional,
                unevaluated,
                ..
            } => {
                for (_, s) in properties.iter_mut() {
                    prune_by_type(s);
                }
                for (_, s) in pattern_properties.iter_mut() {
                    prune_by_type(s);
                }
                if let IrAdditional::Schema(s) = additional {
                    prune_by_type(s);
                }
                if let IrAdditional::Schema(s) = unevaluated {
                    prune_by_type(s);
                }
            }
            IrOp::DependentSchemas(deps) => {
                for (_, s) in deps {
                    prune_by_type(s);
                }
            }
            IrOp::PropertyNames(s) => prune_by_type(s),
            IrOp::IfThenElse { if_, then_, else_ } => {
                prune_by_type(if_);
                if let Some(t) = then_ {
                    prune_by_type(t);
                }
                if let Some(e) = else_ {
                    prune_by_type(e);
                }
            }
            IrOp::AnyOf(children) | IrOp::OneOf(children) => {
                for c in children {
                    prune_by_type(c);
                }
            }
            IrOp::Not(child) => prune_by_type(child),
            _ => {}
        }
    }
}

/// Pass 3: merge lower bounds (`minimum` + `exclusiveMinimum`) into one op,
/// same for upper bounds.
fn merge_bounds(node: &mut IrSchema) {
    let has_lower = node.ops.iter().any(|op| matches!(op, IrOp::Minimum { .. }));
    if has_lower {
        let mut bound = f64::NEG_INFINITY;
        let mut exclusive = false;
        for op in &node.ops {
            if let IrOp::Minimum {
                bound: b,
                exclusive: e,
            } = op
            {
                if *b > bound {
                    bound = *b;
                    exclusive = *e;
                } else if (*b - bound).abs() < f64::EPSILON && *e {
                    exclusive = true;
                }
            }
        }
        node.ops.retain(|op| !matches!(op, IrOp::Minimum { .. }));
        node.ops.push(IrOp::Minimum { bound, exclusive });
    }

    let has_upper = node.ops.iter().any(|op| matches!(op, IrOp::Maximum { .. }));
    if has_upper {
        let mut bound = f64::INFINITY;
        let mut exclusive = false;
        for op in &node.ops {
            if let IrOp::Maximum {
                bound: b,
                exclusive: e,
            } = op
            {
                if *b < bound {
                    bound = *b;
                    exclusive = *e;
                } else if (*b - bound).abs() < f64::EPSILON && *e {
                    exclusive = true;
                }
            }
        }
        node.ops.retain(|op| !matches!(op, IrOp::Maximum { .. }));
        node.ops.push(IrOp::Maximum { bound, exclusive });
    }

    for op in &mut node.ops {
        match op {
            IrOp::Items { schema, .. } => merge_bounds(schema),
            IrOp::PrefixItems {
                prefixes,
                items,
                unevaluated,
            } => {
                for p in prefixes {
                    merge_bounds(p);
                }
                if let Some(s) = items {
                    merge_bounds(s);
                }
                if let Some(s) = unevaluated {
                    merge_bounds(s);
                }
            }
            IrOp::Contains { schema, .. } => merge_bounds(schema),
            IrOp::Object {
                properties,
                pattern_properties,
                additional,
                unevaluated,
                ..
            } => {
                for (_, s) in properties.iter_mut() {
                    merge_bounds(s);
                }
                for (_, s) in pattern_properties.iter_mut() {
                    merge_bounds(s);
                }
                if let IrAdditional::Schema(s) = additional {
                    merge_bounds(s);
                }
                if let IrAdditional::Schema(s) = unevaluated {
                    merge_bounds(s);
                }
            }
            IrOp::DependentSchemas(deps) => {
                for (_, s) in deps {
                    merge_bounds(s);
                }
            }
            IrOp::PropertyNames(s) => merge_bounds(s),
            IrOp::IfThenElse { if_, then_, else_ } => {
                merge_bounds(if_);
                if let Some(t) = then_ {
                    merge_bounds(t);
                }
                if let Some(e) = else_ {
                    merge_bounds(e);
                }
            }
            IrOp::AnyOf(children) | IrOp::OneOf(children) => {
                for c in children {
                    merge_bounds(c);
                }
            }
            IrOp::Not(child) => merge_bounds(child),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_removes_noops() {
        let ir = IrSchema {
            ops: vec![
                IrOp::Type(vec![tpt_valid_core::DataType::Integer]),
                IrOp::Minimum {
                    bound: 0.0,
                    exclusive: false,
                },
            ],
        };
        let opt = optimize(ir);
        assert_eq!(opt.ops.len(), 2);
    }

    #[test]
    fn bounds_merge() {
        let ir = IrSchema {
            ops: vec![
                IrOp::Minimum {
                    bound: 0.0,
                    exclusive: false,
                },
                IrOp::Minimum {
                    bound: 5.0,
                    exclusive: false,
                },
                IrOp::Maximum {
                    bound: 100.0,
                    exclusive: false,
                },
                IrOp::Maximum {
                    bound: 100.0,
                    exclusive: true,
                },
            ],
        };
        let opt = optimize(ir);
        assert_eq!(opt.ops.len(), 2);
        assert!(opt
            .ops
            .iter()
            .any(|op| matches!(op, IrOp::Minimum { bound: b, exclusive: false } if *b == 5.0)));
        assert!(opt
            .ops
            .iter()
            .any(|op| matches!(op, IrOp::Maximum { bound: b, exclusive: true } if *b == 100.0)));
    }
}
