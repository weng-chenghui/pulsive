//! Load balancer with health checks

use crate::config::{LoadBalanceMethod, UpstreamConfig, UpstreamServer};
use reqwest::Client;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// State of a backend server
#[derive(Debug, Clone)]
pub struct BackendState {
    /// Server configuration
    pub config: UpstreamServer,
    /// Is the server healthy?
    pub healthy: bool,
    /// Active connection count
    pub active_connections: u64,
    /// Total requests served
    pub total_requests: u64,
    /// Last health check time
    pub last_health_check: Option<Instant>,
    /// Last response time in milliseconds
    pub last_response_time_ms: Option<u64>,
}

impl BackendState {
    fn new(config: UpstreamServer) -> Self {
        Self {
            config,
            healthy: true, // Assume healthy until proven otherwise
            active_connections: 0,
            total_requests: 0,
            last_health_check: None,
            last_response_time_ms: None,
        }
    }
}

/// Load balancer for an upstream pool
pub struct LoadBalancer {
    /// Pool name
    name: String,
    /// Load balancing method
    method: LoadBalanceMethod,
    /// Backend servers
    backends: Arc<RwLock<Vec<BackendState>>>,
    /// Round-robin counter
    rr_counter: AtomicUsize,
    /// Health check configuration
    health_check_interval: Duration,
    health_check_path: String,
    #[allow(dead_code)]
    health_check_timeout: Duration,
    /// HTTP client for proxying and health checks
    client: Client,
    /// Weighted selection indices (for weighted round-robin)
    weighted_indices: Arc<RwLock<Vec<usize>>>,
    weighted_counter: AtomicUsize,
}

impl LoadBalancer {
    /// Create a new load balancer from upstream config
    pub fn new(config: &UpstreamConfig) -> Self {
        let backends: Vec<BackendState> = config
            .servers
            .iter()
            .map(|s| BackendState::new(s.clone()))
            .collect();

        // Build weighted indices for weighted round-robin
        let mut weighted_indices = Vec::new();
        for (i, server) in config.servers.iter().enumerate() {
            for _ in 0..server.weight {
                weighted_indices.push(i);
            }
        }

        let client = Client::builder()
            .timeout(Duration::from_millis(config.health_check_timeout_ms))
            .build()
            .unwrap_or_default();

        Self {
            name: config.name.clone(),
            method: config.method,
            backends: Arc::new(RwLock::new(backends)),
            rr_counter: AtomicUsize::new(0),
            health_check_interval: Duration::from_millis(config.health_check_interval_ms),
            health_check_path: config.health_check_path.clone(),
            health_check_timeout: Duration::from_millis(config.health_check_timeout_ms),
            client,
            weighted_indices: Arc::new(RwLock::new(weighted_indices)),
            weighted_counter: AtomicUsize::new(0),
        }
    }

    /// Get the pool name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Select a backend server based on the load balancing method
    pub async fn select_backend(&self) -> Option<String> {
        let backends = self.backends.read().await;

        // Get healthy, non-backup servers
        let healthy: Vec<(usize, &BackendState)> = backends
            .iter()
            .enumerate()
            .filter(|(_, b)| b.healthy && !b.config.backup)
            .collect();

        // If no healthy primary servers, try backups
        let candidates = if healthy.is_empty() {
            backends
                .iter()
                .enumerate()
                .filter(|(_, b)| b.healthy && b.config.backup)
                .collect::<Vec<_>>()
        } else {
            healthy
        };

        if candidates.is_empty() {
            return None;
        }

        let selected_idx = match self.method {
            LoadBalanceMethod::RoundRobin => {
                let counter = self.rr_counter.fetch_add(1, Ordering::Relaxed);
                candidates[counter % candidates.len()].0
            }
            LoadBalanceMethod::LeastConn => {
                // Find minimum connection count
                let min_conns = candidates
                    .iter()
                    .map(|(_, b)| b.active_connections)
                    .min()
                    .unwrap_or(0);

                // Get all backends with minimum connections
                let min_backends: Vec<_> = candidates
                    .iter()
                    .filter(|(_, b)| b.active_connections == min_conns)
                    .collect();

                // Round-robin among tied backends
                if min_backends.len() > 1 {
                    let counter = self.rr_counter.fetch_add(1, Ordering::Relaxed);
                    min_backends[counter % min_backends.len()].0
                } else {
                    min_backends.first().map(|(i, _)| *i).unwrap_or(0)
                }
            }
            LoadBalanceMethod::Weighted => {
                let indices = self.weighted_indices.read().await;
                if indices.is_empty() {
                    return None;
                }
                let counter = self.weighted_counter.fetch_add(1, Ordering::Relaxed);
                let idx = indices[counter % indices.len()];
                // Make sure the selected index is healthy
                if backends.get(idx).map(|b| b.healthy).unwrap_or(false) {
                    idx
                } else {
                    // Fallback to round-robin among healthy
                    candidates[counter % candidates.len()].0
                }
            }
        };

        backends.get(selected_idx).map(|b| b.config.address.clone())
    }

