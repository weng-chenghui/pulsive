//! In-memory response cache using moka

use hyper::body::Bytes;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

/// Cached response data
#[derive(Clone)]
pub struct CachedResponse {
    /// Response body
    pub body: Bytes,
    /// Content-Type header
    pub content_type: String,
    /// Additional headers
    pub headers: Vec<(String, String)>,
}

/// Response cache with TTL support
#[derive(Clone)]
pub struct ResponseCache {
    cache: Cache<String, Arc<CachedResponse>>,
    #[allow(dead_code)]
    default_ttl: Duration,
}

impl ResponseCache {
    /// Create a new response cache
    pub fn new(max_entries: u64, default_ttl_secs: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_entries)
            .time_to_live(Duration::from_secs(default_ttl_secs))
            .build();

        Self {
            cache,
            default_ttl: Duration::from_secs(default_ttl_secs),
        }
    }

    /// Get a cached response
    pub async fn get(&self, key: &str) -> Option<Arc<CachedResponse>> {
        self.cache.get(key).await
    }

    /// Insert a response into the cache
    pub async fn insert(&self, key: String, response: CachedResponse) {
        self.cache.insert(key, Arc::new(response)).await;
    }

    /// Insert with custom TTL (note: uses default TTL, custom TTL would require cache per TTL)
    pub async fn insert_with_ttl(&self, key: String, response: CachedResponse, _ttl_secs: u64) {
        // moka's time_to_live is set at cache creation
        // For per-entry TTL, we'd need a different approach
        self.cache.insert(key, Arc::new(response)).await;
    }

    /// Remove an entry from the cache
    pub async fn invalidate(&self, key: &str) {
        self.cache.invalidate(key).await;
    }

    /// Clear all cache entries
    pub async fn clear(&self) {
        self.cache.invalidate_all();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.cache.entry_count(),
            weighted_size: self.cache.weighted_size(),
        }
    }

    /// Generate a cache key from request info
    pub fn make_key(uri: &str, query: Option<&str>) -> String {
        match query {
            Some(q) if !q.is_empty() => format!("{}?{}", uri, q),
            _ => uri.to_string(),
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entry_count: u64,
    pub weighted_size: u64,
}

/// Cache event for pulsive integration
#[derive(Debug, Clone)]
pub enum CacheEvent {
    Hit { key: String },
    Miss { key: String },
    Insert { key: String, size: usize },
    Evict { key: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_insert_get() {
        let cache = ResponseCache::new(100, 60);

        let response = CachedResponse {
            body: Bytes::from("Hello, World!"),
            content_type: "text/plain".to_string(),
            headers: vec![],
        };

        cache.insert("test-key".to_string(), response).await;

        let cached = cache.get("test-key").await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().body, Bytes::from("Hello, World!"));
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = ResponseCache::new(100, 60);
        let cached = cache.get("nonexistent").await;
        assert!(cached.is_none());
    }

    #[test]
    fn test_make_key() {
        assert_eq!(ResponseCache::make_key("/page", None), "/page");
        assert_eq!(ResponseCache::make_key("/page", Some("")), "/page");
        assert_eq!(ResponseCache::make_key("/page", Some("a=1")), "/page?a=1");
    }
}
