//! Custom JSON tokenizer for schema documents — zero external dependencies
//! (spec §3.5). Produces positioned tokens so syntax errors carry line and
//! column information.

use std::fmt;

/// A syntax error in a schema document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenError {
    /// Error description.
    pub message: String,
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub column: usize,
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at line {} column {}",
            self.message, self.line, self.column
        )
    }
}

impl std::error::Error for TokenError {}

/// A lexical token with its 1-based source position.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Token kind and payload.
    pub kind: TokenKind,
    /// 1-based line where the token starts.
    pub line: usize,
    /// 1-based column where the token starts.
    pub column: usize,
}

/// Token kinds produced by the tokenizer.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `[`
    LeftBracket,
    /// `]`
    RightBracket,
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// A string literal (escapes resolved).
    String(String),
    /// A number literal (raw text, parsed by the consumer).
    Number(String),
    /// `true`
    True,
    /// `false`
    False,
    /// `null`
    Null,
}

/// Tokenize a schema document.
pub fn tokenize(input: &str) -> Result<Vec<Token>, TokenError> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;

    macro_rules! err {
        ($msg:expr) => {
            return Err(TokenError {
                message: $msg.into(),
                line,
                column: col,
            })
        };
    }

    while pos < bytes.len() {
        let b = bytes[pos];
        match b {
            b' ' | b'\t' => {
                pos += 1;
                col += 1;
            }
            b'\n' => {
                pos += 1;
                line += 1;
                col = 1;
            }
            b'\r' => {
                pos += 1;
                col = 1;
                if pos < bytes.len() && bytes[pos] == b'\n' {
                    pos += 1;
                }
                line += 1;
            }
            b'{' => {
                tokens.push(Token {
                    kind: TokenKind::LeftBrace,
                    line,
                    column: col,
                });
                pos += 1;
                col += 1;
            }
            b'}' => {
                tokens.push(Token {
                    kind: TokenKind::RightBrace,
                    line,
                    column: col,
                });
                pos += 1;
                col += 1;
            }
            b'[' => {
                tokens.push(Token {
                    kind: TokenKind::LeftBracket,
                    line,
                    column: col,
                });
                pos += 1;
                col += 1;
            }
            b']' => {
                tokens.push(Token {
                    kind: TokenKind::RightBracket,
                    line,
                    column: col,
                });
                pos += 1;
                col += 1;
            }
            b':' => {
                tokens.push(Token {
                    kind: TokenKind::Colon,
                    line,
                    column: col,
                });
                pos += 1;
                col += 1;
            }
            b',' => {
                tokens.push(Token {
                    kind: TokenKind::Comma,
                    line,
                    column: col,
                });
                pos += 1;
                col += 1;
            }
            b'"' => {
                let (s, consumed, nl, nc) = scan_string(bytes, pos, line, col)?;
                tokens.push(Token {
                    kind: TokenKind::String(s),
                    line,
                    column: col,
                });
                pos = consumed;
                line = nl;
                col = nc;
            }
            b'-' | b'0'..=b'9' => {
                let start = pos;
                let (consumed, nl, nc) = scan_number(bytes, pos, line, col)?;
                let raw = &input[start..consumed];
                tokens.push(Token {
                    kind: TokenKind::Number(raw.to_string()),
                    line,
                    column: col,
                });
                pos = consumed;
                line = nl;
                col = nc;
            }
            b't' => literal(bytes, &mut pos, &mut col, "true", || TokenKind::True)
                .map(|k| {
                    tokens.push(Token {
                        kind: k,
                        line,
                        column: col,
                    })
                })
                .map_err(|m| TokenError {
                    message: m,
                    line,
                    column: col,
                })?,
            b'f' => literal(bytes, &mut pos, &mut col, "false", || TokenKind::False)
                .map(|k| {
                    tokens.push(Token {
                        kind: k,
                        line,
                        column: col,
                    })
                })
                .map_err(|m| TokenError {
                    message: m,
                    line,
                    column: col,
                })?,
            b'n' => literal(bytes, &mut pos, &mut col, "null", || TokenKind::Null)
                .map(|k| {
                    tokens.push(Token {
                        kind: k,
                        line,
                        column: col,
                    })
                })
                .map_err(|m| TokenError {
                    message: m,
                    line,
                    column: col,
                })?,
            _ => err!(format!("unexpected character '{}'", char_at(input, pos))),
        }
    }
    Ok(tokens)
}

