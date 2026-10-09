//! Gateway server lifecycle — coordinates DNS, proxy, and route watching.
//!
//! The gateway runs as a background process with three concurrent tasks:
//!
//! 1. DNS resolver (UDP)
//! 2. HTTP reverse proxy (TCP)
//! 3. Route table file watcher
//!
//! All three tasks share the route table via `Arc<RwLock<RouteTable>>`.
//! The file watcher reloads the table when the JSON file changes. Shutdown
//! is coordinated via a `tokio::sync::watch` channel.

use std::fs::OpenOptions;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::watch;
use tracing::{debug, error, info};

use crate::dns::{run_dns_server, run_dns_server_on, DnsCache, DnsConfig};
use crate::error::GatewayError;
use crate::identity::{self, GatewayIdentityProbe, GatewayRecordSnapshot};
use crate::proxy::{
    run_proxy_server, run_proxy_server_on, run_tls_proxy_server, run_tls_proxy_server_on,
    ProxyConfig,
};
#[cfg(target_os = "macos")]
use crate::resolver_setup;
use crate::routes::{LiveRouteTable, RouteTable};
use crate::stats::GatewayStats;
use crate::tcp_alias::run_tcp_alias_manager;
use crate::tls::{
    server_config_from_resolver, sync_private_sni_resolver_from_dir, sync_sni_resolver_from_dir,
    SniCertResolver, TlsConfig,
};

const IDLE_SHUTDOWN_DELAY: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdleShutdownAction {
    None,
    Arm,
    Cancel,
}

/// Configuration for the full gateway.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// DNS resolver configuration.
    pub dns: DnsConfig,

    /// HTTP proxy configuration.
    pub proxy: ProxyConfig,

    /// TLS configuration. If Some, HTTPS proxy is enabled.
    pub tls: Option<TlsConfig>,

    /// Path to the route table JSON file.
    pub route_table_path: PathBuf,

    /// Path to the persisted loopback-IP assignment registry.
    pub loopback_registry_path: PathBuf,

    /// Path to the PID file for lifecycle management.
    pub pid_file_path: PathBuf,

    /// State and host-effect policy for this gateway instance.
    pub mode: GatewayMode,
}

/// Selects the operator gateway defaults or a caller-owned private instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayMode {
    /// Existing operator setup, including its established system integration.
    Operator,
    /// Isolated state and loopback-only endpoints owned by the caller.
    Private(PrivateGatewayPolicy),
}

/// Typed ownership boundary for private gateway state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateGatewayPolicy {
    state_root: PathBuf,
}

impl PrivateGatewayPolicy {
    /// Select a pre-created caller-owned state directory.
    pub fn new(state_root: PathBuf) -> Self {
        Self { state_root }
    }

    /// Private state directory supplied by the caller.
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }
}

impl GatewayConfig {
    /// Create a gateway config using the standard effigy paths.
    ///
    /// `gateway_dir` is typically `~/.effigy/gateway/`.
    pub fn standard(gateway_dir: PathBuf) -> Self {
        Self {
            dns: DnsConfig::default(),
            proxy: ProxyConfig {
                tls_bind_addr: Some(SocketAddr::from(([127, 0, 0, 1], 443))),
                ..ProxyConfig::default()
            },
            tls: Some(TlsConfig::new(gateway_dir.join("certs"))),
            route_table_path: gateway_dir.join("routes.json"),
            loopback_registry_path: gateway_dir.join("loopback-ips.json"),
            pid_file_path: gateway_dir.join("gateway.pid"),
            mode: GatewayMode::Operator,
        }
    }

    /// Create a private gateway using dynamic, unprivileged loopback binds.
    pub fn private(state_root: PathBuf) -> Self {
        let mut config = Self::standard(state_root.clone()).with_addrs(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            SocketAddr::from(([127, 0, 0, 1], 0)),
        );
        config.tls =
            Some(TlsConfig::new(state_root.join("certs")).with_ca_root(state_root.join("ca")));
        config.proxy.tls_bind_addr = Some(SocketAddr::from(([127, 0, 0, 1], 0)));
        config.mode = GatewayMode::Private(PrivateGatewayPolicy::new(state_root));
        config
    }

    /// Validate private state and bind policy before startup or lifecycle I/O.
    pub fn validate_private_mode(&self) -> Result<(), GatewayError> {
        let GatewayMode::Private(policy) = &self.mode else {
            return Ok(());
        };
        crate::private_state::validate_root(policy.state_root())?;
        let root = policy.state_root();
        let tls_paths_match = self.tls.as_ref().is_some_and(|tls| {
            tls.certs_dir == root.join("certs")
                && tls.ca_root.as_deref() == Some(root.join("ca").as_path())
        });
        if self.route_table_path != root.join("routes.json")
            || self.loopback_registry_path != root.join("loopback-ips.json")
            || self.pid_file_path != root.join("gateway.pid")
            || !tls_paths_match
        {
            return Err(GatewayError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "private gateway state and certificate paths must remain inside its selected root",
            )));
        }
        let addresses = [
            Some(self.dns.bind_addr),
            Some(self.proxy.bind_addr),
            self.proxy.tls_bind_addr,
        ];
        if addresses.into_iter().flatten().any(|address| {
            !address.ip().is_loopback() || (address.port() != 0 && address.port() < 1024)
        }) {
            return Err(GatewayError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "private gateway endpoints must use unprivileged loopback addresses",
            )));
        }
        if self.proxy.tls_bind_addr.is_none() || self.tls.is_none() {
            return Err(GatewayError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "private gateway mode requires an HTTPS listener and certificate directory",
            )));
        }
        Ok(())
    }

    /// Whether this configuration selects private caller-owned state.
    pub fn is_private(&self) -> bool {
        matches!(&self.mode, GatewayMode::Private(_))
    }

    /// Private caller-owned state root, if selected.
    pub fn private_state_root(&self) -> Option<&Path> {
        match &self.mode {
            GatewayMode::Operator => None,
            GatewayMode::Private(policy) => Some(policy.state_root()),
        }
    }

    /// Use custom bind addresses.
    pub fn with_addrs(mut self, dns_addr: SocketAddr, proxy_addr: SocketAddr) -> Self {
        self.dns.bind_addr = dns_addr;
        self.proxy.bind_addr = proxy_addr;
        self
    }

    /// Use a custom TLD.
    pub fn with_tld(mut self, tld: String) -> Self {
        self.dns.tld = tld;
        self
    }

    /// Enable HTTPS with TLS certificates from the given directory.
    ///
    /// Also sets the HTTPS bind address on the proxy config.
    pub fn with_tls(mut self, certs_dir: PathBuf, https_addr: SocketAddr) -> Self {
        self.tls = Some(TlsConfig::new(certs_dir));
        self.proxy.tls_bind_addr = Some(https_addr);
        self
    }
}

