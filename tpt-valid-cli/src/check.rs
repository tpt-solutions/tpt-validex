//! `validex check <schema> <data>` — validate JSON / JSONL / CSV files with
//! summary output in text, JSON, JUnit XML or SARIF, and optional valid /
//! error record sinks.

use std::io::{BufWriter, Write};

use serde_json::{json, Value};

use tpt_valid_core::{
    coerce_value, validate_batch, CoerceOptions, CsvDialect, CsvStats, ErrorClusterer, RowOutcome,
    ValidationError, ValidationOptions,
};
use tpt_valid_schema::{SchemaRegistry, Validator};

use crate::Args;

/// One failing record, keyed by position (JSONL line number, array index, or
/// CSV line).
struct RecordErrors {
    position: usize,
    label: String,
    errors: Vec<ValidationError>,
}

struct CheckStats {
    total: usize,
    valid: usize,
    invalid: usize,
    parse_errors: usize,
}

pub(crate) fn run(args: &[String]) -> Result<bool, String> {
    let args = Args::parse(args)?;
    let mut positional = args.positional.iter();
    let schema_path = positional.next().ok_or("check requires a <schema> path")?;
    let data_path = positional.next().ok_or("check requires a <data> path")?;

    let schema_text = std::fs::read_to_string(schema_path)
        .map_err(|e| format!("cannot read schema {schema_path}: {e}"))?;
    let validator = match args.get("registry") {
        Some(registry_path) => {
            let registry = load_registry(registry_path)?;
            Validator::new_with(&schema_text, &registry)
        }
        None => Validator::new(&schema_text),
    }
    .map_err(|e| format!("invalid schema {schema_path}: {e}"))?;

    let fail_fast = args.has("fail-fast");
    let max_errors = args
        .get("max-errors")
        .map(|v| v.parse::<usize>().map_err(|_| "invalid --max-errors"))
        .transpose()?
        .unwrap_or(1000);
    let opts = ValidationOptions {
        fail_fast,
        max_errors,
        ..ValidationOptions::default()
    };

    let dialect = CsvDialect {
        delimiter: crate::check_delimiter(args.get("delimiter"))?,
        has_headers: !args.has("no-headers"),
        ..CsvDialect::default()
    };
    let quiet = args.has("quiet");
    let format = args.get("format").unwrap_or("text");
    let coerce = if args.has("coerce") {
        Some(CoerceOptions {
            trim: true,
            numeric_strings: true,
            booleans: true,
            empty_string_as_null: true,
            ..CoerceOptions::default()
        })
    } else {
        None
    };
    let top_errors = args
        .get("top-errors")
        .map(|v| v.parse::<usize>().map_err(|_| "invalid --top-errors"))
        .transpose()?
        .unwrap_or(0);

    let data_lower = data_path.to_ascii_lowercase();
    let (stats, records, clusterer, coerce_changes) = if data_lower.ends_with(".csv") {
        run_csv(
            &validator,
            data_path,
            &dialect,
            &opts,
            &args,
            coerce.as_ref(),
        )?
    } else if data_lower.ends_with(".jsonl") || data_lower.ends_with(".ndjson") {
        run_jsonl(&validator, data_path, &opts, &args, coerce.as_ref())?
    } else {
        run_json(&validator, data_path, &opts, &args, coerce.as_ref())?
    };

    if top_errors > 0 {
        let clusters = clusterer.finish();
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        let _ = writeln!(
            out,
            "top failure patterns ({} shown):",
            top_errors.min(clusters.len())
        );
        for cluster in clusters.iter().take(top_errors) {
            let _ = writeln!(
                out,
                "  {:>5}× {} — {} (first at record {})",
                cluster.count, cluster.path, cluster.expected, cluster.first_position
            );
        }
    }
    if !coerce_changes.is_empty() && !quiet {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        let _ = writeln!(
            out,
            "coerced {} value(s) before validation",
            coerce_changes.len()
        );
    }

    report(format, data_path, &stats, &records, quiet)?;
    Ok(stats.invalid > 0 || stats.parse_errors > 0)
}

fn load_registry(path: &str) -> Result<SchemaRegistry, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let map: serde_json::Map<String, Value> =
        serde_json::from_str(&text).map_err(|e| format!("{path} is not a JSON object: {e}"))?;
    let mut registry = SchemaRegistry::new();
    for (uri, doc) in map {
        registry.insert_value(uri, doc);
    }
    Ok(registry)
}