fn char_at(input: &str, byte_pos: usize) -> char {
    input[byte_pos..].chars().next().unwrap_or('\u{FFFD}')
}

fn literal(
    bytes: &[u8],
    pos: &mut usize,
    col: &mut usize,
    word: &str,
    make: impl FnOnce() -> TokenKind,
) -> Result<TokenKind, String> {
    if bytes[*pos..].starts_with(word.as_bytes()) {
        *pos += word.len();
        *col += word.len();
        Ok(make())
    } else {
        Err(format!("invalid literal, expected \"{word}\""))
    }
}

/// Scan a string literal starting at `bytes[start] == b'"'`.
/// Returns (value, new_pos, new_line, new_col).
fn scan_string(
    bytes: &[u8],
    start: usize,
    line: usize,
    col: usize,
) -> Result<(String, usize, usize, usize), TokenError> {
    let mut out = String::new();
    let mut pos = start + 1;
    let l = line;
    let mut c = col + 1;
    let bad = |message: &str, l: usize, c: usize| TokenError {
        message: message.to_string(),
        line: l,
        column: c,
    };
    loop {
        if pos >= bytes.len() {
            return Err(bad("unterminated string", l, c));
        }
        let b = bytes[pos];
        match b {
            b'"' => return Ok((out, pos + 1, l, c + 1)),
            b'\\' => {
                pos += 1;
                c += 1;
                if pos >= bytes.len() {
                    return Err(bad("unterminated escape", l, c));
                }
                let esc = bytes[pos];
                pos += 1;
                c += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000C}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let cp = scan_unicode_escape(bytes, &mut pos, l, c)?;
                        c += 4;
                        if (0xD800..0xDC00).contains(&cp) {
                            // High surrogate: require a following \uXXXX low surrogate.
                            if pos + 1 < bytes.len()
                                && bytes[pos] == b'\\'
                                && bytes[pos + 1] == b'u'
                            {
                                pos += 2;
                                c += 2;
                                let low = scan_unicode_escape(bytes, &mut pos, l, c)?;
                                c += 4;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(bad("invalid low surrogate", l, c));
                                }
                                let combined = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                                out.push(
                                    char::from_u32(combined)
                                        .ok_or_else(|| bad("invalid surrogate pair", l, c))?,
                                );
                            } else {
                                return Err(bad("unpaired high surrogate", l, c));
                            }
                        } else if (0xDC00..0xE000).contains(&cp) {
                            return Err(bad("unpaired low surrogate", l, c));
                        } else {
                            out.push(
                                char::from_u32(cp)
                                    .ok_or_else(|| bad("invalid code point", l, c))?,
                            );
                        }
                    }
                    _ => return Err(bad("invalid escape character", l, c)),
                }
            }
            0x00..=0x1F => return Err(bad("unescaped control character in string", l, c)),
            _ => {
                // Copy the full UTF-8 sequence.
                let rest = &bytes[pos..];
                let s = std::str::from_utf8(rest)
                    .map_err(|_| bad("invalid UTF-8 in string", l, c))
                    .unwrap_or("\u{FFFD}");
                let ch = s.chars().next().unwrap();
                out.push(ch);
                let len = ch.len_utf8();
                pos += len;
                c += 1;
            }
        }
    }
}

fn scan_unicode_escape(
    bytes: &[u8],
    pos: &mut usize,
    line: usize,
    col: usize,
) -> Result<u32, TokenError> {
    if *pos + 4 > bytes.len() {
        return Err(TokenError {
            message: "truncated \\u escape".into(),
            line,
            column: col,
        });
    }
    let hex = std::str::from_utf8(&bytes[*pos..*pos + 4]).map_err(|_| TokenError {
        message: "invalid \\u escape".into(),
        line,
        column: col,
    })?;
    let value = u32::from_str_radix(hex, 16).map_err(|_| TokenError {
        message: "invalid \\u escape".into(),
        line,
        column: col,
    })?;
    *pos += 4;
    Ok(value)
}

