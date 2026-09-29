//! Parallel batch validation via `rayon` (spec §5.5 "Batch" mode).

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::{check, PathCursor, ValidationOptions};
use crate::error::{ErrorCollector, ValidationError};
use crate::node::ValidationNode;

/// Result of validating one item in a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationOutcome {
    /// Index of the item in the input batch.
    pub index: usize,
    /// Whether the item was valid.
    pub valid: bool,
    /// Collected errors (empty when valid).
    pub errors: Vec<ValidationError>,
}

/// Validate a batch of JSON values in parallel across all cores.
///
/// The compiled state machine is read-only and shared; only the per-item
/// collectors are thread-local.
///
/// # Examples
///
/// ```
/// use tpt_valid_core::{validate_batch, DataType, ValidationNode, ValidationOptions};
/// use serde_json::{json, Value};
///
/// let node = ValidationNode::CheckType(DataType::Integer);
/// let batch = vec![json!(1), json!("x"), json!(3)];
/// let outcomes = validate_batch(&node, &batch, &ValidationOptions::default());
/// assert_eq!(outcomes.len(), 3);
/// assert!(outcomes[0].valid);
/// assert!(!outcomes[1].valid);
/// ```
pub fn validate_batch(
    node: &ValidationNode,
    values: &[Value],
    opts: &ValidationOptions,
) -> Vec<ValidationOutcome> {
    values
        .par_iter()
        .enumerate()
        .map(|(index, value)| {
            let mut collector = ErrorCollector::new(opts.fail_fast, opts.max_errors);
            let mut path = PathCursor::new();
            check(node, value, &mut path, opts, &mut collector);
            let errors = collector.into_errors();
            let valid = errors.is_empty();
            ValidationOutcome {
                index,
                valid,
                errors,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::node::{AdditionalProperties, ObjectShape};
    use crate::types::DataType;

    fn user_schema() -> ValidationNode {
        ValidationNode::all(vec![
            ValidationNode::CheckType(DataType::Object),
            ValidationNode::CheckObjectEx(ObjectShape {
                properties: vec![
                    (
                        "name".into(),
                        ValidationNode::all(vec![
                            ValidationNode::CheckType(DataType::String),
                            ValidationNode::CheckMinLength(1),
                        ]),
                    ),
                    (
                        "age".into(),
                        ValidationNode::all(vec![
                            ValidationNode::CheckType(DataType::Integer),
                            ValidationNode::CheckMinimum(0.0),
                            ValidationNode::CheckMaximum(150.0),
                        ]),
                    ),
                ],
                ..Default::default()
            }),
        ])
    }

    #[test]
    fn batch_reports_per_item_outcomes() {
        let node = user_schema();
        let batch = vec![
            json!({"name": "Alice", "age": 30}),
            json!({"name": "Bob", "age": "25"}),
            json!({"name": "", "age": 200}),
        ];
        let outcomes = validate_batch(&node, &batch, &ValidationOptions::default());
        assert_eq!(outcomes.len(), 3);
        assert!(outcomes[0].valid);
        assert_eq!(outcomes[0].index, 0);
        assert!(!outcomes[1].valid);
        assert_eq!(outcomes[1].errors[0].path, "$.age");
        assert!(!outcomes[2].valid);
        assert_eq!(outcomes[2].errors.len(), 2);
        assert_eq!(outcomes[2].errors[0].path, "$.name");
        assert_eq!(outcomes[2].errors[1].path, "$.age");
    }

    #[test]
    fn batch_parallelism_handles_large_input() {
        let node = ValidationNode::CheckType(DataType::Integer);
        let batch: Vec<Value> = (0..10_000)
            .map(|i| if i % 7 == 0 { json!("x") } else { json!(i) })
            .collect();
        let outcomes = validate_batch(&node, &batch, &ValidationOptions::default());
        assert_eq!(outcomes.len(), 10_000);
        assert_eq!(outcomes.iter().filter(|o| !o.valid).count(), 1_429);
    }

    #[test]
    fn additional_default_is_allow() {
        assert!(matches!(
            AdditionalProperties::default(),
            AdditionalProperties::Allow
        ));
    }
}
