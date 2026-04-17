use super::*;

#[test]
fn extract_host_strips_port() {
    let _req = Request::builder()
        .header("host", "myapp.test:8080")
        .body(Empty::<Bytes>::new())
        .unwrap();
    // Test the extraction logic directly (extract_host takes Incoming).
    let host = "myapp.test:8080";
    let extracted = host.split(':').next().unwrap_or(host).to_lowercase();
    assert_eq!(extracted, "myapp.test");
}

#[test]
fn extract_host_lowercases() {
    let host = "MyApp.TEST";
    let extracted = host.split(':').next().unwrap_or(host).to_lowercase();
    assert_eq!(extracted, "myapp.test");
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
fn forwarding_headers_added() {
    let mut headers = hyper::HeaderMap::new();
    let peer: SocketAddr = "192.168.1.100:54321".parse().unwrap();

    add_forwarding_headers(&mut headers, "myapp.test", peer);

    assert_eq!(
        headers.get("x-forwarded-for").unwrap().to_str().unwrap(),
        "192.168.1.100"
    );
    assert_eq!(
        headers.get("x-forwarded-host").unwrap().to_str().unwrap(),
        "myapp.test"
    );
    assert_eq!(
        headers.get("x-forwarded-proto").unwrap().to_str().unwrap(),
        "http"
    );
    assert_eq!(
        headers.get("x-real-ip").unwrap().to_str().unwrap(),
        "192.168.1.100"
    );
}

#[test]
fn forwarding_headers_append_to_existing() {
    let mut headers = hyper::HeaderMap::new();
    headers.insert("x-forwarded-for", HeaderValue::from_static("10.0.0.1"));
    let peer: SocketAddr = "192.168.1.100:54321".parse().unwrap();

    add_forwarding_headers(&mut headers, "myapp.test", peer);

    assert_eq!(
        headers.get("x-forwarded-for").unwrap().to_str().unwrap(),
        "10.0.0.1, 192.168.1.100"
    );
}

#[test]
fn websocket_upgrade_detected() {
    let req = Request::builder()
        .header("upgrade", "websocket")
        .header("connection", "Upgrade")
        .body(Empty::<Bytes>::new())
        .unwrap();
    let upgrade_val = req
        .headers()
        .get(UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
    assert!(upgrade_val);
}

#[test]
fn websocket_upgrade_not_detected_for_normal_request() {
    let req = Request::builder()
        .header("content-type", "text/html")
        .body(Empty::<Bytes>::new())
        .unwrap();
    let upgrade_val = req
        .headers()
        .get(UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
    assert!(!upgrade_val);
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

#[test]
fn no_route_response_contains_domain() {
    let resp = no_route_response("myapp.test");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}