/// Status of a running gateway.
#[derive(Debug, Clone)]
pub struct GatewayStatus {
    /// Process ID of the running gateway.
    pub pid: u32,

    /// DNS resolver bind address.
    pub dns_addr: SocketAddr,

    /// HTTP proxy bind address.
    pub proxy_addr: SocketAddr,

    /// HTTPS proxy bind address when TLS is enabled.
    pub https_addr: Option<SocketAddr>,

    /// Number of registered routes.
    pub route_count: usize,

    /// All registered routes.
    pub routes: Vec<crate::routes::Route>,

    /// Build identity of the running gateway daemon, if recorded.
    pub binary_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateGatewayAddressRecord {
    pid: u32,
    identity_digest: String,
    dns_addr: SocketAddr,
    proxy_addr: SocketAddr,
    https_addr: SocketAddr,
}

struct PrivateStartupRecordGuard {
    snapshot: GatewayRecordSnapshot,
    address_path: PathBuf,
    address_bytes: Option<Vec<u8>>,
    active: bool,
}

impl PrivateStartupRecordGuard {
    fn new(snapshot: GatewayRecordSnapshot, address_path: PathBuf) -> Self {
        Self {
            snapshot,
            address_path,
            address_bytes: None,
            active: true,
        }
    }

    fn record_address_bytes(&mut self, bytes: Vec<u8>) {
        self.address_bytes = Some(bytes);
    }

    fn disarm(&mut self) {
        self.active = false;
    }

    fn finish(&mut self) -> Result<(), GatewayError> {
        if let Some(expected) = &self.address_bytes {
            if std::fs::read(&self.address_path).ok().as_ref() != Some(expected) {
                return Err(GatewayError::ProcessStateUnknown {
                    pid: self.snapshot.pid(),
                });
            }
            std::fs::remove_file(&self.address_path).map_err(GatewayError::Io)?;
        }
        if !identity::remove_if_unchanged(&self.snapshot)? {
            return Err(GatewayError::ProcessStateUnknown {
                pid: self.snapshot.pid(),
            });
        }
        self.disarm();
        Ok(())
    }
}

impl Drop for PrivateStartupRecordGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Some(expected) = &self.address_bytes {
            if std::fs::read(&self.address_path).ok().as_ref() == Some(expected) {
                let _ = std::fs::remove_file(&self.address_path);
            }
        }
        let _ = identity::remove_if_unchanged(&self.snapshot);
    }
}

struct PrivateGatewayListeners {
    dns: UdpSocket,
    proxy: TcpListener,
    https: TcpListener,
    dns_addr: SocketAddr,
    proxy_addr: SocketAddr,
    https_addr: SocketAddr,
}

/// Gateway status paired with the exact trusted record generation that
/// authenticated it. Lifecycle code must retain this token through signalling
/// and conditional removal.
#[derive(Debug, Clone)]
pub struct VerifiedGatewayStatus {
    /// Stable public gateway status.
    pub status: GatewayStatus,
    /// Exact trusted record pair used for the live identity comparison.
    pub snapshot: GatewayRecordSnapshot,
}

impl std::ops::Deref for VerifiedGatewayStatus {
    type Target = GatewayStatus;

    fn deref(&self) -> &Self::Target {
        &self.status
    }
}

/// Write a PID file for lifecycle management.
#[cfg(test)]
fn write_pid_file(path: &Path) -> Result<(), GatewayError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, std::process::id().to_string())?;
    Ok(())
}

fn gateway_version_file_for(pid_file_path: &Path) -> PathBuf {
    pid_file_path.with_extension("version")
}

fn write_gateway_version_file(pid_path: &Path) -> Result<(), GatewayError> {
    identity::publish_gateway_version(pid_path, &effigy_core::build_info::active_version())
}

fn read_gateway_version_file(path: &Path) -> Result<Option<String>, GatewayError> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)?;
    let value = raw.trim();
    if value.is_empty() {
        return Ok(None);
    }
    Ok(Some(value.to_owned()))
}

/// Read the PID from a PID file.
pub fn read_pid_file(path: &Path) -> Result<u32, GatewayError> {
    if !path.exists() {
        return Err(GatewayError::NotRunning);
    }
    let content = std::fs::read_to_string(path)?;
    let pid = content
        .trim()
        .parse::<u32>()
        .map_err(|_| GatewayError::NotRunning)?;
    checked_gateway_pid(pid).ok_or(GatewayError::NotRunning)?;
    Ok(pid)
}

/// Convert a persisted gateway PID to the positive signed type used by Unix
/// process APIs. PID 0 addresses a process group and PID 1 is outside Effigy's
/// daemon ownership domain.
pub fn checked_gateway_pid(pid: u32) -> Option<i32> {
    let pid_t = i32::try_from(pid).ok()?;
    (pid_t > 1).then_some(pid_t)
}

/// Remove the PID file.
#[cfg(test)]
fn remove_pid_file(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(gateway_version_file_for(path));
}

