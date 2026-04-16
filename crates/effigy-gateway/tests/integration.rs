//! Integration tests for the gateway.
//!
//! These tests start real DNS and proxy servers on ephemeral ports and
//! verify end-to-end behavior.

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use effigy_gateway::dns::{run_dns_server, DnsConfig};
use effigy_gateway::proxy::{run_proxy_server, ProxyConfig};
use effigy_gateway::routes::{Route, RouteSource, RouteTable};

use chrono::Utc;
use tokio::sync::watch;

/// Find an available port by binding to 0 and reading what the OS assigned.
async fn available_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Find an available UDP port.
async fn available_udp_port() -> u16 {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket.local_addr().unwrap().port()
}

fn test_route(domain: &str, target: &str) -> Route {
    Route {
        domain: domain.to_string(),
        target: target.to_string(),
        source: RouteSource::Container,
        project: "/tmp/test".to_string(),
        tls: false,
        registered: Utc::now(),
    }
}

/// Start a minimal HTTP server on a given port that echoes back request info.
async fn start_echo_server(port: u16) -> tokio::task::JoinHandle<()> {
    use http_body_util::Full;
    use hyper::body::Bytes;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .unwrap();

    tokio::spawn(async move {
        // Accept just a few connections (enough for tests).
        for _ in 0..10 {
            if let Ok((stream, _)) = listener.accept().await {
                let io = TokioIo::new(stream);
                tokio::spawn(async move {
                    let service = service_fn(|req: Request<hyper::body::Incoming>| async move {
                        let method = req.method().to_string();
                        let uri = req.uri().to_string();
                        let host = req
                            .headers()
                            .get("host")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("unknown")
                            .to_string();
                        let forwarded_for = req
                            .headers()
                            .get("x-forwarded-for")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("none")
                            .to_string();
                        let forwarded_host = req
                            .headers()
                            .get("x-forwarded-host")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("none")
                            .to_string();

                        let body = format!(
                            "method={method}\nuri={uri}\nhost={host}\n\
                             x-forwarded-for={forwarded_for}\n\
                             x-forwarded-host={forwarded_host}\n"
                        );

                        Ok::<_, hyper::Error>(Response::new(Full::new(Bytes::from(body))))
                    });

                    let _ = http1::Builder::new().serve_connection(io, service).await;
                });
            }
        }
    })
}

// --- DNS integration tests ---

#[tokio::test]
async fn dns_resolves_registered_domain_end_to_end() {
    let dns_port = available_udp_port().await;
    let config = DnsConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], dns_port)),
        tld: "test".to_string(),
        resolve_to: std::net::Ipv4Addr::LOCALHOST,
    };

    let mut table = RouteTable::new();
    table.upsert(test_route("myapp.test", "127.0.0.1:8080"));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Start DNS server.
    let dns_handle = tokio::spawn(run_dns_server(config, shared, shutdown_rx));

    // Give the server a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Send a DNS query.
    use hickory_proto::op::{Message, MessageType, OpCode, Query};
    use hickory_proto::rr::{Name, RecordType, RData};
    use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
    use std::str::FromStr;

    let mut query_msg = Message::new();
    query_msg.set_id(42);
    query_msg.set_message_type(MessageType::Query);
    query_msg.set_op_code(OpCode::Query);
    query_msg.set_recursion_desired(true);
    query_msg.add_query(Query::query(
        Name::from_str("myapp.test.").unwrap(),
        RecordType::A,
    ));

    let query_bytes = query_msg.to_bytes().unwrap();

    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(&query_bytes, format!("127.0.0.1:{dns_port}"))
        .await
        .unwrap();

    let mut buf = vec![0u8; 512];
    let (len, _) = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        socket.recv_from(&mut buf),
    )
    .await
    .expect("DNS response timeout")
    .unwrap();

    let response = Message::from_bytes(&buf[..len]).unwrap();
    assert_eq!(response.id(), 42);
    assert_eq!(response.answers().len(), 1);
    match response.answers()[0].data() {
        RData::A(a) => assert_eq!(a.0, std::net::Ipv4Addr::LOCALHOST),
        other => panic!("expected A record, got {other:?}"),
    }

    // Shutdown.
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), dns_handle).await;
}

// --- Proxy integration tests ---

#[tokio::test]
async fn proxy_routes_to_correct_upstream() {
    let upstream_port = available_port().await;
    let proxy_port = available_port().await;

    // Start echo server as the upstream.
    let _echo = start_echo_server(upstream_port).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Set up route table.
    let mut table = RouteTable::new();
    table.upsert(test_route(
        "myapp.test",
        &format!("127.0.0.1:{upstream_port}"),
    ));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        connect_timeout: std::time::Duration::from_secs(5),
    };

    // Start proxy.
    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Make a request through the proxy.
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{proxy_port}/test-path"))
        .header("host", "myapp.test")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("method=GET"), "body: {body}");
    assert!(body.contains("uri=/test-path"), "body: {body}");
    assert!(body.contains("x-forwarded-host=myapp.test"), "body: {body}");
    assert!(body.contains("x-forwarded-for=127.0.0.1"), "body: {body}");

    // Shutdown.
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

#[tokio::test]
async fn proxy_returns_no_route_page() {
    let proxy_port = available_port().await;

    let shared = Arc::new(RwLock::new(RouteTable::new()));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        connect_timeout: std::time::Duration::from_secs(5),
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{proxy_port}/"))
        .header("host", "unknown.test")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 503);
    assert!(
        response.headers().contains_key("x-effigy-gateway"),
        "response should have x-effigy-gateway header"
    );
    let body = response.text().await.unwrap();
    assert!(body.contains("unknown.test"), "body should mention domain: {body}");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

#[tokio::test]
async fn proxy_returns_bad_gateway_for_unreachable_upstream() {
    let proxy_port = available_port().await;
    // Use a port that nothing is listening on.
    let dead_port = available_port().await;

    let mut table = RouteTable::new();
    table.upsert(test_route(
        "dead.test",
        &format!("127.0.0.1:{dead_port}"),
    ));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        connect_timeout: std::time::Duration::from_secs(1),
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{proxy_port}/"))
        .header("host", "dead.test")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 502);
    let body = response.text().await.unwrap();
    assert!(body.contains("Failed to connect"), "body: {body}");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}
