//! Static HTTP server with load balancing, caching, and rate limiting
//!
//! This example demonstrates pulsive's reactive architecture applied to
//! a real-world HTTP server scenario with Nginx-like features.
//!
//! ## Pulsive Integration
//!
//! - **Entities**: Routes, backend servers, server stats tracked as pulsive entities
//! - **Events**: request_received, cache_hit, cache_miss, rate_limited, proxy_error
//! - **Routing**: Pulsive-based routing using entity queries and expression evaluation
//! - **Tick Handlers**: Stats aggregation, periodic logging
//!
//! ## Routing Modes
//!
//! Set `ROUTING_MODE` environment variable:
//! - `imperative` (default): Traditional Router implementation
//! - `pulsive`: Route matching via pulsive model queries
//! - `pulsive_expr`: Route matching via pulsive expression engine

#![allow(dead_code)]

mod cache;
mod config;
mod proxy;
mod pulsive_router;
mod rate_limit;
mod router;
mod static_files;

use cache::ResponseCache;
use config::Config;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use pulsive_core::{
    effect::{Effect, ModifyOp},
    runtime::{EventHandler, TickHandler},
    DefId, EntityId, EntityRef, Expr, Model, Msg, Runtime, Value,
};
use pulsive_router::PulsiveRouter;
use proxy::LoadBalancerManager;
use rate_limit::{RateLimitConfig, RateLimitResult, RateLimiter};
use router::Router;
use static_files::{error_response, generate_autoindex, redirect_response, serve_file, FileResponse};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::net::TcpListener;

/// Routing mode for benchmarking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingMode {
    /// Traditional imperative routing
    Imperative,
    /// Pulsive-based routing with model queries
    Pulsive,
    /// Pulsive routing with expression engine
    PulsiveExpr,
}

impl RoutingMode {
    fn from_env() -> Self {
        match std::env::var("ROUTING_MODE").as_deref() {
            Ok("pulsive") => RoutingMode::Pulsive,
            Ok("pulsive_expr") => RoutingMode::PulsiveExpr,
            _ => RoutingMode::Imperative,
        }
    }
}

/// Server state shared across all connections
struct ServerState {
    /// Configuration
    config: Config,
    /// Imperative routers per server block
    imperative_routers: Vec<Router>,
    /// Pulsive routers per server block
    pulsive_routers: Vec<PulsiveRouter>,
    /// Load balancer manager
    lb_manager: LoadBalancerManager,
    /// Response cache
    cache: ResponseCache,
    /// Rate limiters per location (key: location path)
    rate_limiters: HashMap<String, RateLimiter>,
    /// Pulsive model (state) - protected by RwLock for async access
    model: RwLock<Model>,
    /// Pulsive runtime for reactive event handling
    runtime: RwLock<Runtime>,
    /// Server entity ID for stats tracking
    server_entity_id: EntityId,
    /// Backend entity IDs mapped by address
    backend_entities: HashMap<String, EntityId>,
    /// Routing mode
    routing_mode: RoutingMode,
}