    /// Increment active connections for a backend
    pub async fn increment_connections(&self, address: &str) {
        let mut backends = self.backends.write().await;
        if let Some(backend) = backends.iter_mut().find(|b| b.config.address == address) {
            backend.active_connections += 1;
        }
    }

    /// Decrement active connections and record request
    pub async fn decrement_connections(&self, address: &str, response_time_ms: Option<u64>) {
        let mut backends = self.backends.write().await;
        if let Some(backend) = backends.iter_mut().find(|b| b.config.address == address) {
            backend.active_connections = backend.active_connections.saturating_sub(1);
            backend.total_requests += 1;
            if let Some(time) = response_time_ms {
                backend.last_response_time_ms = Some(time);
            }
        }
    }

    /// Mark a backend as unhealthy
    pub async fn mark_unhealthy(&self, address: &str) {
        let mut backends = self.backends.write().await;
        if let Some(backend) = backends.iter_mut().find(|b| b.config.address == address) {
            backend.healthy = false;
        }
    }

    /// Mark a backend as healthy
    pub async fn mark_healthy(&self, address: &str) {
        let mut backends = self.backends.write().await;
        if let Some(backend) = backends.iter_mut().find(|b| b.config.address == address) {
            backend.healthy = true;
        }
    }

    /// Perform health checks on all backends
    pub async fn health_check(&self) -> Vec<HealthCheckResult> {
        let backends = self.backends.read().await;
        let addresses: Vec<String> = backends.iter().map(|b| b.config.address.clone()).collect();
        drop(backends);

        let mut results = Vec::new();

        for address in addresses {
            let url = format!("http://{}{}", address, self.health_check_path);
            let start = Instant::now();

            let result = match self.client.get(&url).send().await {
                Ok(response) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    let healthy = response.status().is_success();

                    if healthy {
                        self.mark_healthy(&address).await;
                    } else {
                        self.mark_unhealthy(&address).await;
                    }

                    HealthCheckResult {
                        address: address.clone(),
                        healthy,
                        response_time_ms: Some(elapsed),
                        error: None,
                    }
                }
                Err(e) => {
                    self.mark_unhealthy(&address).await;
                    HealthCheckResult {
                        address: address.clone(),
                        healthy: false,
                        response_time_ms: None,
                        error: Some(e.to_string()),
                    }
                }
            };

            // Update last health check time
            let mut backends = self.backends.write().await;
            if let Some(backend) = backends.iter_mut().find(|b| b.config.address == address) {
                backend.last_health_check = Some(Instant::now());
            }

            results.push(result);
        }