/// Outcome of probing whether a recorded gateway PID is alive.
///
/// A probe that cannot run or returns ambiguous output is [`Self::Unknown`],
/// not [`Self::ConfirmedAbsent`]. Lifecycle decisions must branch on this
/// value instead of collapsing it to a `bool`, so an unavailable probe can
/// never be reported as a stopped gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayProcessProbe {
    /// A live, non-zombie process with exactly this PID was confirmed.
    Running,
    /// The probe confirmed that no live process with this PID exists.
    ConfirmedAbsent,
    /// The probe was unavailable or its output could not be interpreted.
    Unknown,
}

impl GatewayProcessProbe {
    /// True only when the probe confirmed a running process.
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    /// True only when the probe confirmed the process is gone.
    pub fn is_confirmed_absent(self) -> bool {
        matches!(self, Self::ConfirmedAbsent)
    }

    /// True only when the probe could not determine the process state.
    pub fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown)
    }
}

/// Captured `ps` invocation result used to classify a probe.
#[cfg(unix)]
#[derive(Debug, Clone)]
struct PsProbeOutput {
    /// Whether `ps` exited zero.
    success: bool,
    /// Standard output bytes.
    stdout: Vec<u8>,
    /// Standard error bytes.
    stderr: Vec<u8>,
}

/// Probe whether the process with `pid` is alive.
///
/// A `ps` launch failure, a non-empty diagnostic from a failed `ps`, or
/// malformed/ambiguous rows all yield [`GatewayProcessProbe::Unknown`]. Only a
/// completed `ps` with no matching row (the documented absent result on macOS
/// and Linux) or an exact zombie row is [`GatewayProcessProbe::ConfirmedAbsent`].
#[cfg(unix)]
pub fn probe_gateway_process(pid: u32) -> GatewayProcessProbe {
    probe_gateway_process_with(pid, |pid_t| {
        let pid_text = pid_t.to_string();
        let output = std::process::Command::new("ps")
            .args(["-p", pid_text.as_str(), "-o", "pid=", "-o", "stat="])
            .output()
            .ok()?;
        Some(PsProbeOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    })
}

#[cfg(unix)]
fn probe_gateway_process_with(
    pid: u32,
    probe: impl FnOnce(i32) -> Option<PsProbeOutput>,
) -> GatewayProcessProbe {
    let Some(pid_t) = checked_gateway_pid(pid) else {
        // PID 0, PID 1, and values outside the signed PID domain can never be
        // the gateway, so no probe is dispatched.
        return GatewayProcessProbe::ConfirmedAbsent;
    };
    let Some(output) = probe(pid_t) else {
        return GatewayProcessProbe::Unknown;
    };
    if !output.success {
        // `ps -p <pid>` exits non-zero with no output when no process matches
        // (macOS and Linux). Any diagnostic output means `ps` itself failed,
        // which is not proof of absence.
        return if output.stdout.is_empty() && output.stderr.is_empty() {
            GatewayProcessProbe::ConfirmedAbsent
        } else {
            GatewayProcessProbe::Unknown
        };
    }
    classify_ps_rows(pid_t, &output.stdout)
}

#[cfg(unix)]
fn classify_ps_rows(pid_t: i32, stdout: &[u8]) -> GatewayProcessProbe {
    let output = String::from_utf8_lossy(stdout);
    let mut rows = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let Some(row) = rows.next() else {
        return GatewayProcessProbe::Unknown;
    };
    if rows.next().is_some() {
        return GatewayProcessProbe::Unknown;
    }

    let mut fields = row.split_whitespace();
    let Some(row_pid) = fields.next().and_then(|pid| pid.parse::<i32>().ok()) else {
        return GatewayProcessProbe::Unknown;
    };
    let Some(state) = fields.next() else {
        return GatewayProcessProbe::Unknown;
    };
    if row_pid != pid_t || fields.next().is_some() {
        return GatewayProcessProbe::Unknown;
    }
    if state.starts_with('Z') {
        // A zombie has already exited; it cannot serve traffic and its parent
        // will reap it, so it counts as confirmed absent for lifecycle use.
        GatewayProcessProbe::ConfirmedAbsent
    } else {
        GatewayProcessProbe::Running
    }
}

/// Compatibility predicate: true only for a confirmed running process.
///
/// Lifecycle decisions use [`probe_gateway_process`] directly so an
/// [`GatewayProcessProbe::Unknown`] result is never collapsed to `false`.
#[cfg(unix)]
pub fn process_is_running(pid: u32) -> bool {
    probe_gateway_process(pid).is_running()
}

#[cfg(not(unix))]
pub fn process_is_running(pid: u32) -> bool {
    // On non-Unix, conservatively assume running.
    checked_gateway_pid(pid).is_some()
}

#[cfg(not(unix))]
pub fn probe_gateway_process(pid: u32) -> GatewayProcessProbe {
    // On non-Unix there is no portable probe; report a valid domain as running
    // so lifecycle callers never claim a stop they did not observe.
    if checked_gateway_pid(pid).is_some() {
        GatewayProcessProbe::Running
    } else {
        GatewayProcessProbe::ConfirmedAbsent
    }
}

/// Get the status of the gateway, if running.
pub fn get_status(config: &GatewayConfig) -> Result<GatewayStatus, GatewayError> {
    get_verified_gateway_status(config).map(|verified| verified.status)
}

/// Read status only when the persisted gateway generation matches the live
/// process. The callback may perform the bounded elevated read-only identity
/// query; it receives the already validated record snapshot and cannot select
/// a different target.
pub fn get_verified_gateway_status_with(
    config: &GatewayConfig,
    identity_probe: impl Fn(
        &identity::GatewayIdentityRecord,
        &GatewayRecordSnapshot,
    ) -> GatewayIdentityProbe,
) -> Result<VerifiedGatewayStatus, GatewayError> {
    get_verified_gateway_status_with_probes(config, probe_gateway_process, identity_probe)
}

fn get_verified_gateway_status_with_probes(
    config: &GatewayConfig,
    process_probe: impl Fn(u32) -> GatewayProcessProbe,
    identity_probe: impl Fn(
        &identity::GatewayIdentityRecord,
        &GatewayRecordSnapshot,
    ) -> GatewayIdentityProbe,
) -> Result<VerifiedGatewayStatus, GatewayError> {
    config.validate_private_mode()?;
    let Some(snapshot) = identity::read_snapshot(&config.pid_file_path)? else {
        return Err(GatewayError::NotRunning);
    };
    let pid = snapshot.pid();

    // The command reading this PID file is not the detached daemon it owns.
    // Treat a self-reference as unverifiable without probing or removing it.
    if pid == std::process::id() {
        return Err(GatewayError::ProcessStateUnknown { pid });
    }

    let Some(record) = snapshot.record() else {
        if !snapshot.is_legacy_pid_only() {
            return Err(GatewayError::ProcessStateUnknown { pid });
        }
        return match process_probe(pid) {
            GatewayProcessProbe::Running | GatewayProcessProbe::ConfirmedAbsent => {
                Err(GatewayError::LegacyIdentityRequired { pid })
            }
            GatewayProcessProbe::Unknown => Err(GatewayError::ProcessStateUnknown { pid }),
        };
    };

    match process_probe(pid) {
        GatewayProcessProbe::Running => {}
        GatewayProcessProbe::ConfirmedAbsent => {
            // Status is read-only. Stale authenticated records are cleared
            // after the transition lock, by start (`check_existing_gateway_pid`)
            // or stop compare-and-remove.
            return Err(GatewayError::NotRunning);
        }
        GatewayProcessProbe::Unknown => {
            // Ambiguous probe: keep the PID and version records in place for
            // reconciliation and refuse to claim the gateway is stopped.
            return Err(GatewayError::ProcessStateUnknown { pid });
        }
    }

    match identity_probe(record, &snapshot) {
        GatewayIdentityProbe::Matched => {}
        GatewayIdentityProbe::Mismatch => {
            return Err(GatewayError::NotRunning);
        }
        GatewayIdentityProbe::PermissionDenied | GatewayIdentityProbe::Unknown => {
            return Err(GatewayError::ProcessStateUnknown { pid });
        }
    }

    let table = RouteTable::load(&config.route_table_path)?;
    let (dns_addr, proxy_addr, https_addr) = if config.is_private() {
        let record = read_private_gateway_addresses(config, &snapshot)?;
        (record.dns_addr, record.proxy_addr, Some(record.https_addr))
    } else {
        (
            config.dns.bind_addr,
            config.proxy.bind_addr,
            config.proxy.tls_bind_addr,
        )
    };

    Ok(VerifiedGatewayStatus {
        status: GatewayStatus {
            pid,
            dns_addr,
            proxy_addr,
            https_addr,
            route_count: table.len(),
            routes: table.all_routes().into_iter().cloned().collect(),
            binary_version: read_gateway_version_file(&gateway_version_file_for(
                &config.pid_file_path,
            ))?,
        },
        snapshot,
    })
}

fn read_private_gateway_addresses(
    config: &GatewayConfig,
    snapshot: &GatewayRecordSnapshot,
) -> Result<PrivateGatewayAddressRecord, GatewayError> {
    let path = config
        .private_state_root()
        .map(crate::private_state::private_endpoint_path)
        .ok_or_else(|| {
            GatewayError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "private gateway endpoint receipt requested for operator state",
            ))
        })?;
    let bytes = std::fs::read(&path).map_err(|error| {
        GatewayError::Io(std::io::Error::new(
            error.kind(),
            format!(
                "private gateway endpoint receipt {} is unavailable: {error}",
                path.display()
            ),
        ))
    })?;
    let record: PrivateGatewayAddressRecord = serde_json::from_slice(&bytes).map_err(|error| {
        GatewayError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "invalid private gateway endpoint receipt {}: {error}",
                path.display()
            ),
        ))
    })?;
    if record.pid != snapshot.pid()
        || snapshot.digest().as_deref() != Some(record.identity_digest.as_str())
        || [record.dns_addr, record.proxy_addr, record.https_addr]
            .into_iter()
            .any(|address| !address.ip().is_loopback() || address.port() < 1024)
    {
        return Err(GatewayError::ProcessStateUnknown {
            pid: snapshot.pid(),
        });
    }
    Ok(record)
}

