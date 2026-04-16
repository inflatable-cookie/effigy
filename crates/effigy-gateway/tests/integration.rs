//! Integration tests for the gateway.
//!
//! These tests start real DNS and proxy servers on ephemeral ports and
//! verify end-to-end behavior.

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use effigy_gateway::dns::{run_dns_server, DnsConfig};
use effigy_gateway::proxy::{run_proxy_server, ProxyConfig};
use effigy_gateway::routes::{Route, RouteSource, RouteTable};
use effigy_gateway::stats::GatewayStats;

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
    let dns_handle = tokio::spawn(run_dns_server(config, shared, Arc::new(GatewayStats::new()), shutdown_rx));

    // Give the server a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Send a DNS query.
    use hickory_proto::op::{Message, MessageType, OpCode, Query};
    use hickory_proto::rr::{Name, RData, RecordType};
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
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 0,
    };

    // Start proxy.
    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
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
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 0,
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
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
    assert!(
        body.contains("unknown.test"),
        "body should mention domain: {body}"
    );

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

// --- Gateway internal endpoints ──────────────────────────────────────

#[tokio::test]
async fn gateway_health_endpoint() {
    let proxy_port = available_port().await;
    let shared = Arc::new(RwLock::new(RouteTable::new()));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 0,
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{proxy_port}/_effigy/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

#[tokio::test]
async fn gateway_routes_endpoint() {
    let proxy_port = available_port().await;

    let mut table = RouteTable::new();
    table.upsert(test_route("app.test", "127.0.0.1:8080"));
    table.upsert(test_route("api.test", "127.0.0.1:3000"));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 0,
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{proxy_port}/_effigy/routes"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "application/json"
    );

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["count"], 2);
    let routes = body["routes"].as_array().unwrap();
    let domains: Vec<&str> = routes
        .iter()
        .map(|r| r["domain"].as_str().unwrap())
        .collect();
    assert!(domains.contains(&"app.test"));
    assert!(domains.contains(&"api.test"));

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

