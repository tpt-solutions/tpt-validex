//! Proc macros for `tpt-validex`: the `schema!` DSL (spec §5.1, Phase 8).
//!
//! The macro parses the DSL at compile time, rejects semantic errors
//! (`min > max`, unknown options/formats, malformed syntax) with
//! `compile_error!`, and emits AST-construction code that flows through the
//! same IR → state-machine pipeline as JSON Schema.
//!
//! Grammar:
//!
//! ```text
//! schema  := type_expr
//! entry   := ("required" | "optional") STRING "=>" type_expr
//! object  := "object" "{" (entry ",")* "}"
//! array   := "array" "(" opts? ")"           // opts include items = <type>
//! scalar  := ("string"|"integer"|"number"|"boolean"|"null"|"any") "(" opts? ")"
//! opt     := IDENT "=" (STRING | INT | FLOAT | true | false | [list] | type_expr)
//! ```
//!
//! This crate intentionally has zero dependencies beyond `proc_macro`.

use proc_macro::{Delimiter, TokenStream, TokenTree};

/// Build the `schema!` macro: compile a schema from the DSL into a
/// `validex::Validator`.
#[proc_macro]
pub fn schema(input: TokenStream) -> TokenStream {
    let trees: Vec<TokenTree> = input.into_iter().collect();
    let mut parser = Parser { trees, pos: 0 };
    let result = parser.parse_type_expr().and_then(|ty| {
        if let Some(extra) = parser.peek() {
            return Err(format!("unexpected token after schema: {extra}"));
        }
        check_type(&ty)?;
        Ok(emit(&ty))
    });
    match result {
        Ok(code) => compile(code),
        Err(message) => compile(format!("::std::compile_error!{{\"schema!: {message}\"}}")),
    }
}

fn compile(code: String) -> TokenStream {
    code.parse()
        .expect("validex-macros internal error: generated invalid tokens")
}

// ---------------------------------------------------------------------------
// DSL AST
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum DslValue {
    Str(String),
    Int(i128),
    Float(f64),
    Bool(bool),
    List(Vec<DslValue>),
}

/// An option value: a literal, or (for `items`) a nested type expression.
#[derive(Debug, Clone)]
enum OptVal {
    V(DslValue),
    T(DslType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScalarKind {
    Str,
    Integer,
    Number,
    Bool,
    Null,
    Any,
}

impl ScalarKind {
    fn keyword(self) -> &'static str {
        match self {
            ScalarKind::Str => "string",
            ScalarKind::Integer => "integer",
            ScalarKind::Number => "number",
            ScalarKind::Bool => "boolean",
            ScalarKind::Null => "null",
            ScalarKind::Any => "any",
        }
    }
}

#[derive(Debug, Clone)]
enum DslType {
    Scalar {
        kind: ScalarKind,
        opts: Vec<(String, OptVal)>,
    },
    Array {
        items: Option<Box<DslType>>,
        opts: Vec<(String, OptVal)>,
    },
    Object(Vec<DslEntry>),
}

#[derive(Debug, Clone)]
struct DslEntry {
    required: bool,
    name: String,
    ty: DslType,
}

/// Formats accepted by `format=` (mirrors `tpt_valid_core::Format`).
const KNOWN_FORMATS: &[&str] = &[
    "email",
    "uri",
    "url",
    "iri",
    "date",
    "date-time",
    "datetime",
    "uuid",
    "ipv4",
    "ipv6",
    "hostname",
    "idn-hostname",
];

// ---------------------------------------------------------------------------
// Token-stream parser
// ---------------------------------------------------------------------------

struct Parser {
    trees: Vec<TokenTree>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<String> {
        self.trees.get(self.pos).map(|t| t.to_string())
    }

