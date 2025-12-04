# Pulsive HTTP Server

A static HTTP server with Nginx-like features, demonstrating Pulsive's reactive architecture for real-world applications.

## Features

| Feature | Description |
|---------|-------------|
| **Static Files** | Serve files with automatic MIME type detection |
| **Directory Listing** | Autoindex for browsing directories |
| **Load Balancing** | Round-robin, least-connections, weighted distribution |
| **Health Checks** | Automatic backend monitoring with failover |
| **Rate Limiting** | Per-IP token bucket algorithm |
| **Response Caching** | In-memory LRU cache with TTL |
| **URL Rewriting** | Safe O(n) regex rewrites (no ReDoS) |
| **HTTP Redirects** | 301/302 redirect support |
| **RON Configuration** | Nginx-like config in Rust Object Notation |

## Quick Start

```bash
# From the http_server directory
cargo run -- config/server.ron

# Or from workspace root
cargo run -p http_server -- examples/http_server/config/server.ron
```

Then visit: http://localhost:8080

## Configuration

Configuration uses RON (Rust Object Notation) format. See `config/server.ron` for a full example.

### Basic Example

```ron
(
    servers: [
        (
            listen: ["0.0.0.0:8080"],
            root: "./www",
            locations: [
                (
                    path: "/static",
                    root: Some("./www/static"),
                    autoindex: true,
                ),
                (
                    path: "/api",
                    proxy_pass: Some("backend"),
                    rate_limit: Some((
                        requests: 100,
                        per_secs: 60,
                    )),
                ),
            ],
        ),
    ],
    upstreams: [
        (
            name: "backend",
            method: LeastConn,
            servers: [
                ( address: "127.0.0.1:3001", weight: 2 ),
                ( address: "127.0.0.1:3002", weight: 1 ),
            ],
        ),
    ],
)
```

### Configuration Reference

#### Server Block

| Field | Type | Description |
|-------|------|-------------|
| `listen` | `[String]` | Addresses to bind (e.g., `"0.0.0.0:8080"`) |
| `server_name` | `[String]` | Virtual host names (optional) |
| `root` | `String` | Document root directory |
| `index` | `[String]` | Index files (default: `["index.html"]`) |
| `error_pages` | `{u16: String}` | Custom error pages |
| `locations` | `[Location]` | URL path routing rules |
| `add_headers` | `{String: String}` | Headers to add to responses |

#### Location Block

| Field | Type | Description |
|-------|------|-------------|
| `path` | `String` | URL prefix or regex (prefix with `~`) |
| `root` | `Option<String>` | Override document root |
| `proxy_pass` | `Option<String>` | Upstream pool name |
| `return_code` | `Option<u16>` | Redirect status code |
| `return_url` | `Option<String>` | Redirect destination |
| `rewrite` | `Option<String>` | URL rewrite pattern |
| `autoindex` | `bool` | Enable directory listing |
| `cache_ttl_secs` | `Option<u64>` | Cache TTL in seconds |
| `rate_limit` | `Option<RateLimit>` | Rate limiting config |

#### Upstream Block

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Pool name for `proxy_pass` |
| `method` | `LoadBalanceMethod` | `RoundRobin`, `LeastConn`, or `Weighted` |
| `servers` | `[Server]` | Backend servers |
| `health_check_interval_ms` | `u64` | Health check interval |
| `health_check_path` | `String` | Health check endpoint |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     HTTP Request                            │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Rate Limiter                             │
│              (Token Bucket per IP)                          │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      Router                                 │
│         (Prefix match → Regex match → Default)              │
└─────────────────────────────┬───────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
        ┌──────────┐   ┌──────────┐   ┌──────────────┐
        │ Static   │   │ Redirect │   │ Proxy Pass   │
        │ Files    │   │          │   │ (Load Bal.)  │
        └────┬─────┘   └──────────┘   └──────┬───────┘
             │                               │
             ▼                               ▼
        ┌──────────┐                  ┌──────────────┐
        │ Response │                  │   Upstream   │
        │  Cache   │                  │   Backends   │
        └──────────┘                  └──────────────┘
