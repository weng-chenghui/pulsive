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
| `pulsive-core` | Reactive entity tracking |

## Pulsive Integration

This example demonstrates pulsive-core's reactive patterns:

- **Entities**: Track server stats, backend health
- **Events**: Request received, cache hit/miss, rate limited
- **Model**: Server configuration and runtime state

The server uses pulsive's entity system to track:
- Total requests served
- Cache hit/miss ratios
- Rate limiting events
- Backend health status

## License

MIT OR Apache-2.0

