//! Request router with location matching

use crate::config::{CompiledLocation, LocationConfig, ServerConfig};
use std::collections::HashMap;

/// Router for matching requests to locations
pub struct Router {
    /// Compiled locations in priority order (longest prefix first, then regex)
    locations: Vec<CompiledLocation>,
    /// Default server config
    server_config: ServerConfig,
}

/// Result of routing a request
#[derive(Debug)]
pub struct RouteMatch {
    /// The matched location config
    pub location: LocationConfig,
    /// Capture groups from regex match
    pub captures: Vec<String>,
    /// Rewritten path (if rewrite was applied)
    pub rewritten_path: Option<String>,
    /// Effective document root
    pub root: Option<String>,
    /// Effective index files
    pub index: Vec<String>,
}

impl Router {
    /// Create a new router from server configuration
    pub fn new(config: ServerConfig) -> Result<Self, RouterError> {
        let mut locations = Vec::new();

        // Compile all locations
        for loc_config in &config.locations {
            let compiled = CompiledLocation::compile(loc_config.clone())
                .map_err(|e| RouterError::RegexError(e.to_string()))?;
            locations.push(compiled);
        }

        // Sort locations: non-regex first (by path length desc), then regex
        locations.sort_by(|a, b| match (a.is_regex, b.is_regex) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            (false, false) => b.config.path.len().cmp(&a.config.path.len()),
            (true, true) => std::cmp::Ordering::Equal,
        });

        Ok(Self {
            locations,
            server_config: config,
        })
    }

    /// Match a request path to a location
    pub fn route(&self, path: &str) -> Option<RouteMatch> {
        for location in &self.locations {
            if let Some(captures) = location.matches(path) {
                let rewritten_path = location.apply_rewrite(path, &captures);

                // Determine effective root and index
                let root = location
                    .config
                    .root
                    .clone()
                    .or_else(|| Some(self.server_config.root.clone()));
                let index = location
                    .config
                    .index
                    .clone()
                    .unwrap_or_else(|| self.server_config.index.clone());

                return Some(RouteMatch {
                    location: location.config.clone(),
                    captures,
                    rewritten_path,
                    root,
                    index,
                });
            }
        }

        // No location matched - use server defaults
        Some(RouteMatch {
            location: LocationConfig {
                path: "/".to_string(),
                root: Some(self.server_config.root.clone()),
                index: Some(self.server_config.index.clone()),
                proxy_pass: None,
                return_code: None,
                return_url: None,
                rewrite: None,
                autoindex: false,
                try_files: vec![],
                cache_ttl_secs: None,
                rate_limit: None,
                add_headers: HashMap::new(),
            },
            captures: vec![],
            rewritten_path: None,
            root: Some(self.server_config.root.clone()),
            index: self.server_config.index.clone(),
        })
    }

    /// Get error page path for a status code
    pub fn error_page(&self, status: u16) -> Option<&String> {
        self.server_config.error_pages.get(&status)
    }

    /// Get default headers
    pub fn default_headers(&self) -> &HashMap<String, String> {
        &self.server_config.add_headers
    }

    /// Get the server config
    pub fn server_config(&self) -> &ServerConfig {
        &self.server_config
    }
}

/// Router error
#[derive(Debug)]
pub enum RouterError {
    RegexError(String),
}

impl std::fmt::Display for RouterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouterError::RegexError(e) => write!(f, "Regex error: {}", e),
        }
    }
}

impl std::error::Error for RouterError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;

    fn test_server_config() -> ServerConfig {
        ServerConfig {
            listen: vec!["0.0.0.0:8080".to_string()],
            server_name: vec![],
            root: "./www".to_string(),
            index: vec!["index.html".to_string()],
            error_pages: HashMap::new(),
            locations: vec![
                LocationConfig {
                    path: "/api".to_string(),
                    root: None,
                    index: None,
                    proxy_pass: Some("backend".to_string()),
                    return_code: None,
                    return_url: None,
                    rewrite: None,
                    autoindex: false,
                    try_files: vec![],
                    cache_ttl_secs: None,
                    rate_limit: None,
                    add_headers: HashMap::new(),
                },
                LocationConfig {
                    path: "/static".to_string(),
                    root: Some("./assets".to_string()),
                    index: None,
                    proxy_pass: None,
                    return_code: None,
                    return_url: None,
                    rewrite: None,
                    autoindex: true,
                    try_files: vec![],
                    cache_ttl_secs: None,
                    rate_limit: None,
                    add_headers: HashMap::new(),
                },
            ],
            add_headers: HashMap::new(),
        }
    }

    #[test]
    fn test_route_prefix_match() {
        let router = Router::new(test_server_config()).unwrap();

        let route = router.route("/api/users").unwrap();
        assert_eq!(route.location.proxy_pass, Some("backend".to_string()));

        let route = router.route("/static/image.png").unwrap();
        assert_eq!(route.root, Some("./assets".to_string()));
        assert!(route.location.autoindex);
    }

    #[test]
    fn test_route_default() {
        let router = Router::new(test_server_config()).unwrap();

        let route = router.route("/index.html").unwrap();
        assert_eq!(route.root, Some("./www".to_string()));
    }
}
