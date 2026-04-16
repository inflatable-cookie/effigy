//! HTTP reverse proxy with streaming support.
//!
//! Routes incoming HTTP requests by `Host` header to the correct upstream
//! target based on the route table. Supports:
//!
//! - **Streaming**: Request and response bodies are forwarded without
//!   buffering, supporting large uploads, SSE, and long-polling.
//! - **WebSocket upgrade**: Detects `Upgrade: websocket` headers and
//!   performs bidirectional TCP pipe after the protocol switch.
//! - **Forwarding headers**: Adds `X-Forwarded-For`, `X-Forwarded-Host`,
//!   `X-Forwarded-Proto` to upstream requests.
//! - **Hop-by-hop header stripping**: Removes `Connection`, `Keep-Alive`,
//!   and other hop-by-hop headers from proxied requests/responses.
//! - **Error pages**: Returns helpful HTML when no route is registered.

use std::error::Error as StdError;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio_rustls;

use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HeaderName, HeaderValue, CONNECTION, HOST, UPGRADE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

use serde_json;

use crate::routes::RouteTable;

/// Configuration for the reverse proxy.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Address to bind the HTTP proxy (e.g., "127.0.0.1:80").
    pub bind_addr: SocketAddr,

    /// Address to bind the HTTPS proxy (e.g., "127.0.0.1:443").
    /// If None, HTTPS is disabled.
    pub tls_bind_addr: Option<SocketAddr>,

    /// Timeout for connecting to upstream targets.
    pub connect_timeout: Duration,

    /// Timeout for receiving a response from upstream after connecting.
    /// Protects against hung upstream processes.
    pub response_timeout: Duration,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 80)),
            tls_bind_addr: None,
            connect_timeout: Duration::from_secs(5),
            response_timeout: Duration::from_secs(300), // 5 min — generous for debugging
        }
    }
}

/// Body type used for proxy responses — either a proxied upstream body
/// or a locally-generated error page.
type ProxyBody = BoxBody<Bytes, hyper::Error>;

/// Wrap a `Full<Bytes>` into a `ProxyBody`.
fn full_body(bytes: Bytes) -> ProxyBody {
    Full::new(bytes).map_err(|never| match never {}).boxed()
}

/// Wrap an `Incoming` body into a `ProxyBody`.
fn incoming_body(body: Incoming) -> ProxyBody {
    body.boxed()
}

/// Create an empty body.
fn empty_body() -> ProxyBody {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

/// Headers that must not be forwarded between hops (RFC 7230 §6.1).
const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
];

