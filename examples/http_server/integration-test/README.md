# Pulsive HTTP Server - Integration Tests

This directory contains a Docker Compose setup for running integration and performance tests against the Pulsive HTTP server.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Docker Network                           │
│                                                             │
│  ┌─────────────┐     ┌─────────────────┐     ┌───────────┐ │
│  │  Backend 1  │     │  Pulsive Server │     │ Load Test │ │
│  │  (Python)   │◄────┤  (Rust/Load     │◄────┤  Client   │ │
│  │  port 8000  │     │   Balancer)     │     │  (hey)    │ │
│  └─────────────┘     │  port 8080      │     └───────────┘ │
│                      └────────┬────────┘                    │
│  ┌─────────────┐              │                             │
│  │  Backend 2  │◄─────────────┘                             │
│  │  (Python)   │                                            │
│  │  port 8000  │                                            │
│  └─────────────┘                                            │
└─────────────────────────────────────────────────────────────┘
```

## Components

| Service | Description |
|---------|-------------|
| `backend1` | Python HTTP server simulating upstream service |
| `backend2` | Second backend for load balancing tests |
| `pulsive` | The Pulsive HTTP server with load balancing |
| `loadtest` | Test client using [hey](https://github.com/rakyll/hey) |

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) (20.10+)
- [Docker Compose](https://docs.docker.com/compose/install/) (v2+)

## Quick Start

```bash
# Run all tests (builds images and runs automatically)
docker compose up --build

# Run in detached mode
docker compose up --build -d

# View logs
docker compose logs -f loadtest

# Stop and clean up
docker compose down
```

## What Gets Tested

### Phase 1: Service Health
- Waits for all services to be healthy
- Verifies backend health endpoints
- Confirms Pulsive server is responding

### Phase 2: Functional Tests
1. **Static file serving** - Verifies index.html is served correctly
2. **Directory listing** - Tests autoindex functionality on /static/
3. **Load balancing** - Verifies requests are distributed across backends
4. **HTTP redirects** - Tests 301 redirect on /old-page

### Phase 3: Performance Tests
Uses `hey` to benchmark:
- Static file serving (1000 requests, 50 concurrent)
- Load balanced API (1000 requests, 50 concurrent)
- High concurrency (5000 requests, 200 concurrent)

### Phase 4: Rate Limiting
- Sends 150 rapid requests to /api endpoint
- Verifies rate limiting kicks in (limit: 100 req/min)

## Sample Output

```
======================================
  Pulsive HTTP Server Integration Test
======================================

[Phase 1] Waiting for services to be ready...
Waiting for Backend 1... ready
Waiting for Backend 2... ready
Waiting for Pulsive Server... ready

[Phase 2] Functional Tests
-----------------------------------
Test 1: Static file serving... PASS
Test 2: Directory listing... PASS
Test 3: Load balancing distribution... PASS (backend-1: 11, backend-2: 9)
Test 4: HTTP redirect... PASS

[Phase 3] Performance Tests
-----------------------------------
Test: Static file performance (1000 requests, 50 concurrent)
  Requests/sec: 8234.12
  Average:      0.0061 secs
  
Test: Load balanced API (1000 requests, 50 concurrent)
  Requests/sec: 4521.33
  Average:      0.0110 secs

[Phase 4] Rate Limiting Test
-----------------------------------
Test: Rate limiting on /api... PASS (47 requests rate-limited)

======================================
  Integration Tests Complete!
======================================
```

## Running Individual Services

```bash
# Start only backends
docker compose up backend1 backend2

# Start pulsive server (requires backends)
docker compose up pulsive

# Run tests against already-running services
docker compose run loadtest
```

## Manual Testing

While services are running, you can test manually:

```bash
# Static files
curl http://localhost:8080/
curl http://localhost:8080/static/

# Load balanced API (run multiple times)
curl http://localhost:8080/api/echo

# Test redirect
curl -I http://localhost:8080/old-page
```

## Configuration

The test configuration is in `server-test.ron`. Key settings:

| Setting | Value | Description |
|---------|-------|-------------|
| Rate limit | 100 req/60s | Per-IP on /api endpoints |
| Health check | 3s interval | Backend health monitoring |
| Load balance | LeastConn | Routes to backend with fewer connections |
| Cache TTL | 30s | Response caching duration |

## Troubleshooting

### Build fails
```bash
# Clean rebuild
docker compose build --no-cache
```

### Services not healthy
```bash
# Check individual service logs
docker compose logs backend1
docker compose logs pulsive
```

### Port already in use
```bash
# Change the exposed port in docker-compose.yml
# Or stop conflicting services
lsof -i :8080
```

## Why Docker Compose?

We evaluated several alternatives:

| Tool | Pros | Cons |
|------|------|------|
| **Docker Compose** ✓ | Simple, standard, well-documented | Requires Docker |
| Podman Compose | Rootless, Docker-compatible | Less tooling |
| Testcontainers | Programmatic, from test code | More complex setup |
| Dagger.io | CI/CD native, cacheable | Steeper learning curve |
| Kubernetes (Kind) | Production-like | Overkill for local tests |

Docker Compose remains the best choice for local integration testing due to its simplicity and wide adoption.

## Files

```
integration-test/
├── README.md              # This file
├── docker-compose.yml     # Service orchestration
├── Dockerfile.server      # Pulsive server image
├── Dockerfile.backend     # Python backend image
├── Dockerfile.loadtest    # Test client image
├── backend_server.py      # Backend server code
├── run_tests.sh          # Test script
└── server-test.ron       # Pulsive config for tests
```

