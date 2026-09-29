//! Schema error types (syntax, semantic, unsupported constructs).

use std::fmt;

/// An error produced while compiling a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    /// The schema is not valid JSON, or is not a JSON object/boolean.
    Syntax(String),
    /// A keyword's value violates cross-keyword invariants
    /// (e.g. `minimum` > `maximum`), or a value is the wrong type.
    Semantic {
        /// The offending keyword.
        keyword: String,
        /// What is wrong with it.
        message: String,
    },
    /// The schema uses a keyword we deliberately do not support yet
    /// (see the compliance matrix in `docs/compliance.md`).
    Unsupported {
        /// The unsupported keyword.
        keyword: String,
        /// Explanation / suggested workaround.
        message: String,
    },
}

impl SchemaError {
    /// Semantic error constructor.
    pub fn semantic(keyword: impl Into<String>, message: impl Into<String>) -> Self {
        SchemaError::Semantic {
            keyword: keyword.into(),
            message: message.into(),
        }
    }

    /// Unsupported-keyword constructor.
    pub fn unsupported(keyword: impl Into<String>, message: impl Into<String>) -> Self {
        SchemaError::Unsupported {
            keyword: keyword.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaError::Syntax(m) => write!(f, "invalid schema: {m}"),
            SchemaError::Semantic { keyword, message } => {
                write!(f, "invalid schema: {keyword}: {message}")
            }
            SchemaError::Unsupported { keyword, message } => {
                write!(f, "unsupported schema keyword {keyword}: {message}")
            }
        }
    }
}

impl std::error::Error for SchemaError {}

impl From<crate::tokenizer::TokenError> for SchemaError {
    fn from(e: crate::tokenizer::TokenError) -> Self {
        SchemaError::Syntax(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_forms() {
        assert!(SchemaError::Syntax("bad json".into())
            .to_string()
            .contains("invalid schema"));
        assert!(SchemaError::semantic("minimum", "min > max")
            .to_string()
            .contains("minimum"));
        assert!(SchemaError::unsupported("$ref", "not supported")
            .to_string()
            .contains("unsupported"));
    }
}