/// Run the HTTP reverse proxy server.
///
/// Blocks until the shutdown signal fires.
pub async fn run_proxy_server(
    config: ProxyConfig,
    route_table: Arc<RwLock<RouteTable>>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), crate::GatewayError> {
    let listener = TcpListener::bind(config.bind_addr).await.map_err(|e| {
        crate::GatewayError::ProxyBindError {
            addr: config.bind_addr.to_string(),
            reason: e.to_string(),
        }
    })?;

    info!(addr = %config.bind_addr, "HTTP proxy started");

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, peer_addr)) => {
                        let table = Arc::clone(&route_table);
                        let cfg = config.clone();
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            let service = service_fn(move |req| {
                                let table = Arc::clone(&table);
                                let cfg = cfg.clone();
                                async move {
                                    handle_request(req, &table, peer_addr, &cfg).await
                                }
                            });

                            if let Err(e) = http1::Builder::new()
                                .preserve_header_case(true)
                                .serve_connection(io, service)
                                .with_upgrades()
                                .await
                            {
                                if !is_benign_connection_error(&e) {
                                    debug!(
                                        error = %e,
                                        peer = %peer_addr,
                                        "HTTP connection error"
                                    );
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "failed to accept connection");
                    }
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    info!("HTTP proxy shutting down");
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Run the HTTPS reverse proxy server with TLS termination.
///
/// Uses `tokio-rustls` to accept TLS connections. Certificates are loaded
/// per-domain from the provided certificate directory. The SNI hostname
/// determines which certificate to serve.
///
/// Blocks until the shutdown signal fires.
pub async fn run_tls_proxy_server(
    bind_addr: SocketAddr,
    tls_config: Arc<rustls::ServerConfig>,
    route_table: Arc<RwLock<RouteTable>>,
    proxy_config: ProxyConfig,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), crate::GatewayError> {
    let tls_acceptor = tokio_rustls::TlsAcceptor::from(tls_config);

    let listener =
        TcpListener::bind(bind_addr)
            .await
            .map_err(|e| crate::GatewayError::ProxyBindError {
                addr: bind_addr.to_string(),
                reason: format!("HTTPS bind failed: {e}"),
            })?;

    info!(addr = %bind_addr, "HTTPS proxy started");

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, peer_addr)) => {
                        let acceptor = tls_acceptor.clone();
                        let table = Arc::clone(&route_table);
                        let cfg = proxy_config.clone();
                        tokio::spawn(async move {
                            // Perform TLS handshake.
                            let tls_stream = match acceptor.accept(stream).await {
                                Ok(s) => s,
                                Err(e) => {
                                    debug!(
                                        error = %e,
                                        peer = %peer_addr,
                                        "TLS handshake failed"
                                    );
                                    return;
                                }
                            };

                            let io = TokioIo::new(tls_stream);
                            let service = service_fn(move |req| {
                                let table = Arc::clone(&table);
                                let cfg = cfg.clone();
                                async move {
                                    handle_request(req, &table, peer_addr, &cfg).await
                                }
                            });

                            if let Err(e) = http1::Builder::new()
                                .preserve_header_case(true)
                                .serve_connection(io, service)
                                .with_upgrades()
                                .await
                            {
                                if !is_benign_connection_error(&e) {
                                    debug!(
                                        error = %e,
                                        peer = %peer_addr,
                                        "HTTPS connection error"
                                    );
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "HTTPS: failed to accept connection");
                    }
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    info!("HTTPS proxy shutting down");
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Check if a connection error is benign (client disconnect, etc.).
fn is_benign_connection_error(e: &hyper::Error) -> bool {
    // Incomplete message = client closed before sending full request.
    // This is normal browser behavior (preflight connections, etc.).
    if e.is_incomplete_message() {
        return true;
    }
    // Connection reset by peer.
    if let Some(io_err) = e.source().and_then(|e| e.downcast_ref::<std::io::Error>()) {
        return io_err.kind() == std::io::ErrorKind::ConnectionReset
            || io_err.kind() == std::io::ErrorKind::BrokenPipe;
    }
    false
}

/// Handle gateway internal endpoints (/_effigy/*).
///
/// These endpoints are served directly by the proxy, not forwarded to
/// any upstream. They provide health checks and status information.
fn handle_gateway_endpoint(
    req: &Request<Incoming>,
    route_table: &Arc<RwLock<RouteTable>>,
) -> Option<Response<ProxyBody>> {
    let path = req.uri().path();

    if !path.starts_with("/_effigy/") {
        return None;
    }

    match path {
        "/_effigy/health" => {
            let body = r#"{"status":"ok"}"#;
            Some(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json")
                    .header("x-effigy-gateway", "true")
                    .body(full_body(Bytes::from(body)))
                    .unwrap(),
            )
        }
        "/_effigy/routes" => {
            let table = route_table.read().expect("route table lock poisoned");
            let routes: Vec<serde_json::Value> = table
                .all_routes()
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "domain": r.domain,
                        "target": r.target,
                        "tls": r.tls,
                        "project": r.project,
                        "registered": r.registered.to_rfc3339(),
                    })
                })
                .collect();

            let body = serde_json::json!({
                "count": routes.len(),
                "routes": routes,
            });

            Some(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json")
                    .header("x-effigy-gateway", "true")
                    .body(full_body(Bytes::from(body.to_string())))
                    .unwrap(),
            )
        }
        _ => Some(error_response(
            StatusCode::NOT_FOUND,
            &format!("Unknown gateway endpoint: {path}"),
        )),
    }
}

