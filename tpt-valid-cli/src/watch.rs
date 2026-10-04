//! `validex watch` — continuously validate files mapped to schemas by a
//! `.validex.toml` config, re-validating when files change (mtime polling,
//! no external dependencies).
//!
//! ```toml
//! interval_ms = 500
//!
//! [[watch]]
//! schema = "schemas/user.schema.json"
//! files = ["data/users/*.jsonl"]
//! ```

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use tpt_valid_schema::Validator;

use crate::{glob, toml_min, Args};

/// One schema → files mapping from the config.
struct WatchRule {
    schema_path: String,
    schema: Validator,
    patterns: Vec<String>,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let args = Args::parse(args)?;
    let config_path = args.get("config").unwrap_or(".validex.toml").to_string();
    let text = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("cannot read {config_path}: {e}"))?;
    let doc = toml_min::parse(&text).map_err(|e| format!("invalid config {config_path}: {e}"))?;

    let interval = doc
        .root
        .get("interval_ms")
        .map(|v| match v {
            toml_min::TomlValue::Int(n) if *n > 0 => *n as u64,
            _ => 500,
        })
        .unwrap_or(500);

    let mut rules = Vec::new();
    for (name, table) in &doc.tables {
        if name != "watch" {
            continue;
        }
        let schema_path = table
            .get("schema")
            .and_then(toml_min::TomlValue::as_str)
            .ok_or("each [[watch]] needs a schema = \"...\" entry")?;
        let patterns = table
            .get("files")
            .and_then(toml_min::TomlValue::as_list)
            .ok_or("each [[watch]] needs a files = [...] entry")?
            .to_vec();
        let schema_text = std::fs::read_to_string(schema_path)
            .map_err(|e| format!("cannot read schema {schema_path}: {e}"))?;
        let schema = Validator::new(&schema_text)
            .map_err(|e| format!("invalid schema {schema_path}: {e}"))?;
        rules.push(WatchRule {
            schema_path: schema_path.to_string(),
            schema,
            patterns,
        });
    }
    if rules.is_empty() {
        return Err(format!(
            "no [[watch]] sections found in {config_path}; see --help for the config format"
        ));
    }

    // Expand patterns once; new files matching a pattern are picked up on
    // the next scan.
    println!("watching {} rule(s) — Ctrl+C to stop", rules.len());
    let mut mtimes: HashMap<String, SystemTime> = HashMap::new();
    loop {
        let mut changed = false;
        for rule in &rules {
            for path in expand(&rule.patterns) {
                let current = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                let previous = mtimes.get(&path).copied();
                match (previous, current) {
                    (Some(prev), Some(now)) if prev != now => {
                        changed = true;
                        mtimes.insert(path.clone(), now);
                        validate_one(rule, &path);
                    }
                    (None, Some(now)) => {
                        mtimes.insert(path.clone(), now);
                        validate_one(rule, &path);
                    }
                    _ => {}
                }
            }
        }
        if !changed {
            // Sleep in small slices; a plain sleep keeps the loop simple and
            // portable.
            std::thread::sleep(Duration::from_millis(interval));
        } else {
            std::thread::sleep(Duration::from_millis(interval.min(50)));
        }
    }
}

/// Expand glob patterns against the current directory tree.
fn expand(patterns: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for pattern in patterns {
        for path in walk(std::path::Path::new("."), 0) {
            let path_str = path.to_string_lossy().replace('\\', "/");
            let trimmed = path_str.strip_prefix("./").unwrap_or(&path_str);
            if glob::matches(pattern, trimmed) {
                out.push(trimmed.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Bounded recursive walk (depth-limited; skips .git and node_modules).
fn walk(dir: &std::path::Path, depth: usize) -> Vec<std::path::PathBuf> {
    if depth > 12 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name == ".git" || name == "node_modules" || name == "target" {
            continue;
        }
        if path.is_dir() {
            out.extend(walk(&path, depth + 1));
        } else {
            out.push(path);
        }
    }
    out
}

fn validate_one(rule: &WatchRule, path: &str) {
    let Ok(text) = std::fs::read_to_string(path) else {
        println!("{path}: unreadable");
        return;
    };
    let is_jsonl = path.ends_with(".jsonl") || path.ends_with(".ndjson");
    let total_valid = if is_jsonl {
        let mut valid = 0usize;
        let mut invalid = 0usize;
        let mut parse_errors = 0usize;
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match tpt_valid_parser::parse(line) {
                Ok(value) => {
                    if rule.schema.validate(&value).is_valid() {
                        valid += 1;
                    } else {
                        invalid += 1;
                        println!("{path}: line {} invalid", i + 1);
                    }
                }
                Err(e) => {
                    parse_errors += 1;
                    println!("{path}: line {} parse error ({e})", i + 1);
                }
            }
        }
        if invalid == 0 && parse_errors == 0 {
            println!("{path}: OK ({valid} records)");
        } else {
            println!(
                "{path}: {valid} valid, {invalid} invalid, {parse_errors} parse errors (schema {})",
                rule.schema_path
            );
        }
        invalid + parse_errors
    } else {
        match tpt_valid_parser::parse(&text) {
            Ok(value) => {
                let report = rule.schema.validate(&value);
                if report.is_valid() {
                    println!("{path}: OK");
                    0
                } else {
                    for error in report.errors.iter().take(10) {
                        println!("{path}: {}: {}", error.path, error.message);
                    }
                    1
                }
            }
            Err(e) => {
                println!("{path}: parse error ({e})");
                1
            }
        }
    };
    let _ = total_valid;
}
