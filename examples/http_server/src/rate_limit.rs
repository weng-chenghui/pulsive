//! Per-IP rate limiting using token bucket algorithm

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Token bucket for a single IP
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Current number of tokens
    tokens: f64,
    /// Maximum tokens (burst capacity)
    max_tokens: f64,
    /// Tokens added per second
    refill_rate: f64,
    /// Last time tokens were updated
    last_update: Instant,
}

impl TokenBucket {
    fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate,
            last_update: Instant::now(),
        }
    }

    /// Try to consume a token, refilling based on elapsed time
    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        self.last_update = now;

        // Refill tokens based on elapsed time
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);

        // Try to consume a token
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Get remaining tokens
    fn remaining(&self) -> u32 {
        self.tokens as u32
    }
}

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests in the window
    pub requests: u32,
    /// Time window in seconds
    pub per_secs: u32,
}

impl RateLimitConfig {
    /// Calculate tokens per second (refill rate)
    fn refill_rate(&self) -> f64 {
        self.requests as f64 / self.per_secs as f64
    }
}

/// Rate limiter manager
#[derive(Clone)]
pub struct RateLimiter {
    /// Buckets per IP address
    buckets: Arc<RwLock<HashMap<IpAddr, TokenBucket>>>,
    /// Default config
    config: RateLimitConfig,
    /// Cleanup interval
    cleanup_interval: Duration,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            config,
            cleanup_interval: Duration::from_secs(60),
        }
    }

    /// Check if a request from an IP should be allowed
    pub async fn check(&self, ip: IpAddr) -> RateLimitResult {
        let mut buckets = self.buckets.write().await;

        let bucket = buckets.entry(ip).or_insert_with(|| {
            TokenBucket::new(
                self.config.requests as f64,
                self.config.refill_rate(),
            )
        });

        if bucket.try_consume() {
            RateLimitResult::Allowed {
                remaining: bucket.remaining(),
                limit: self.config.requests,
            }
        } else {
            RateLimitResult::Limited {
                retry_after: Duration::from_secs_f64(1.0 / self.config.refill_rate()),
                limit: self.config.requests,
            }
        }
    }

    /// Check with a custom config (for per-location rate limits)
    pub async fn check_with_config(&self, ip: IpAddr, config: &RateLimitConfig) -> RateLimitResult {
        let mut buckets = self.buckets.write().await;

        let bucket = buckets.entry(ip).or_insert_with(|| {
            TokenBucket::new(config.requests as f64, config.refill_rate())
        });

        if bucket.try_consume() {
            RateLimitResult::Allowed {
                remaining: bucket.remaining(),
                limit: config.requests,
            }
        } else {
            RateLimitResult::Limited {
                retry_after: Duration::from_secs_f64(1.0 / config.refill_rate()),
                limit: config.requests,
            }
        }
    }

    /// Clean up old buckets that haven't been used recently
    pub async fn cleanup(&self) {
        let mut buckets = self.buckets.write().await;
        let now = Instant::now();
        let max_age = Duration::from_secs(300); // 5 minutes

        buckets.retain(|_, bucket| {
            now.duration_since(bucket.last_update) < max_age
        });
    }

    /// Start background cleanup task
    pub fn start_cleanup_task(self) -> tokio::task::JoinHandle<()> {
        let limiter = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(limiter.cleanup_interval);
            loop {
                interval.tick().await;
                limiter.cleanup().await;
            }
        })
    }

    /// Get stats about the rate limiter
    pub async fn stats(&self) -> RateLimiterStats {
        let buckets = self.buckets.read().await;
        RateLimiterStats {
            tracked_ips: buckets.len(),
        }
    }
}

/// Result of a rate limit check
#[derive(Debug, Clone)]
pub enum RateLimitResult {
    /// Request is allowed
    Allowed {
        remaining: u32,
        limit: u32,
    },
    /// Request is rate limited
    Limited {
        retry_after: Duration,
        limit: u32,
    },
}

impl RateLimitResult {
    /// Check if the request is allowed
    pub fn is_allowed(&self) -> bool {
        matches!(self, RateLimitResult::Allowed { .. })
    }
}

/// Rate limiter statistics
#[derive(Debug, Clone)]
pub struct RateLimiterStats {
    pub tracked_ips: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_rate_limit_allows_within_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            requests: 10,
            per_secs: 1,
        });

        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        for _ in 0..10 {
            let result = limiter.check(ip).await;
            assert!(result.is_allowed());
        }
    }

    #[tokio::test]
    async fn test_rate_limit_blocks_over_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            requests: 2,
            per_secs: 1,
        });

        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        assert!(limiter.check(ip).await.is_allowed());
        assert!(limiter.check(ip).await.is_allowed());
        assert!(!limiter.check(ip).await.is_allowed());
    }

    #[tokio::test]
    async fn test_different_ips_have_separate_limits() {
        let limiter = RateLimiter::new(RateLimitConfig {
            requests: 1,
            per_secs: 1,
        });

        let ip1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));

        assert!(limiter.check(ip1).await.is_allowed());
        assert!(limiter.check(ip2).await.is_allowed());
        assert!(!limiter.check(ip1).await.is_allowed());
        assert!(!limiter.check(ip2).await.is_allowed());
    }
}