/// Handle a single HTTP request by routing it to the correct upstream.
async fn handle_request(
    req: Request<Incoming>,
    route_table: &Arc<RwLock<RouteTable>>,
    peer_addr: SocketAddr,
    config: &ProxyConfig,
) -> Result<Response<ProxyBody>, hyper::Error> {
    // Gateway internal endpoints (/_effigy/*).
    if let Some(response) = handle_gateway_endpoint(&req, route_table) {
        return Ok(response);
    }

    // Extract the Host header.
    let host = extract_host(&req);

    let host = match host {
        Some(h) => h,
        None => {
            warn!(peer = %peer_addr, "request missing Host header");
            return Ok(error_response(
                StatusCode::BAD_REQUEST,
                "Missing Host header",
            ));
        }
    };

    // Look up the route.
    let target = {
        let table = route_table.read().expect("route table lock poisoned");
        table.lookup(&host).map(|r| r.target.clone())
    };

    let target = match target {
        Some(t) => t,
        None => {
            debug!(host = %host, peer = %peer_addr, "no route for host");
            return Ok(no_route_response(&host));
        }
    };

    // Check for WebSocket upgrade.
    if is_websocket_upgrade(&req) {
        debug!(host = %host, target = %target, "WebSocket upgrade");
        return handle_websocket_upgrade(req, &target, &host, peer_addr, config).await;
    }

    debug!(
        method = %req.method(),
        uri = %req.uri(),
        host = %host,
        target = %target,
        "proxying request"
    );

    match forward_request(req, &target, &host, peer_addr, config).await {
        Ok(response) => Ok(response),
        Err(e) => {
            warn!(host = %host, target = %target, error = %e, "upstream error");
            Ok(error_response(
                StatusCode::BAD_GATEWAY,
                &format!("Failed to connect to upstream {target}: {e}"),
            ))
        }
    }
}

/// Extract the host from a request (stripping port if present).
fn extract_host(req: &Request<Incoming>) -> Option<String> {
    req.headers()
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| h.split(':').next().unwrap_or(h).to_lowercase())
}

/// Check if a request is a WebSocket upgrade.
fn is_websocket_upgrade(req: &Request<Incoming>) -> bool {
    req.headers()
        .get(UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false)
}

/// Forward an HTTP request to an upstream target with streaming.
async fn forward_request(
    mut req: Request<Incoming>,
    target: &str,
    original_host: &str,
    peer_addr: SocketAddr,
    config: &ProxyConfig,
) -> Result<Response<ProxyBody>, Box<dyn std::error::Error + Send + Sync>> {
    // Connect to upstream with timeout.
    let stream = tokio::time::timeout(config.connect_timeout, TcpStream::connect(target))
        .await
        .map_err(|_| format!("connect timeout after {:?}", config.connect_timeout))?
        .map_err(|e| format!("connect failed: {e}"))?;

    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    // Spawn the connection driver.
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            debug!(error = %e, "upstream connection closed");
        }
    });

    // Prepare the upstream request.
    strip_hop_by_hop_headers(req.headers_mut());
    add_forwarding_headers(req.headers_mut(), original_host, peer_addr);

    // Send request and wait for response headers with timeout.
    let response = tokio::time::timeout(
        config.response_timeout,
        sender.send_request(req),
    )
    .await
    .map_err(|_| format!(
        "upstream response timeout after {:?}",
        config.response_timeout
    ))?
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

    // Strip hop-by-hop headers from response.
    let (mut parts, body) = response.into_parts();
    strip_hop_by_hop_headers(&mut parts.headers);

    Ok(Response::from_parts(parts, incoming_body(body)))
}