impl ServerState {
    fn new(config: Config, routing_mode: RoutingMode) -> Result<Self, Box<dyn std::error::Error>> {
        // Build imperative routers for each server block
        let imperative_routers: Vec<Router> = config
            .servers
            .iter()
            .map(|s| Router::new(s.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        // Create load balancer manager
        let lb_manager = LoadBalancerManager::new(&config.upstreams);

        // Create cache
        let cache_config = config.cache.clone().unwrap_or_default();
        let cache = ResponseCache::new(cache_config.max_entries, cache_config.default_ttl_secs);

        // Create rate limiters for locations with rate limits
        let mut rate_limiters = HashMap::new();
        for server in &config.servers {
            for location in &server.locations {
                if let Some(ref rl) = location.rate_limit {
                    rate_limiters.insert(
                        location.path.clone(),
                        RateLimiter::new(RateLimitConfig {
                            requests: rl.requests,
                            per_secs: rl.per_secs,
                        }),
                    );
                }
            }
        }

        // Initialize pulsive model
        let mut model = Model::new();

        // Create server entity to track stats
        let server_entity = model.entities.create("http_server");
        server_entity.set("total_requests", Value::Int(0));
        server_entity.set("cache_hits", Value::Int(0));
        server_entity.set("cache_misses", Value::Int(0));
        server_entity.set("rate_limited", Value::Int(0));
        server_entity.set("proxy_requests", Value::Int(0));
        server_entity.set("proxy_errors", Value::Int(0));
        server_entity.set("static_served", Value::Int(0));
        server_entity.set("errors", Value::Int(0));
        server_entity.set("bytes_sent", Value::Int(0));
        server_entity.set("routing_mode", Value::String(format!("{:?}", routing_mode)));
        let server_entity_id = server_entity.id;

        // Create backend entities for each upstream server
        let mut backend_entities = HashMap::new();
        for upstream in &config.upstreams {
            for server in &upstream.servers {
                let backend = model.entities.create("backend");
                backend.set("address", Value::String(server.address.clone()));
                backend.set("upstream", Value::String(upstream.name.clone()));
                backend.set("weight", Value::Int(server.weight as i64));
                backend.set("healthy", Value::Bool(true));
                backend.set("requests", Value::Int(0));
                backend.set("errors", Value::Int(0));
                backend_entities.insert(server.address.clone(), backend.id);
            }
        }

        // Build pulsive routers (creates route entities in the model)
        let pulsive_routers: Vec<PulsiveRouter> = config
            .servers
            .iter()
            .map(|s| PulsiveRouter::new(&mut model, s))
            .collect::<Result<Vec<_>, _>>()?;

        // Initialize pulsive runtime with event handlers
        let mut runtime = Runtime::new();

        // Event handler: Increment total_requests on request_received
        runtime.on_event(EventHandler {
            event_id: DefId::new("request_received"),
            condition: None,
            effects: vec![Effect::ModifyProperty {
                property: "total_requests".to_string(),
                op: ModifyOp::Add,
                value: Expr::Literal(Value::Int(1)),
            }],
            priority: 0,
        });

        // Event handler: Increment cache_hits
        runtime.on_event(EventHandler {
            event_id: DefId::new("cache_hit"),
            condition: None,
            effects: vec![Effect::ModifyProperty {
                property: "cache_hits".to_string(),
                op: ModifyOp::Add,
                value: Expr::Literal(Value::Int(1)),
            }],
            priority: 0,
        });

        // Event handler: Increment cache_misses
        runtime.on_event(EventHandler {
            event_id: DefId::new("cache_miss"),
            condition: None,
            effects: vec![Effect::ModifyProperty {
                property: "cache_misses".to_string(),
                op: ModifyOp::Add,
                value: Expr::Literal(Value::Int(1)),
            }],
            priority: 0,
        });

        // Event handler: Increment rate_limited
        runtime.on_event(EventHandler {
            event_id: DefId::new("rate_limited"),
            condition: None,
            effects: vec![Effect::ModifyProperty {
                property: "rate_limited".to_string(),
                op: ModifyOp::Add,
                value: Expr::Literal(Value::Int(1)),
            }],
            priority: 0,
        });

        // Event handler: Increment proxy_requests
        runtime.on_event(EventHandler {
            event_id: DefId::new("proxy_request"),
            condition: None,
            effects: vec![Effect::ModifyProperty {
                property: "proxy_requests".to_string(),
                op: ModifyOp::Add,
                value: Expr::Literal(Value::Int(1)),
            }],
            priority: 0,
        });

        // Event handler: Increment proxy_errors
        runtime.on_event(EventHandler {
            event_id: DefId::new("proxy_error"),
            condition: None,
            effects: vec![Effect::ModifyProperty {
                property: "proxy_errors".to_string(),
                op: ModifyOp::Add,
                value: Expr::Literal(Value::Int(1)),
            }],
            priority: 0,
        });

        // Event handler: Increment static_served
        runtime.on_event(EventHandler {
            event_id: DefId::new("static_served"),
            condition: None,
            effects: vec![Effect::ModifyProperty {
                property: "static_served".to_string(),
                op: ModifyOp::Add,
                value: Expr::Literal(Value::Int(1)),
            }],
            priority: 0,
        });

        // Event handler: Route matched (for pulsive routing stats)
        runtime.on_event(EventHandler {
            event_id: DefId::new("route_matched"),
            condition: None,
            effects: vec![Effect::Log {
                level: pulsive_core::effect::LogLevel::Debug,
                message: Expr::Literal(Value::String("Route matched".to_string())),
            }],
            priority: 0,
        });

        // Tick handler: Log stats every tick (for http_server entity)
        runtime.on_tick(TickHandler {
            id: DefId::new("stats_logger"),
            condition: None,
            target_kind: Some(DefId::new("http_server")),
            effects: vec![Effect::Log {
                level: pulsive_core::effect::LogLevel::Info,
                message: Expr::Literal(Value::String("Stats tick".to_string())),
            }],
            priority: 100,
        });

        Ok(Self {
            config,
            imperative_routers,
            pulsive_routers,
            lb_manager,
            cache,
            rate_limiters,
            model: RwLock::new(model),
            runtime: RwLock::new(runtime),
            server_entity_id,
            backend_entities,
            routing_mode,
        })
    }

    /// Send an event through the pulsive runtime
    async fn emit_event(&self, event_id: &str) {
        let mut runtime = self.runtime.write().await;
        let mut model = self.model.write().await;
        let tick = model.current_tick();

        let msg = Msg::event(event_id, EntityRef::Entity(self.server_entity_id), tick);
        runtime.send(msg);
        runtime.process_queue(&mut model);
    }

    /// Send an event with parameters
    async fn emit_event_with_params(&self, event_id: &str, params: Vec<(&str, Value)>) {
        let mut runtime = self.runtime.write().await;
        let mut model = self.model.write().await;
        let tick = model.current_tick();

        let mut msg = Msg::event(event_id, EntityRef::Entity(self.server_entity_id), tick);
        for (key, value) in params {
            msg.params.insert(key.to_string(), value);
        }
        runtime.send(msg);
        runtime.process_queue(&mut model);
    }

    /// Get current stats from the model
    async fn get_stats(&self) -> ServerStats {
        let model = self.model.read().await;
        if let Some(entity) = model.entities.get(self.server_entity_id) {
            ServerStats {
                total_requests: entity.get_number("total_requests").unwrap_or(0.0) as u64,
                cache_hits: entity.get_number("cache_hits").unwrap_or(0.0) as u64,
                cache_misses: entity.get_number("cache_misses").unwrap_or(0.0) as u64,
                rate_limited: entity.get_number("rate_limited").unwrap_or(0.0) as u64,
                proxy_requests: entity.get_number("proxy_requests").unwrap_or(0.0) as u64,
                proxy_errors: entity.get_number("proxy_errors").unwrap_or(0.0) as u64,
                static_served: entity.get_number("static_served").unwrap_or(0.0) as u64,
                errors: entity.get_number("errors").unwrap_or(0.0) as u64,
            }
        } else {
            ServerStats::default()
        }
    }

    /// Advance the pulsive clock (triggers tick handlers)
    async fn tick(&self) {
        let mut runtime = self.runtime.write().await;
        let mut model = self.model.write().await;
        runtime.tick(&mut model);
    }

    /// Get route stats (only for pulsive routing modes)
    async fn get_route_stats(&self) -> Vec<pulsive_router::RouteStats> {
        let model = self.model.read().await;
        self.pulsive_routers
            .first()
            .map(|r| r.get_route_stats(&model))
            .unwrap_or_default()
    }
}

/// Server statistics (read from pulsive model)
#[derive(Debug, Default)]
pub struct ServerStats {
    pub total_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub rate_limited: u64,
    pub proxy_requests: u64,
    pub proxy_errors: u64,
    pub static_served: u64,
    pub errors: u64,
}

/// Handle an incoming HTTP request
async fn handle_request(
    state: Arc<ServerState>,
    remote_addr: SocketAddr,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let query = uri.query().map(|s| s.to_string());
    let host = req
        .headers()
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost")
        .to_string();

    // Emit request_received event
    state.emit_event("request_received").await;

    // Route the request based on routing mode
    let route_result = match state.routing_mode {
        RoutingMode::Imperative => {
            // Use traditional imperative router
            let router = state
                .imperative_routers
                .iter()
                .find(|r| {
                    let server = r.server_config();
                    server.server_name.is_empty() || server.server_name.contains(&host)
                })
                .unwrap_or_else(|| state.imperative_routers.first().unwrap());

            router.route(&path).map(|r| RouteInfo {
                path: r.location.path.clone(),
                root: r.root,
                index: r.index,
                proxy_pass: r.location.proxy_pass.clone(),
                return_code: r.location.return_code,
                return_url: r.location.return_url.clone(),
                autoindex: r.location.autoindex,
                rewritten_path: r.rewritten_path,
                rate_limit: r.location.rate_limit.as_ref().map(|rl| (rl.requests, rl.per_secs)),
            })
        }
        RoutingMode::Pulsive => {
            // Use pulsive-based router with model queries
            let router = state.pulsive_routers.first().unwrap();
            let mut model = state.model.write().await;
            router.route(&mut model, &path).map(|r| {
                // Emit route_matched event
                drop(model);
                RouteInfo {
                    path: r.route.path.clone(),
                    root: r.route.root.clone(),
                    index: r.route.index.clone(),
                    proxy_pass: r.route.proxy_pass.clone(),
                    return_code: r.route.return_code,
                    return_url: r.route.return_url.clone(),
                    autoindex: r.route.autoindex,
                    rewritten_path: r.rewritten_path,
                    rate_limit: r.route.rate_limit,
                }
            })
        }
        RoutingMode::PulsiveExpr => {
            // Use pulsive routing with expression engine
            let router = state.pulsive_routers.first().unwrap();
            let mut model = state.model.write().await;
            router.route_with_expr(&mut model, &path).map(|r| {
                drop(model);
                RouteInfo {
                    path: r.route.path.clone(),
                    root: r.route.root.clone(),
                    index: r.route.index.clone(),
                    proxy_pass: r.route.proxy_pass.clone(),
                    return_code: r.route.return_code,
                    return_url: r.route.return_url.clone(),
                    autoindex: r.route.autoindex,
                    rewritten_path: r.rewritten_path,
                    rate_limit: r.route.rate_limit,
                }
            })
        }
    };

    let route = match route_result {
        Some(r) => r,
        None => {
            return Ok(error_response(StatusCode::NOT_FOUND, "Page not found"));
        }
    };

    // Check rate limit
    if let Some((requests, per_secs)) = route.rate_limit {
        if let Some(limiter) = state.rate_limiters.get(&route.path) {
            let result = limiter.check(remote_addr.ip()).await;
            if !result.is_allowed() {
                // Emit rate_limited event
                state.emit_event("rate_limited").await;

                if let RateLimitResult::Limited { retry_after, limit } = result {
                    let mut response = error_response(
                        StatusCode::TOO_MANY_REQUESTS,
                        "Rate limit exceeded",
                    );
                    response.headers_mut().insert(
                        "Retry-After",
                        retry_after.as_secs().to_string().parse().unwrap(),
                    );
                    response.headers_mut().insert(
                        "X-RateLimit-Limit",
                        limit.to_string().parse().unwrap(),
                    );
                    return Ok(response);
                }
            }
        }
    }

    // Handle redirect
    if let (Some(code), Some(ref url)) = (route.return_code, &route.return_url) {
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::FOUND);
        return Ok(redirect_response(status, url));
    }

    // Handle rewrite
    let effective_path = route.rewritten_path.as_ref().unwrap_or(&path);

    // Handle proxy pass
    if let Some(ref upstream_name) = route.proxy_pass {
        if let Some(lb) = state.lb_manager.get(upstream_name) {
            // Emit proxy_request event
            state.emit_event("proxy_request").await;

            // Collect headers
            let headers: Vec<(String, String)> = req
                .headers()
                .iter()
                .filter(|(k, _)| k.as_str() != "host")
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();

            // Read body
            let body = match http_body_util::BodyExt::collect(req.into_body()).await {
                Ok(collected) => collected.to_bytes().to_vec(),
                Err(_) => vec![],
            };

            // Proxy the request
            match lb.proxy_request(method.as_str(), effective_path, headers, body).await {
                Ok(proxy_resp) => {
                    let mut builder = Response::builder().status(proxy_resp.status);
                    for (key, value) in proxy_resp.headers {
                        if !is_hop_by_hop_header(&key) {
                            builder = builder.header(&key, &value);
                        }
                    }
                    return Ok(builder
                        .body(Full::new(Bytes::from(proxy_resp.body)))
                        .unwrap());
                }
                Err(e) => {
                    // Emit proxy_error event
                    state.emit_event("proxy_error").await;

                    eprintln!("[proxy] Error: {}", e);
                    return Ok(error_response(
                        StatusCode::BAD_GATEWAY,
                        &format!("Proxy error: {}", e),
                    ));
                }
            }
        } else {
            return Ok(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Unknown upstream: {}", upstream_name),
            ));
        }
    }

