//! Error types for the gateway crate.

use std::net::Ipv4Addr;
use std::path::PathBuf;

/// Errors that can occur during gateway operations.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// A fenced endpoint transaction, import or observation was refused.
    #[error("gateway endpoint store: {reason}")]
    EndpointStore { reason: String },
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

    /// Another checkout still serves this domain, so the claim is live.
    #[error("gateway domain `{domain}` is owned by checkout `{project}`, which still serves it; stop that checkout with `effigy container down` there, or configure a distinct domain for this worktree")]
    ForeignRoute { domain: String, project: String },

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

    /// No full port range remains available for a new project allocation.
    #[error("port registry exhausted: no free {range}-port range remains; run `effigy container ports status` for capacity and reclaim candidates")]
    PortRegistryExhausted { range: u16 },

    /// The bounded loopback-IP pool is exhausted.
    #[error("loopback pool exhausted for range {range_start}–{range_end}")]
    LoopbackPoolExhausted {
        range_start: Ipv4Addr,
        range_end: Ipv4Addr,
    },

    /// Loopback alias provisioning failed.
    #[error("failed to provision loopback alias {ip}: {reason}")]
    LoopbackAliasProvision { ip: Ipv4Addr, reason: String },

    /// A planned dead-claim release no longer matches the committed endpoint
    /// snapshot.
    #[error("gateway endpoint `{entry}` changed before its release; read and retry")]
    DeadClaimChanged { entry: String },

    /// Gateway is not running.
    #[error("gateway is not running")]
    NotRunning,

    /// The launchd listener handoff refused before any listener was adopted.
    #[error("{0}")]
    ListenerHandoff(#[from] crate::listener_handoff::ListenerHandoffError),

    /// The recorded gateway PID could not be probed.
    ///
    /// Distinct from [`Self::NotRunning`]: the process state is ambiguous, so
    /// callers must not report a stop, delete the PID records, or start a
    /// replacement. The PID and version records stay in place for
    /// reconciliation.
    #[error(
        "cannot determine whether gateway process {pid} is running; leaving PID state in place for reconciliation"
    )]
    ProcessStateUnknown { pid: u32 },

    /// A numeric-only gateway record cannot authenticate its process.
    #[error(
        "legacy gateway identity migration required for PID {pid}: `gateway.identity` is missing. Effigy cannot verify or signal this process, remove its records, or start a replacement from `up`/`down` or managed auto-start. The pre-0.14 recovery command is retired. Ask the host administrator to verify and stop the old daemon; after confirmed absence, remove only its PID/version records and run `effigy gateway up`."
    )]
    LegacyIdentityRequired { pid: u32 },

    /// Another `gateway up`, `down`, or `recover` holds the transition lock.
    #[error(
        "another gateway up, down, or recover command holds the transition lock; retry when that command finishes"
    )]
    TransitionLockHeld,

    /// Resolver suffix is invalid.
    #[error("resolver suffix `{suffix}` is invalid: {reason}")]
    InvalidResolverSuffix { suffix: String, reason: String },

    /// File watcher error.
    #[error("file watcher error: {0}")]
    WatcherError(String),

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
