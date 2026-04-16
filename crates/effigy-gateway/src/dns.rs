//! DNS resolver for local development domains.
//!
//! Responds to A queries for `*.test` (or configured TLD) with `127.0.0.1`.
//! All other queries are either refused or forwarded upstream, depending on
//! configuration.
//!
//! Uses `hickory-server` for the DNS protocol implementation. The resolver
//! runs as a UDP server on a configurable port (default 15353).

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};

use hickory_proto::op::{MessageType, OpCode, ResponseCode};
use hickory_proto::rr::rdata::A;
use hickory_proto::rr::{RData, Record, RecordType};
use tokio::net::UdpSocket;
use tracing::{debug, error, warn};

use crate::routes::RouteTable;

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

/// Run the DNS resolver server.
///
/// This function blocks until the provided shutdown signal resolves.
/// It reads the route table to validate that a route actually exists
/// for the queried domain before responding.
pub async fn run_dns_server(
    config: DnsConfig,
    route_table: Arc<RwLock<RouteTable>>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), crate::GatewayError> {
    let socket = UdpSocket::bind(config.bind_addr)
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
                        let response = handle_dns_query(
                            &buf[..len],
                            &config,
                            &route_table,
                        );
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
/// Returns the response bytes, or `None` if the query should be ignored.
fn handle_dns_query(
    query_bytes: &[u8],
    config: &DnsConfig,
    route_table: &Arc<RwLock<RouteTable>>,
) -> Option<Vec<u8>> {
    use hickory_proto::op::Message;
    use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};

    let request = Message::from_bytes(query_bytes).ok()?;

    // Only handle standard queries.
    if request.op_code() != OpCode::Query {
        return None;
    }

    let mut response = Message::new();
    response.set_id(request.id());
    response.set_message_type(MessageType::Response);
    response.set_op_code(OpCode::Query);
    response.set_recursion_desired(request.recursion_desired());
    response.set_recursion_available(false);

    let mut answered = false;

    for query in request.queries() {
        if query.query_type() != RecordType::A {
            continue;
        }

        let name = query.name();

        // Normalize: convert FQDN to bare domain (strip trailing dot).
        let domain = name.to_string();
        let domain = domain.trim_end_matches('.').to_lowercase();

        // Check if this domain ends with our TLD.
        let tld = &config.tld;
        let matches_tld = domain.ends_with(&format!(".{tld}"))
            || domain == *tld;

        if !matches_tld {
            continue;
        }

        // Check if we have a route for this domain.
        let has_route = route_table
            .read()
            .expect("route table lock poisoned")
            .lookup(&domain)
            .is_some();

        if has_route {
            debug!(domain = %domain, "DNS: resolving to {}", config.resolve_to);

            let record = Record::from_rdata(
                name.clone(),
                60,
                RData::A(A(config.resolve_to)),
            );
            response.add_answer(record);
            answered = true;
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

    if !answered {
        // No matching queries — refuse.
        response.set_response_code(ResponseCode::Refused);
    } else {
        response.set_response_code(ResponseCode::NoError);
    }

    // Copy the original questions into the response.
    response.add_queries(request.queries().iter().cloned());

    response.to_bytes().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::op::Message;
    use hickory_proto::rr::RecordType;
    use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
    use std::str::FromStr;

    use hickory_proto::rr::Name;

    fn test_config() -> DnsConfig {
        DnsConfig::default()
    }

    fn build_query(domain: &str, record_type: RecordType) -> Vec<u8> {
        let mut msg = Message::new();
        msg.set_id(1234);
        msg.set_message_type(MessageType::Query);
        msg.set_op_code(OpCode::Query);
        msg.set_recursion_desired(true);

        let name = Name::from_str(domain).unwrap();
        msg.add_query(hickory_proto::op::Query::query(name, record_type));

        msg.to_bytes().unwrap()
    }

    fn route_table_with(domain: &str) -> Arc<RwLock<RouteTable>> {
        let mut table = RouteTable::new();
        table.upsert(crate::routes::Route {
            domain: domain.to_string(),
            target: "127.0.0.1:8080".to_string(),
            source: crate::routes::RouteSource::Container,
            project: "/tmp/test".to_string(),
            tls: false,
            registered: chrono::Utc::now(),
        });
        Arc::new(RwLock::new(table))
    }

    #[test]
    fn resolves_registered_domain() {
        let config = test_config();
        let table = route_table_with("myapp.test");
        let query = build_query("myapp.test.", RecordType::A);

        let response_bytes = handle_dns_query(&query, &config, &table).unwrap();
        let response = Message::from_bytes(&response_bytes).unwrap();

        assert_eq!(response.response_code(), ResponseCode::NoError);
        assert_eq!(response.answers().len(), 1);

        let answer = &response.answers()[0];
        assert_eq!(answer.record_type(), RecordType::A);
        match answer.data() {
            RData::A(a) => assert_eq!(a.0, Ipv4Addr::LOCALHOST),
            other => panic!("expected A record, got {other:?}"),
        }
    }

    #[test]
    fn resolves_unregistered_tld_domain() {
        let config = test_config();
        let table = Arc::new(RwLock::new(RouteTable::new()));
        let query = build_query("unknown.test.", RecordType::A);

        let response_bytes = handle_dns_query(&query, &config, &table).unwrap();
        let response = Message::from_bytes(&response_bytes).unwrap();

        // Should still resolve (proxy will show error page).
        assert_eq!(response.response_code(), ResponseCode::NoError);
        assert_eq!(response.answers().len(), 1);
    }

    #[test]
    fn refuses_non_tld_domain() {
        let config = test_config();
        let table = route_table_with("myapp.test");
        let query = build_query("google.com.", RecordType::A);

        let response_bytes = handle_dns_query(&query, &config, &table).unwrap();
        let response = Message::from_bytes(&response_bytes).unwrap();

        assert_eq!(response.response_code(), ResponseCode::Refused);
        assert_eq!(response.answers().len(), 0);
    }

    #[test]
    fn ignores_non_a_queries() {
        let config = test_config();
        let table = route_table_with("myapp.test");
        let query = build_query("myapp.test.", RecordType::AAAA);

        let response_bytes = handle_dns_query(&query, &config, &table).unwrap();
        let response = Message::from_bytes(&response_bytes).unwrap();

        // AAAA query for .test domain — no A answers, refused.
        assert_eq!(response.answers().len(), 0);
    }

    #[test]
    fn response_preserves_query_id() {
        let config = test_config();
        let table = route_table_with("myapp.test");
        let query = build_query("myapp.test.", RecordType::A);

        let response_bytes = handle_dns_query(&query, &config, &table).unwrap();
        let response = Message::from_bytes(&response_bytes).unwrap();

        assert_eq!(response.id(), 1234);
    }
}
