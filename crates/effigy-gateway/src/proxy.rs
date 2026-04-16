//! HTTP reverse proxy.
//!
//! Routes incoming HTTP requests by `Host` header to the correct upstream
//! target based on the route table. Returns a helpful error page when no
//! route is registered for a domain.

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};

use crate::routes::RouteTable;

/// Configuration for the reverse proxy.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Address to bind the HTTP proxy (e.g., "127.0.0.1:80").
    pub bind_addr: SocketAddr,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 80)),
        }
    }
}

/// Run the HTTP reverse proxy server.
///
/// Blocks until the shutdown signal fires.
pub async fn run_proxy_server(
    config: ProxyConfig,
    route_table: Arc<RwLock<RouteTable>>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), crate::GatewayError> {
    let listener = TcpListener::bind(config.bind_addr)
        .await
        .map_err(|e| crate::GatewayError::ProxyBindError {
            addr: config.bind_addr.to_string(),
            reason: e.to_string(),
        })?;

    info!(addr = %config.bind_addr, "HTTP proxy started");

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, peer_addr)) => {
                        let table = Arc::clone(&route_table);
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            let service = service_fn(move |req| {
                                let table = Arc::clone(&table);
                                async move { handle_request(req, &table, peer_addr).await }
                            });

                            if let Err(e) = http1::Builder::new()
                                .preserve_header_case(true)
                                .serve_connection(io, service)
                                .await
                            {
                                // Connection reset by peer is normal.
                                if !e.is_incomplete_message() {
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

/// Handle a single HTTP request by routing it to the correct upstream.
async fn handle_request(
    req: Request<Incoming>,
    route_table: &Arc<RwLock<RouteTable>>,
    peer_addr: SocketAddr,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    // Extract the Host header.
    let host = req
        .headers()
        .get(hyper::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| {
            // Strip port from Host header if present.
            h.split(':').next().unwrap_or(h).to_string()
        });

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

    debug!(host = %host, target = %target, "proxying request");

    // Forward the request to the upstream.
    match forward_request(req, &target).await {
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

/// Forward an HTTP request to an upstream target and return the response.
async fn forward_request(
    req: Request<Incoming>,
    target: &str,
) -> Result<Response<Full<Bytes>>, Box<dyn std::error::Error + Send + Sync>> {
    let stream = tokio::net::TcpStream::connect(target).await?;
    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    // Spawn the connection driver.
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            debug!(error = %e, "upstream connection error");
        }
    });

    // Rebuild the request for the upstream.
    let (parts, body) = req.into_parts();
    let upstream_body = body.collect().await?.to_bytes();
    let mut upstream_req = Request::from_parts(parts, Full::new(upstream_body));

    // Ensure the Host header is set correctly.
    upstream_req
        .headers_mut()
        .insert(hyper::header::HOST, target.parse().unwrap_or_else(|_| {
            hyper::header::HeaderValue::from_static("localhost")
        }));

    let response = sender.send_request(upstream_req).await?;

    // Collect the response body.
    let (parts, body) = response.into_parts();
    let body_bytes = body.collect().await?.to_bytes();
    Ok(Response::from_parts(parts, Full::new(body_bytes)))
}

/// Generate an error response.
fn error_response(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
    let body = format!(
        "<html><head><title>Effigy Gateway - {status}</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:2em;color:#333}}\
         h1{{color:#c00}}pre{{background:#f5f5f5;padding:1em;border-radius:4px}}</style></head>\
         <body><h1>{status}</h1><pre>{message}</pre></body></html>"
    );
    Response::builder()
        .status(status)
        .header("content-type", "text/html; charset=utf-8")
        .header("x-effigy-gateway", "true")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

/// Generate a response for domains with no registered route.
fn no_route_response(host: &str) -> Response<Full<Bytes>> {
    let body = format!(
        "<html><head><title>Effigy Gateway - No Route</title>\
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
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}
