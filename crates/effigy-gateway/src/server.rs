//! Gateway server lifecycle — coordinates DNS, proxy, and route watching.
//!
//! The gateway runs as a background process with three concurrent tasks:
//!
//! 1. DNS resolver (UDP)
//! 2. HTTP reverse proxy (TCP)
//! 3. Endpoint Store watcher
//!
//! All three tasks share the route table via `Arc<RwLock<RouteTable>>`.
//! The watcher reloads committed snapshots when the kernel database changes. Shutdown
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
use crate::routes::{LiveRouteTable, RouteTable};
use crate::stats::GatewayStats;
use crate::tcp_alias::run_tcp_alias_manager;
use crate::tls::{
    server_config_from_resolver, sync_private_sni_resolver_from_dir, sync_sni_resolver_from_dir,
    SniCertResolver, TlsConfig,
};

const IDLE_SHUTDOWN_DELAY: Duration = Duration::from_secs(5 * 60);

/// How often the daemon drops cached certificates no route names.
const CERT_PRUNE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

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

    /// Path to the PID file for lifecycle management.
    pub pid_file_path: PathBuf,

    /// State and host-effect policy for this gateway instance.
    pub mode: GatewayMode,

    /// Opt the daemon into adopting launchd-activated loopback listeners
    /// (HTTP `127.0.0.1:80`, HTTPS `127.0.0.1:443`) instead of binding them.
    /// Only the administrator-installed launchd job sets this flag.
    pub listener_handoff: bool,
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
            pid_file_path: gateway_dir.join("gateway.pid"),
            mode: GatewayMode::Operator,
            listener_handoff: false,
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
        if self.listener_handoff {
            return Err(GatewayError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "private gateway mode never adopts launchd handoff listeners",
            )));
        }
        crate::private_state::validate_root(policy.state_root())?;
        let root = policy.state_root();
        let tls_paths_match = self.tls.as_ref().is_some_and(|tls| {
            tls.certs_dir == root.join("certs")
                && tls.ca_root.as_deref() == Some(root.join("ca").as_path())
        });
        if self.route_table_path != root.join("routes.json")
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

    /// Opt the daemon into adopting launchd-activated loopback listeners.
    pub fn with_listener_handoff(mut self, enabled: bool) -> Self {
        self.listener_handoff = enabled;
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

    /// Stable label used by the rendered and JSON diagnosis.
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::ConfirmedAbsent => "confirmed-absent",
            Self::Unknown => "unknown",
        }
    }
}

/// Which persisted record shape a read-only diagnosis observed.
///
/// Labels only; no file bytes, paths, digests, or process arguments are
/// exposed. Used to separate a liveness-capture failure from an identity
/// verification failure without leaking raw probe output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayRecordKind {
    /// A versioned sidecar is present (valid or not); identity decides.
    Authenticated,
    /// Decimal PID with no sidecar at all.
    LegacyPidOnly,
    /// Present but untrusted: malformed, mismatched, unsafe, or symlink.
    UntrustedOrMalformed,
    /// This caller may not read the record at all (kernel permission denial).
    /// Kept apart from an untrusted record so headless callers can tell a
    /// root-owned record they cannot open from a malformed one.
    Unreadable,
    /// No PID record at all.
    Absent,
    /// The record names this process; never probed or signalled.
    SelfReference,
}

impl GatewayRecordKind {
    /// Stable label used by the rendered and JSON diagnosis.
    pub fn label(self) -> &'static str {
        match self {
            Self::Authenticated => "authenticated",
            Self::LegacyPidOnly => "legacy-pid-only",
            Self::UntrustedOrMalformed => "untrusted-or-malformed",
            Self::Unreadable => "unreadable",
            Self::Absent => "absent",
            Self::SelfReference => "self-reference",
        }
    }
}

/// Local identity outcome used by the read-only diagnosis.
///
/// `PermissionDenied` is distinct from `Unknown`: the kernel refused this
/// caller access to the live process identity. No elevation is performed
/// here; the runner's bounded reader remains the only approved follow-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayIdentityOutcome {
    /// PID, boot identity, and precise start identity all match.
    Matched,
    /// A readable live generation differs from the record.
    Mismatch,
    /// The kernel denied this caller access to the live identity.
    PermissionDenied,
    /// Identity could not be read or trusted.
    Unknown,
    /// No identity check applied (absent/legacy/self/unprobed record).
    NotChecked,
}