    fn next(&mut self) -> Option<TokenTree> {
        let t = self.trees.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn expect_punct(&mut self, ch: char) -> Result<(), String> {
        match self.next() {
            Some(TokenTree::Punct(p)) if p.as_char() == ch => Ok(()),
            other => Err(format!("expected '{ch}', found {}", describe(other))),
        }
    }

    fn ident(&mut self, what: &str) -> Result<String, String> {
        match self.next() {
            Some(TokenTree::Ident(i)) => Ok(i.to_string()),
            other => Err(format!("expected {what}, found {}", describe(other))),
        }
    }

    fn group(&mut self, delimiter: Delimiter, what: &str) -> Result<Vec<TokenTree>, String> {
        match self.next() {
            Some(TokenTree::Group(g)) if g.delimiter() == delimiter => {
                Ok(g.stream().into_iter().collect())
            }
            other => Err(format!("expected {what}, found {}", describe(other))),
        }
    }

    fn string_lit(&mut self, what: &str) -> Result<String, String> {
        match self.next() {
            Some(TokenTree::Literal(l)) => unquote_string(&l.to_string())
                .ok_or_else(|| format!("expected {what}, found non-string literal `{l}`")),
            other => Err(format!("expected {what}, found {}", describe(other))),
        }
    }

    fn value(&mut self) -> Result<DslValue, String> {
        match self.next() {
            Some(TokenTree::Literal(l)) => {
                let text = l.to_string();
                if text.starts_with('"') {
                    let s = unquote_string(&text)
                        .ok_or_else(|| format!("malformed string literal `{text}`"))?;
                    Ok(DslValue::Str(s))
                } else {
                    let cleaned = text.replace('_', "");
                    if let Ok(i) = cleaned.parse::<i128>() {
                        Ok(DslValue::Int(i))
                    } else if let Ok(f) = cleaned.parse::<f64>() {
                        Ok(DslValue::Float(f))
                    } else {
                        Err(format!("unsupported literal `{text}`"))
                    }
                }
            }
            Some(TokenTree::Punct(p)) if p.as_char() == '-' => match self.next() {
                Some(TokenTree::Literal(l)) => {
                    let text = format!("-{}", l.to_string().replace('_', ""));
                    if let Ok(i) = text.parse::<i128>() {
                        Ok(DslValue::Int(i))
                    } else if let Ok(f) = text.parse::<f64>() {
                        Ok(DslValue::Float(f))
                    } else {
                        Err(format!("unsupported negative literal `{text}`"))
                    }
                }
                other => Err(format!(
                    "expected number after '-', found {}",
                    describe(other)
                )),
            },
            Some(TokenTree::Ident(i)) => match i.to_string().as_str() {
                "true" => Ok(DslValue::Bool(true)),
                "false" => Ok(DslValue::Bool(false)),
                other => Err(format!("expected a value, found `{other}`")),
            },
            other => Err(format!("expected a value, found {}", describe(other))),
        }
    }

    fn list_value(&mut self) -> Result<DslValue, String> {
        let trees = self.group(Delimiter::Bracket, "a bracketed list `[...]`")?;
        let mut inner = Parser { trees, pos: 0 };
        let mut values = Vec::new();
        while inner.peek().is_some() {
            values.push(inner.value()?);
            if inner.peek().is_some() {
                inner.expect_punct(',')?;
            }
        }
        Ok(DslValue::List(values))
    }

    fn parse_type_expr(&mut self) -> Result<DslType, String> {
        let name =
            self.ident("a type name (object|string|integer|number|boolean|null|array|any)")?;
        match name.as_str() {
            "object" => {
                let trees = self.group(Delimiter::Brace, "`{ ... }` object body")?;
                let mut inner = Parser { trees, pos: 0 };
                let entries = inner.parse_entries()?;
                if let Some(extra) = inner.peek() {
                    return Err(format!("unexpected token in object body: {extra}"));
                }
                Ok(DslType::Object(entries))
            }
            "array" => {
                let opts = self.parse_opts()?;
                let mut items = None;
                let mut rest = Vec::with_capacity(opts.len());
                for (key, value) in opts {
                    if key == "items" {
                        if items.is_some() {
                            return Err("duplicate option `items`".into());
                        }
                        match value {
                            OptVal::T(t) => items = Some(Box::new(t)),
                            OptVal::V(_) => return Err("`items` must be a type expression".into()),
                        }
                    } else {
                        rest.push((key, value));
                    }
                }
                Ok(DslType::Array { items, opts: rest })
            }
            "string" | "integer" | "number" | "boolean" | "null" | "any" => {
                let kind = match name.as_str() {
                    "string" => ScalarKind::Str,
                    "integer" => ScalarKind::Integer,
                    "number" => ScalarKind::Number,
                    "boolean" => ScalarKind::Bool,
                    "null" => ScalarKind::Null,
                    _ => ScalarKind::Any,
                };
                Ok(DslType::Scalar { kind, opts: self.parse_opts()? })
            }
            other => Err(format!(
                "unknown type `{other}` (expected object|string|integer|number|boolean|null|array|any)"
            )),
        }
    }

    /// Parse `(...)` options; absent parens yield an empty list.
    fn parse_opts(&mut self) -> Result<Vec<(String, OptVal)>, String> {
        let mut opts = Vec::new();
        if self.peek().map(|t| t.starts_with('(')).unwrap_or(false) {
            let trees = self.group(Delimiter::Parenthesis, "`(...)` options")?;
            let mut inner = Parser { trees, pos: 0 };
            while inner.peek().is_some() {
                let key = inner.ident("an option name")?;
                inner.expect_punct('=')?;
                let value = if key == "items" {
                    OptVal::T(inner.parse_type_expr()?)
                } else if inner.peek().map(|t| t.starts_with('[')).unwrap_or(false) {
                    OptVal::V(inner.list_value()?)
                } else {
                    OptVal::V(inner.value()?)
                };
                opts.push((key, value));
                if inner.peek().is_some() {
                    inner.expect_punct(',')?;
                }
            }
        }
        Ok(opts)
    }

    fn parse_entries(&mut self) -> Result<Vec<DslEntry>, String> {
        let mut entries = Vec::new();
        while self.peek().is_some() {
            let modifier = self.ident("`required` or `optional`")?;
            let required = match modifier.as_str() {
                "required" => true,
                "optional" => false,
                other => {
                    return Err(format!(
                        "expected `required` or `optional`, found `{other}`"
                    ))
                }
            };
            let name = self.string_lit("a property name string")?;
            self.expect_punct('=')?;
            self.expect_punct('>')?;
            let ty = self.parse_type_expr()?;
            entries.push(DslEntry { required, name, ty });
            if self.peek().is_some() {
                self.expect_punct(',')?;
            }
        }
        Ok(entries)
    }
}

fn describe(token: Option<TokenTree>) -> String {
    match token {
        None => "end of input".to_string(),
        Some(t) => format!("`{t}`"),
    }
}

/// Unquote a Rust string literal (supports common escapes).
fn unquote_string(literal: &str) -> Option<String> {
    let inner = literal.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '0' => out.push('\0'),
            'u' => {
                if chars.next()? != '{' {
                    return None;
                }
                let mut hex = String::new();
                for hc in chars.by_ref() {
                    if hc == '}' {
                        break;
                    }
                    hex.push(hc);
                }
                let code = u32::from_str_radix(&hex, 16).ok()?;
                out.push(char::from_u32(code)?);
            }
            _ => return None,
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Semantic validation
// ---------------------------------------------------------------------------

fn check_type(ty: &DslType) -> Result<(), String> {
    match ty {
        DslType::Scalar { kind, opts } => check_scalar(*kind, opts),
        DslType::Array { items, opts } => {
            let allowed = ["min_items", "max_items", "unique"];
            check_dup_and_allowed(opts, &allowed, "array")?;
            for (key, value) in opts {
                check_value(key, values_of(value)?)?;
            }
            if let (Some(min), Some(max)) = (opt_int(opts, "min_items"), opt_int(opts, "max_items"))
            {
                if min > max {
                    return Err(format!("min_items ({min}) must be <= max_items ({max})"));
                }
            }
            if let Some(items) = items {
                check_type(items)?;
            }
            Ok(())
        }
        DslType::Object(entries) => {
            let mut names = std::collections::HashSet::new();
            for entry in entries {
                if !names.insert(entry.name.clone()) {
                    return Err(format!("duplicate property \"{}\"", entry.name));
                }
                check_type(&entry.ty)?;
            }
            Ok(())
        }
    }
}

fn check_scalar(kind: ScalarKind, opts: &[(String, OptVal)]) -> Result<(), String> {
    let (allowed, what): (&[&str], &str) = match kind {
        ScalarKind::Str => (
            ["min_length", "max_length", "pattern", "format", "enum"].as_slice(),
            "string",
        ),
        ScalarKind::Integer | ScalarKind::Number => (
            [
                "min",
                "max",
                "exclusive_min",
                "exclusive_max",
                "multiple_of",
                "enum",
            ]
            .as_slice(),
            kind.keyword(),
        ),
        _ => ([].as_slice(), kind.keyword()),
    };
    check_dup_and_allowed(opts, allowed, what)?;
    for (key, value) in opts {
        check_value(key, values_of(value)?)?;
    }
    if kind == ScalarKind::Str {
        if let (Some(min), Some(max)) = (opt_int(opts, "min_length"), opt_int(opts, "max_length")) {
            if min > max {
                return Err(format!("min_length ({min}) must be <= max_length ({max})"));
            }
        }
    }
    if matches!(kind, ScalarKind::Integer | ScalarKind::Number) {
        let lower = opt_float(opts, "min").or(opt_float(opts, "exclusive_min"));
        let upper = opt_float(opts, "max").or(opt_float(opts, "exclusive_max"));
        if let (Some(min), Some(max)) = (lower, upper) {
            if min > max {
                return Err(format!("min ({min}) must be <= max ({max})"));
            }
        }
        if let Some(m) = opt_float(opts, "multiple_of") {
            if m <= 0.0 {
                return Err(format!("multiple_of must be > 0, got {m}"));
            }
        }
    }
    Ok(())
}

fn check_dup_and_allowed(
    opts: &[(String, OptVal)],
    allowed: &[&str],
    what: &str,
) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for (key, _) in opts {
        if !allowed.contains(&key.as_str()) {
            let hint = if allowed.is_empty() {
                "this type takes no options".to_string()
            } else {
                format!("allowed: {}", allowed.join(", "))
            };
            return Err(format!("option `{key}` is not valid for `{what}` ({hint})"));
        }
        if !seen.insert(key.clone()) {
            return Err(format!("duplicate option `{key}`"));
        }
    }
    Ok(())
}

fn values_of(value: &OptVal) -> Result<&DslValue, String> {
    match value {
        OptVal::V(v) => Ok(v),
        OptVal::T(_) => Err("`items` is the only option taking a type expression".to_string()),
    }
}

fn opt_int(opts: &[(String, OptVal)], name: &str) -> Option<i128> {
    opts.iter().find_map(|(k, v)| match v {
        OptVal::V(DslValue::Int(i)) if k == name => Some(*i),
        OptVal::V(DslValue::Float(f)) if k == name => Some(*f as i128),
        _ => None,
    })
}

fn opt_float(opts: &[(String, OptVal)], name: &str) -> Option<f64> {
    opts.iter().find_map(|(k, v)| match v {
        OptVal::V(DslValue::Int(i)) if k == name => Some(*i as f64),
        OptVal::V(DslValue::Float(f)) if k == name => Some(*f),
        _ => None,
    })
}

fn check_value(key: &str, value: &DslValue) -> Result<(), String> {
    let expect = |ok: bool, want: &str| -> Result<(), String> {
        if ok {
            Ok(())
        } else {
            Err(format!("option `{key}` expects {want}"))
        }
    };
    match key {
        "min_length" | "max_length" | "min_items" | "max_items" => expect(
            matches!(value, DslValue::Int(i) if *i >= 0),
            "a non-negative integer",
        ),
        "unique" => expect(matches!(value, DslValue::Bool(_)), "`true` or `false`"),
        "pattern" => expect(matches!(value, DslValue::Str(_)), "a regex string"),
        "format" => match value {
            DslValue::Str(f) => {
                if KNOWN_FORMATS.contains(&f.as_str()) {
                    Ok(())
                } else {
                    Err(format!(
                        "unknown format \"{f}\" (known: {})",
                        KNOWN_FORMATS.join(", ")
                    ))
                }
            }
            _ => expect(false, "a string"),
        },
        "enum" => expect(
            matches!(value, DslValue::List(_)),
            "a bracketed list of literals",
        ),
        _ => Ok(()), // numeric options accept Int or Float
    }
}

// ---------------------------------------------------------------------------
// Code generation: DSL AST → `::validex::ast::SchemaAst` expression
// ---------------------------------------------------------------------------

fn emit(ty: &DslType) -> String {
    format!(
        "match ::validex::Validator::from_ast(&{}) {{\n\
         ::std::result::Result::Ok(__validator) => __validator,\n\
         ::std::result::Result::Err(__e) => ::std::panic!(\"invalid `schema!` schema: {{}}\", __e),\n\
         }}",
        schema_ast_expr(ty)
    )
}

fn schema_ast_expr(ty: &DslType) -> String {
    format!(
        "::validex::ast::SchemaAst::Object(::std::boxed::Box::new({}))",
        object_ast_expr(ty)
    )
}

fn object_ast_expr(ty: &DslType) -> String {
    let mut fields: Vec<String> = Vec::new();
    match ty {
        DslType::Scalar { kind, opts } => {
            fields.push(format!(
                "types: vec![{}.to_string()]",
                kind.keyword().quote()
            ));
            for (key, value) in opts {
                let v = match value {
                    OptVal::V(v) => v,
                    OptVal::T(_) => unreachable!("checked: only items uses T"),
                };
                match (key.as_str(), v) {
                    ("min_length", DslValue::Int(i)) => {
                        fields.push(format!("min_length: Some({i})"));
                    }
                    ("max_length", DslValue::Int(i)) => {
                        fields.push(format!("max_length: Some({i})"));
                    }
                    ("pattern", DslValue::Str(s)) => {
                        fields.push(format!("pattern: Some({s:?}.to_string())"));
                    }
                    ("format", DslValue::Str(s)) => {
                        fields.push(format!("format: Some({s:?}.to_string())"));
                    }
                    ("enum", DslValue::List(values)) => {
                        let items: Vec<String> = values.iter().map(json_expr).collect();
                        fields.push(format!("enum_values: Some(vec![{}])", items.join(", ")));
                    }
                    ("min", _) => num_field(&mut fields, "minimum", value),
                    ("max", _) => num_field(&mut fields, "maximum", value),
                    ("exclusive_min", _) => num_field(&mut fields, "exclusive_minimum", value),
                    ("exclusive_max", _) => num_field(&mut fields, "exclusive_maximum", value),
                    ("multiple_of", _) => num_field(&mut fields, "multiple_of", value),
                    ("enum", _) => {}
                    _ => unreachable!("validated by check_type"),
                }
            }
        }
        DslType::Array { items, opts } => {
            fields.push("types: vec![\"array\".to_string()]".to_string());
            if let Some(items) = items {
                fields.push(format!("items: Some(Box::new({}))", schema_ast_expr(items)));
            }
            for (key, value) in opts {
                let v = match value {
                    OptVal::V(v) => v,
                    OptVal::T(_) => unreachable!("checked"),
                };
                match (key.as_str(), v) {
                    ("min_items", DslValue::Int(i)) => {
                        fields.push(format!("min_items: Some({i})"));
                    }
                    ("max_items", DslValue::Int(i)) => {
                        fields.push(format!("max_items: Some({i})"));
                    }
                    ("unique", DslValue::Bool(b)) => {
                        fields.push(format!("unique_items: Some({b})"));
                    }
                    _ => unreachable!("validated by check_type"),
                }
            }
        }
        DslType::Object(entries) => {
            fields.push("types: vec![\"object\".to_string()]".to_string());
            let properties: Vec<String> = entries
                .iter()
                .map(|e| format!("({:?}.to_string(), {})", e.name, schema_ast_expr(&e.ty)))
                .collect();
            fields.push(format!("properties: vec![{}]", properties.join(", ")));
            let required: Vec<String> = entries
                .iter()
                .filter(|e| e.required)
                .map(|e| format!("{:?}.to_string()", e.name))
                .collect();
            fields.push(format!("required: vec![{}]", required.join(", ")));
        }
    }
    format!(
        "::validex::ast::ObjectAst {{ {}, ..::validex::ast::ObjectAst::default() }}",
        fields.join(", ")
    )
}

fn num_field(fields: &mut Vec<String>, rust_name: &str, value: &OptVal) {
    let assignment = match value {
        OptVal::V(DslValue::Int(i)) => format!("Some({i} as f64)"),
        OptVal::V(DslValue::Float(f)) => format!("Some({f:?})"),
        _ => unreachable!("validated by check_type"),
    };
    fields.push(format!("{rust_name}: {assignment}"));
}

fn json_expr(value: &DslValue) -> String {
    match value {
        DslValue::Str(s) => format!("::validex::json!({s:?})"),
        DslValue::Int(i) => format!("::validex::json!({i})"),
        DslValue::Float(f) => format!("::validex::json!({f:?})"),
        DslValue::Bool(b) => format!("::validex::json!({b})"),
        DslValue::List(_) => unreachable!("nested lists are not supported in enum"),
    }
}

trait Quote {
    fn quote(&self) -> String;
}
impl Quote for &str {
    fn quote(&self) -> String {
        format!("{self:?}")
    }
}
