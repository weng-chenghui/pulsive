//! RON configuration parsing for the HTTP server

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Root configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Server blocks (virtual hosts)
    pub servers: Vec<ServerConfig>,
    /// Upstream pools for load balancing
    #[serde(default)]
    pub upstreams: Vec<UpstreamConfig>,
    /// Cache configuration
    #[serde(default)]
    pub cache: Option<CacheConfig>,
    /// Access log path
    #[serde(default)]
    pub access_log: Option<String>,
}

/// Server block configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    /// Listen addresses (e.g., "0.0.0.0:8080")
    pub listen: Vec<String>,
    /// Server names (hostnames) for virtual hosting
    #[serde(default)]
    pub server_name: Vec<String>,
    /// Default document root
    #[serde(default = "default_root")]
    pub root: String,
    /// Default index files
    #[serde(default = "default_index")]
    pub index: Vec<String>,
    /// Custom error pages: status code -> path
    #[serde(default)]
    pub error_pages: HashMap<u16, String>,
    /// Location blocks
    #[serde(default)]
    pub locations: Vec<LocationConfig>,
    /// Default headers to add
    #[serde(default)]
    pub add_headers: HashMap<String, String>,
}

fn default_root() -> String {
    ".".to_string()
}

fn default_index() -> Vec<String> {
    vec!["index.html".to_string(), "index.htm".to_string()]
}

/// Location block configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocationConfig {
    /// Path pattern (prefix or regex with ~ prefix)
    pub path: String,
    /// Document root override
    #[serde(default)]
    pub root: Option<String>,
    /// Index files override
    #[serde(default)]
    pub index: Option<Vec<String>>,
    /// Proxy pass to upstream
    #[serde(default)]
    pub proxy_pass: Option<String>,
    /// Redirect status code
    #[serde(default)]
    pub return_code: Option<u16>,
    /// Redirect URL
    #[serde(default)]
    pub return_url: Option<String>,
    /// Rewrite destination (with $1, $2 capture groups)
    #[serde(default)]
    pub rewrite: Option<String>,
    /// Enable directory listing
    #[serde(default)]
    pub autoindex: bool,
    /// Try files in order
    #[serde(default)]
    pub try_files: Vec<String>,
    /// Cache TTL in seconds
    #[serde(default)]
    pub cache_ttl_secs: Option<u64>,
    /// Rate limiting
    #[serde(default)]
    pub rate_limit: Option<RateLimitConfig>,
    /// Additional headers
    #[serde(default)]
    pub add_headers: HashMap<String, String>,
}

/// Upstream pool configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpstreamConfig {
    /// Pool name
    pub name: String,
    /// Load balancing method
    #[serde(default)]
    pub method: LoadBalanceMethod,
    /// Backend servers
    pub servers: Vec<UpstreamServer>,
    /// Health check interval in milliseconds
    #[serde(default = "default_health_interval")]
    pub health_check_interval_ms: u64,
    /// Health check path
    #[serde(default = "default_health_path")]
    pub health_check_path: String,
    /// Timeout for health checks in milliseconds
    #[serde(default = "default_health_timeout")]
    pub health_check_timeout_ms: u64,
}

fn default_health_interval() -> u64 {
    5000
}

fn default_health_path() -> String {
    "/".to_string()
}

fn default_health_timeout() -> u64 {
    2000
}

/// Backend server in an upstream pool
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpstreamServer {
    /// Server address (host:port)
    pub address: String,
    /// Weight for weighted load balancing
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// Is this a backup server?
    #[serde(default)]
    pub backup: bool,
}

fn default_weight() -> u32 {
    1
}

/// Load balancing method
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq)]
pub enum LoadBalanceMethod {
    /// Round-robin (default)
    #[default]
    RoundRobin,
    /// Least connections
    LeastConn,
    /// Weighted round-robin
    Weighted,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    /// Maximum requests
    pub requests: u32,
    /// Per time window in seconds
    pub per_secs: u32,
}

/// Cache configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    /// Maximum cache entries
    #[serde(default = "default_cache_entries")]
    pub max_entries: u64,
    /// Default TTL in seconds
    #[serde(default = "default_cache_ttl")]
    pub default_ttl_secs: u64,
}