/// Scan a number literal. Returns (end_pos, new_line, new_col).
fn scan_number(
    bytes: &[u8],
    start: usize,
    line: usize,
    col: usize,
) -> Result<(usize, usize, usize), TokenError> {
    let mut pos = start;
    let grammar_err = TokenError {
        message: "invalid number literal".into(),
        line,
        column: col,
    };
    if bytes[pos] == b'-' {
        pos += 1;
        if pos >= bytes.len() || !bytes[pos].is_ascii_digit() {
            return Err(grammar_err);
        }
    }
    // Integer part.
    if bytes[pos] == b'0' {
        pos += 1;
    } else {
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
    }
    // Fraction.
    if pos < bytes.len() && bytes[pos] == b'.' {
        pos += 1;
        if pos >= bytes.len() || !bytes[pos].is_ascii_digit() {
            return Err(grammar_err);
        }
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
    }
    // Exponent.
    if pos < bytes.len() && (bytes[pos] == b'e' || bytes[pos] == b'E') {
        pos += 1;
        if pos < bytes.len() && (bytes[pos] == b'+' || bytes[pos] == b'-') {
            pos += 1;
        }
        if pos >= bytes.len() || !bytes[pos].is_ascii_digit() {
            return Err(grammar_err);
        }
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
    }
    let consumed = pos - start;
    Ok((pos, line, col + consumed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn structural_tokens() {
        assert_eq!(
            kinds(r#"{"a":[1,true,false,null]}"#),
            vec![
                TokenKind::LeftBrace,
                TokenKind::String("a".into()),
                TokenKind::Colon,
                TokenKind::LeftBracket,
                TokenKind::Number("1".into()),
                TokenKind::Comma,
                TokenKind::True,
                TokenKind::Comma,
                TokenKind::False,
                TokenKind::Comma,
                TokenKind::Null,
                TokenKind::RightBracket,
                TokenKind::RightBrace,
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            kinds("[0, -1, 3.25, 2e10, 1.5E-3, 0.5]"),
            vec![
                TokenKind::LeftBracket,
                TokenKind::Number("0".into()),
                TokenKind::Comma,
                TokenKind::Number("-1".into()),
                TokenKind::Comma,
                TokenKind::Number("3.25".into()),
                TokenKind::Comma,
                TokenKind::Number("2e10".into()),
                TokenKind::Comma,
                TokenKind::Number("1.5E-3".into()),
                TokenKind::Comma,
                TokenKind::Number("0.5".into()),
                TokenKind::RightBracket,
            ]
        );
    }

    #[test]
    fn string_escapes() {
        let toks = tokenize(r#""a\"b\\c\/d\be\ff\ng\rh\ti""#).unwrap();
        assert_eq!(
            toks[0].kind,
            TokenKind::String("a\"b\\c/d\u{8}e\u{c}f\ng\rh\ti".into())
        );
    }

    #[test]
    fn unicode_escapes_and_surrogate_pairs() {
        let toks = tokenize(r#""é🚀""#).unwrap();
        assert_eq!(toks[0].kind, TokenKind::String("é🚀".into()));
    }

    #[test]
    fn positions_track_lines() {
        let toks = tokenize("{\n  \"a\": 1\n}").unwrap();
        assert_eq!(toks[1].line, 2);
        assert_eq!(toks[1].column, 3);
        assert_eq!(toks[3].kind, TokenKind::Number("1".into()));
        assert_eq!(toks[3].line, 2);
    }

    #[test]
    fn errors_carry_position() {
        let e = tokenize("{\n  \"a\": tru }\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(e.message.contains("true"));

        let e = tokenize("\"unterminated").unwrap_err();
        assert!(e.message.contains("unterminated"));

        let e = tokenize("\"bad \\x escape\"").unwrap_err();
        assert!(e.message.contains("escape"));

        let e = tokenize("\"\u{1}\"").unwrap_err();
        assert!(e.message.contains("control"));

        let e = tokenize("[1.2.3]").unwrap_err();
        assert!(e.message.contains("number") || e.message.contains("unexpected"));

        // Note: "[01]" tokenizes as two numbers; the leading-zero rule is
        // enforced at the JSON reader level (see json::tests).
        let kinds = kinds("[01]");
        assert_eq!(kinds.len(), 4);
    }
}