impl GatewayIdentityOutcome {
    /// Stable label used by the rendered and JSON diagnosis.
    pub fn label(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Mismatch => "mismatch",
            Self::PermissionDenied => "permission-denied",
            Self::Unknown => "unknown",
            Self::NotChecked => "not-checked",
        }
    }
}

/// Bounded structural diagnosis of the current gateway state.
///
/// Read-only: file and liveness probes only, no signals, no record
/// mutation, no elevation, no raw output. A diagnosis describes the current
/// observation, not the historical cause of an earlier failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayStatusDiagnosis {
    /// Recorded PID, when a PID record was read.
    pub pid: Option<u32>,
    /// Which record shape was observed.
    pub record: GatewayRecordKind,
    /// Liveness probe result, when a PID was probed.
    pub process_probe: Option<GatewayProcessProbe>,
    /// Local identity outcome, when an authenticated record was checked.
    pub identity: GatewayIdentityOutcome,
}

/// Diagnose the current gateway state with the production probes.
///
/// Never fails closed into a lifecycle decision: callers must still branch
/// on the verified-status result. The diagnosis is an honest same-field
/// hint only and never substitutes for a successful identity match.
pub fn diagnose_gateway_status(config: &GatewayConfig) -> GatewayStatusDiagnosis {
    diagnose_gateway_status_with(config, probe_gateway_process, identity::probe_live_identity)
}

fn diagnose_gateway_status_with(
    config: &GatewayConfig,
    process_probe: impl Fn(u32) -> GatewayProcessProbe,
    identity_probe: impl Fn(&identity::GatewayIdentityRecord) -> GatewayIdentityProbe,
) -> GatewayStatusDiagnosis {
    if config.validate_private_mode().is_err() {
        return GatewayStatusDiagnosis {
            pid: None,
            record: GatewayRecordKind::UntrustedOrMalformed,
            process_probe: None,
            identity: GatewayIdentityOutcome::NotChecked,
        };
    }
    let snapshot = match identity::read_snapshot(&config.pid_file_path) {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return GatewayStatusDiagnosis {
                pid: None,
                record: GatewayRecordKind::Absent,
                process_probe: None,
                identity: GatewayIdentityOutcome::NotChecked,
            };
        }
        Err(error) => {
            return GatewayStatusDiagnosis {
                pid: None,
                record: if identity_permission_denied_error(&error) {
                    GatewayRecordKind::Unreadable
                } else {
                    GatewayRecordKind::UntrustedOrMalformed
                },
                process_probe: None,
                identity: GatewayIdentityOutcome::NotChecked,
            };
        }
    };
    let pid = snapshot.pid();
    if pid == std::process::id() {
        return GatewayStatusDiagnosis {
            pid: Some(pid),
            record: GatewayRecordKind::SelfReference,
            process_probe: None,
            identity: GatewayIdentityOutcome::NotChecked,
        };
    }
    let Some(record) = snapshot.record() else {
        if snapshot.is_legacy_pid_only() {
            let probe = process_probe(pid);
            return GatewayStatusDiagnosis {
                pid: Some(pid),
                record: GatewayRecordKind::LegacyPidOnly,
                process_probe: Some(probe),
                identity: GatewayIdentityOutcome::NotChecked,
            };
        }
        return GatewayStatusDiagnosis {
            pid: Some(pid),
            record: GatewayRecordKind::UntrustedOrMalformed,
            process_probe: None,
            identity: GatewayIdentityOutcome::NotChecked,
        };
    };
    let probe = process_probe(pid);
    if probe != GatewayProcessProbe::Running {
        return GatewayStatusDiagnosis {
            pid: Some(pid),
            record: GatewayRecordKind::Authenticated,
            process_probe: Some(probe),
            identity: GatewayIdentityOutcome::NotChecked,
        };
    }
    let identity = match identity_probe(record) {
        GatewayIdentityProbe::Matched => GatewayIdentityOutcome::Matched,
        GatewayIdentityProbe::Mismatch => GatewayIdentityOutcome::Mismatch,
        GatewayIdentityProbe::PermissionDenied => GatewayIdentityOutcome::PermissionDenied,
        GatewayIdentityProbe::Unknown => GatewayIdentityOutcome::Unknown,
    };
    GatewayStatusDiagnosis {
        pid: Some(pid),
        record: GatewayRecordKind::Authenticated,
        process_probe: Some(probe),
        identity,
    }
}