#[tokio::test]
async fn gateway_unknown_endpoint_returns_404() {
    let proxy_port = available_port().await;
    let shared = Arc::new(RwLock::new(RouteTable::new()));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 0,
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{proxy_port}/_effigy/nonexistent"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

// --- Proxy: error handling ───────────────────────────────────────────

#[tokio::test]
async fn proxy_returns_bad_gateway_for_unreachable_upstream() {
    let proxy_port = available_port().await;
    // Use a port that nothing is listening on.
    let dead_port = available_port().await;

    let mut table = RouteTable::new();
    table.upsert(test_route("dead.test", &format!("127.0.0.1:{dead_port}")));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(1),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 0,
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
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

#[tokio::test]
async fn proxy_response_timeout_returns_bad_gateway() {
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper_util::rt::TokioIo;

    let upstream_port = available_port().await;
    let proxy_port = available_port().await;

    // Start a server that accepts connections but never responds.
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{upstream_port}"))
        .await
        .unwrap();
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let io = TokioIo::new(stream);
            let service = service_fn(|_req: hyper::Request<hyper::body::Incoming>| async move {
                // Hang forever.
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                Ok::<_, hyper::Error>(hyper::Response::new(http_body_util::Full::new(
                    hyper::body::Bytes::new(),
                )))
            });
            let _ = http1::Builder::new().serve_connection(io, service).await;
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut table = RouteTable::new();
    table.upsert(test_route(
        "slow.test",
        &format!("127.0.0.1:{upstream_port}"),
    ));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        // Very short response timeout to test quickly.
        response_timeout: std::time::Duration::from_millis(200),
        max_request_body: 0,
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let response = client
        .get(format!("http://127.0.0.1:{proxy_port}/"))
        .header("host", "slow.test")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 502);
    let body = response.text().await.unwrap();
    assert!(body.contains("timeout"), "should mention timeout: {body}");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

#[tokio::test]
async fn proxy_rejects_oversized_request_body() {
    let proxy_port = available_port().await;

    let mut table = RouteTable::new();
    table.upsert(test_route("myapp.test", "127.0.0.1:9999"));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 1024, // 1KB limit
    };

    let proxy_handle = tokio::spawn(run_proxy_server(
        proxy_config,
        shared,
        Arc::new(GatewayStats::new()),
        shutdown_rx,
    ));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{proxy_port}/upload"))
        .header("host", "myapp.test")
        .header("content-length", "10000")
        .body("x".repeat(10000))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 413);
    let body = response.text().await.unwrap();
    assert!(body.contains("too large"), "body: {body}");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

// --- Proxy: POST body forwarding ─────────────────────────────────────

/// Start an echo server that returns the request body back.
async fn start_body_echo_server(port: u16) -> tokio::task::JoinHandle<()> {
    use http_body_util::{BodyExt, Full};
    use hyper::body::Bytes;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .unwrap();

    tokio::spawn(async move {
        for _ in 0..10 {
            if let Ok((stream, _)) = listener.accept().await {
                let io = TokioIo::new(stream);
                tokio::spawn(async move {
                    let service = service_fn(|req: Request<hyper::body::Incoming>| async move {
                        let method = req.method().to_string();
                        let content_type = req
                            .headers()
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("none")
                            .to_string();

                        let body_bytes = req.into_body().collect().await.unwrap().to_bytes();
                        let body_len = body_bytes.len();
                        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

                        let response_body = format!(
                            "method={method}\n\
                                 content-type={content_type}\n\
                                 body-length={body_len}\n\
                                 body={body_str}\n"
                        );

                        Ok::<_, hyper::Error>(Response::new(Full::new(Bytes::from(response_body))))
                    });
                    let _ = http1::Builder::new().serve_connection(io, service).await;
                });
            }
        }
    })
}

#[tokio::test]
async fn proxy_forwards_post_body() {
    let upstream_port = available_port().await;
    let proxy_port = available_port().await;

    let _echo = start_body_echo_server(upstream_port).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut table = RouteTable::new();
    table.upsert(test_route(
        "myapp.test",
        &format!("127.0.0.1:{upstream_port}"),
    ));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 0,
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{proxy_port}/api/data"))
        .header("host", "myapp.test")
        .header("content-type", "application/json")
        .body(r#"{"name":"test","value":42}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("method=POST"), "body: {body}");
    assert!(
        body.contains("content-type=application/json"),
        "body: {body}"
    );
    assert!(body.contains("body-length=26"), "body: {body}");
    assert!(
        body.contains(r#"body={"name":"test","value":42}"#),
        "body: {body}"
    );

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

#[tokio::test]
async fn proxy_forwards_large_body() {
    let upstream_port = available_port().await;
    let proxy_port = available_port().await;

    let _echo = start_body_echo_server(upstream_port).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut table = RouteTable::new();
    table.upsert(test_route(
        "myapp.test",
        &format!("127.0.0.1:{upstream_port}"),
    ));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 0,
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 100KB body.
    let large_body = "x".repeat(100_000);
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{proxy_port}/upload"))
        .header("host", "myapp.test")
        .body(large_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(
        body.contains("body-length=100000"),
        "should forward full 100KB body: {body}"
    );

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

#[tokio::test]
async fn proxy_multiple_concurrent_requests() {
    let upstream_port = available_port().await;
    let proxy_port = available_port().await;

    let _echo = start_echo_server(upstream_port).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut table = RouteTable::new();
    table.upsert(test_route(
        "myapp.test",
        &format!("127.0.0.1:{upstream_port}"),
    ));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 0,
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 5 concurrent requests.
    let client = reqwest::Client::new();
    let mut handles = Vec::new();
    for i in 0..5 {
        let client = client.clone();
        let port = proxy_port;
        handles.push(tokio::spawn(async move {
            let response = client
                .get(format!("http://127.0.0.1:{port}/request-{i}"))
                .header("host", "myapp.test")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert!(
                body.contains(&format!("uri=/request-{i}")),
                "request {i}: {body}"
            );
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}

#[tokio::test]
async fn proxy_preserves_response_status_and_headers() {
    use http_body_util::Full;
    use hyper::body::Bytes;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::Response;
    use hyper_util::rt::TokioIo;

    let upstream_port = available_port().await;
    let proxy_port = available_port().await;

    // Server that returns custom status + headers.
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{upstream_port}"))
        .await
        .unwrap();
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let io = TokioIo::new(stream);
            let service = service_fn(|_req: hyper::Request<hyper::body::Incoming>| async move {
                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(201)
                        .header("x-custom-response", "hello")
                        .header("content-type", "application/json")
                        .body(Full::new(Bytes::from(r#"{"ok":true}"#)))
                        .unwrap(),
                )
            });
            let _ = http1::Builder::new().serve_connection(io, service).await;
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut table = RouteTable::new();
    table.upsert(test_route(
        "myapp.test",
        &format!("127.0.0.1:{upstream_port}"),
    ));
    let shared = Arc::new(RwLock::new(table));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let proxy_config = ProxyConfig {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        tls_bind_addr: None,
        connect_timeout: std::time::Duration::from_secs(5),
        response_timeout: std::time::Duration::from_secs(30),
        max_request_body: 0,
    };

    let proxy_handle = tokio::spawn(run_proxy_server(proxy_config, shared, Arc::new(GatewayStats::new()), shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{proxy_port}/"))
        .header("host", "myapp.test")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 201);
    assert_eq!(
        response
            .headers()
            .get("x-custom-response")
            .unwrap()
            .to_str()
            .unwrap(),
        "hello"
    );
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "application/json"
    );
    let body = response.text().await.unwrap();
    assert_eq!(body, r#"{"ok":true}"#);

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), proxy_handle).await;
}
