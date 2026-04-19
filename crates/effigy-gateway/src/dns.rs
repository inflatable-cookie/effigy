//! DNS resolver for local development domains.
//!
//! Responds to A queries for `*.test` (or configured TLD) with `127.0.0.1`.
//! All other queries are either refused or forwarded upstream, depending on
//! configuration.
//!
//! Uses `hickory-server` for the DNS protocol implementation. The resolver
//! runs as a UDP server on a configurable port (default 15353).

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use hickory_proto::op::{OpCode, ResponseCode};
use hickory_proto::rr::rdata::A;
use hickory_proto::rr::{RData, Record, RecordType};
use tokio::net::UdpSocket;
use tracing::{debug, error, warn};

use crate::routes::RouteTable;
use crate::stats::GatewayStats;

/// Configuration for the DNS resolver.
#[derive(Debug, Clone)]
pub struct DnsConfig {
    /// Address to bind the DNS UDP server (e.g., "127.0.0.1:15353").
    pub bind_addr: SocketAddr,

    /// The TLD to resolve (e.g., "test"). Queries for `*.test` will be
    /// answered. Everything else is refused.
    pub tld: String,

    /// The IP address to return for matching queries.
    pub resolve_to: Ipv4Addr,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 15353)),
            tld: "test".to_string(),
            resolve_to: Ipv4Addr::LOCALHOST,
        }
    }
}

/// Cached result of a route table lookup for a domain.
#[derive(Debug, Clone)]
pub(crate) struct DnsCacheEntry {
    /// Whether the domain has a registered route.
    has_route: bool,
    /// When this entry was cached.
    cached_at: Instant,
}

/// Simple DNS lookup cache to reduce route table lock contention.
///
/// Entries expire after a short TTL (2 seconds by default). This is
/// fast enough that route registration changes are picked up quickly,
/// but long enough to collapse the burst of parallel DNS queries that
/// browsers make for a single page load.
///
/// Shared between the DNS server and the route table file watcher so
/// the watcher can invalidate the cache when routes change.
pub struct DnsCache {
    entries: Mutex<HashMap<String, DnsCacheEntry>>,
    ttl: Duration,
}

impl DnsCache {
    /// Create a new cache with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Look up a domain in the cache. Returns None if not cached or expired.
    pub fn get(&self, domain: &str) -> Option<bool> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(domain)?;
        if entry.cached_at.elapsed() < self.ttl {
            Some(entry.has_route)
        } else {
            None
        }
    }

    /// Store a lookup result in the cache.
    pub fn put(&self, domain: String, has_route: bool) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(
                domain,
                DnsCacheEntry {
                    has_route,
                    cached_at: Instant::now(),
                },
            );

            // Prune expired entries if the cache is getting large.
            if entries.len() > 100 {
                let ttl = self.ttl;
                entries.retain(|_, e| e.cached_at.elapsed() < ttl);
            }
        }
    }

    /// Invalidate the entire cache.
    ///
    /// Called by the route table file watcher when routes change.
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

