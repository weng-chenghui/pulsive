//! Static HTTP server with load balancing
//!
//! This example demonstrates using pulsive for reactive server management:
//! - Request events trigger handlers
//! - Entity tracking for upstreams and connections
//! - Tick-based health checks
//! - In-memory caching and rate limiting
//! - Pulsive-based routing with expression evaluation

#![allow(dead_code)]

pub mod cache;
pub mod config;
pub mod proxy;
pub mod pulsive_router;
pub mod rate_limit;
pub mod router;
pub mod static_files;

pub use cache::{CacheStats, CachedResponse, ResponseCache};
pub use config::{CacheConfig, Config, LoadBalanceMethod};
pub use proxy::{LoadBalancer, LoadBalancerManager, ProxyError, ProxyResponse, BackendStats};
pub use rate_limit::{RateLimitResult, RateLimiter, RateLimiterStats};
pub use router::{RouteMatch, Router};
