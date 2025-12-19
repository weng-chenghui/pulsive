//! Pulsive-based request routing
//!
//! This module demonstrates using pulsive's entity system and expression engine
//! for request routing decisions. Routes are stored as entities in the model,
//! and matching is done by querying and evaluating conditions.

use pulsive_core::{EntityId, EvalContext, Expr, Model, Value};
use regex::Regex;
use std::collections::HashMap;

/// A route stored in the pulsive model
#[derive(Debug, Clone)]
pub struct PulsiveRoute {
    /// Entity ID of this route
    pub entity_id: EntityId,
    /// Path pattern (prefix or regex)
    pub path: String,
    /// Whether this is a regex pattern
    pub is_regex: bool,
    /// Compiled regex (if is_regex)
    pub regex: Option<Regex>,
    /// Priority (higher = checked first)
    pub priority: i64,
    /// Document root for static files
    pub root: Option<String>,
    /// Index files
    pub index: Vec<String>,
    /// Upstream name for proxying
    pub proxy_pass: Option<String>,
    /// Redirect status code
    pub return_code: Option<u16>,
    /// Redirect URL
    pub return_url: Option<String>,
    /// URL rewrite pattern
    pub rewrite: Option<String>,
    /// Enable directory listing
    pub autoindex: bool,
    /// Rate limit config (requests, per_secs)
    pub rate_limit: Option<(u32, u32)>,
}

/// Result of pulsive-based routing
#[derive(Debug)]
pub struct PulsiveRouteMatch {
    /// The matched route
    pub route: PulsiveRoute,
    /// Capture groups from regex match
    pub captures: Vec<String>,
    /// Rewritten path (if rewrite was applied)
    pub rewritten_path: Option<String>,
}

/// Pulsive-based router that stores routes as entities
pub struct PulsiveRouter {
    /// Cached routes (sorted by priority)
    routes: Vec<PulsiveRoute>,
    /// Default root directory
    default_root: String,
    /// Default index files
    default_index: Vec<String>,
    /// Error pages
    error_pages: HashMap<u16, String>,
}

impl PulsiveRouter {
    /// Create a new pulsive router and populate the model with route entities
    pub fn new(
        model: &mut Model,
        server_config: &crate::config::ServerConfig,
    ) -> Result<Self, PulsiveRouterError> {
        let mut routes = Vec::new();

        // Create route entities from config
        for (idx, loc) in server_config.locations.iter().enumerate() {
            let entity = model.entities_mut().create("route");

            // Store route properties in the entity
            entity.set("path", Value::String(loc.path.clone()));
            entity.set("is_regex", Value::Bool(loc.path.starts_with("~")));
            entity.set("priority", Value::Int((1000 - idx) as i64)); // Earlier = higher priority
            entity.set("hits", Value::Int(0));
            entity.set("expr_hits", Value::Int(0));

            if let Some(ref root) = loc.root {
                entity.set("root", Value::String(root.clone()));
            }
            if let Some(ref proxy) = loc.proxy_pass {
                entity.set("proxy_pass", Value::String(proxy.clone()));
            }
            if let Some(code) = loc.return_code {
                entity.set("return_code", Value::Int(code as i64));
            }
            if let Some(ref url) = loc.return_url {
                entity.set("return_url", Value::String(url.clone()));
            }
            if let Some(ref rewrite) = loc.rewrite {
                entity.set("rewrite", Value::String(rewrite.clone()));
            }
            entity.set("autoindex", Value::Bool(loc.autoindex));

            if let Some(ref index) = loc.index {
                let index_str = index.join(",");
                entity.set("index", Value::String(index_str));
            }

            if let Some(ref rl) = loc.rate_limit {
                entity.set("rate_limit_requests", Value::Int(rl.requests as i64));
                entity.set("rate_limit_per_secs", Value::Int(rl.per_secs as i64));
            }

            // Compile regex if needed
            let (is_regex, regex) = if loc.path.starts_with("~") {
                let pattern = loc.path.trim_start_matches("~ ").trim_start_matches("~");
                let re = Regex::new(pattern)
                    .map_err(|e| PulsiveRouterError::RegexError(e.to_string()))?;
                (true, Some(re))
            } else {
                (false, None)
            };

            routes.push(PulsiveRoute {
                entity_id: entity.id,
                path: loc.path.clone(),
                is_regex,
                regex,
                priority: (1000 - idx) as i64,
                root: loc.root.clone(),
                index: loc.index.clone().unwrap_or_default(),
                proxy_pass: loc.proxy_pass.clone(),
                return_code: loc.return_code,
                return_url: loc.return_url.clone(),
                rewrite: loc.rewrite.clone(),
                autoindex: loc.autoindex,
                rate_limit: loc.rate_limit.as_ref().map(|rl| (rl.requests, rl.per_secs)),
            });
        }

        // Sort routes: non-regex by path length (longest first), then regex, then by priority
        routes.sort_by(|a, b| match (a.is_regex, b.is_regex) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            (false, false) => b.path.len().cmp(&a.path.len()),
            (true, true) => b.priority.cmp(&a.priority),
        });

