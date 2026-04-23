//! Error types for the gateway crate.

use std::net::Ipv4Addr;
use std::path::PathBuf;

/// Errors that can occur during gateway operations.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// The route table file could not be read.
    #[error("failed to read route table at {path}: {reason}")]
    RouteTableReadError { path: PathBuf, reason: String },

    /// The route table file contains invalid JSON.
    #[error("invalid route table JSON at {path}: {reason}")]
    RouteTableParseError { path: PathBuf, reason: String },

    /// The route table file could not be written.
    #[error("failed to write route table at {path}: {reason}")]
    RouteTableWriteError { path: PathBuf, reason: String },

    /// The loopback registry file could not be read.
    #[error("failed to read loopback registry at {path}: {reason}")]
    LoopbackRegistryRead { path: PathBuf, reason: String },

    /// The loopback registry file contains invalid JSON.
    #[error("invalid loopback registry JSON at {path}: {reason}")]
    LoopbackRegistryParse { path: PathBuf, reason: String },

    /// The loopback registry file could not be written.
    #[error("failed to write loopback registry at {path}: {reason}")]
    LoopbackRegistryWrite { path: PathBuf, reason: String },

    /// A route for the given domain already exists.
    #[error("route already registered for domain '{domain}'")]
    DuplicateRoute { domain: String },

    /// No route found for the given domain.
    #[error("no route registered for domain '{domain}'")]
    RouteNotFound { domain: String },

    /// DNS server failed to bind.
    #[error("DNS server failed to bind on {addr}: {reason}")]
    DnsBindError { addr: String, reason: String },

    /// HTTP proxy failed to bind.
    #[error("HTTP proxy failed to bind on {addr}: {reason}")]
    ProxyBindError { addr: String, reason: String },

    /// TCP alias listener failed to bind.
    #[error("TCP alias listener failed to bind on {addr}: {reason}")]
    TcpAliasBindError { addr: String, reason: String },

    /// TCP alias target is invalid.
    #[error("TCP alias target `{target}` is invalid: {reason}")]
    TcpAliasTargetInvalid { target: String, reason: String },

    /// TCP alias failed to connect to its upstream target.
    #[error("TCP alias failed to connect to upstream `{target}`: {reason}")]
    TcpAliasConnectError { target: String, reason: String },

    /// TCP alias stream forwarding failed.
    #[error("TCP alias stream forwarding failed for `{target}`: {reason}")]
    TcpAliasIoError { target: String, reason: String },

    /// Failed to connect to upstream target.
    #[error("failed to connect to upstream {target} for domain '{domain}': {reason}")]
    UpstreamConnectError {
        domain: String,
        target: String,
        reason: String,
    },

    /// TLS certificate error.
    #[error("TLS certificate error for domain '{domain}': {reason}")]
    TlsError { domain: String, reason: String },

    /// TLS certificate file not found.
    #[error("TLS certificate not found at {path}")]
    TlsCertNotFound { path: PathBuf },

    /// Gateway is already running.
    #[error("gateway is already running (PID {pid})")]
    AlreadyRunning { pid: u32 },

    /// Port range conflicts with an existing allocation.
    #[error(
        "port range {base}–{} for project '{project}' conflicts with \
         existing allocation for '{conflicting_project}'",
        base + range
    )]
    PortConflict {
        project: String,
        conflicting_project: String,
        base: u16,
        range: u16,
    },

    /// The bounded loopback-IP pool is exhausted.
    #[error("loopback pool exhausted for range {range_start}–{range_end}")]
    LoopbackPoolExhausted {
        range_start: Ipv4Addr,
        range_end: Ipv4Addr,
    },

    /// Loopback alias provisioning failed.
    #[error("failed to provision loopback alias {ip}: {reason}")]
    LoopbackAliasProvision { ip: Ipv4Addr, reason: String },

    /// Gateway is not running.
    #[error("gateway is not running")]
    NotRunning,

    /// File watcher error.
    #[error("file watcher error: {0}")]
    WatcherError(String),

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
