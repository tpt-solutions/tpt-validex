//! `validex diff <old> <new>` — compare two schema versions and report
//! changes that could break data which validated against the old schema.
//!
//! Compatibility rules (single-version, per Draft 2020-12 subset):
//! * narrowing the `type` set,
//! * adding `required` keys (or removing declared properties),
//! * tightening numeric/string/array bounds,
//! * changing `const`, or shrinking `enum`,
//! * adding `additionalProperties: false` / `unevaluatedProperties: false`,
//! * changing `format` or `pattern` (reported as risky).
//!
//! Everything else (new optional properties, annotation changes) is
//! informational.

use serde_json::{json, Map, Value};

use crate::Args;

pub(crate) fn run(args: &[String]) -> Result<bool, String> {
    let args = Args::parse(args)?;
    let mut positional = args.positional.iter();
    let old_path = positional.next().ok_or("diff requires <old-schema>")?;
    let new_path = positional.next().ok_or("diff requires <new-schema>")?;

    let old = read_schema(old_path)?;
    let new = read_schema(new_path)?;

    let mut changes = Vec::new();
    compare(&old, &new, "$", &mut changes);

    let breaking = changes.iter().any(|c| c.breaking);
    match args.get("format").unwrap_or("text") {
        "text" => {
            for change in &changes {
                let marker = if change.breaking { "BREAKING" } else { "note" };
                println!("[{marker}] {}: {}", change.path, change.description);
            }
            if changes.is_empty() {
                println!("schemas are compatible: no detectable changes");
            } else if breaking {
                println!(
                    "{breaking_count} breaking change(s)",
                    breaking_count = changes.iter().filter(|c| c.breaking).count()
                );
            } else {
                println!("no breaking changes");
            }
        }
        "json" => {
            let report = json!({
                "compatible": !breaking,
                "changes": changes.iter().map(|c| json!({
                    "path": c.path,
                    "breaking": c.breaking,
                    "description": c.description,
                })).collect::<Vec<_>>(),
            });
            println!("{report}");
        }
        other => return Err(format!("unknown --format \"{other}\" (expected text|json)")),
    }
    Ok(!breaking)
}

fn read_schema(path: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    tpt_valid_parser::parse(&text).map_err(|e| format!("invalid JSON in {path}: {e}"))
}

struct Change {
    path: String,
    breaking: bool,
    description: String,
}

fn change(path: &str, breaking: bool, description: String) -> Change {
    Change {
        path: path.to_string(),
        breaking,
        description,
    }
}

fn compare(old: &Value, new: &Value, path: &str, out: &mut Vec<Change>) {
    match (old, new) {
        (Value::Object(old_map), Value::Object(new_map)) => {
            compare_objects(old_map, new_map, path, out)
        }
        _ => {
            if old != new {
                out.push(change(
                    path,
                    true,
                    format!("schema changed from {old} to {new}"),
                ));
            }
        }
    }
}

