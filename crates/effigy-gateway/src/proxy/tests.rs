use super::*;
use crate::routes::{Route, RouteSource};
use chrono::Utc;

/// The route key a request names: its `Host` header without the port, folded
/// to lower case, and nothing when the request carries no usable header.
#[test]
fn extract_host_takes_the_lowercased_name_without_its_port() {
    let cases = [
        ("myapp.test:8080", Some("myapp.test")),
        ("MyApp.TEST", Some("myapp.test")),
        ("myapp.test", Some("myapp.test")),
        ("API.myapp.test:443", Some("api.myapp.test")),
    ];
    for (host, expected) in cases {
        let req = Request::builder()
            .header("host", host)
            .body(Empty::<Bytes>::new())
            .unwrap();
        assert_eq!(extract_host(&req).as_deref(), expected, "header {host}");
    }
    let req = Request::builder().body(Empty::<Bytes>::new()).unwrap();
    assert_eq!(extract_host(&req), None);
}

#[test]
fn hop_by_hop_headers_stripped() {
    let mut headers = hyper::HeaderMap::new();
    headers.insert("connection", HeaderValue::from_static("keep-alive"));
    headers.insert("keep-alive", HeaderValue::from_static("timeout=5"));
    headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));
    headers.insert("content-type", HeaderValue::from_static("text/html"));
    headers.insert("x-custom", HeaderValue::from_static("preserved"));

    strip_hop_by_hop_headers(&mut headers);

    assert!(!headers.contains_key("connection"));
    assert!(!headers.contains_key("keep-alive"));
    assert!(!headers.contains_key("transfer-encoding"));
    assert!(headers.contains_key("content-type"));
    assert!(headers.contains_key("x-custom"));
}

#[test]
fn connection_listed_headers_stripped() {
    let mut headers = hyper::HeaderMap::new();
    headers.insert(
        "connection",
        HeaderValue::from_static("x-custom-hop, keep-alive"),
    );
    headers.insert("x-custom-hop", HeaderValue::from_static("value"));
    headers.insert("x-preserved", HeaderValue::from_static("value"));

    strip_hop_by_hop_headers(&mut headers);

    assert!(!headers.contains_key("x-custom-hop"));
    assert!(headers.contains_key("x-preserved"));
}

#[test]
fn forwarding_headers_record_peer_proto_and_append() {
    let peer: SocketAddr = "192.168.1.100:54321".parse().unwrap();

    let mut http = hyper::HeaderMap::new();
    add_forwarding_headers(&mut http, "myapp.test", peer, false);
    assert_eq!(
        http.get("x-forwarded-for").unwrap().to_str().unwrap(),
        "192.168.1.100"
    );
    assert_eq!(
        http.get("x-forwarded-host").unwrap().to_str().unwrap(),
        "myapp.test"
    );
    assert_eq!(
        http.get("x-forwarded-proto").unwrap().to_str().unwrap(),
        "http"
    );
    assert_eq!(
        http.get("x-real-ip").unwrap().to_str().unwrap(),
        "192.168.1.100"
    );

    let mut https = hyper::HeaderMap::new();
    add_forwarding_headers(&mut https, "myapp.test", peer, true);
    assert_eq!(
        https.get("x-forwarded-proto").unwrap().to_str().unwrap(),
        "https"
    );

    let mut chained = hyper::HeaderMap::new();
    chained.insert("x-forwarded-for", HeaderValue::from_static("10.0.0.1"));
    add_forwarding_headers(&mut chained, "myapp.test", peer, false);
    assert_eq!(
        chained.get("x-forwarded-for").unwrap().to_str().unwrap(),
        "10.0.0.1, 192.168.1.100"
    );
}

#[test]
fn strip_hop_by_hop_except_upgrade_preserves_websocket() {
    let mut headers = hyper::HeaderMap::new();
    headers.insert("connection", HeaderValue::from_static("Upgrade"));
    headers.insert("upgrade", HeaderValue::from_static("websocket"));
    headers.insert("keep-alive", HeaderValue::from_static("timeout=5"));

    strip_hop_by_hop_headers_except_upgrade(&mut headers);

    assert!(headers.contains_key("upgrade"));
    assert_eq!(
        headers.get("connection").unwrap().to_str().unwrap(),
        "upgrade"
    );
    assert!(!headers.contains_key("keep-alive"));
}

