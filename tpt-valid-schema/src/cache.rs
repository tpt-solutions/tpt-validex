//! Compiled-schema cache (spec §4.2 step 6: "State machine is cached for
//! reuse").
//!
//! Keyed by the schema text; capacity-bounded with a clear-on-overflow
//! policy (simple, allocation-light, and predictable for library use).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::error::SchemaError;
use crate::validator::Validator;

/// A thread-safe, capacity-bounded cache of compiled validators.
pub struct SchemaCache {
    inner: Mutex<HashMap<String, Arc<Validator>>>,
    capacity: usize,
}

impl SchemaCache {
    /// Create a cache holding up to `capacity` compiled schemas. When the
    /// capacity is exceeded, the cache is cleared (simple amortized policy).
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            capacity: capacity.max(1),
        }
    }

    /// Get a compiled validator for `schema`, compiling and caching it on
    /// first use.
    pub fn get_or_compile(&self, schema: &str) -> Result<Arc<Validator>, SchemaError> {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = map.get(schema) {
            return Ok(Arc::clone(v));
        }
        let compiled = Arc::new(Validator::new(schema)?);
        if map.len() >= self.capacity {
            map.clear();
        }
        map.insert(schema.to_string(), Arc::clone(&compiled));
        Ok(compiled)
    }

    /// Number of cached schemas.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop all cached schemas.
    pub fn clear(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

impl Default for SchemaCache {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_CAPACITY)
    }
}

/// Default capacity for the process-wide cache.
pub const DEFAULT_CACHE_CAPACITY: usize = 128;

/// The process-wide schema cache used by [`Validator::cached`].
pub fn global_cache() -> &'static SchemaCache {
    static CACHE: OnceLock<SchemaCache> = OnceLock::new();
    CACHE.get_or_init(SchemaCache::default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn caches_compiled_schemas() {
        let cache = SchemaCache::new(8);
        let schema = r#"{"type": "integer"}"#;
        let a = cache.get_or_compile(schema).unwrap();
        let b = cache.get_or_compile(schema).unwrap();
        assert!(
            Arc::ptr_eq(&a, &b),
            "same schema text returns the same validator"
        );
        assert_eq!(cache.len(), 1);
        assert!(cache
            .get_or_compile(schema)
            .unwrap()
            .validate(&json!(1))
            .is_valid());
        assert!(!cache
            .get_or_compile(schema)
            .unwrap()
            .validate(&json!("1"))
            .is_valid());
    }

    #[test]
    fn clear_on_overflow() {
        let cache = SchemaCache::new(2);
        cache.get_or_compile(r#"{"type":"integer"}"#).unwrap();
        cache.get_or_compile(r#"{"type":"string"}"#).unwrap();
        cache.get_or_compile(r#"{"type":"null"}"#).unwrap();
        assert!(cache.len() <= 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn invalid_schema_not_cached() {
        let cache = SchemaCache::new(4);
        assert!(cache.get_or_compile("{invalid").is_err());
        assert!(cache.is_empty());
    }
}