        results
    }

    /// Proxy a request to a backend
    pub async fn proxy_request(
        &self,
        method: &str,
        path: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<ProxyResponse, ProxyError> {
        let backend = self
            .select_backend()
            .await
            .ok_or(ProxyError::NoHealthyBackend)?;

        let url = format!("http://{}{}", backend, path);
        self.increment_connections(&backend).await;

        let start = Instant::now();

        let mut request = match method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "DELETE" => self.client.delete(&url),
            "PATCH" => self.client.patch(&url),
            "HEAD" => self.client.head(&url),
            _ => return Err(ProxyError::UnsupportedMethod(method.to_string())),
        };

        for (key, value) in headers {
            request = request.header(&key, &value);
        }

        if !body.is_empty() {
            request = request.body(body);
        }

        let result = request.send().await;
        let elapsed = start.elapsed().as_millis() as u64;
        self.decrement_connections(&backend, Some(elapsed)).await;

        match result {
            Ok(response) => {
                let status = response.status().as_u16();
                let headers: Vec<(String, String)> = response
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let body = response.bytes().await.unwrap_or_default().to_vec();

                Ok(ProxyResponse {
                    status,
                    headers,
                    body,
                    backend_address: backend,
                    response_time_ms: elapsed,
                })
            }
            Err(e) => {
                // Mark backend as unhealthy on connection errors
                if e.is_connect() || e.is_timeout() {
                    self.mark_unhealthy(&backend).await;
                }
                Err(ProxyError::RequestFailed(e.to_string()))
            }
        }
    }

    /// Get stats for all backends
    pub async fn stats(&self) -> Vec<BackendStats> {
        let backends = self.backends.read().await;
        backends
            .iter()
            .map(|b| BackendStats {
                address: b.config.address.clone(),
                healthy: b.healthy,
                active_connections: b.active_connections,
                total_requests: b.total_requests,
                weight: b.config.weight,
                backup: b.config.backup,
            })
            .collect()
    }

    /// Start background health check task
    pub fn start_health_check_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let lb = self.clone();
        let interval = self.health_check_interval;

        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                let results = lb.health_check().await;
                for result in results {
                    if !result.healthy {
                        eprintln!(
                            "[health] Backend {} unhealthy: {:?}",
                            result.address, result.error
                        );
                    }
                }
            }
        })
    }
}

/// Result of a health check
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub address: String,
    pub healthy: bool,
    pub response_time_ms: Option<u64>,
    pub error: Option<String>,
}

/// Proxy response
#[derive(Debug)]
pub struct ProxyResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub backend_address: String,
    pub response_time_ms: u64,
}

/// Proxy error
#[derive(Debug)]
pub enum ProxyError {
    NoHealthyBackend,
    UnsupportedMethod(String),
    RequestFailed(String),
}

impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyError::NoHealthyBackend => write!(f, "No healthy backend available"),
            ProxyError::UnsupportedMethod(m) => write!(f, "Unsupported HTTP method: {}", m),
            ProxyError::RequestFailed(e) => write!(f, "Request failed: {}", e),
        }
    }
}

impl std::error::Error for ProxyError {}

/// Backend statistics
#[derive(Debug, Clone)]
pub struct BackendStats {
    pub address: String,
    pub healthy: bool,
    pub active_connections: u64,
    pub total_requests: u64,
    pub weight: u32,
    pub backup: bool,
}

/// Load balancer manager for multiple upstream pools
pub struct LoadBalancerManager {
    pools: HashMap<String, Arc<LoadBalancer>>,
}

impl LoadBalancerManager {
    /// Create a new manager from config
    pub fn new(upstreams: &[UpstreamConfig]) -> Self {
        let mut pools = HashMap::new();
        for upstream in upstreams {
            let lb = Arc::new(LoadBalancer::new(upstream));
            pools.insert(upstream.name.clone(), lb);
        }
        Self { pools }
    }

    /// Get a load balancer by name
    pub fn get(&self, name: &str) -> Option<Arc<LoadBalancer>> {
        self.pools.get(name).cloned()
    }

    /// Start health check tasks for all pools
    pub fn start_health_checks(&self) -> Vec<tokio::task::JoinHandle<()>> {
        self.pools
            .values()
            .map(|lb| lb.clone().start_health_check_task())
            .collect()
    }

    /// Get all pool names
    pub fn pool_names(&self) -> Vec<String> {
        self.pools.keys().cloned().collect()
    }
}