    // Handle static files
    if let Some(ref root) = route.root {
        // Strip the location prefix from the path to get relative path
        let relative_path = if !route.path.starts_with("~") && effective_path.starts_with(&route.path) {
            let stripped = &effective_path[route.path.len()..];
            if stripped.is_empty() { "/" } else { stripped }
        } else {
            effective_path
        };

        // Check cache first
        let cache_key = ResponseCache::make_key(relative_path, query.as_deref());
        if let Some(cached) = state.cache.get(&cache_key).await {
            // Emit cache_hit event
            state.emit_event("cache_hit").await;

                            let mut response = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", &cached.content_type)
                .header("X-Cache", "HIT")
                .header("X-Routing-Mode", format!("{:?}", state.routing_mode));

            for (key, value) in &cached.headers {
                response = response.header(key.as_str(), value.as_str());
            }

            return Ok(response.body(Full::new(cached.body.clone())).unwrap());
        }

        // Emit cache_miss event
        state.emit_event("cache_miss").await;

        // Serve file
        match serve_file(root, relative_path, &route.index).await {
            FileResponse::Found(response) => {
                // Emit static_served event
                state.emit_event("static_served").await;
                Ok(response)
            }
            FileResponse::Directory(dir_path) => {
                if route.autoindex {
                    match generate_autoindex(&dir_path, &path).await {
                        Ok(html) => {
                            state.emit_event("static_served").await;
                            Ok(Response::builder()
                                .status(StatusCode::OK)
                                .header("Content-Type", "text/html; charset=utf-8")
                                .header("X-Routing-Mode", format!("{:?}", state.routing_mode))
                                .body(Full::new(Bytes::from(html)))
                                .unwrap())
                        }
                        Err(e) => Ok(error_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            &format!("Failed to generate directory listing: {}", e),
                        )),
                    }
                } else {
                    Ok(error_response(StatusCode::FORBIDDEN, "Directory listing not allowed"))
                }
            }
            FileResponse::NotFound => {
                // Check for custom error page
                let default_root = state.pulsive_routers.first()
                    .map(|r| r.default_root().to_string())
                    .unwrap_or_else(|| "./www".to_string());
                
                // Try to serve 404 page
                if let FileResponse::Found(resp) = serve_file(&default_root, "/404.html", &[]).await {
                            return Ok(Response::builder()
                                .status(StatusCode::NOT_FOUND)
                        .body(resp.into_body())
                                .unwrap());
                }
                Ok(error_response(StatusCode::NOT_FOUND, "File not found"))
            }
            FileResponse::Error(e) => {
                Ok(error_response(StatusCode::INTERNAL_SERVER_ERROR, &e))
            }
        }
    } else {
        Ok(error_response(StatusCode::NOT_FOUND, "No root configured"))
    }
}