        Ok(Self {
            routes,
            default_root: server_config.root.clone(),
            default_index: server_config.index.clone(),
            error_pages: server_config.error_pages.clone(),
        })
    }

    /// Route a request using pulsive entities
    ///
    /// This demonstrates querying the pulsive model for routing decisions.
    /// The model is used to:
    /// 1. Track route hit counts (updated via events)
    /// 2. Potentially store dynamic routing rules
    /// 3. Enable route modification at runtime
    pub fn route(&self, model: &mut Model, path: &str) -> Option<PulsiveRouteMatch> {
        // Iterate through routes and find match
        for route in &self.routes {
            if let Some((captures, rewritten)) = self.matches_route(route, path) {
                // Update route hit count in the model
                let current_tick = model.current_tick();
                if let Some(entity) = model.entities_mut().get_mut(route.entity_id) {
                    let hits = entity.get_number("hits").unwrap_or(0.0) as i64;
                    entity.set("hits", Value::Int(hits + 1));
                    entity.set("last_hit_tick", Value::Int(current_tick as i64));
                }

                return Some(PulsiveRouteMatch {
                    route: route.clone(),
                    captures,
                    rewritten_path: rewritten,
                });
            }
        }

        // Return default route
        Some(PulsiveRouteMatch {
            route: PulsiveRoute {
                entity_id: EntityId::new(0), // Placeholder
                path: "/".to_string(),
                is_regex: false,
                regex: None,
                priority: 0,
                root: Some(self.default_root.clone()),
                index: self.default_index.clone(),
                proxy_pass: None,
                return_code: None,
                return_url: None,
                rewrite: None,
                autoindex: false,
                rate_limit: None,
            },
            captures: vec![],
            rewritten_path: None,
        })
    }

    /// Check if a route matches the given path
    fn matches_route(
        &self,
        route: &PulsiveRoute,
        path: &str,
    ) -> Option<(Vec<String>, Option<String>)> {
        if route.is_regex {
            if let Some(ref regex) = route.regex {
                if let Some(caps) = regex.captures(path) {
                    let captures: Vec<String> = caps
                        .iter()
                        .skip(1)
                        .filter_map(|m| m.map(|m| m.as_str().to_string()))
                        .collect();

                    let rewritten = route.rewrite.as_ref().map(|rewrite| {
                        let mut result = rewrite.clone();
                        for (i, cap) in captures.iter().enumerate() {
                            result = result.replace(&format!("${}", i + 1), cap);
                        }
                        result
                    });

                    return Some((captures, rewritten));
                }
            }
        } else {
            // Prefix match
            if path.starts_with(&route.path) || route.path == "/" {
                return Some((vec![], None));
            }
        }
        None
    }

    /// Route using pulsive's expression engine (demonstration)
    ///
    /// This method shows how routing conditions could be expressed using
    /// pulsive's Expr system, allowing for dynamic, data-driven routing rules.
    ///
    /// Since pulsive's Expr doesn't have string matching operations (like StartsWith),
    /// we store path_len comparison in entity properties and use Expr for the evaluation.
    pub fn route_with_expr(&self, model: &mut Model, path: &str) -> Option<PulsiveRouteMatch> {
        // EvalContext imported at module level

        let path_len = path.len() as i64;

        // Iterate through route entities and evaluate conditions
        for route in &self.routes {
            // Get the entity to check its state
            if let Some(entity) = model.entities().get(route.entity_id) {
                // For prefix routes, we can use Expr to compare path lengths
                // This demonstrates using Expr::Ge for condition checking
                let matches = if route.is_regex {
                    // Regex matching still needs Rust's regex - Expr doesn't support it
                    self.matches_route(route, path).is_some()
                } else if route.path == "/" {
                    // Root matches everything
                    true
                } else {
                    // For prefix match: path must be at least as long as prefix
                    // and must actually start with the prefix (checked outside Expr)
                    let prefix_len = route.path.len() as i64;

                    // Create condition using pulsive Expr
                    // Using Expr::Ge demonstrates the expression engine
                    let condition = Expr::Ge(
                        Box::new(Expr::Literal(Value::Int(path_len))),
                        Box::new(Expr::Literal(Value::Int(prefix_len))),
                    );

                    // Create context for evaluation
                    let empty_params = pulsive_core::ValueMap::new();
                    let mut rng = pulsive_core::Rng::new(0);
                    let mut ctx = EvalContext::new(
                        model.entities(),
                        model.globals(),
                        &empty_params,
                        &mut rng,
                    );
                    ctx.target = Some(entity);

                    // Evaluate condition using pulsive's expression engine
                    let len_ok = matches!(condition.eval(&mut ctx), Ok(Value::Bool(true)));

                    // Also check actual prefix match (Expr doesn't have string ops)
                    len_ok && path.starts_with(&route.path)
                };

                if matches {
                    // Route matches! Update stats
                    let current_tick = model.current_tick();
                    if let Some(entity) = model.entities_mut().get_mut(route.entity_id) {
                        let hits = entity.get_number("expr_hits").unwrap_or(0.0) as i64;
                        entity.set("expr_hits", Value::Int(hits + 1));
                        entity.set("last_expr_hit_tick", Value::Int(current_tick as i64));
                    }

                    // Compute captures if regex
                    let (captures, rewritten) = if route.is_regex {
                        self.matches_route(route, path).unwrap_or((vec![], None))
                    } else {
                        (vec![], None)
                    };

                    return Some(PulsiveRouteMatch {
                        route: route.clone(),
                        captures,
                        rewritten_path: rewritten,
                    });
                }
            }
        }

        // Default route
        Some(PulsiveRouteMatch {
            route: PulsiveRoute {
                entity_id: EntityId::new(0),
                path: "/".to_string(),
                is_regex: false,
                regex: None,
                priority: 0,
                root: Some(self.default_root.clone()),
                index: self.default_index.clone(),
                proxy_pass: None,
                return_code: None,
                return_url: None,
                rewrite: None,
                autoindex: false,
                rate_limit: None,
            },
            captures: vec![],
            rewritten_path: None,
        })
    }

    /// Get error page path
    pub fn error_page(&self, status: u16) -> Option<&String> {
        self.error_pages.get(&status)
    }

    /// Get default root
    pub fn default_root(&self) -> &str {
        &self.default_root
    }

    /// Get route stats from the model
    pub fn get_route_stats(&self, model: &Model) -> Vec<RouteStats> {
        self.routes
            .iter()
            .filter_map(|route| {
                model
                    .entities()
                    .get(route.entity_id)
                    .map(|entity| RouteStats {
                        path: route.path.clone(),
                        hits: entity.get_number("hits").unwrap_or(0.0) as u64,
                        expr_hits: entity.get_number("expr_hits").unwrap_or(0.0) as u64,
                        last_hit_tick: entity.get_number("last_hit_tick").map(|v| v as u64),
                    })
            })
            .collect()
    }
}