/// Render a diagnosis with fixed labels only. No paths, digests, raw `ps`
/// output, arguments, environment, or credentials are included. The headless
/// UI flag neither grants privilege nor changes terminal requirements.
pub fn render_gateway_diagnosis(diagnosis: &GatewayStatusDiagnosis) -> String {
    let process = diagnosis
        .process_probe
        .map(GatewayProcessProbe::label)
        .unwrap_or("not-probed");
    let mut rendered = format!(
        "record={} process={} identity={}",
        diagnosis.record.label(),
        process,
        diagnosis.identity.label(),
    );
    if diagnosis.identity == GatewayIdentityOutcome::PermissionDenied {
        rendered.push_str(
            "; identity unreadable to this user (approve the bounded read-only check from an interactive terminal; noninteractive/headless use stays unknown)",
        );
    }
    if diagnosis.record == GatewayRecordKind::LegacyPidOnly {
        rendered.push_str("; pre-0.14 numeric-only record requires host administrator migration");
    }
    rendered
}

/// True when the trusted record read failed because this caller may not open
/// the record. An unreadable record is reported as such, never as malformed.
fn identity_permission_denied_error(error: &GatewayError) -> bool {
    matches!(
        error,
        GatewayError::Io(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || matches!(error.raw_os_error(), Some(libc::EACCES) | Some(libc::EPERM))
    )
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
    let https_bind_addr = config.proxy.tls_bind_addr.ok_or_else(|| {
        GatewayError::Io(std::io::Error::other("gateway requires an HTTPS listener"))
    })?;
    let direct = async {
        let proxy = TcpListener::bind(config.proxy.bind_addr).await?;
        let https = TcpListener::bind(https_bind_addr).await?;
        Ok::<_, std::io::Error>((proxy, https))
    }
    .await;
    #[cfg(target_os = "macos")]
    let direct = match direct {
        Err(error)
            if !config.is_private()
                && [config.proxy.bind_addr, https_bind_addr]
                    .iter()
                    .all(|addr| matches!(addr.port(), 80 | 443))
                && error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            let endpoints = [config.proxy.bind_addr, https_bind_addr];
            let handed =
                tokio::task::spawn_blocking(move || crate::bind_broker::request(endpoints))
                    .await
                    .map_err(std::io::Error::other)??;
            Ok(handed.into_tokio()?)
        }
        other => other,
    };
    let (proxy, https) = direct.map_err(|error| GatewayError::ProxyBindError {
        addr: config.proxy.bind_addr.to_string(),
        reason: format!("listener acquisition failed: {error}"),
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
    .any(|address| !address.ip().is_loopback() || (config.is_private() && address.port() < 1024))
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
    run_gateway_with_cancel(config, None).await
}

pub async fn run_operator_gateway(
    config: GatewayConfig,
    cancel: effigy_kernel::ctx::CancelToken,
) -> Result<(), GatewayError> {
    run_gateway_with_cancel(config, Some(cancel)).await
}

async fn run_gateway_with_cancel(
    config: GatewayConfig,
    cancel: Option<effigy_kernel::ctx::CancelToken>,
) -> Result<(), GatewayError> {
    if !config.is_private() {
        crate::operator::require_operator()?;
        if cancel.is_none() {
            return Err(GatewayError::Io(std::io::Error::other(
                "operator gateway requires the binary signal router",
            )));
        }
    }
    config.validate_private_mode()?;
    if let Some(root) = config.private_state_root() {
        crate::private_state::prepare_root(root)?;
    }

    // Check if already running.
    check_existing_gateway_pid(&config, probe_gateway_process)?;

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
    let private_listeners = if config.is_private() || !config.listener_handoff {
        let listeners = bind_private_gateway_listeners(&config).await?;
        runtime_config.dns.bind_addr = listeners.dns_addr;
        runtime_config.proxy.bind_addr = listeners.proxy_addr;
        runtime_config.proxy.tls_bind_addr = Some(listeners.https_addr);
        Some(listeners)
    } else {
        None
    };

    // The launchd handoff adopts the two administrator-owned loopback
    // listeners before anything is published. A refusal here is fail-closed:
    // no generation record, no adopted socket, no fallback bind, and the
    // daemon exits so launchd reports the failed job instead of a gateway.
    let handed_listeners = if config.listener_handoff {
        if config.tls.is_none() || config.proxy.tls_bind_addr.is_none() {
            return Err(GatewayError::ListenerHandoff(
                crate::listener_handoff::ListenerHandoffError::HttpsEndpointUnserved,
            ));
        }
        Some(crate::listener_handoff::adopt_handed_listeners()?)
    } else {
        None
    };
    let (handed_http, handed_https): (
        Option<tokio::net::TcpListener>,
        Option<tokio::net::TcpListener>,
    ) = match handed_listeners {
        Some(listeners) => {
            let (http, https) = listeners.into_tokio()?;
            (Some(http), Some(https))
        }
        None => (None, None),
    };

    // Install Unix signal handlers before publishing the generation record.
    // A setup failure must not leave a record for a daemon that cannot shut
    // down through the signals promised by this service.
    #[cfg(unix)]
    let private_signals = if config.is_private() {
        use tokio::signal::unix::SignalKind;

        Some((
            tokio::signal::unix::signal(SignalKind::terminate())?,
            tokio::signal::unix::signal(SignalKind::interrupt())?,
        ))
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
    let mut published_snapshot = if config.is_private() {
        Some(identity::publish_current_gateway(
            &config.pid_file_path,
            operator_uid,
        )?)
    } else {
        None
    };
    let mut private_record_guard = config.private_state_root().map(|root| {
        PrivateStartupRecordGuard::new(
            published_snapshot
                .as_ref()
                .expect("private record was published")
                .clone(),
            crate::private_state::private_endpoint_path(root),
        )
    });
    if config.is_private() {
        write_gateway_version_file(&config.pid_file_path)?;
    }
    if let (Some(listeners), Some(guard)) = (&private_listeners, &mut private_record_guard) {
        let record = PrivateGatewayAddressRecord {
            pid: published_snapshot
                .as_ref()
                .expect("private record was published")
                .pid(),
            identity_digest: published_snapshot
                .as_ref()
                .expect("private record was published")
                .digest()
                .ok_or_else(|| GatewayError::ProcessStateUnknown {
                    pid: published_snapshot
                        .as_ref()
                        .expect("private record was published")
                        .pid(),
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

    // Resolver and alias installation belongs exclusively to gateway setup.
    // Route reloads update the table and clear the DNS cache without host mutations.
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

    // Run DNS and proxy concurrently. Keep every task in one owned set so a
    // shutdown signal can stop and join all listeners before identity cleanup.
    // Handoff mode serves HTTP/HTTPS on the adopted launchd sockets and keeps
    // DNS, its route watcher, and the TCP alias services on their ordinary
    // unprivileged startup paths.
    let (publication_tx, publication_rx) = watch::channel(config.is_private());
    let mut server_tasks = tokio::task::JoinSet::new();
    let prebound_https_listener = match private_listeners {
        Some(listeners) => {
            let dns_config = runtime_config.dns.clone();
            let proxy_config = runtime_config.proxy.clone();
            let dns_table = Arc::clone(&shared_table);
            let proxy_table = Arc::clone(&shared_table);
            let dns_stats = Arc::clone(&stats);
            let proxy_stats = Arc::clone(&stats);
            let dns_cache = Arc::clone(&dns_cache);
            let dns_shutdown = shutdown_rx.clone();
            let mut publication_rx = publication_rx.clone();
            let proxy_shutdown = shutdown_rx.clone();
            server_tasks.spawn(async move {
                (
                    "DNS server",
                    async {
                        while !*publication_rx.borrow() {
                            if publication_rx.changed().await.is_err() {
                                return Ok(());
                            }
                        }
                        run_dns_server_on(
                            listeners.dns,
                            dns_config,
                            dns_table,
                            dns_stats,
                            dns_cache,
                            dns_shutdown,
                        )
                        .await
                    }
                    .await,
                )
            });
            server_tasks.spawn(async move {
                (
                    "proxy server",
                    run_proxy_server_on(
                        listeners.proxy,
                        proxy_config,
                        proxy_table,
                        proxy_stats,
                        proxy_shutdown,
                    )
                    .await,
                )
            });
            Some(listeners.https)
        }
        None => {
            let dns_config = runtime_config.dns.clone();
            let dns_table = Arc::clone(&shared_table);
            let dns_stats = Arc::clone(&stats);
            let dns_cache = Arc::clone(&dns_cache);
            let dns_shutdown = shutdown_rx.clone();
            server_tasks.spawn(async move {
                (
                    "DNS server",
                    run_dns_server(dns_config, dns_table, dns_stats, dns_cache, dns_shutdown).await,
                )
            });
            let proxy_config = runtime_config.proxy.clone();
            let proxy_table = Arc::clone(&shared_table);
            let proxy_stats = Arc::clone(&stats);
            let proxy_shutdown = shutdown_rx.clone();
            match handed_http {
                Some(listener) => {
                    server_tasks.spawn(async move {
                        (
                            "proxy server",
                            run_proxy_server_on(
                                listener,
                                proxy_config,
                                proxy_table,
                                proxy_stats,
                                proxy_shutdown,
                            )
                            .await,
                        )
                    });
                }
                None => {
                    server_tasks.spawn(async move {
                        (
                            "proxy server",
                            run_proxy_server(
                                proxy_config,
                                proxy_table,
                                proxy_stats,
                                proxy_shutdown,
                            )
                            .await,
                        )
                    });
                }
            }
            handed_https
        }
    };

    let alias_table = Arc::clone(&shared_table);
    let alias_shutdown = shutdown_rx.clone();
    let (alias_ready_tx, alias_ready_rx) = tokio::sync::oneshot::channel();
    let private = config.is_private();
    server_tasks.spawn(async move {
        let result = if private {
            let _ = alias_ready_tx.send(Ok(()));
            run_tcp_alias_manager(alias_table, alias_shutdown).await
        } else {
            crate::tcp_alias::run_tcp_alias_manager_ready(
                alias_table,
                alias_shutdown,
                alias_ready_tx,
            )
            .await
        };
        ("TCP alias manager", result)
    });

    // Optionally start the HTTPS proxy.
    let _tls_watcher = if let (Some(tls_addr), Some(tls_config)) =
        (runtime_config.proxy.tls_bind_addr, &runtime_config.tls)
    {
        let certs_dir = tls_config.certs_dir.clone();
        if !runtime_config.is_private() {
            std::fs::create_dir_all(&certs_dir)?;
        }
        // Certificates are a cache keyed by domain. Nothing withdraws one with
        // its route, so the daemon drops the ones no route names once they are
        // old enough to be worth nothing.
        let prune_dir = certs_dir.clone();
        let prune_table = Arc::clone(&shared_table);
        let prune_shutdown = shutdown_rx.clone();
        server_tasks.spawn(async move {
            (
                "certificate cache prune",
                run_cert_cache_prune(prune_dir, prune_table, prune_shutdown).await,
            )
        });
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
        match prebound_https_listener {
            Some(listener) => {
                let table = Arc::clone(&shared_table);
                let tls_stats = Arc::clone(&stats);
                let proxy_config = runtime_config.proxy.clone();
                server_tasks.spawn(async move {
                    (
                        "HTTPS server",
                        run_tls_proxy_server_on(
                            listener,
                            server_config,
                            table,
                            tls_stats,
                            proxy_config,
                            shutdown_rx,
                        )
                        .await,
                    )
                });
            }
            None => {
                let table = Arc::clone(&shared_table);
                let tls_stats = Arc::clone(&stats);
                let proxy_config = runtime_config.proxy.clone();
                server_tasks.spawn(async move {
                    (
                        "HTTPS server",
                        run_tls_proxy_server(
                            tls_addr,
                            server_config,
                            table,
                            tls_stats,
                            proxy_config,
                            shutdown_rx,
                        )
                        .await,
                    )
                });
            }
        }
        Some(tls_watcher)
    } else {
        None
    };

    // Start consuming signals only after every server task has cloned the
    // shutdown receiver. The Unix handlers were registered before identity
    // publication, so a signal received during startup is buffered here
    // instead of being sent before later receivers can observe it.
    let signal_tx = shutdown_tx.clone();
    #[cfg(unix)]
    let signal_task = tokio::spawn(async move {
        if let Some((mut terminate_signal, mut interrupt_signal)) = private_signals {
            tokio::select! {
                _ = terminate_signal.recv() => {},
                _ = interrupt_signal.recv() => {},
            }
        } else if let Some(cancel) = cancel {
            while cancel.cause().is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        let _ = signal_tx.send(true);
    });
    if !config.is_private() {
        let ready = match tokio::time::timeout(Duration::from_secs(2), alias_ready_rx).await {
            Ok(Ok(Ok(()))) => {
                let config = runtime_config.clone();
                tokio::task::spawn_blocking(move || {
                    crate::operator::http_test(&config, Duration::from_secs(2))
                })
                .await
                .map_err(std::io::Error::other)
                .and_then(|result| result)
            }
            Ok(Ok(Err(error))) => Err(std::io::Error::other(format!(
                "TCP alias startup failed: the alias listener reported {error}"
            ))),
            Ok(Err(error)) => Err(std::io::Error::other(format!(
                "TCP alias startup failed: the readiness channel closed ({error})"
            ))),
            Err(_) => Err(std::io::Error::other(
                "TCP alias startup failed: no readiness report within 2s; run `effigy gateway status` for the recorded diagnosis",
            )),
        };
        if let Err(error) = ready {
            let _ = shutdown_tx.send(true);
            drop(publication_tx);
            while server_tasks.join_next().await.is_some() {}
            signal_task.abort();
            return Err(GatewayError::Io(error));
        }
        let _ = publication_tx.send(true);
        // Observe probes use bounded blocking sockets. Keep the runtime free
        // to poll the very HTTP/DNS tasks being tested, including tasks woken
        // into the current worker's non-stealable local queue.
        let test_config = runtime_config.clone();
        let tested = tokio::task::spawn_blocking(move || {
            crate::operator::self_test(&test_config, Duration::from_secs(2))
        })
        .await
        .map_err(std::io::Error::other)
        .and_then(|result| result);
        if let Err(error) = tested {
            let _ = shutdown_tx.send(true);
            while server_tasks.join_next().await.is_some() {}
            signal_task.abort();
            return Err(GatewayError::Io(error));
        }
        published_snapshot = Some(identity::publish_current_gateway(
            &config.pid_file_path,
            operator_uid,
        )?);
        write_gateway_version_file(&config.pid_file_path)?;
    }
    #[cfg(not(unix))]
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("received shutdown signal");
                let _ = signal_tx.send(true);
            }
            Err(error) => warn!(error = %error, "failed to install shutdown signal handler"),
        }
    });

    // Any task exit starts shutdown for the rest. Drain the complete owned set
    // before removing the exact generation record or returning an error.
    let mut first_error = server_tasks
        .join_next()
        .await
        .map(joined_server_task_result)
        .and_then(Result::err);
    let _ = shutdown_tx.send(true);
    while let Some(result) = server_tasks.join_next().await {
        if first_error.is_none() {
            first_error = joined_server_task_result(result).err();
        }
    }
    if let Some(error) = first_error {
        return Err(error);
    }

    // Clean up PID file.
    if let Some(guard) = &mut private_record_guard {
        guard.finish()?;
    } else {
        identity::remove_if_unchanged(
            published_snapshot
                .as_ref()
                .expect("standard ready record was published"),
        )?;
    }

    info!("gateway stopped");
    Ok(())
}

/// Drop cached certificates no route names once their age passes
/// `CERT_CACHE_MAX_AGE`, then wait for the daemon to stop.
async fn run_cert_cache_prune(
    certs_dir: PathBuf,
    table: Arc<RwLock<RouteTable>>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), GatewayError> {
    loop {
        let routed = crate::locks::read_tolerant(&table)
            .all_routes()
            .into_iter()
            .filter(|route| route.tls)
            .map(|route| route.domain.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let dir = certs_dir.clone();
        let pruned = tokio::task::spawn_blocking(move || {
            crate::tls::prune_orphan_certs(
                &dir,
                &routed,
                crate::tls::CERT_CACHE_MAX_AGE,
                std::time::SystemTime::now(),
            )
        })
        .await
        .map_err(|error| GatewayError::WatcherError(error.to_string()))?;
        match pruned {
            Ok(domains) if !domains.is_empty() => {
                debug!(domains = ?domains, "pruned cached certificates no route names")
            }
            Ok(_) => {}
            Err(error) => debug!(error = %error, "certificate cache prune skipped"),
        }
        tokio::select! {
            _ = tokio::time::sleep(CERT_PRUNE_INTERVAL) => {}
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    return Ok(());
                }
            }
        }
    }
}

fn joined_server_task_result(
    result: Result<(&'static str, Result<(), GatewayError>), tokio::task::JoinError>,
) -> Result<(), GatewayError> {
    match result {
        Ok((label, result)) => handle_server_task_result(Ok(result), label),
        Err(error) => handle_server_task_result(Err(error), "gateway server"),
    }
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
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
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
                        // Route reload never elevates or changes platform setup.
                    }
                    _ => {}
                }
            }
            Err(e) => {
                error!(error = %e, "file watcher error");
            }
        })
        .map_err(|e| GatewayError::WatcherError(e.to_string()))?;

    // Watch the database directory, including rollback-journal removal: that
    // removal marks a commit whose earlier writes could still be unreadable.
    let store = crate::endpoint_store::EndpointStore::for_routes(path);
    let database = store.database_path();
    let watch_dir = database.parent().expect("kernel database parent");
    // A gateway normally starts before any publisher (a fresh machine, and
    // `up`/`dev` start it themselves), so the store directory it must watch
    // need not exist yet. Creating it is the same private directory the
    // store's first transaction would create; the watcher then sees that
    // first commit instead of serving an empty table.
    store.prepare_directory()?;
    watcher
        .watch(watch_dir, RecursiveMode::NonRecursive)
        .map_err(|e| GatewayError::WatcherError(e.to_string()))?;

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
    let new_table = RouteTable::load(path)?;
    if private {
        validate_private_routes(&new_table)?;
    }
    let action = apply_reloaded_route_table(table, new_table, dns_cache, private);
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
                path.parent().unwrap_or(path).to_path_buf(),
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

