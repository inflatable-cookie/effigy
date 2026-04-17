//! Gateway runtime statistics.
//!
//! Tracks request counts, DNS query counts, and uptime for the gateway
//! status API and TUI display.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Thread-safe gateway statistics using atomic counters.
///
/// Shared across all async tasks via `Arc<GatewayStats>`.
pub struct GatewayStats {
    /// When the gateway started.
    started_at: Instant,

    /// Total HTTP requests handled (including errors).
    pub http_requests: AtomicU64,

    /// Total requests forwarded to upstream.
    pub proxied_requests: AtomicU64,

    /// Total requests that hit the no-route error page.
    pub no_route_requests: AtomicU64,

    /// Total upstream errors (connect failures, timeouts, etc.).
    pub upstream_errors: AtomicU64,

    /// Total DNS queries received.
    pub dns_queries: AtomicU64,

    /// Total DNS queries that matched a registered route.
    pub dns_resolved: AtomicU64,

    /// Total WebSocket upgrades initiated.
    pub websocket_upgrades: AtomicU64,

    /// Total requests rejected for body size limit.
    pub body_too_large: AtomicU64,
}

impl GatewayStats {
    /// Create a new stats tracker.
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            http_requests: AtomicU64::new(0),
            proxied_requests: AtomicU64::new(0),
            no_route_requests: AtomicU64::new(0),
            upstream_errors: AtomicU64::new(0),
            dns_queries: AtomicU64::new(0),
            dns_resolved: AtomicU64::new(0),
            websocket_upgrades: AtomicU64::new(0),
            body_too_large: AtomicU64::new(0),
        }
    }

    /// Uptime in seconds.
    pub fn uptime_secs(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    /// Serialize stats to a JSON value.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "uptime_secs": self.uptime_secs(),
            "http_requests": self.http_requests.load(Ordering::Relaxed),
            "proxied_requests": self.proxied_requests.load(Ordering::Relaxed),
            "no_route_requests": self.no_route_requests.load(Ordering::Relaxed),
            "upstream_errors": self.upstream_errors.load(Ordering::Relaxed),
            "dns_queries": self.dns_queries.load(Ordering::Relaxed),
            "dns_resolved": self.dns_resolved.load(Ordering::Relaxed),
            "websocket_upgrades": self.websocket_upgrades.load(Ordering::Relaxed),
            "body_too_large": self.body_too_large.load(Ordering::Relaxed),
        })
    }

    /// Increment a counter by 1.
    pub fn inc(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for GatewayStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "stats/tests.rs"]
mod tests;
