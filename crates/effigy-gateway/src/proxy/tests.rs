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