/// Remove the private listener receipt only when it still belongs to the
/// exact gateway generation captured by the caller.
pub fn remove_private_gateway_addresses_if_unchanged(
    config: &GatewayConfig,
    snapshot: &GatewayRecordSnapshot,
) -> Result<bool, GatewayError> {
    config.validate_private_mode()?;
    let Some(root) = config.private_state_root() else {
        return Ok(false);
    };
    let path = crate::private_state::private_endpoint_path(root);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(GatewayError::Io(error)),
    };
    let record: PrivateGatewayAddressRecord = serde_json::from_slice(&bytes).map_err(|error| {
        GatewayError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "invalid private gateway endpoint receipt {}: {error}",
                path.display()
            ),
        ))
    })?;
    if record.pid != snapshot.pid()
        || snapshot.digest().as_deref() != Some(record.identity_digest.as_str())
        || std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice())
    {
        return Ok(false);
    }
    std::fs::remove_file(path).map_err(GatewayError::Io)?;
    Ok(true)
}

async fn bind_private_gateway_listeners(
    config: &GatewayConfig,
) -> Result<PrivateGatewayListeners, GatewayError> {
    let dns = UdpSocket::bind(config.dns.bind_addr)
        .await
        .map_err(|error| GatewayError::DnsBindError {
            addr: config.dns.bind_addr.to_string(),
            reason: error.to_string(),
        })?;
    let proxy = TcpListener::bind(config.proxy.bind_addr)
        .await
        .map_err(|error| GatewayError::ProxyBindError {
            addr: config.proxy.bind_addr.to_string(),
            reason: error.to_string(),
        })?;
    let https_bind_addr =
        config
            .proxy
            .tls_bind_addr
            .ok_or_else(|| GatewayError::ProxyBindError {
                addr: "<missing>".to_owned(),
                reason: "private gateway requires an HTTPS listener".to_owned(),
            })?;
    let https =
        TcpListener::bind(https_bind_addr)
            .await
            .map_err(|error| GatewayError::ProxyBindError {
                addr: https_bind_addr.to_string(),
                reason: format!("HTTPS bind failed: {error}"),
            })?;
    let listeners = PrivateGatewayListeners {
        dns_addr: dns.local_addr().map_err(GatewayError::Io)?,
        proxy_addr: proxy.local_addr().map_err(GatewayError::Io)?,
        https_addr: https.local_addr().map_err(GatewayError::Io)?,
        dns,
        proxy,
        https,
    };
    if [
        listeners.dns_addr,
        listeners.proxy_addr,
        listeners.https_addr,
    ]
    .into_iter()
    .any(|address| !address.ip().is_loopback() || address.port() < 1024)
    {
        return Err(GatewayError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "operating system selected an unsafe private gateway endpoint",
        )));
    }
    Ok(listeners)
}

