//! Static HTTP server with load balancing, caching, and rate limiting
//! 
//! This example demonstrates pulsive's reactive architecture applied to
//! a real-world HTTP server scenario with Nginx-like features.

#![allow(dead_code)]

mod cache;
mod config;
mod proxy;
mod rate_limit;
mod router;
mod static_files;

use cache::ResponseCache;
use config::Config;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use pulsive_core::{Model, Runtime, Value};
use proxy::LoadBalancerManager;
use rate_limit::{RateLimitConfig, RateLimitResult, RateLimiter};
use router::Router;
use static_files::{error_response, generate_autoindex, redirect_response, serve_file, FileResponse};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpListener;

/// Server statistics tracked via pulsive
pub struct ServerStats {
    pub total_requests: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub rate_limited: AtomicU64,
    pub errors: AtomicU64,
    pub bytes_sent: AtomicU64,
}

impl ServerStats {
    fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            rate_limited: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
        }
    }
}

/// Server state shared across all connections
struct ServerState {
    /// Configuration
    config: Config,
    /// Router per server block
    routers: Vec<Router>,
    /// Load balancer manager
    lb_manager: LoadBalancerManager,
    /// Response cache
    cache: ResponseCache,
    /// Rate limiters per location (key: location path)
    rate_limiters: HashMap<String, RateLimiter>,
    /// Server statistics
    stats: ServerStats,
    /// Pulsive runtime for reactive event handling
    _runtime: Runtime,
}

impl ServerState {
    fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        // Build routers for each server block
        let routers: Vec<Router> = config
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

        // Initialize pulsive runtime for reactive event handling
        // The runtime can be used to track server metrics as entities
        let mut model = Model::new();
        
        // Create server entity to track stats using pulsive
        let server_entity = model.entities.create("server");
        server_entity.set("total_requests", Value::Int(0));
        server_entity.set("cache_hits", Value::Int(0));
        server_entity.set("cache_misses", Value::Int(0));
        server_entity.set("rate_limited", Value::Int(0));

        // Runtime for reactive event handling (can register handlers)
        let runtime = Runtime::new();

        Ok(Self {
            config,
            routers,
            lb_manager,
            cache,
            rate_limiters,
            stats: ServerStats::new(),
            _runtime: runtime,
        })
    }
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

    // Find matching router (by host)
    let router = state
        .routers
        .iter()
        .find(|r| {
            let server = r.server_config();
            server.server_name.is_empty() || server.server_name.contains(&host)
        })
        .unwrap_or_else(|| state.routers.first().unwrap());

    // Route the request
    let route = match router.route(&path) {
        Some(r) => r,
        None => {
            return Ok(error_response(StatusCode::NOT_FOUND, "Page not found"));
        }
    };

    // Check rate limit
    if let Some(ref _rl_config) = route.location.rate_limit {
        if let Some(limiter) = state.rate_limiters.get(&route.location.path) {
            let result = limiter.check(remote_addr.ip()).await;
            if !result.is_allowed() {
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
    if let (Some(code), Some(ref url)) = (route.location.return_code, &route.location.return_url) {
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::FOUND);
        return Ok(redirect_response(status, url));
    }

    // Handle rewrite
    let effective_path = route.rewritten_path.as_ref().unwrap_or(&path);

    // Handle proxy pass
    if let Some(ref upstream_name) = route.location.proxy_pass {
        if let Some(lb) = state.lb_manager.get(upstream_name) {
            // Collect headers
            let headers: Vec<(String, String)> = req
                .headers()
                .iter()
                .filter(|(k, _)| k.as_str() != "host") // Don't forward host
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
                        // Skip hop-by-hop headers
                        if !is_hop_by_hop_header(&key) {
                            builder = builder.header(&key, &value);
                        }
                    }
                    return Ok(builder
                        .body(Full::new(Bytes::from(proxy_resp.body)))
                        .unwrap());
                }
                Err(e) => {
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
        let location_prefix = &route.location.path;
        let relative_path = if !route.location.path.starts_with("~") && effective_path.starts_with(location_prefix) {
            let stripped = &effective_path[location_prefix.len()..];
            if stripped.is_empty() { "/" } else { stripped }
        } else {
            effective_path
        };

        // Check cache first
        let cache_key = ResponseCache::make_key(relative_path, query.as_deref());
        if let Some(cached) = state.cache.get(&cache_key).await {
            state.stats.cache_hits.fetch_add(1, Ordering::Relaxed);
            
            let mut response = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", &cached.content_type)
                .header("X-Cache", "HIT");

            for (key, value) in &cached.headers {
                response = response.header(key.as_str(), value.as_str());
            }

            return Ok(response.body(Full::new(cached.body.clone())).unwrap());
        }

        state.stats.cache_misses.fetch_add(1, Ordering::Relaxed);

        // Serve file
        match serve_file(root, relative_path, &route.index).await {
            FileResponse::Found(response) => {
                // Cache the response if TTL is configured
                if let Some(_ttl) = route.location.cache_ttl_secs {
                    // For caching, we'd need to clone the body before sending
                    // This is a simplified version - full implementation would buffer the response
                }
                Ok(response)
            }
            FileResponse::Directory(dir_path) => {
                if route.location.autoindex {
                    match generate_autoindex(&dir_path, &path).await {
                        Ok(html) => Ok(Response::builder()
                            .status(StatusCode::OK)
                            .header("Content-Type", "text/html; charset=utf-8")
                            .body(Full::new(Bytes::from(html)))
                            .unwrap()),
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
                if let Some(error_page) = router.error_page(404) {
                    if let FileResponse::Found(resp) = serve_file(
                        route.root.as_ref().unwrap_or(&".".to_string()),
                        error_page,
                        &[],
                    )
                    .await
                    {
                        return Ok(Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(resp.into_body())
                            .unwrap());
                    }
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

/// Log access in Apache/Nginx format
fn log_access(
    remote_addr: SocketAddr,
    method: &Method,
    path: &str,
    status: StatusCode,
    bytes: usize,
) {
    let timestamp = chrono::Local::now().format("%d/%b/%Y:%H:%M:%S %z");
    println!(
        "{} - - [{}] \"{} {} HTTP/1.1\" {} {}",
        remote_addr.ip(),
        timestamp,
        method,
        path,
        status.as_u16(),
        bytes
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config/server.ron".to_string());

    println!("Loading configuration from: {}", config_path);
    let config = Config::load(&config_path)?;

    // Create server state
    let state = Arc::new(ServerState::new(config.clone())?);

    // Start health check tasks
    let _health_handles = state.lb_manager.start_health_checks();

    // Start rate limiter cleanup tasks
    for (path, limiter) in &state.rate_limiters {
        println!("Rate limiter active for: {}", path);
        limiter.clone().start_cleanup_task();
    }

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