/// Only a private gateway stops itself when its route table stays empty. The
/// machine gateway outlives its last route, so a later `container up` or
/// `up` finds it still running (effigy#480).
fn apply_reloaded_route_table(
    table: &Arc<RwLock<RouteTable>>,
    new_table: RouteTable,
    dns_cache: &Arc<DnsCache>,
    idle_shutdown: bool,
) -> IdleShutdownAction {
    let action = {
        let mut guard = crate::locks::write_tolerant(table);
        let was_empty = guard.is_empty();
        let is_empty = new_table.is_empty();
        *guard = new_table;
        match (was_empty, is_empty) {
            (false, true) if idle_shutdown => IdleShutdownAction::Arm,
            (true, false) if idle_shutdown => IdleShutdownAction::Cancel,
            _ => IdleShutdownAction::None,
        }
    };
    dns_cache.clear();
    action
}

/// Stops the gateway when its route table is still empty after `delay`. The
/// stop is recorded in the gateway's operator log before the shutdown signal,
/// so the cause survives the clean exit.
fn schedule_idle_shutdown(
    table: Arc<RwLock<RouteTable>>,
    idle_shutdown_generation: Arc<AtomicU64>,
    shutdown_tx: watch::Sender<bool>,
    generation: u64,
    delay: Duration,
    state_dir: PathBuf,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if idle_shutdown_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        if crate::locks::read_tolerant(&table).is_empty() {
            let cause = format!("route table empty for {} s", delay.as_secs());
            effigy_kernel::run::record_gateway_operator(
                &state_dir,
                effigy_kernel::run::OperatorAction::Down,
                &cause,
            );
            info!(cause = %cause, "route table stayed empty through idle timeout; stopping gateway");
            let _ = shutdown_tx.send(true);
        }
    })
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

#[cfg(all(test, feature = "endpoint-store-faults"))]
mod endpoint_faults;
