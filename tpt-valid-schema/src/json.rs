//! Minimal recursive-descent JSON reader over the custom [`tokenizer`]
//! output — zero external dependencies. Produces [`serde_json::Value`] for
//! schema documents with position-annotated syntax errors.

use serde_json::{Map, Number, Value};

use crate::tokenizer::{tokenize, Token, TokenError, TokenKind};

/// Parse a schema document (JSON text) into a [`Value`].
///
/// Duplicate object keys keep the last occurrence (matching common JSON
/// implementations). Trailing content after the top-level value is rejected.
pub fn from_str(input: &str) -> Result<Value, TokenError> {
    let tokens = tokenize(input)?;
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
    };
    let value = parser.parse_value()?;
    if parser.pos < tokens.len() {
        let t = &tokens[parser.pos];
        return Err(TokenError {
            message: "trailing content after JSON value".into(),
            line: t.line,
            column: t.column,
        });
    }
    Ok(value)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Result<&'a Token, TokenError> {
        let t = self.tokens.get(self.pos).ok_or_else(|| TokenError {
            message: "unexpected end of input".into(),
            line: 0,
            column: 0,
        })?;
        self.pos += 1;
        Ok(t)
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<(), TokenError> {
        let t = self.next()?;
        if t.kind == kind {
            Ok(())
        } else {
            Err(TokenError {
                message: format!("expected {what}"),
                line: t.line,
                column: t.column,
            })
        }
    }

    fn parse_value(&mut self) -> Result<Value, TokenError> {
        let t = self.next()?;
        match &t.kind {
            TokenKind::LeftBrace => self.parse_object(),
            TokenKind::LeftBracket => self.parse_array(),
            TokenKind::String(s) => Ok(Value::String(s.clone())),
            TokenKind::Number(raw) => {
                // Prefer integer-backed numbers so schema keywords like
                // `minLength` read as exact integers.
                let number = if raw.contains(['.', 'e', 'E']) {
                    None
                } else {
                    raw.parse::<i64>()
                        .map(Number::from)
                        .ok()
                        .or_else(|| raw.parse::<u64>().map(Number::from).ok())
                };
                let number = number
                    .or_else(|| raw.parse::<f64>().ok().and_then(Number::from_f64))
                    .ok_or_else(|| TokenError {
                        message: format!("number out of range: {raw}"),
                        line: t.line,
                        column: t.column,
                    })?;
                Ok(Value::Number(number))
            }
            TokenKind::True => Ok(Value::Bool(true)),
            TokenKind::False => Ok(Value::Bool(false)),
            TokenKind::Null => Ok(Value::Null),
            other => Err(TokenError {
                message: format!("expected a value, found {other:?}"),
                line: t.line,
                column: t.column,
            }),
        }
    }

    fn parse_object(&mut self) -> Result<Value, TokenError> {
        let mut map = Map::new();
        if self
            .peek()
            .map(|t| t.kind == TokenKind::RightBrace)
            .unwrap_or(false)
        {
            self.pos += 1;
            return Ok(Value::Object(map));
        }
        loop {
            let key_tok = self.next()?;
            let key = match &key_tok.kind {
                TokenKind::String(s) => s.clone(),
                _ => {
                    return Err(TokenError {
                        message: "expected object key".into(),
                        line: key_tok.line,
                        column: key_tok.column,
                    })
                }
            };
            self.expect(TokenKind::Colon, "\":\"")?;
            let value = self.parse_value()?;
            map.insert(key, value);
            let t = self.next()?;
            match &t.kind {
                TokenKind::Comma => {
                    if self
                        .peek()
                        .map(|t| t.kind == TokenKind::RightBrace)
                        .unwrap_or(false)
                    {
                        return Err(TokenError {
                            message: "trailing comma in object".into(),
                            line: t.line,
                            column: t.column,
                        });
                    }
                }
                TokenKind::RightBrace => return Ok(Value::Object(map)),
                _ => {
                    return Err(TokenError {
                        message: "expected ',' or '}' in object".into(),
                        line: t.line,
                        column: t.column,
                    })
                }
            }
        }
    }

    fn parse_array(&mut self) -> Result<Value, TokenError> {
        let mut items = Vec::new();
        if self
            .peek()
            .map(|t| t.kind == TokenKind::RightBracket)
            .unwrap_or(false)
        {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            let t = self.next()?;
            match &t.kind {
                TokenKind::Comma => {
                    if self
                        .peek()
                        .map(|t| t.kind == TokenKind::RightBracket)
                        .unwrap_or(false)
                    {
                        return Err(TokenError {
                            message: "trailing comma in array".into(),
                            line: t.line,
                            column: t.column,
                        });
                    }
                }
                TokenKind::RightBracket => return Ok(Value::Array(items)),
                _ => {
                    return Err(TokenError {
                        message: "expected ',' or ']' in array".into(),
                        line: t.line,
                        column: t.column,
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_schema_like_documents() {
        let v = from_str(
            r#"{
              "$schema": "https://json-schema.org/draft/2020-12/schema",
              "type": "object",
              "properties": {
                "age": {"type": "integer", "minimum": 0},
                "tags": {"type": "array", "items": {"type": "string"}}
              },
              "required": ["age"]
            }"#,
        )
        .unwrap();
        assert_eq!(v["type"], "object");
        assert_eq!(v["properties"]["age"]["minimum"], 0);
        assert_eq!(v["required"][0], "age");
    }

    #[test]
    fn parses_scalars_and_nested() {
        assert_eq!(from_str("true").unwrap(), json!(true));
        assert_eq!(from_str("null").unwrap(), json!(null));
        assert_eq!(from_str("-2.5e2").unwrap(), json!(-250.0));
        assert_eq!(
            from_str(r#"{"a":[1,{"b":null}]}"#).unwrap(),
            json!({"a":[1,{"b":null}]})
        );
    }

    #[test]
    fn duplicate_keys_keep_last() {
        assert_eq!(from_str(r#"{"a":1,"a":2}"#).unwrap(), json!({"a":2}));
    }

    #[test]
    fn syntax_errors_with_positions() {
        assert!(from_str("{").is_err());
        let e = from_str("[1, 2,]").unwrap_err();
        assert!(e.message.contains("trailing comma"));
        let e = from_str("{\"a\" 1}").unwrap_err();
        assert!(e.message.contains("expected"));
        assert!(from_str("{} extra").is_err());
        assert!(
            from_str("[01]").unwrap_err().message.contains("expected"),
            "leading zero tokenizes as two numbers; the reader rejects the result"
        );
    }
}