type Records = Vec<RecordErrors>;

fn run_json(
    validator: &Validator,
    path: &str,
    opts: &ValidationOptions,
    args: &Args,
    coerce: Option<&CoerceOptions>,
) -> Result<
    (
        CheckStats,
        Records,
        ErrorClusterer,
        Vec<tpt_valid_core::CoerceChange>,
    ),
    String,
> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let value =
        tpt_valid_parser::parse(&text).map_err(|e| format!("invalid JSON in {path}: {e}"))?;

    let mut valid_out = open_valid_jsonl(args)?;
    let mut errors_out = open_errors(args)?;

    // A top-level array is validated item-by-item (paths carry the index).
    let items: Vec<(String, Value)> = match value {
        Value::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, v)| (format!("$[{i}]"), v))
            .collect(),
        other => vec![("$".to_string(), other)],
    };

    let mut stats = CheckStats {
        total: items.len(),
        valid: 0,
        invalid: 0,
        parse_errors: 0,
    };
    let mut records = Records::new();
    let mut clusterer = ErrorClusterer::new();
    let mut coerce_changes = Vec::new();
    let values = items_values(&items);
    let values = if let Some(coerce) = coerce {
        let mut coerced = values.clone();
        for value in &mut coerced {
            coerce_changes.extend(coerce_value(value, coerce));
        }
        coerced
    } else {
        values
    };
    let outcomes = validate_batch(validator.root(), &values, opts);
    for ((label, value), outcome) in items.into_iter().zip(outcomes) {
        if outcome.valid {
            stats.valid += 1;
            if let Some(out) = valid_out.as_mut() {
                writeln!(out, "{value}").map_err(io_err)?;
            }
        } else {
            stats.invalid += 1;
            if let Some(out) = errors_out.as_mut() {
                let record =
                    json!({"line": outcome.index + 1, "row": value, "errors": outcome.errors});
                writeln!(out, "{record}").map_err(io_err)?;
            }
            clusterer.record(outcome.index + 1, &outcome.errors);
            records.push(RecordErrors {
                position: outcome.index + 1,
                label,
                errors: outcome.errors,
            });
        }
    }
    Ok((stats, records, clusterer, coerce_changes))
}

fn items_values(items: &[(String, Value)]) -> Vec<Value> {
    items.iter().map(|(_, v)| v.clone()).collect()
}

fn run_jsonl(
    validator: &Validator,
    path: &str,
    opts: &ValidationOptions,
    args: &Args,
    coerce: Option<&CoerceOptions>,
) -> Result<
    (
        CheckStats,
        Records,
        ErrorClusterer,
        Vec<tpt_valid_core::CoerceChange>,
    ),
    String,
> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut valid_out = open_valid_jsonl(args)?;
    let mut errors_out = open_errors(args)?;

    let mut stats = CheckStats {
        total: 0,
        valid: 0,
        invalid: 0,
        parse_errors: 0,
    };
    let mut records = Records::new();
    let mut clusterer = ErrorClusterer::new();
    let mut coerce_changes = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        stats.total += 1;
        match tpt_valid_parser::parse(line) {
            Ok(mut value) => {
                if let Some(coerce) = coerce {
                    coerce_changes.extend(coerce_value(&mut value, coerce));
                }
                let report = validator.validate_with(&value, opts);
                if report.is_valid() {
                    stats.valid += 1;
                    if let Some(out) = valid_out.as_mut() {
                        writeln!(out, "{value}").map_err(io_err)?;
                    }
                } else {
                    stats.invalid += 1;
                    if let Some(out) = errors_out.as_mut() {
                        let record =
                            json!({"line": line_no, "row": value, "errors": report.errors});
                        writeln!(out, "{record}").map_err(io_err)?;
                    }
                    clusterer.record(line_no, &report.errors);
                    records.push(RecordErrors {
                        position: line_no,
                        label: format!("line {line_no}"),
                        errors: report.errors,
                    });
                }
            }
            Err(e) => {
                stats.parse_errors += 1;
                records.push(RecordErrors {
                    position: line_no,
                    label: format!("line {line_no}"),
                    errors: vec![ValidationError::new(
                        "$",
                        format!("invalid JSON: {e}"),
                        "valid JSON",
                        "parse error",
                    )],
                });
                if opts.fail_fast {
                    break;
                }
            }
        }
    }
    Ok((stats, records, clusterer, coerce_changes))
}