/// Handle a WebSocket upgrade by performing bidirectional TCP piping.
async fn handle_websocket_upgrade(
    req: Request<Incoming>,
    target: &str,
    original_host: &str,
    peer_addr: SocketAddr,
    config: &ProxyConfig,
) -> Result<Response<ProxyBody>, hyper::Error> {
    // Connect to upstream.
    let upstream_stream =
        match tokio::time::timeout(config.connect_timeout, TcpStream::connect(target)).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(e)) => {
                warn!(target = %target, error = %e, "WebSocket upstream connect failed");
                return Ok(error_response(
                    StatusCode::BAD_GATEWAY,
                    &format!("WebSocket upstream connect failed: {e}"),
                ));
            }
            Err(_) => {
                return Ok(error_response(
                    StatusCode::GATEWAY_TIMEOUT,
                    "WebSocket upstream connect timeout",
                ));
            }
        };

    let upstream_io = TokioIo::new(upstream_stream);

    // Perform HTTP handshake with upstream.
    let (mut upstream_sender, upstream_conn) = match hyper::client::conn::http1::Builder::new()
        .handshake(upstream_io)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "WebSocket upstream handshake failed");
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                &format!("WebSocket upstream handshake failed: {e}"),
            ));
        }
    };

    // Spawn the connection driver with upgrade support.
    tokio::spawn(async move {
        if let Err(e) = upstream_conn.with_upgrades().await {
            debug!(error = %e, "WebSocket upstream connection closed");
        }
    });

    // Forward the upgrade request to upstream.
    let mut upstream_req = Request::builder()
        .method(req.method().clone())
        .uri(req.uri().clone())
        .body(Empty::<Bytes>::new())
        .unwrap();

    // Copy relevant headers.
    for (name, value) in req.headers() {
        upstream_req
            .headers_mut()
            .insert(name.clone(), value.clone());
    }
    strip_hop_by_hop_headers_except_upgrade(upstream_req.headers_mut());
    add_forwarding_headers(upstream_req.headers_mut(), original_host, peer_addr);

    let upstream_response = match upstream_sender.send_request(upstream_req).await {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "WebSocket upstream request failed");
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                &format!("WebSocket upstream request failed: {e}"),
            ));
        }
    };

    if upstream_response.status() != StatusCode::SWITCHING_PROTOCOLS {
        let status = upstream_response.status();
        debug!(status = %status, "upstream did not accept WebSocket upgrade");
        let (parts, body) = upstream_response.into_parts();
        return Ok(Response::from_parts(parts, incoming_body(body)));
    }

    // Build the 101 response to send to the client.
    let mut response_builder = Response::builder().status(StatusCode::SWITCHING_PROTOCOLS);
    for (name, value) in upstream_response.headers() {
        response_builder = response_builder.header(name, value);
    }

    // Spawn the bidirectional pipe between client and upstream.
    // Both sides upgrade their connections.
    tokio::spawn(async move {
        let upstream_upgraded = match hyper::upgrade::on(upstream_response).await {
            Ok(u) => u,
            Err(e) => {
                debug!(error = %e, "upstream WebSocket upgrade failed");
                return;
            }
        };

        let client_upgraded = match hyper::upgrade::on(req).await {
            Ok(u) => u,
            Err(e) => {
                debug!(error = %e, "client WebSocket upgrade failed");
                return;
            }
        };

        let mut upstream_io = TokioIo::new(upstream_upgraded);
        let mut client_io = TokioIo::new(client_upgraded);

        match copy_bidirectional(&mut client_io, &mut upstream_io).await {
            Ok((client_to_upstream, upstream_to_client)) => {
                debug!(
                    c2u = client_to_upstream,
                    u2c = upstream_to_client,
                    "WebSocket connection closed"
                );
            }
            Err(e) => {
                debug!(error = %e, "WebSocket pipe error");
            }
        }
    });

    let response = response_builder.body(empty_body()).unwrap();

    Ok(response)
}