/// Run the DNS resolver server.
///
/// This function blocks until the provided shutdown signal resolves.
/// It reads the route table to validate that a route actually exists
/// for the queried domain before responding.
pub async fn run_dns_server(
    config: DnsConfig,
    route_table: Arc<RwLock<RouteTable>>,
    stats: Arc<GatewayStats>,
    dns_cache: Arc<DnsCache>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), crate::GatewayError> {
    let socket =
        UdpSocket::bind(config.bind_addr)
            .await
            .map_err(|e| crate::GatewayError::DnsBindError {
                addr: config.bind_addr.to_string(),
                reason: e.to_string(),
            })?;

    debug!(addr = %config.bind_addr, tld = %config.tld, "DNS resolver started");

    let mut buf = vec![0u8; 512];

    loop {
        tokio::select! {
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok((len, src)) => {
                        GatewayStats::inc(&stats.dns_queries);

                        let (response, resolved) = handle_dns_query(
                            &buf[..len],
                            &config,
                            &route_table,
                            &dns_cache,
                        );
                        if resolved {
                            GatewayStats::inc(&stats.dns_resolved);
                        }
                        if let Some(response_bytes) = response {
                            if let Err(e) = socket.send_to(&response_bytes, src).await {
                                warn!(error = %e, "failed to send DNS response");
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "DNS recv error");
                    }
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    debug!("DNS resolver shutting down");
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Handle a single DNS query packet.
///
/// Returns `(response_bytes, was_resolved)` where `was_resolved` is true
/// when the query matched a registered route (not just the TLD).
fn handle_dns_query(
    query_bytes: &[u8],
    config: &DnsConfig,
    route_table: &Arc<RwLock<RouteTable>>,
    cache: &DnsCache,
) -> (Option<Vec<u8>>, bool) {
    use hickory_proto::op::Message;
    use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};

    let request = match Message::from_bytes(query_bytes) {
        Ok(r) => r,
        Err(_) => return (None, false),
    };

    // Only handle standard queries.
    if request.metadata.op_code != OpCode::Query {
        return (None, false);
    }

    let mut response = Message::response(request.metadata.id, OpCode::Query);
    response.metadata.recursion_desired = request.metadata.recursion_desired;
    response.metadata.recursion_available = false;

    let mut answered = false;
    let mut matched_tld = false;
    let mut resolved_route = false;

    for query in &request.queries {
        // We only serve A records. But if the query is for our TLD
        // (even for AAAA), we mark it as matched so we return NoError
        // instead of Refused. This prevents slow dual-stack lookups
        // in browsers that try AAAA first.
        let query_name = query.name().to_string();
        let query_domain = query_name.trim_end_matches('.').to_lowercase();
        let tld_check = &config.tld;
        if query_domain.ends_with(&format!(".{tld_check}")) || query_domain == *tld_check {
            matched_tld = true;
        }

        if query.query_type() != RecordType::A {
            continue;
        }

        let name = query.name();

        // Normalize: convert FQDN to bare domain (strip trailing dot).
        let domain = name.to_string();
        let domain = domain.trim_end_matches('.').to_lowercase();

        // Check if this domain ends with our TLD.
        let tld = &config.tld;
        let matches_tld = domain.ends_with(&format!(".{tld}")) || domain == *tld;

        if !matches_tld {
            continue;
        }

        // Check if we have a route for this domain (cache-first).
        let has_route = if let Some(cached) = cache.get(&domain) {
            cached
        } else {
            let result = route_table
                .read()
                .expect("route table lock poisoned")
                .lookup(&domain)
                .is_some();
            cache.put(domain.clone(), result);
            result
        };

        if has_route {
            debug!(domain = %domain, "DNS: resolving to {}", config.resolve_to);

            let record = Record::from_rdata(name.clone(), 60, RData::A(A(config.resolve_to)));
            response.add_answer(record);
            answered = true;
            resolved_route = true;
        } else {
            // Domain matches TLD but no route registered.
            // Still resolve to localhost — the proxy will return a
            // helpful error page.
            debug!(domain = %domain, "DNS: resolving (no route, will proxy to error)");

            let record = Record::from_rdata(
                name.clone(),
                10, // Short TTL for unregistered domains.
                RData::A(A(config.resolve_to)),
            );
            response.add_answer(record);
            answered = true;
        }
    }

    if answered || matched_tld {
        // Either we have an answer, or the query was for our TLD (even
        // if we don't have A records for it, e.g., AAAA queries).
        // Return NoError so browsers don't retry with different strategies.
        response.metadata.response_code = ResponseCode::NoError;
    } else {
        // Not our TLD — refuse.
        response.metadata.response_code = ResponseCode::Refused;
    }

    // Copy the original questions into the response.
    response.add_queries(request.queries.iter().cloned());

    (response.to_bytes().ok(), resolved_route)
}

#[cfg(test)]
#[path = "dns/tests.rs"]
mod tests;