fn run_csv(
    validator: &Validator,
    path: &str,
    dialect: &CsvDialect,
    opts: &ValidationOptions,
    args: &Args,
    coerce: Option<&CoerceOptions>,
) -> Result<
    (
        CheckStats,
        Records,
        ErrorClusterer,
        Vec<tpt_valid_core::CoerceChange>,
    ),
    String,
> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut valid_csv_out = open_valid_csv(args)?;
    let mut errors_out = open_errors(args)?;

    let mut stats = CsvStats::default();
    let mut records = Records::new();
    let mut clusterer = ErrorClusterer::new();
    let coerce_changes = Vec::new();
    let quote = dialect.quote;
    let delim = dialect.delimiter;
    let result = tpt_valid_core::validate_csv_stream(
        validator.root(),
        std::io::BufReader::new(file),
        dialect,
        opts,
        |row| {
            match row {
                RowOutcome::Valid { fields, .. } => {
                    stats.total_rows += 1;
                    if let Some(coerce_opts) = coerce {
                        let _ = coerce_opts; // CSV cells are typed at parse time
                    }
                    stats.valid_rows += 1;
                    if let Some(out) = valid_csv_out.as_mut() {
                        let line = fields
                            .iter()
                            .map(|f| csv_quote(f, delim, quote))
                            .collect::<Vec<_>>()
                            .join(&(delim as char).to_string());
                        let _ = writeln!(out, "{line}");
                    }
                }
                RowOutcome::Invalid {
                    line,
                    value,
                    errors,
                    ..
                } => {
                    stats.total_rows += 1;
                    clusterer.record(line, &errors);
                    stats.invalid_rows += 1;
                    if let Some(out) = errors_out.as_mut() {
                        let record = json!({"line": line, "row": value, "errors": errors});
                        let _ = writeln!(out, "{record}");
                    }
                    records.push(RecordErrors {
                        position: line,
                        label: format!("line {line}"),
                        errors,
                    });
                }
                RowOutcome::ParseError { line, message } => {
                    stats.total_rows += 1;
                    stats.parse_errors += 1;
                    if let Some(out) = errors_out.as_mut() {
                        let record = json!({"line": line, "error": message});
                        let _ = writeln!(out, "{record}");
                    }
                    records.push(RecordErrors {
                        position: line,
                        label: format!("line {line}"),
                        errors: vec![ValidationError::new(
                            "$",
                            message,
                            "valid CSV",
                            "parse error",
                        )],
                    });
                }
            }
            true
        },
    )
    .map_err(|e| format!("CSV error: {e}"))?;
    stats = result;
    Ok((
        CheckStats {
            total: stats.total_rows,
            valid: stats.valid_rows,
            invalid: stats.invalid_rows,
            parse_errors: stats.parse_errors,
        },
        records,
        clusterer,
        coerce_changes,
    ))
}

fn csv_quote(field: &str, delim: u8, quote: u8) -> String {
    let needs_quoting = field.contains(delim as char)
        || field.contains(quote as char)
        || field.contains('\n')
        || field.contains('\r');
    if needs_quoting {
        format!(
            "{}{}{}",
            quote as char,
            field.replace(
                quote as char,
                &format!("{}{}", quote as char, quote as char)
            ),
            quote as char
        )
    } else {
        field.to_string()
    }
}

fn open_errors(args: &Args) -> Result<Option<BufWriter<std::fs::File>>, String> {
    match args.get("errors") {
        Some(path) => {
            let file =
                std::fs::File::create(path).map_err(|e| format!("cannot create {path}: {e}"))?;
            Ok(Some(BufWriter::new(file)))
        }
        None => Ok(None),
    }
}

fn open_valid_jsonl(args: &Args) -> Result<Option<BufWriter<std::fs::File>>, String> {
    match args.get("valid") {
        Some(path) => {
            let file =
                std::fs::File::create(path).map_err(|e| format!("cannot create {path}: {e}"))?;
            Ok(Some(BufWriter::new(file)))
        }
        None => Ok(None),
    }
}

fn open_valid_csv(args: &Args) -> Result<Option<BufWriter<std::fs::File>>, String> {
    // Same sink as --valid; for CSV input the valid records are re-quoted CSV.
    open_valid_jsonl(args)
}

fn io_err(e: std::io::Error) -> String {
    format!("I/O error: {e}")
}