fn write_private_gateway_addresses(
    path: &Path,
    record: &PrivateGatewayAddressRecord,
) -> Result<Vec<u8>, GatewayError> {
    let bytes = serde_json::to_vec(record).map_err(|error| {
        GatewayError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to encode private gateway endpoints: {error}"),
        ))
    })?;
    let temp_path = crate::atomic_write::temp_path(path, "addresses");
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temp_path)
            .map_err(GatewayError::Io)?
    };
    #[cfg(not(unix))]
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)
        .map_err(GatewayError::Io)?;
    file.write_all(&bytes).map_err(GatewayError::Io)?;
    file.sync_all().map_err(GatewayError::Io)?;
    std::fs::rename(&temp_path, path).map_err(GatewayError::Io)?;
    Ok(bytes)
}

/// Read status using this process's kernel identity access. Ordinary
/// unelevated callers may use the bounded runner-side reader when the kernel
/// denies access to a privileged gateway process.
pub fn get_verified_gateway_status(
    config: &GatewayConfig,
) -> Result<VerifiedGatewayStatus, GatewayError> {
    get_verified_gateway_status_with(config, |record, _| identity::probe_live_identity(record))
}

#[cfg(test)]
fn get_status_with_probe(
    config: &GatewayConfig,
    process_probe: impl Fn(u32) -> GatewayProcessProbe,
) -> Result<GatewayStatus, GatewayError> {
    get_verified_gateway_status_with_probes(config, process_probe, |_, _| {
        GatewayIdentityProbe::Matched
    })
    .map(|verified| verified.status)
}

/// Refuse to start when an existing gateway PID is live or cannot be probed.
///
/// A stale record is cleared only when the probe confirms absence or a
/// readable live identity proves the recorded generation has been replaced.
/// An unavailable identity/probe preserves records and refuses replacement.
fn check_existing_gateway_pid(
    config: &GatewayConfig,
    probe: impl Fn(u32) -> GatewayProcessProbe,
) -> Result<(), GatewayError> {
    check_existing_gateway_pid_with(config, probe, identity::probe_live_identity)
}

fn check_existing_gateway_pid_with(
    config: &GatewayConfig,
    probe: impl Fn(u32) -> GatewayProcessProbe,
    identity_probe: impl Fn(&identity::GatewayIdentityRecord) -> GatewayIdentityProbe,
) -> Result<(), GatewayError> {
    if let Some(snapshot) = identity::read_snapshot(&config.pid_file_path)? {
        let pid = snapshot.pid();
        let Some(record) = snapshot.record() else {
            if !snapshot.is_legacy_pid_only() || pid == std::process::id() {
                return Err(GatewayError::ProcessStateUnknown { pid });
            }
            return match probe(pid) {
                GatewayProcessProbe::Running | GatewayProcessProbe::ConfirmedAbsent => {
                    Err(GatewayError::LegacyIdentityRequired { pid })
                }
                GatewayProcessProbe::Unknown => Err(GatewayError::ProcessStateUnknown { pid }),
            };
        };
        match probe(pid) {
            GatewayProcessProbe::Running => match identity_probe(record) {
                GatewayIdentityProbe::Matched => {
                    return Err(GatewayError::AlreadyRunning { pid });
                }
                GatewayIdentityProbe::Mismatch => {
                    clear_private_addresses_for_snapshot(config, &snapshot)?;
                    if !identity::remove_if_unchanged(&snapshot)? {
                        return Err(GatewayError::ProcessStateUnknown { pid });
                    }
                }
                GatewayIdentityProbe::PermissionDenied | GatewayIdentityProbe::Unknown => {
                    return Err(GatewayError::ProcessStateUnknown { pid });
                }
            },
            GatewayProcessProbe::ConfirmedAbsent => {
                clear_private_addresses_for_snapshot(config, &snapshot)?;
                if !identity::remove_if_unchanged(&snapshot)? {
                    return Err(GatewayError::ProcessStateUnknown { pid });
                }
            }
            GatewayProcessProbe::Unknown => {
                return Err(GatewayError::ProcessStateUnknown { pid });
            }
        }
    }
    Ok(())
}

fn clear_private_addresses_for_snapshot(
    config: &GatewayConfig,
    snapshot: &GatewayRecordSnapshot,
) -> Result<(), GatewayError> {
    if config.is_private() && !remove_private_gateway_addresses_if_unchanged(config, snapshot)? {
        return Err(GatewayError::ProcessStateUnknown {
            pid: snapshot.pid(),
        });
    }
    Ok(())
}