```

## Load Balancing

### Methods

| Method | Description |
|--------|-------------|
| `RoundRobin` | Rotate through backends sequentially |
| `LeastConn` | Route to backend with fewest active connections |
| `Weighted` | Distribute based on server weights |

### Health Checks

Backends are automatically monitored:
- Periodic HTTP health checks (configurable interval)
- Failed backends are marked unhealthy
- Traffic automatically reroutes to healthy backends
- Backends recover when health checks pass

### Failover

```ron
servers: [
    ( address: "primary:8000", weight: 2 ),
    ( address: "backup:8000", backup: true ),  // Used only when primary is down
]
```

## Rate Limiting

Token bucket algorithm with per-IP tracking:

```ron
rate_limit: Some((
    requests: 100,    // Max requests
    per_secs: 60,     // Time window
))
```

- Returns `429 Too Many Requests` when limit exceeded
- Includes `Retry-After` header
- Automatic bucket cleanup for inactive IPs

## URL Rewriting

Safe regex rewriting using Rust's `regex` crate (guaranteed O(n), no ReDoS):

```ron
// Rewrite /user/123 to /api/users/123
(
    path: "~ ^/user/(\\d+)",
    rewrite: Some("/api/users/$1"),
)
```

## Integration Tests

Full integration test suite with Docker Compose:

```bash
cd integration-test
docker compose up --build
```

### What Gets Tested

- Service health and connectivity
- Static file serving
- Load balancing distribution
- HTTP redirects
- Performance benchmarks (using `hey`)
- Rate limiting

See [integration-test/README.md](integration-test/README.md) for details.

## Performance

Benchmarks from integration tests (Docker, Apple Silicon):

| Test | Requests/sec | Avg Response |
|------|--------------|--------------|
| Static files | ~40,000 | 1.2ms |
| Load balanced API | ~1,000 | 2.1ms |
| High concurrency (200) | ~59,000 | 3.1ms |

## Project Structure

```
http_server/
├── Cargo.toml
├── README.md
├── config/
│   └── server.ron          # Example configuration
├── www/                    # Static files
│   ├── index.html
│   ├── 404.html
│   ├── 500.html
│   └── static/
│       ├── style.css
│       └── script.js
├── src/
│   ├── main.rs             # Entry point, request handling
│   ├── lib.rs              # Module exports
│   ├── config.rs           # RON configuration parsing
│   ├── router.rs           # URL routing with regex
│   ├── static_files.rs     # File serving, MIME detection
│   ├── proxy.rs            # Load balancer, health checks
│   ├── cache.rs            # Response caching (moka)
│   └── rate_limit.rs       # Token bucket rate limiting
└── integration-test/
    ├── README.md
    ├── docker-compose.yml
    ├── Dockerfile.*
    └── run_tests.sh
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime |
| `hyper` | HTTP server |
| `reqwest` | HTTP client (proxy, health checks) |
| `regex` | Safe URL rewriting |
| `moka` | In-memory LRU cache |
| `ron` | Configuration parsing |
| `pulsive-core` | Reactive entity/event system |
| `pulsive-db` | Persistence (available for stats) |

## Pulsive Integration

This example demonstrates pulsive-core's reactive patterns applied to a real HTTP server:

### Routing Modes

The server supports three routing modes, selectable via `ROUTING_MODE` environment variable:

| Mode | Env Value | Description |
|------|-----------|-------------|
| **Imperative** | `imperative` (default) | Traditional Router with compiled locations |
| **Pulsive** | `pulsive` | Routes as entities, model queries for matching |
| **Pulsive Expr** | `pulsive_expr` | Uses pulsive's expression engine for conditions |

