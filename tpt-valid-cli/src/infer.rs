//! `validex infer <data>` — generate a starter JSON Schema from sample data.
//!
//! JSON / JSONL inputs are sampled per key (with bounded recursion into
//! nested objects); CSV inputs reuse the streaming reader's column type
//! inference. The result is a conservative schema: string/number/integer/
//! boolean/null types per column, `required` only for keys present in every
//! sampled record.

use serde_json::{json, Map, Value};

use tpt_valid_core::{infer_column_types, ColumnType, CsvDialect, CsvReader};

use crate::Args;

/// How many records to sample.
const SAMPLE_LIMIT: usize = 1000;
/// How deep to merge nested objects.
const MAX_DEPTH: usize = 5;

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let args = Args::parse(args)?;
    let data_path = args
        .positional
        .first()
        .ok_or("infer requires a <data> path")?;

    let dialect = CsvDialect {
        delimiter: crate::check_delimiter(args.get("delimiter"))?,
        has_headers: !args.has("no-headers"),
        ..CsvDialect::default()
    };

    let lower = data_path.to_ascii_lowercase();
    let schema = if lower.ends_with(".csv") {
        infer_csv(data_path, &dialect)?
    } else if lower.ends_with(".jsonl") || lower.ends_with(".ndjson") {
        let text = std::fs::read_to_string(data_path)
            .map_err(|e| format!("cannot read {data_path}: {e}"))?;
        let samples: Vec<Value> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(SAMPLE_LIMIT)
            .map(|l| tpt_valid_parser::parse(l).map_err(|e| format!("invalid JSONL: {e}")))
            .collect::<Result<Vec<_>, _>>()?;
        infer_from_samples(&samples)
    } else {
        let text = std::fs::read_to_string(data_path)
            .map_err(|e| format!("cannot read {data_path}: {e}"))?;
        let value = tpt_valid_parser::parse(&text).map_err(|e| format!("invalid JSON: {e}"))?;
        match value {
            Value::Array(items) => infer_from_samples(&items),
            other => infer_from_samples(std::slice::from_ref(&other)),
        }
    };

    let rendered = serde_json::to_string_pretty(&schema).expect("schema serializes");
    match args.get("output") {
        Some(out_path) => {
            std::fs::write(out_path, rendered + "\n")
                .map_err(|e| format!("cannot write {out_path}: {e}"))?;
            println!("schema written to {out_path}");
        }
        None => println!("{rendered}"),
    }
    Ok(())
}

fn infer_csv(path: &str, dialect: &CsvDialect) -> Result<Value, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut reader = CsvReader::with_dialect(std::io::BufReader::new(file), dialect.clone());

    let headers: Vec<String> = if dialect.has_headers {
        match reader
            .next_record()
            .map_err(|e| format!("CSV error: {e}"))?
        {
            Some(h) => h.fields,
            None => return Ok(json!({"type": "object"})),
        }
    } else {
        Vec::new()
    };

    let mut sample: Vec<Vec<String>> = Vec::new();
    while sample.len() < SAMPLE_LIMIT {
        match reader
            .next_record()
            .map_err(|e| format!("CSV error: {e}"))?
        {
            Some(record) => sample.push(record.fields),
            None => break,
        }
    }
    let width = if dialect.has_headers {
        headers.len()
    } else {
        sample.first().map(|r| r.len()).unwrap_or(0)
    };
    let names: Vec<String> = if dialect.has_headers {
        headers
    } else {
        (0..width).map(|i| format!("column_{i}")).collect()
    };
    let types = infer_column_types(sample.iter().cloned(), width);

    let mut properties = Map::new();
    let mut required = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let column_type = types.get(i).copied().unwrap_or(ColumnType::String);
        let keyword = match column_type {
            ColumnType::Boolean => "boolean",
            ColumnType::Integer => "integer",
            ColumnType::Float => "number",
            ColumnType::String => "string",
        };
        let present_in_all = !sample.is_empty()
            && sample
                .iter()
                .all(|row| row.get(i).is_some_and(|cell| !cell.is_empty()));
        if present_in_all {
            required.push(name.clone());
        }
        let type_value = if nullable_column(&sample, i) {
            json!([keyword, "null"])
        } else {
            json!(keyword)
        };
        properties.insert(name.clone(), json!({"type": type_value}));
    }
    Ok(json!({
        "type": "object",
        "properties": properties,
        "required": required,
    }))
}