/// Run the gateway server.
///
/// This function blocks until a shutdown signal is received (SIGTERM/SIGINT).
/// It starts the DNS resolver, HTTP proxy, and route table file watcher
/// concurrently.
pub async fn run_gateway(config: GatewayConfig) -> Result<(), GatewayError> {
    config.validate_private_mode()?;
    if let Some(root) = config.private_state_root() {
        crate::private_state::prepare_root(root)?;
    }

    // Check if already running.
    check_existing_gateway_pid(&config, probe_gateway_process)?;

    if config.is_private()
        && config.route_table_path.exists()
        && crate::trust::load_trusted(&config.route_table_path)?.is_none()
    {
        return Err(GatewayError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "private gateway refuses an untrusted route table and preserves it",
        )));
    }

    // Private listeners are bound before publishing process identity. A
    // collision therefore leaves no live-generation record and reports only
    // endpoints held by this private fixture.
    let live_table = LiveRouteTable::new(config.route_table_path.clone())?;
    let shared_table = live_table.shared_table();
    if config.is_private() {
        validate_private_routes(&crate::locks::read_tolerant(&shared_table))?;
    }
    let mut runtime_config = config.clone();
    let private_tls_resolver = if config.is_private() {
        let tls_config = config.tls.as_ref().ok_or_else(|| GatewayError::TlsError {
            domain: "<CA>".to_owned(),
            reason: "private gateway requires TLS configuration".to_owned(),
        })?;
        if !tls_config
            .private_ca_cert_path()
            .is_some_and(|path| path.is_file())
        {
            return Err(GatewayError::TlsError {
                domain: "<CA>".to_owned(),
                reason: "private gateway requires its fixture CA certificate under the selected state root".to_owned(),
            });
        }
        let resolver = Arc::new(SniCertResolver::new());
        let cert_count = sync_private_sni_resolver_from_dir(&resolver, &tls_config.certs_dir)?;
        if cert_count == 0 {
            return Err(GatewayError::TlsError {
                domain: "<CA>".to_owned(),
                reason: format!(
                    "private HTTPS startup requires a certificate under {}",
                    tls_config.certs_dir.display()
                ),
            });
        }
        Some((resolver, cert_count))
    } else {
        None
    };
    if let Some(root) = config.private_state_root() {
        let address_path = crate::private_state::private_endpoint_path(root);
        if address_path.exists() {
            return Err(GatewayError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "private gateway endpoint receipt {} has no active startup owner; preserving it",
                    address_path.display()
                ),
            )));
        }
    }
    let private_listeners = if config.is_private() {
        let listeners = bind_private_gateway_listeners(&config).await?;
        runtime_config.dns.bind_addr = listeners.dns_addr;
        runtime_config.proxy.bind_addr = listeners.proxy_addr;
        runtime_config.proxy.tls_bind_addr = Some(listeners.https_addr);
        Some(listeners)
    } else {
        None
    };

    // Publish the sidecar and decimal compatibility PID as one generation.
    // The record belongs to the operator even when this daemon runs as root.
    let operator_uid = match std::env::var("EFFIGY_GATEWAY_OPERATOR_UID") {
        Ok(value) => value.parse::<u32>().map_err(|_| {
            GatewayError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "gateway operator UID is malformed",
            ))
        })?,
        Err(std::env::VarError::NotPresent) => nix::unistd::Uid::effective().as_raw(),
        Err(error) => return Err(GatewayError::Io(std::io::Error::other(error))),
    };
    let published_snapshot =
        identity::publish_current_gateway(&config.pid_file_path, operator_uid)?;
    let mut private_record_guard = config.private_state_root().map(|root| {
        PrivateStartupRecordGuard::new(
            published_snapshot.clone(),
            crate::private_state::private_endpoint_path(root),
        )
    });
    write_gateway_version_file(&config.pid_file_path)?;
    if let (Some(listeners), Some(guard)) = (&private_listeners, &mut private_record_guard) {
        let record = PrivateGatewayAddressRecord {
            pid: published_snapshot.pid(),
            identity_digest: published_snapshot.digest().ok_or_else(|| {
                GatewayError::ProcessStateUnknown {
                    pid: published_snapshot.pid(),
                }
            })?,
            dns_addr: listeners.dns_addr,
            proxy_addr: listeners.proxy_addr,
            https_addr: listeners.https_addr,
        };
        let bytes = write_private_gateway_addresses(&guard.address_path, &record)?;
        guard.record_address_bytes(bytes);
    }

    // Create stats tracker.
    let stats = Arc::new(GatewayStats::new());

    // Create DNS lookup cache (shared with file watcher for invalidation).
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));

    // Create shutdown channel.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Set up OS signal handler.
    let signal_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("received shutdown signal");
        let _ = signal_tx.send(true);
    });

    // Reconcile route-driven `/etc/resolver/` files against the
    // initial route table. Best-effort — we run as root here, but
    // surfacing fs errors aborts the daemon, which is wrong for a
    // resolver-side concern. Just log and continue.
    reconcile_route_resolver_files_from_table(&shared_table, &runtime_config);

    // Set up file watcher for route table.
    // When routes change, the watcher reloads the table, clears the
    // DNS cache so new routes are picked up immediately, and
    // re-reconciles `/etc/resolver/` so non-managed-TLD route domains
    // get system resolver files written/removed in lockstep.
    let watcher_table = Arc::clone(&shared_table);
    let watcher_cache = Arc::clone(&dns_cache);
    let watcher_path = runtime_config.route_table_path.clone();
    let watcher_config = runtime_config.clone();
    let idle_shutdown_generation = Arc::new(AtomicU64::new(0));
    let _watcher = setup_file_watcher(
        &watcher_path,
        watcher_table,
        watcher_cache,
        idle_shutdown_generation,
        shutdown_tx.clone(),
        watcher_config,
    )?;

    let has_tls = runtime_config.proxy.tls_bind_addr.is_some() && runtime_config.tls.is_some();

    info!(
        dns = %runtime_config.dns.bind_addr,
        proxy = %runtime_config.proxy.bind_addr,
        tls = has_tls,
        tld = %runtime_config.dns.tld,
        "gateway starting"
    );

    // Run DNS and proxy concurrently.
    let (dns_handle, proxy_handle, private_https_listener) = match private_listeners {
        Some(listeners) => (
            tokio::spawn(run_dns_server_on(
                listeners.dns,
                runtime_config.dns.clone(),
                Arc::clone(&shared_table),
                Arc::clone(&stats),
                Arc::clone(&dns_cache),
                shutdown_rx.clone(),
            )),
            tokio::spawn(run_proxy_server_on(
                listeners.proxy,
                runtime_config.proxy.clone(),
                Arc::clone(&shared_table),
                Arc::clone(&stats),
                shutdown_rx.clone(),
            )),
            Some(listeners.https),
        ),
        None => (
            tokio::spawn(run_dns_server(
                runtime_config.dns.clone(),
                Arc::clone(&shared_table),
                Arc::clone(&stats),
                Arc::clone(&dns_cache),
                shutdown_rx.clone(),
            )),
            tokio::spawn(run_proxy_server(
                runtime_config.proxy.clone(),
                Arc::clone(&shared_table),
                Arc::clone(&stats),
                shutdown_rx.clone(),
            )),
            None,
        ),
    };

    let tcp_alias_handle = tokio::spawn(run_tcp_alias_manager(
        Arc::clone(&shared_table),
        shutdown_rx.clone(),
    ));

    // Optionally start the HTTPS proxy.
    let (_tls_watcher, tls_handle) = if let (Some(tls_addr), Some(tls_config)) =
        (runtime_config.proxy.tls_bind_addr, &runtime_config.tls)
    {
        let certs_dir = tls_config.certs_dir.clone();
        if !runtime_config.is_private() {
            std::fs::create_dir_all(&certs_dir)?;
        }
        let (resolver, cert_count) = match private_tls_resolver {
            Some((resolver, count)) => (resolver, count),
            None => {
                let resolver = Arc::new(SniCertResolver::new());
                let count = sync_sni_resolver_from_dir(&resolver, &certs_dir)?;
                (resolver, count)
            }
        };
        if cert_count == 0 {
            info!(
                "HTTPS enabled but no certificates found in {} — \
                     run `effigy gateway setup-tls` and start a TLS-enabled route",
                certs_dir.display()
            );
        } else {
            info!(certs = cert_count, "loaded TLS certificates for HTTPS");
        }
        let tls_watcher = setup_tls_watcher(
            &certs_dir,
            Arc::clone(&resolver),
            runtime_config.is_private(),
        )?;
        let server_config = Arc::new(server_config_from_resolver(resolver));
        let handle = match private_https_listener {
            Some(listener) => tokio::spawn(run_tls_proxy_server_on(
                listener,
                server_config,
                Arc::clone(&shared_table),
                Arc::clone(&stats),
                runtime_config.proxy.clone(),
                shutdown_rx,
            )),
            None => tokio::spawn(run_tls_proxy_server(
                tls_addr,
                server_config,
                Arc::clone(&shared_table),
                Arc::clone(&stats),
                runtime_config.proxy.clone(),
                shutdown_rx,
            )),
        };
        (Some(tls_watcher), Some(handle))
    } else {
        (None, None)
    };

    // Wait for any server task to finish (typically all stop on shutdown).
    tokio::select! {
        result = dns_handle => handle_server_task_result(result, "DNS server")?,
        result = proxy_handle => handle_server_task_result(result, "proxy server")?,
        result = tcp_alias_handle => handle_server_task_result(result, "TCP alias manager")?,
        result = async {
            if let Some(handle) = tls_handle {
                handle.await
            } else {
                // No TLS handle — never resolves, so the other branches win.
                std::future::pending().await
            }
        } => handle_server_task_result(result, "HTTPS server")?,
    }

    // Clean up PID file.
    if let Some(guard) = &mut private_record_guard {
        guard.finish()?;
    } else {
        identity::remove_if_unchanged(&published_snapshot)?;
    }

    info!("gateway stopped");
    Ok(())
}