/// Unified route info from either router type
struct RouteInfo {
    path: String,
    root: Option<String>,
    index: Vec<String>,
    proxy_pass: Option<String>,
    return_code: Option<u16>,
    return_url: Option<String>,
    autoindex: bool,
    rewritten_path: Option<String>,
    rate_limit: Option<(u32, u32)>,
}

/// Check if a header is a hop-by-hop header that shouldn't be forwarded
fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Determine routing mode
    let routing_mode = RoutingMode::from_env();
    
    // Load configuration
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config/server.ron".to_string());

    println!("Loading configuration from: {}", config_path);
    let config = Config::load(&config_path)?;

    // Create server state with pulsive integration
    let state = Arc::new(ServerState::new(config.clone(), routing_mode)?);

    // Start health check tasks
    let _health_handles = state.lb_manager.start_health_checks();

    // Start rate limiter cleanup tasks
    for (path, limiter) in &state.rate_limiters {
        println!("Rate limiter active for: {}", path);
        limiter.clone().start_cleanup_task();
    }

    // Start pulsive tick task (for stats aggregation)
    let tick_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            tick_state.tick().await;

            // Log current stats
            let stats = tick_state.get_stats().await;
            println!(
                "[stats] mode={:?} requests={} cache_hits={} cache_misses={} rate_limited={} proxy={} static={}",
                tick_state.routing_mode,
                stats.total_requests,
                stats.cache_hits,
                stats.cache_misses,
                stats.rate_limited,
                stats.proxy_requests,
                stats.static_served
            );

            // Log route stats for pulsive modes
            if tick_state.routing_mode != RoutingMode::Imperative {
                let route_stats = tick_state.get_route_stats().await;
                for rs in route_stats {
                    if rs.hits > 0 || rs.expr_hits > 0 {
                        println!(
                            "  [route] path={} hits={} expr_hits={}",
                            rs.path, rs.hits, rs.expr_hits
                        );
                    }
                }
            }
        }
    });

    // Bind to all configured addresses
    let mut handles = Vec::new();
    for server in &config.servers {
        for addr_str in &server.listen {
            let addr: SocketAddr = addr_str.parse()?;
            let listener = TcpListener::bind(addr).await?;
            println!("Listening on http://{}", addr);

            let state = state.clone();
            let handle = tokio::spawn(async move {
                loop {
                    let (stream, remote_addr) = match listener.accept().await {
                        Ok(conn) => conn,
                        Err(e) => {
                            eprintln!("Accept error: {}", e);
                            continue;
                        }
                    };

                    let state = state.clone();
                    tokio::spawn(async move {
                        let io = TokioIo::new(stream);
                        let service = service_fn(move |req| {
                            let state = state.clone();
                            async move { handle_request(state, remote_addr, req).await }
                        });

                        if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                            eprintln!("Connection error: {}", e);
                        }
                    });
                }
            });
            handles.push(handle);
        }
    }

    println!("\nServer started. Press Ctrl+C to stop.\n");

    // Print server info
    println!("=== Pulsive HTTP Server ===");
    println!("Routing Mode: {:?}", routing_mode);
    println!("  Set ROUTING_MODE env var to: imperative, pulsive, or pulsive_expr");
    println!();
    println!("Pulsive Integration:");
    println!("  - Entities: http_server (stats), backend (per upstream), route (per location)");
    println!("  - Events: request_received, cache_hit, cache_miss, rate_limited, proxy_request, proxy_error, static_served");
    println!("  - Tick: Stats aggregation every 10 seconds");
    println!();
    println!("Features:");
    println!("  - Static file serving with MIME detection");
    println!("  - Regex URL rewriting (safe O(n) regex)");
    println!("  - In-memory response caching");
    println!("  - Per-IP rate limiting");
    if !config.upstreams.is_empty() {
        println!("  - Load balancing ({} upstream pools)", config.upstreams.len());
        for upstream in &config.upstreams {
            println!(
                "    - {}: {} backends ({:?})",
                upstream.name,
                upstream.servers.len(),
                upstream.method
            );
        }
    }
    println!();

    // Wait for all servers
    for handle in handles {
        handle.await?;
    }

    Ok(())
}