/// Columns with empty cells in the sample get a `null` alternative.
fn nullable_column(sample: &[Vec<String>], index: usize) -> bool {
    sample
        .iter()
        .any(|row| row.get(index).map_or(true, |cell| cell.is_empty()))
}

fn infer_from_samples(samples: &[Value]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    let object_count = samples.iter().filter(|s| s.is_object()).count();

    let mut key_samples: Vec<(String, Vec<&Value>)> = Vec::new();
    for sample in samples {
        if let Value::Object(map) = sample {
            for (key, value) in map {
                let entry = key_samples
                    .iter_mut()
                    .find(|(k, _)| k == key)
                    .map(|(_, values)| {
                        values.push(value);
                    });
                if entry.is_none() {
                    key_samples.push((key.clone(), vec![value]));
                }
            }
        }
    }
    key_samples.sort_by(|a, b| a.0.cmp(&b.0));

    for (key, values) in key_samples {
        if values.len() == object_count && object_count > 0 {
            required.push(key.clone());
        }
        let nullable = values.iter().any(|v| v.is_null());
        let keyword = merged_type(&values, 0);
        let schema = match (keyword, nullable) {
            (Some(k), true) => json!({"type": [k, "null"]}),
            (Some(k), false) => json!({"type": k}),
            (None, _) => {
                // Mixed/complex shapes: fall back to per-value structure.
                merge_structured(&values)
            }
        };
        properties.insert(key, schema);
    }

    if object_count > 0 {
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
        })
    } else {
        let refs: Vec<&Value> = samples.iter().collect();
        merge_structured(&refs)
    }
}

/// The common scalar type of the sampled values, or None for mixed shapes.
fn merged_type(values: &[&Value], depth: usize) -> Option<&'static str> {
    let mut keyword: Option<&'static str> = None;
    for value in values {
        let k = match value {
            Value::Null => continue, // nulls are tracked separately
            Value::Bool(_) => "boolean",
            Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    "integer"
                } else {
                    "number"
                }
            }
            Value::String(_) => "string",
            Value::Array(_) => {
                return if depth < MAX_DEPTH {
                    None
                } else {
                    Some("array")
                }
            }
            Value::Object(_) => {
                return if depth < MAX_DEPTH {
                    None
                } else {
                    Some("object")
                }
            }
        };
        keyword = match keyword {
            None => Some(k),
            Some(prev) if prev == k => Some(k),
            Some("integer") if k == "number" => Some("number"),
            Some("number") if k == "integer" => Some("number"),
            _ => return None,
        };
    }
    keyword
}

/// Build a structured schema for arrays/objects/mixed samples.
fn merge_structured(values: &[&Value]) -> Value {
    if values.iter().all(|v| v.is_array()) {
        let items: Vec<Value> = values
            .iter()
            .filter_map(|v| v.as_array())
            .flat_map(|a| a.iter())
            .cloned()
            .collect();
        let refs: Vec<&Value> = items.iter().collect();
        let item_schema = match merged_type(&refs, 1) {
            Some(k) => json!({"type": k}),
            None if items.is_empty() => Value::Bool(true),
            None => merge_structured(&refs),
        };
        return json!({"type": "array", "items": item_schema});
    }
    if values.iter().all(|v| v.is_object()) && values.len() == values.len() {
        // Recurse a single level for object samples.
        let owned: Vec<Value> = values.iter().map(|v| (*v).clone()).collect();
        let sub = infer_from_samples(&owned);
        if sub.is_object() && sub["type"] == "object" {
            return sub;
        }
    }
    // Mixed shapes: accept anything.
    Value::Bool(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_object_types_and_required() {
        let samples = vec![
            json!({"name": "Alice", "age": 30, "note": null}),
            json!({"name": "Bob", "age": 25.5, "note": "hi"}),
        ];
        let schema = infer_from_samples(&samples);
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["name"]["type"], "string");
        assert_eq!(schema["properties"]["age"]["type"], "number");
        assert_eq!(
            schema["properties"]["note"]["type"],
            json!(["string", "null"])
        );
        // Keys present in every sampled record are required, even when they
        // can also be null (the type union covers it).
        assert_eq!(schema["required"], json!(["age", "name", "note"]));
    }

    #[test]
    fn infers_arrays() {
        let samples = vec![json!({"tags": ["a", "b"]}), json!({"tags": []})];
        let schema = infer_from_samples(&samples);
        assert_eq!(schema["properties"]["tags"]["type"], "array");
        assert_eq!(schema["properties"]["tags"]["items"]["type"], "string");
    }
}