/// Set up a filesystem watcher that reloads the route table when it changes.
fn setup_file_watcher(
    path: &Path,
    table: Arc<RwLock<RouteTable>>,
    dns_cache: Arc<DnsCache>,
    idle_shutdown_generation: Arc<AtomicU64>,
    shutdown_tx: watch::Sender<bool>,
    gateway_config: GatewayConfig,
) -> Result<RecommendedWatcher, GatewayError> {
    let watched_path = path.to_path_buf();

    let mut watcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(ev) => {
                use notify::EventKind;
                match ev.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        debug!(path = %watched_path.display(), "route table changed, reloading");
                        if let Err(error) = reload_route_table_and_maybe_schedule_shutdown(
                            &watched_path,
                            &table,
                            &dns_cache,
                            &idle_shutdown_generation,
                            &shutdown_tx,
                            gateway_config.is_private(),
                        ) {
                            error!(error = %error, "failed to reload route table");
                        }
                        reconcile_route_resolver_files_from_table(&table, &gateway_config);
                    }
                    _ => {}
                }
            }
            Err(e) => {
                error!(error = %e, "file watcher error");
            }
        })
        .map_err(|e| GatewayError::WatcherError(e.to_string()))?;

    // Watch the parent directory (the file might not exist yet).
    let watch_dir = path.parent().unwrap_or(path.as_ref());
    if watch_dir.exists() {
        watcher
            .watch(watch_dir, RecursiveMode::NonRecursive)
            .map_err(|e| GatewayError::WatcherError(e.to_string()))?;
    }

    Ok(watcher)
}