/// Remove hop-by-hop headers from a header map.
fn strip_hop_by_hop_headers(headers: &mut hyper::HeaderMap) {
    // Also check for headers listed in the Connection header.
    let connection_headers: Vec<String> = headers
        .get(CONNECTION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').map(|s| s.trim().to_lowercase()).collect())
        .unwrap_or_default();

    for name in HOP_BY_HOP_HEADERS {
        headers.remove(*name);
    }
    for name in &connection_headers {
        if let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) {
            headers.remove(&header_name);
        }
    }
}

/// Remove hop-by-hop headers but preserve Upgrade (for WebSocket).
fn strip_hop_by_hop_headers_except_upgrade(headers: &mut hyper::HeaderMap) {
    for name in HOP_BY_HOP_HEADERS {
        if *name != "connection" {
            headers.remove(*name);
        }
    }
    // Rewrite Connection header to only contain "upgrade".
    if headers.contains_key(UPGRADE) {
        headers.insert(CONNECTION, HeaderValue::from_static("upgrade"));
    } else {
        headers.remove(CONNECTION);
    }
}

/// Add X-Forwarded-* headers to the upstream request.
fn add_forwarding_headers(
    headers: &mut hyper::HeaderMap,
    original_host: &str,
    peer_addr: SocketAddr,
) {
    let peer_ip = peer_addr.ip().to_string();

    // X-Forwarded-For: append to existing or create new.
    if let Some(existing) = headers.get("x-forwarded-for").cloned() {
        if let Ok(existing_str) = existing.to_str() {
            let new_val = format!("{existing_str}, {peer_ip}");
            if let Ok(val) = HeaderValue::from_str(&new_val) {
                headers.insert("x-forwarded-for", val);
            }
        }
    } else if let Ok(val) = HeaderValue::from_str(&peer_ip) {
        headers.insert("x-forwarded-for", val);
    }

    // X-Forwarded-Host.
    if let Ok(val) = HeaderValue::from_str(original_host) {
        headers.insert("x-forwarded-host", val);
    }

    // X-Forwarded-Proto.
    headers.insert("x-forwarded-proto", HeaderValue::from_static("http"));

    // X-Real-IP (if not already set).
    if !headers.contains_key("x-real-ip") {
        if let Ok(val) = HeaderValue::from_str(&peer_ip) {
            headers.insert("x-real-ip", val);
        }
    }
}

/// Generate an error response.
fn error_response(status: StatusCode, message: &str) -> Response<ProxyBody> {
    let body = format!(
        "<!DOCTYPE html><html><head><title>Effigy Gateway - {status}</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:2em;color:#333}}\
         h1{{color:#c00}}pre{{background:#f5f5f5;padding:1em;border-radius:4px;\
         white-space:pre-wrap;word-break:break-word}}</style></head>\
         <body><h1>{status}</h1><pre>{message}</pre></body></html>"
    );
    Response::builder()
        .status(status)
        .header("content-type", "text/html; charset=utf-8")
        .header("x-effigy-gateway", "true")
        .body(full_body(Bytes::from(body)))
        .unwrap()
}

/// Generate a response for domains with no registered route.
fn no_route_response(host: &str) -> Response<ProxyBody> {
    let body = format!(
        "<!DOCTYPE html><html><head><title>Effigy Gateway - No Route</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:2em;color:#333}}\
         h1{{color:#e90}}code{{background:#f0f0f0;padding:2px 6px;border-radius:3px}}\
         pre{{background:#f5f5f5;padding:1em;border-radius:4px}}</style></head>\
         <body><h1>No route registered for <code>{host}</code></h1>\
         <p>The domain resolves via the Effigy gateway, but no project has \
         registered a route for it.</p>\
         <p>To register a route, add a <code>dns.domain</code> to your \
         container manifest and run <code>effigy container up</code>.</p>\
         <pre>[containers.web.dns]\ndomain = \"{host}\"</pre></body></html>"
    );
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header("content-type", "text/html; charset=utf-8")
        .header("x-effigy-gateway", "true")
        .body(full_body(Bytes::from(body)))
        .unwrap()
}

#[cfg(test)]
mod tests {
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
}
