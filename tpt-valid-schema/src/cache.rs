//! Compiled-schema cache (spec §4.2 step 6: "State machine is cached for
//! reuse").
//!
//! Keyed by a hash of the schema text (the compiled machine is derived
//! entirely from the text, so equal hashes are treated as equal schemas;
//! SipHash collisions are astronomically unlikely and the cache can be
//! bypassed with [`Validator::new`] when that matters). Capacity-bounded
//! with least-recently-used eviction: hot schemas survive, one-off
//! compilations evict each other.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use crate::error::SchemaError;
use crate::validator::Validator;

#[derive(Debug)]
struct Entry {
    validator: Arc<Validator>,
    last_used: u64,
}

#[derive(Debug, Default)]
struct LruMap {
    map: HashMap<u64, Entry>,
    tick: u64,
}

impl LruMap {
    /// Evict the least-recently-used entry. O(capacity); capacities are
    /// small (default 128), so a linear scan beats a second index structure.
    fn evict_lru(&mut self) {
        let lru_key = self
            .map
            .iter()
            .min_by_key(|(_, e)| e.last_used)
            .map(|(k, _)| *k);
        if let Some(k) = lru_key {
            self.map.remove(&k);
        }
    }
}

/// A thread-safe, capacity-bounded cache of compiled validators with
/// least-recently-used eviction.
pub struct SchemaCache {
    inner: Mutex<LruMap>,
    capacity: usize,
}

/// Hash a schema text into a cache key (stable within a process).
fn hash_schema(schema: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    schema.hash(&mut hasher);
    hasher.finish()
}

impl SchemaCache {
    /// Create a cache holding up to `capacity` compiled schemas. When the
    /// capacity is exceeded, the least-recently-used entry is evicted.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(LruMap::default()),
            capacity: capacity.max(1),
        }
    }

    /// Get a compiled validator for `schema`, compiling and caching it on
    /// first use.
    pub fn get_or_compile(&self, schema: &str) -> Result<Arc<Validator>, SchemaError> {
        let key = hash_schema(schema);
        let mut lru = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        lru.tick += 1;
        let now = lru.tick;
        if let Some(entry) = lru.map.get_mut(&key) {
            entry.last_used = now;
            return Ok(Arc::clone(&entry.validator));
        }
        let compiled = Arc::new(Validator::new(schema)?);
        if lru.map.len() >= self.capacity {
            lru.evict_lru();
        }
        lru.map.insert(
            key,
            Entry {
                validator: Arc::clone(&compiled),
                last_used: now,
            },
        );
        Ok(compiled)
    }

    /// Number of cached schemas.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map
            .len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop all cached schemas.
    pub fn clear(&self) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map
            .clear();
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
    fn evicts_least_recently_used() {
        let cache = SchemaCache::new(2);
        let s1 = r#"{"type": "integer"}"#;
        let s2 = r#"{"type": "string"}"#;
        let s3 = r#"{"type": "null"}"#;

        let v1 = cache.get_or_compile(s1).unwrap();
        let v2 = cache.get_or_compile(s2).unwrap();

        // Touch s1 so s2 becomes the least recently used.
        assert!(Arc::ptr_eq(&cache.get_or_compile(s1).unwrap(), &v1));

        let v3 = cache.get_or_compile(s3).unwrap();
        assert_eq!(cache.len(), 2, "capacity respected");

        // s1 survived (touched) and s3 is cached; s2 was evicted, so
        // recompiling it yields a new instance. (Note: this last lookup
        // itself evicts s1, so it must come last.)
        assert!(Arc::ptr_eq(&cache.get_or_compile(s1).unwrap(), &v1));
        assert!(Arc::ptr_eq(&cache.get_or_compile(s3).unwrap(), &v3));
        assert!(!Arc::ptr_eq(&cache.get_or_compile(s2).unwrap(), &v2));
    }

    #[test]
    fn invalid_schema_not_cached() {
        let cache = SchemaCache::new(4);
        assert!(cache.get_or_compile("{invalid").is_err());
        assert!(cache.is_empty());
    }
}