fn reload_route_table_and_maybe_schedule_shutdown(
    path: &Path,
    table: &Arc<RwLock<RouteTable>>,
    dns_cache: &Arc<DnsCache>,
    idle_shutdown_generation: &Arc<AtomicU64>,
    shutdown_tx: &watch::Sender<bool>,
    private: bool,
) -> Result<(), GatewayError> {
    // Enforce the read-path trust gate (contract 033). An untrusted file keeps
    // the last-known-good in-memory table rather than being adopted.
    let Some(new_table) = crate::trust::load_trusted(path)? else {
        return Ok(());
    };
    if private {
        validate_private_routes(&new_table)?;
    }
    let action = apply_reloaded_route_table(table, new_table, dns_cache);
    debug!("route table reloaded, DNS cache cleared");
    match action {
        IdleShutdownAction::Arm => {
            let generation = idle_shutdown_generation.fetch_add(1, Ordering::SeqCst) + 1;
            schedule_idle_shutdown(
                Arc::clone(table),
                Arc::clone(idle_shutdown_generation),
                shutdown_tx.clone(),
                generation,
                IDLE_SHUTDOWN_DELAY,
            );
        }
        IdleShutdownAction::Cancel => {
            idle_shutdown_generation.fetch_add(1, Ordering::SeqCst);
            debug!("route table became non-empty; cancelled pending idle shutdown");
        }
        IdleShutdownAction::None => {}
    }
    Ok(())
}

fn validate_private_routes(table: &RouteTable) -> Result<(), GatewayError> {
    if table.all_routes().iter().any(|route| {
        route.dns_ip.is_some_and(|address| !address.is_loopback())
            || route.tcp_port.is_some_and(|port| port < 1024)
    }) {
        return Err(GatewayError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "private gateway routes may bind only unprivileged loopback aliases",
        )));
    }
    Ok(())
}

fn apply_reloaded_route_table(
    table: &Arc<RwLock<RouteTable>>,
    new_table: RouteTable,
    dns_cache: &Arc<DnsCache>,
) -> IdleShutdownAction {
    let action = {
        let mut guard = crate::locks::write_tolerant(table);
        let was_empty = guard.is_empty();
        let is_empty = new_table.is_empty();
        *guard = new_table;
        match (was_empty, is_empty) {
            (false, true) => IdleShutdownAction::Arm,
            (true, false) => IdleShutdownAction::Cancel,
            _ => IdleShutdownAction::None,
        }
    };
    dns_cache.clear();
    action
}

fn schedule_idle_shutdown(
    table: Arc<RwLock<RouteTable>>,
    idle_shutdown_generation: Arc<AtomicU64>,
    shutdown_tx: watch::Sender<bool>,
    generation: u64,
    delay: Duration,
) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if idle_shutdown_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        if crate::locks::read_tolerant(&table).is_empty() {
            info!("route table stayed empty through idle timeout; stopping gateway");
            let _ = shutdown_tx.send(true);
        }
    });
}

fn setup_tls_watcher(
    certs_dir: &Path,
    resolver: Arc<SniCertResolver>,
    private: bool,
) -> Result<RecommendedWatcher, GatewayError> {
    let watched_dir = certs_dir.to_path_buf();

    let mut watcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(ev) => {
                use notify::EventKind;
                match ev.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        debug!(path = %watched_dir.display(), "TLS cert directory changed, reloading");
                        let reload = if private {
                            sync_private_sni_resolver_from_dir(&resolver, &watched_dir)
                        } else {
                            sync_sni_resolver_from_dir(&resolver, &watched_dir)
                        };
                        match reload {
                            Ok(count) => {
                                debug!(certs = count, "TLS certs reloaded");
                            }
                            Err(error) => {
                                error!(error = %error, "failed to reload TLS certs");
                            }
                        }
                    }
                    _ => {}
                }
            }
            Err(error) => {
                error!(error = %error, "TLS watcher error");
            }
        })
        .map_err(|e| GatewayError::WatcherError(e.to_string()))?;

    std::fs::create_dir_all(certs_dir)?;
    watcher
        .watch(certs_dir, RecursiveMode::NonRecursive)
        .map_err(|e| GatewayError::WatcherError(e.to_string()))?;

    Ok(watcher)
}

/// Reconcile macOS `/etc/resolver/<suffix>` files against the current
/// set of route domains that fall outside the managed TLD.
///
/// Runs inside the gateway daemon (which runs as root via the
/// elevation flow), so writes go directly without sudo. Best-effort:
/// resolver hygiene must never crash the gateway, so any error is
/// logged and swallowed.
#[cfg(target_os = "macos")]
fn reconcile_route_resolver_files_from_table(
    table: &Arc<RwLock<RouteTable>>,
    config: &GatewayConfig,
) {
    if config.is_private() {
        return;
    }
    let domains: Vec<String> = {
        let guard = crate::locks::read_tolerant(table);
        guard
            .all_routes()
            .into_iter()
            .map(|route| route.domain.clone())
            .collect()
    };
    let suffixes = resolver_setup::route_driven_resolver_suffixes(domains, &config.dns.tld);
    let port = config.dns.bind_addr.port();
    match resolver_setup::reconcile_route_resolver_files(&suffixes, &config.dns.tld, port) {
        Ok(outcome) => {
            if !outcome.added.is_empty() || !outcome.removed.is_empty() {
                info!(
                    added = outcome.added.len(),
                    removed = outcome.removed.len(),
                    "reconciled route-driven /etc/resolver files",
                );
            }
        }
        Err(error) => {
            error!(error = %error, "failed to reconcile route-driven /etc/resolver files");
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn reconcile_route_resolver_files_from_table(
    _table: &Arc<RwLock<RouteTable>>,
    _config: &GatewayConfig,
) {
    // No /etc/resolver/ equivalent off macOS — nothing to do.
}

fn handle_server_task_result(
    result: Result<Result<(), GatewayError>, tokio::task::JoinError>,
    label: &str,
) -> Result<(), GatewayError> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            error!(error = %error, "{label} failed");
            Err(error)
        }
        Err(error) => {
            error!(error = %error, "{label} task failed");
            Err(GatewayError::WatcherError(format!(
                "{label} task failed: {error}"
            )))
        }
    }
}

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;