// ---------------------------------------------------------------------------
// Output formats
// ---------------------------------------------------------------------------

fn report(
    format: &str,
    data_path: &str,
    stats: &CheckStats,
    records: &[RecordErrors],
    quiet: bool,
) -> Result<(), String> {
    match format {
        "text" => report_text(data_path, stats, records, quiet),
        "json" => report_json(data_path, stats, records),
        "junit" => report_junit(data_path, stats),
        "sarif" => report_sarif(data_path, records),
        other => Err(format!(
            "unknown --format \"{other}\" (expected text|json|junit|sarif)"
        )),
    }
}

fn report_text(
    data_path: &str,
    stats: &CheckStats,
    records: &[RecordErrors],
    quiet: bool,
) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if !quiet {
        let shown = records.iter().take(20);
        for record in shown {
            let _ = writeln!(out, "{} — {}", record.label, data_path);
            for error in &record.errors {
                let _ = writeln!(out, "  {}: {}", error.path, error.message);
            }
        }
        if records.len() > 20 {
            let _ = writeln!(out, "... and {} more failing records", records.len() - 20);
        }
    }
    let _ = writeln!(
        out,
        "{}: {} record(s): {} valid, {} invalid, {} parse error(s)",
        data_path, stats.total, stats.valid, stats.invalid, stats.parse_errors
    );
    Ok(())
}

fn report_json(
    data_path: &str,
    stats: &CheckStats,
    records: &[RecordErrors],
) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let report = json!({
        "file": data_path,
        "total": stats.total,
        "valid": stats.valid,
        "invalid": stats.invalid,
        "parseErrors": stats.parse_errors,
        "records": records.iter().map(|r| json!({
            "position": r.position,
            "label": r.label,
            "errors": r.errors,
        })).collect::<Vec<_>>(),
    });
    let _ = writeln!(out, "{report}");
    Ok(())
}

fn report_junit(data_path: &str, stats: &CheckStats) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let failed = stats.invalid > 0 || stats.parse_errors > 0;
    let _ = writeln!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <testsuites>\n  <testsuite name=\"validex: {}\" tests=\"{}\" failures=\"{}\" errors=\"{}\">",
        xml_escape(data_path),
        stats.total,
        stats.invalid,
        stats.parse_errors
    );
    if failed {
        let _ = writeln!(
            out,
            "    <testcase name=\"validation\"><failure message=\"{} of {} records failed\"/></testcase>",
            stats.invalid + stats.parse_errors,
            stats.total
        );
    } else {
        let _ = writeln!(out, "    <testcase name=\"validation\"/>");
    }
    let _ = writeln!(out, "  </testsuite>\n</testsuites>");
    Ok(())
}

fn report_sarif(data_path: &str, records: &[RecordErrors]) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let results: Vec<Value> = records
        .iter()
        .flat_map(|r| {
            r.errors.iter().map(move |e| {
                json!({
                    "ruleId": "validex/schema",
                    "level": "error",
                    "message": {"text": format!("{}: {}", e.path, e.message)},
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": {"uri": data_path},
                            "region": {"startLine": r.position},
                        }
                    }],
                })
            })
        })
        .collect();
    let sarif = json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {"driver": {"name": "validex", "version": env!("CARGO_PKG_VERSION")}},
            "results": results,
        }],
    });
    let _ = writeln!(out, "{sarif}");
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(name: &str, content: &str) -> String {
        let dir = std::env::temp_dir().join(format!("validex-cli-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn check_end_to_end_exit_semantics() {
        let schema = write_temp("s2.json", r#"{"type":"object","required":["age"]}"#);
        let good = write_temp("good.jsonl", "{\"age\":1}\n{\"age\":2}\n");
        let bad = write_temp("bad.jsonl", "{\"age\":1}\n{\"nope\":2}\n");

        let ok = run(&[schema.clone(), good]).unwrap();
        assert!(!ok, "all-valid input exits clean");

        let failed = run(&[schema, bad]).unwrap();
        assert!(failed, "invalid input reports failure");
    }

    #[test]
    fn check_csv_with_options() {
        let schema = write_temp(
            "s3.json",
            r#"{"type":"object","properties":{"zip":{"type":"string"}}}"#,
        );
        let data = write_temp("d3.csv", "zip\n01234\n");
        let ok = run(&[schema, data]).unwrap();
        assert!(!ok, "schema-declared string columns keep leading zeros");
    }
}
