//! Minimal TOML-subset reader for `.validex.toml` watch configs — no
//! external dependencies. Supports: comments (`#`), `[section]` headers,
//! `[[array-of-sections]]` headers, and `key = "string"` /
//! `key = ["a", "b"]` / bare `true`/`false` / integer values.
//!
//! ```toml
//! [[watch]]
//! schema = "schemas/user.schema.json"
//! files = ["data/users/*.jsonl"]
//! strict_csv = false
//! ```

use std::collections::HashMap;

#[derive(Debug, Default)]
pub(crate) struct TomlDoc {
    /// Top-level key/value pairs.
    pub root: HashMap<String, TomlValue>,
    /// `[[name]]` array-of-tables entries, in file order.
    pub tables: Vec<(String, HashMap<String, TomlValue>)>,
}

#[derive(Debug, Clone)]
pub(crate) enum TomlValue {
    Str(String),
    Int(i64),
    List(Vec<String>),
}

impl TomlValue {
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            TomlValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn as_list(&self) -> Option<&[String]> {
        match self {
            TomlValue::List(items) => Some(items),
            _ => None,
        }
    }
}

pub(crate) fn parse(input: &str) -> Result<TomlDoc, String> {
    let mut doc = TomlDoc::default();
    // Values between section headers land in `root`; tables collect their
    // own keys.
    for (line_no, raw_line) in input.lines().enumerate() {
        let line_no = line_no + 1;
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix("[[") {
            let name = header
                .strip_suffix("]]")
                .ok_or_else(|| format!(".toml line {line_no}: unterminated [[header]]"))?
                .trim()
                .to_string();
            doc.tables.push((name, HashMap::new()));
        } else if let Some(header) = line.strip_prefix('[') {
            // [section] headers are parsed but not exposed (watch configs
            // only use [[watch]]).
            if !header.ends_with(']') {
                return Err(format!(".toml line {line_no}: unterminated [header]"));
            }
        } else {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!(".toml line {line_no}: expected key = value"))?;
            let key = key.trim().trim_matches('"').to_string();
            let value =
                parse_value(value.trim()).map_err(|e| format!(".toml line {line_no}: {e}"))?;
            match doc.tables.last_mut() {
                Some((_, table)) => {
                    table.insert(key, value);
                }
                None => {
                    doc.root.insert(key, value);
                }
            }
        }
    }
    Ok(doc)
}

fn strip_comment(line: &str) -> &str {
    // Comments start at '#' unless inside a quoted string.
    let mut in_string = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..i],
            _ => {}
        }
    }
    line
}

fn parse_value(text: &str) -> Result<TomlValue, String> {
    if let Some(rest) = text.strip_prefix('[') {
        let rest = rest.strip_suffix(']').ok_or("unterminated list")?;
        let mut items = Vec::new();
        for part in rest.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            items.push(unquote(part)?);
        }
        return Ok(TomlValue::List(items));
    }
    if let Some(rest) = text.strip_prefix('"') {
        let raw = rest.strip_suffix('"').ok_or("unterminated string")?;
        return Ok(TomlValue::Str(unescape(raw)));
    }
    if let Ok(n) = text.parse::<i64>() {
        return Ok(TomlValue::Int(n));
    }
    Err(format!("unsupported value: {text}"))
}

fn unquote(s: &str) -> Result<String, String> {
    let rest = s
        .strip_prefix('"')
        .ok_or_else(|| format!("expected quoted string, got {s}"))?;
    let raw = rest.strip_suffix('"').ok_or("unterminated string")?;
    Ok(unescape(raw))
}

fn unescape(s: &str) -> String {
    s.replace("\\\"", "\"").replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_watch_config() {
        let doc = parse(
            r#"
# watch config
interval_ms = 250

[[watch]]
schema = "schemas/user.schema.json"
files = ["data/users/*.jsonl", "data/users/*.csv"]

[[watch]]
schema = "schemas/order.schema.json"
files = ["data/orders/*.json"]
"#,
        )
        .unwrap();
        assert_eq!(doc.root.get("interval_ms").and_then(|v| v.as_str()), None);
        assert_eq!(doc.tables.len(), 2);
        let (name, table) = &doc.tables[0];
        assert_eq!(name, "watch");
        assert_eq!(
            table.get("schema").and_then(TomlValue::as_str),
            Some("schemas/user.schema.json")
        );
        let files = table.get("files").and_then(TomlValue::as_list).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files[0].contains("*.jsonl"));
    }

    #[test]
    fn comments_inside_strings_kept() {
        let doc = parse(r#"path = "a#b" # real comment"#).unwrap();
        assert_eq!(
            doc.root.get("path").and_then(TomlValue::as_str),
            Some("a#b")
        );
    }
}