#[test]
fn error_response_has_correct_headers() {
    let resp = error_response(StatusCode::BAD_GATEWAY, "test error");
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "text/html; charset=utf-8"
    );
    assert!(resp.headers().contains_key("x-effigy-gateway"));
}

/// The refusal page names the unclaimed domain and the manifest entry that
/// would claim it, so an operator reads what to add instead of a blank 503.
#[test]
fn no_route_response_names_the_domain_and_its_manifest_entry() {
    let body = no_route_body("myapp.test");
    assert!(body.contains("myapp.test"), "body names the domain: {body}");
    assert!(
        body.contains("[containers.web.dns]"),
        "body names the manifest section: {body}"
    );
    let resp = no_route_response("myapp.test");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn http_to_https_redirect_depends_on_request_scheme() {
    let route = Route {
        domain: "myapp.test".to_owned(),
        target: Some("127.0.0.1:41002".to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Container,
        project: "/tmp/project".to_owned(),
        tls: true,
        managed_listener: None,
        registered: Utc::now(),
    };
    let config = ProxyConfig {
        tls_bind_addr: Some("127.0.0.1:443".parse().unwrap()),
        ..ProxyConfig::default()
    };

    let http = Request::builder()
        .uri("/admin?tab=users")
        .body(Empty::<Bytes>::new())
        .unwrap();
    let response =
        maybe_redirect_http_to_https(&http, &route, &config, false).expect("should redirect");
    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
        "https://myapp.test/admin?tab=users"
    );

    let https = Request::builder()
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();
    assert!(maybe_redirect_http_to_https(&https, &route, &config, true).is_none());
}

struct Abort(tokio::task::JoinHandle<()>);
impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Production `handle_request` → `is_websocket_upgrade` → `handle_websocket_upgrade`
/// vs `forward_request`. Distinct from `strip_hop_by_hop_except_upgrade_preserves_websocket`,
/// which only checks header preservation.
///
/// Upstream returns 101 when any `Upgrade` header arrives, else 200. The
/// websocket path keeps `Upgrade`; the ordinary path strips it (Connection lists
/// it). A case-sensitive or always-false detector fails the mixed-case 101; an
/// always-true detector fails the h2c 200.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handle_request_routes_websocket_upgrade_by_header() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = listener.local_addr().unwrap();
    let _upstream = Abort(tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 512];
                loop {
                    let n = match stream.read(&mut tmp).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let head = String::from_utf8_lossy(&buf);
                let has_upgrade = head.lines().any(|line| {
                    line.split_once(':')
                        .is_some_and(|(name, _)| name.eq_ignore_ascii_case("upgrade"))
                });
                let response = if has_upgrade {
                    "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: upgrade\r\n\r\n"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nordinary"
                };
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    }));

    let table = Arc::new(RwLock::new(RouteTable::new()));
    crate::locks::write_tolerant(&table).upsert(Route {
        domain: "ws.test".into(),
        target: Some(target.to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Task,
        project: "/fixture".into(),
        managed_listener: None,
        tls: false,
        registered: Utc::now(),
    });
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = proxy_listener.local_addr().unwrap();
    let (stop, shutdown) = tokio::sync::watch::channel(false);
    let server = tokio::spawn(run_proxy_server_on(
        proxy_listener,
        ProxyConfig::default(),
        table,
        Arc::new(GatewayStats::new()),
        shutdown,
    ));

    async fn raw_status(addr: SocketAddr, upgrade: Option<&str>) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut request = String::from("GET / HTTP/1.1\r\nHost: ws.test\r\n");
        if let Some(value) = upgrade {
            request.push_str("Connection: Upgrade\r\nUpgrade: ");
            request.push_str(value);
            request.push_str("\r\n");
        }
        request.push_str("\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut buf = [0u8; 256];
        let n = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf))
            .await
            .expect("proxy response")
            .unwrap();
        let text = std::str::from_utf8(&buf[..n]).unwrap();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|token| token.parse().ok())
            .unwrap_or(0);
        assert_ne!(status, 0, "unparseable proxy response: {text:?}");
        status
    }

    for (label, upgrade, expected) in [
        ("mixed-case websocket", Some("WebSocket"), 101),
        ("missing upgrade", None, 200),
        ("h2c", Some("h2c"), 200),
        ("invalid value", Some("not-a-protocol"), 200),
    ] {
        assert_eq!(raw_status(address, upgrade).await, expected, "{label}");
    }

    stop.send(true).unwrap();
    server.await.unwrap().unwrap();
}