```bash
# Run with pulsive routing for benchmarking
ROUTING_MODE=pulsive cargo run -p http_server

# Run with expression-based routing
ROUTING_MODE=pulsive_expr cargo run -p http_server
```

### Entities

| Entity Type | Purpose |
|-------------|---------|
| `http_server` | Server-wide statistics (1 instance) |
| `backend` | Backend server state (1 per upstream server) |
| `route` | Route configuration and hit counters (1 per location) |

### Events

| Event | When Fired | Effect |
|-------|------------|--------|
| `request_received` | Every incoming request | Increments `total_requests` |
| `cache_hit` | Response served from cache | Increments `cache_hits` |
| `cache_miss` | Cache lookup failed | Increments `cache_misses` |
| `rate_limited` | Request rejected by rate limiter | Increments `rate_limited` |
| `proxy_request` | Request forwarded to upstream | Increments `proxy_requests` |
| `proxy_error` | Upstream request failed | Increments `proxy_errors` |
| `static_served` | Static file served | Increments `static_served` |
| `route_matched` | Route matched (pulsive modes) | Logs debug message |

### Pulsive-Based Routing

Routes are stored as entities in the pulsive model:

```rust
// Route entity properties
entity.set("path", "/api");
entity.set("is_regex", false);
entity.set("priority", 100);
entity.set("proxy_pass", "backend");
entity.set("hits", 0);        // Updated on each match
entity.set("expr_hits", 0);   // Updated in pulsive_expr mode
```

The `route_with_expr` mode uses pulsive's expression engine:

```rust
// Build condition: path_len >= prefix_len
let condition = Expr::Ge(
    Box::new(Expr::Literal(Value::Int(path_len))),
    Box::new(Expr::Literal(Value::Int(prefix_len))),
);

// Evaluate using pulsive
let result = condition.eval(&mut ctx);
```

### Event Handlers

Event handlers are registered at startup to reactively update entity properties:

```rust
// Example: Increment total_requests on request_received
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
```

### Tick Handlers

A background task calls `runtime.tick()` every 10 seconds, triggering:
- Stats aggregation
- Route hit statistics (in pulsive modes)
- Periodic logging

```
[stats] mode=Pulsive requests=150 cache_hits=45 cache_misses=105 rate_limited=3 proxy=50 static=97
  [route] path=/api hits=50 expr_hits=0
  [route] path=/static hits=97 expr_hits=0
```

### Architecture with Pulsive

```
┌─────────────────────────────────────────────────────────────┐
│                     HTTP Request                            │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  Pulsive Runtime                            │
│   emit_event("request_received") → EventHandler → Model    │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Route Matching                           │
│   (Imperative | Pulsive Model Query | Pulsive Expr)        │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                 Request Processing                          │
│        (Rate Limit → Cache → Serve/Proxy)                  │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│              emit_event("cache_hit" | "static_served" | …)  │
└─────────────────────────────────────────────────────────────┘
```

### Performance Comparison

Run benchmarks with different routing modes:

```bash
# Benchmark imperative routing (baseline)
ROUTING_MODE=imperative hey -n 10000 -c 100 http://localhost:8080/

# Benchmark pulsive routing
ROUTING_MODE=pulsive hey -n 10000 -c 100 http://localhost:8080/

# Benchmark expression-based routing
ROUTING_MODE=pulsive_expr hey -n 10000 -c 100 http://localhost:8080/
```

### Why Use Pulsive Here?

1. **Reactive Stats**: Stats are entity properties updated by event handlers
2. **Route Entities**: Routes as data, enabling runtime modification
3. **Expression Engine**: Demonstrates pulsive's Expr for condition evaluation
4. **Benchmarkable**: Compare imperative vs reactive routing performance
5. **Extensibility**: Add new events/handlers without modifying core logic
6. **Future-Ready**: Easy to add persistence with `pulsive-db`

## License

MIT OR Apache-2.0