fn compare_objects(
    old: &Map<String, Value>,
    new: &Map<String, Value>,
    path: &str,
    out: &mut Vec<Change>,
) {
    // type narrowing
    if let (Some(old_t), Some(new_t)) = (old.get("type"), new.get("type")) {
        let old_types = type_set(old_t);
        let new_types: std::collections::HashSet<_> = type_set(new_t).into_iter().collect();
        if !new_types.is_subset(&old_types) {
            out.push(change(
                &format!("{path}.type"),
                true,
                format!("type narrowed from {:?} to {:?}", old_types, new_types),
            ));
        }
    } else if old.get("type").is_some() && new.get("type").is_none() {
        out.push(change(
            &format!("{path}.type"),
            false,
            "type constraint removed".into(),
        ));
    }

    // required additions
    let old_required = string_set(old.get("required"));
    let new_required = string_set(new.get("required"));
    for key in new_required.difference(&old_required) {
        out.push(change(
            &format!("{path}.required"),
            true,
            format!("\"{key}\" is now required"),
        ));
    }

    // enum narrowing / const change
    if let (Some(old_e), Some(new_e)) = (old.get("enum"), new.get("enum")) {
        if let (Some(old_e), Some(new_e)) = (old_e.as_array(), new_e.as_array()) {
            let missing: Vec<&Value> = old_e.iter().filter(|v| !new_e.contains(v)).collect();
            if !missing.is_empty() {
                out.push(change(
                    &format!("{path}.enum"),
                    true,
                    format!("enum no longer allows {missing:?}"),
                ));
            }
        }
    }
    if let (Some(old_c), Some(new_c)) = (old.get("const"), new.get("const")) {
        if old_c != new_c {
            out.push(change(
                &format!("{path}.const"),
                true,
                format!("const changed from {old_c} to {new_c}"),
            ));
        }
    }

    // numeric bounds
    for keyword in ["minimum", "exclusiveMinimum", "maximum", "exclusiveMaximum"] {
        if let (Some(old_b), Some(new_b)) = (num_of(old.get(keyword)), num_of(new.get(keyword))) {
            let tightened = match keyword {
                "minimum" | "exclusiveMinimum" => new_b > old_b,
                _ => new_b < old_b,
            };
            if tightened {
                out.push(change(
                    &format!("{path}.{keyword}"),
                    true,
                    format!("{keyword} tightened from {old_b} to {new_b}"),
                ));
            }
        } else if old.get(keyword).is_some() && new.get(keyword).is_none() {
            out.push(change(
                &format!("{path}.{keyword}"),
                false,
                format!("{keyword} removed (loosens the bound)"),
            ));
        }
    }

    // length / item bounds
    for keyword in ["minLength", "minItems", "minProperties"] {
        if let (Some(old_b), Some(new_b)) = (usize_of(old.get(keyword)), usize_of(new.get(keyword)))
        {
            if new_b > old_b {
                out.push(change(
                    &format!("{path}.{keyword}"),
                    true,
                    format!("{keyword} raised from {old_b} to {new_b}"),
                ));
            }
        }
    }
    for keyword in ["maxLength", "maxItems", "maxProperties"] {
        if let (Some(old_b), Some(new_b)) = (usize_of(old.get(keyword)), usize_of(new.get(keyword)))
        {
            if new_b < old_b {
                out.push(change(
                    &format!("{path}.{keyword}"),
                    true,
                    format!("{keyword} lowered from {old_b} to {new_b}"),
                ));
            }
        }
    }

    // exclusivity added
    for (kw, opposite) in [
        ("minimum", "exclusiveMinimum"),
        ("maximum", "exclusiveMaximum"),
    ] {
        if old.get(opposite).is_none() && new.get(opposite).is_some() {
            out.push(change(
                &format!("{path}.{opposite}"),
                true,
                format!("{opposite} added (bound at {kw} becomes exclusive)"),
            ));
        }
    }

    // additionalProperties / unevaluatedProperties restriction added
    for keyword in ["additionalProperties", "unevaluatedProperties"] {
        let old_restricted = is_restricted(old.get(keyword));
        let new_restricted = is_restricted(new.get(keyword));
        if !old_restricted && new_restricted {
            out.push(change(
                &format!("{path}.{keyword}"),
                true,
                format!("{keyword} restriction added"),
            ));
        }
    }

    // pattern / format changes (risky, reported)
    for keyword in ["pattern", "format"] {
        if let (Some(old_v), Some(new_v)) = (old.get(keyword), new.get(keyword)) {
            if old_v != new_v {
                out.push(change(
                    &format!("{path}.{keyword}"),
                    true,
                    format!("{keyword} changed from {old_v} to {new_v}"),
                ));
            }
        }
    }

    // properties: removed → breaking if required or referenced; recurse on kept
    let old_props = old.get("properties").and_then(Value::as_object);
    let new_props = new.get("properties").and_then(Value::as_object);
    if let Some(old_props) = old_props {
        if let Some(new_props) = new_props {
            for (name, old_sub) in old_props {
                match new_props.get(name) {
                    None => {
                        out.push(change(
                            &format!("{path}.properties.{name}"),
                            old_required.contains(name),
                            format!("property \"{name}\" removed"),
                        ));
                    }
                    Some(new_sub) => {
                        compare(old_sub, new_sub, &format!("{path}.properties.{name}"), out);
                    }
                }
            }
            for name in new_props.keys() {
                if !old_props.contains_key(name) {
                    out.push(change(
                        &format!("{path}.properties.{name}"),
                        false,
                        format!("property \"{name}\" added"),
                    ));
                }
            }
        }
    }

    // items recursion
    if let (Some(old_items), Some(new_items)) = (old.get("items"), new.get("items")) {
        if old_items.is_object() && new_items.is_object() {
            compare(old_items, new_items, &format!("{path}.items"), out);
        }
    }

    // combinator recursion (allOf/anyOf/oneOf element-wise when lengths match)
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let (Some(old_a), Some(new_a)) = (
            old.get(keyword).and_then(Value::as_array),
            new.get(keyword).and_then(Value::as_array),
        ) {
            if old_a.len() == new_a.len() {
                for (i, (o, n)) in old_a.iter().zip(new_a.iter()).enumerate() {
                    if o.is_object() && n.is_object() {
                        compare(o, n, &format!("{path}.{keyword}[{i}]"), out);
                    }
                }
            } else {
                out.push(change(
                    &format!("{path}.{keyword}"),
                    false,
                    format!(
                        "{keyword} members changed ({} -> {})",
                        old_a.len(),
                        new_a.len()
                    ),
                ));
            }
        }
    }
}

fn type_set(v: &Value) -> std::collections::HashSet<String> {
    match v {
        Value::String(s) => [s.clone()].into_iter().collect(),
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect(),
        _ => Default::default(),
    }
}

fn string_set(v: Option<&Value>) -> std::collections::HashSet<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn num_of(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

fn usize_of(v: Option<&Value>) -> Option<usize> {
    v.and_then(Value::as_u64).map(|n| n as usize)
}

fn is_restricted(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(false)) | Some(Value::Object(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn changes(old: Value, new: Value) -> Vec<Change> {
        let mut out = Vec::new();
        compare(&old, &new, "$", &mut out);
        out
    }

    #[test]
    fn detects_breaking_changes() {
        let old = json!({"type": "object", "properties": {"age": {"type": "integer", "minimum": 0}}, "required": []});
        let new = json!({"type": "object", "properties": {"age": {"type": "integer", "minimum": 18}}, "required": ["age"]});
        let out = changes(old, new);
        assert!(out.iter().any(|c| c.breaking && c.path == "$.required"));
        assert!(out
            .iter()
            .any(|c| c.breaking && c.description.contains("minimum")));
    }

    #[test]
    fn optional_additions_are_notes() {
        let old = json!({"type": "object", "properties": {"a": {"type": "string"}}});
        let new = json!({"type": "object", "properties": {"a": {"type": "string"}, "b": {"type": "string"}}});
        let out = changes(old, new);
        assert_eq!(out.len(), 1);
        assert!(!out[0].breaking);
    }

    #[test]
    fn additional_properties_forbid_added_is_breaking() {
        let old = json!({"type": "object"});
        let new = json!({"type": "object", "additionalProperties": false});
        let out = changes(old, new);
        assert!(out[0].breaking);
    }
}