fn default_cache_entries() -> u64 {
    1000
}

fn default_cache_ttl() -> u64 {
    60
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: default_cache_entries(),
            default_ttl_secs: default_cache_ttl(),
        }
    }
}

/// Compiled location with regex if needed
#[derive(Clone)]
pub struct CompiledLocation {
    pub config: LocationConfig,
    pub regex: Option<Regex>,
    pub is_regex: bool,
}

impl CompiledLocation {
    /// Compile a location configuration
    pub fn compile(config: LocationConfig) -> Result<Self, regex::Error> {
        let (is_regex, pattern) = if config.path.starts_with("~ ") {
            (true, &config.path[2..])
        } else if config.path.starts_with("~") {
            (true, &config.path[1..])
        } else {
            (false, config.path.as_str())
        };

        let regex = if is_regex {
            Some(Regex::new(pattern)?)
        } else {
            None
        };

        Ok(Self {
            config,
            regex,
            is_regex,
        })
    }

    /// Check if this location matches the given path
    pub fn matches(&self, path: &str) -> Option<Vec<String>> {
        if self.is_regex {
            if let Some(ref regex) = self.regex {
                if let Some(captures) = regex.captures(path) {
                    let groups: Vec<String> = captures
                        .iter()
                        .skip(1) // Skip the full match
                        .map(|m| m.map(|m| m.as_str().to_string()).unwrap_or_default())
                        .collect();
                    return Some(groups);
                }
            }
            None
        } else {
            // Prefix match
            if path.starts_with(&self.config.path) {
                Some(vec![])
            } else {
                None
            }
        }
    }

    /// Apply rewrite rule if configured
    pub fn apply_rewrite(&self, path: &str, captures: &[String]) -> Option<String> {
        if let Some(ref rewrite) = self.config.rewrite {
            let mut result = rewrite.clone();
            for (i, capture) in captures.iter().enumerate() {
                let placeholder = format!("${}", i + 1);
                result = result.replace(&placeholder, capture);
            }
            // Also replace $uri with original path
            result = result.replace("$uri", path);
            Some(result)
        } else {
            None
        }
    }
}

impl Config {
    /// Load configuration from a RON file
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content =
            fs::read_to_string(path.as_ref()).map_err(|e| ConfigError::Io(e.to_string()))?;
        let config: Config =
            ron::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?;
        Ok(config)
    }

    /// Get upstream by name
    pub fn get_upstream(&self, name: &str) -> Option<&UpstreamConfig> {
        self.upstreams.iter().find(|u| u.name == name)
    }
}

/// Configuration error
#[derive(Debug)]
pub enum ConfigError {
    Io(String),
    Parse(String),
    Validation(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "IO error: {}", e),
            ConfigError::Parse(e) => write!(f, "Parse error: {}", e),
            ConfigError::Validation(e) => write!(f, "Validation error: {}", e),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_location() {
        let loc = CompiledLocation::compile(LocationConfig {
            path: "/api".to_string(),
            root: None,
            index: None,
            proxy_pass: None,
            return_code: None,
            return_url: None,
            rewrite: None,
            autoindex: false,
            try_files: vec![],
            cache_ttl_secs: None,
            rate_limit: None,
            add_headers: HashMap::new(),
        })
        .unwrap();

        assert!(loc.matches("/api/users").is_some());
        assert!(loc.matches("/api").is_some());
        assert!(loc.matches("/other").is_none());
    }

    #[test]
    fn test_regex_location() {
        let loc = CompiledLocation::compile(LocationConfig {
            path: "~ ^/user/(\\d+)".to_string(),
            root: None,
            index: None,
            proxy_pass: None,
            return_code: None,
            return_url: None,
            rewrite: Some("/api/users/$1".to_string()),
            autoindex: false,
            try_files: vec![],
            cache_ttl_secs: None,
            rate_limit: None,
            add_headers: HashMap::new(),
        })
        .unwrap();

        let captures = loc.matches("/user/123").unwrap();
        assert_eq!(captures, vec!["123"]);

        let rewritten = loc.apply_rewrite("/user/123", &captures);
        assert_eq!(rewritten, Some("/api/users/123".to_string()));
    }
}