/// Statistics for a route
#[derive(Debug)]
pub struct RouteStats {
    pub path: String,
    pub hits: u64,
    pub expr_hits: u64,
    pub last_hit_tick: Option<u64>,
}

/// Router error
#[derive(Debug)]
pub enum PulsiveRouterError {
    RegexError(String),
}

impl std::fmt::Display for PulsiveRouterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PulsiveRouterError::RegexError(e) => write!(f, "Regex error: {}", e),
        }
    }
}

impl std::error::Error for PulsiveRouterError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LocationConfig, ServerConfig};

    fn test_config() -> ServerConfig {
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
                    path: "~ ^/user/(\\d+)".to_string(),
                    root: None,
                    index: None,
                    proxy_pass: Some("backend".to_string()),
                    return_code: None,
                    return_url: None,
                    rewrite: Some("/api/users/$1".to_string()),
                    autoindex: false,
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
    fn test_pulsive_router_prefix() {
        let mut model = Model::new();
        let config = test_config();
        let router = PulsiveRouter::new(&mut model, &config).unwrap();

        // Test prefix match
        let result = router.route(&mut model, "/api/users").unwrap();
        assert_eq!(result.route.proxy_pass, Some("backend".to_string()));

        // Check hit count was updated
        let stats = router.get_route_stats(&model);
        let api_stats = stats.iter().find(|s| s.path == "/api").unwrap();
        assert_eq!(api_stats.hits, 1);
    }

    #[test]
    fn test_pulsive_router_regex() {
        let mut model = Model::new();
        let config = test_config();
        let router = PulsiveRouter::new(&mut model, &config).unwrap();

        // Test regex match with rewrite
        let result = router.route(&mut model, "/user/123").unwrap();
        assert_eq!(result.captures, vec!["123".to_string()]);
        assert_eq!(result.rewritten_path, Some("/api/users/123".to_string()));
    }

    #[test]
    fn test_pulsive_router_with_expr() {
        let mut model = Model::new();
        let config = test_config();
        let router = PulsiveRouter::new(&mut model, &config).unwrap();

        // Test expression-based routing
        let result = router.route_with_expr(&mut model, "/api/test").unwrap();
        assert_eq!(result.route.proxy_pass, Some("backend".to_string()));

        // Check expr_hits was updated
        let stats = router.get_route_stats(&model);
        let api_stats = stats.iter().find(|s| s.path == "/api").unwrap();
        assert_eq!(api_stats.expr_hits, 1);
    }
}
