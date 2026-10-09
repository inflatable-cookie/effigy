//! Legacy gateway record capture, candidate inspection, and transition lock.

use std::fs::{self, File, OpenOptions};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::GatewayError;
use crate::identity::{self, GatewayRecordSnapshot, LiveProcessIdentity};
use crate::server::{self, GatewayProcessProbe};

const MAX_VERSION_BYTES: usize = 256;
const MAX_EXECUTABLE_PATH: usize = 4096;
#[cfg(target_os = "macos")]
const PROC_PIDPATHINFO_MAXSIZE: u32 = 4096;

/// Transport for one gateway role endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayTransport {
    /// UDP listener (DNS).
    Udp,
    /// TCP listener (HTTP/HTTPS).
    Tcp,
}

/// One process-owned listening endpoint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GatewayEndpoint {
    /// UDP or TCP.
    pub transport: GatewayTransport,
    /// Local bind address.
    #[serde(
        serialize_with = "serialize_socket_addr",
        deserialize_with = "deserialize_socket_addr"
    )]
    pub addr: SocketAddr,
}

fn serialize_socket_addr<S: serde::Serializer>(
    addr: &SocketAddr,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&addr.to_string())
}

fn deserialize_socket_addr<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<SocketAddr, D::Error> {
    let text = String::deserialize(deserializer)?;
    text.parse().map_err(serde::de::Error::custom)
}

impl GatewayEndpoint {
    fn label(&self) -> String {
        let kind = match self.transport {
            GatewayTransport::Udp => "udp",
            GatewayTransport::Tcp => "tcp",
        };
        format!("{kind}:{}", self.addr)
    }
}

/// Canonical production role endpoints. DNS is UDP-only.
pub fn canonical_gateway_role_endpoints() -> Vec<GatewayEndpoint> {
    vec![
        GatewayEndpoint {
            transport: GatewayTransport::Udp,
            addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 15353)),
        },
        GatewayEndpoint {
            transport: GatewayTransport::Tcp,
            addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 80)),
        },
        GatewayEndpoint {
            transport: GatewayTransport::Tcp,
            addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 443)),
        },
    ]
}

/// Result of capturing the on-disk legacy records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyCapture {
    /// No PID or version files.
    Absent,
    /// Readable PID + version, no identity sidecar.
    Legacy(LegacyRecordCapture),
    /// Partial, malformed, untrusted, or identity-bearing records. Preserve.
    Unknown { reason: &'static str },
}

/// Trusted input to inspection and generation-bound stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyRecordCapture {
    /// Digest of the canonical `gateway.pid` path.
    pub target_digest: String,
    /// Validated decimal PID.
    pub pid: u32,
    /// Exact PID file bytes.
    pub pid_bytes: Vec<u8>,
    /// Exact version file bytes.
    pub version_bytes: Vec<u8>,
    /// Digest over PID bytes, a separator, and version bytes.
    pub record_digest: String,
    /// Gateway-directory owner UID.
    pub directory_owner_uid: u32,
    /// Authenticated operator UID (separate from directory owner).
    pub operator_uid: u32,
    /// Parsed version string, if UTF-8.
    pub version: Option<String>,
    pid_path: PathBuf,
}

impl LegacyRecordCapture {
    /// Canonical PID path used for this capture.
    pub fn pid_path(&self) -> &Path {
        &self.pid_path
    }

    /// Re-read and compare exact PID and version bytes.
    pub fn bytes_unchanged(&self) -> Result<bool, GatewayError> {
        match capture_legacy_record(&self.pid_path, self.operator_uid)? {
            LegacyCapture::Legacy(current) => Ok(current.pid_bytes == self.pid_bytes
                && current.version_bytes == self.version_bytes
                && current.record_digest == self.record_digest
                && current.target_digest == self.target_digest
                && current.directory_owner_uid == self.directory_owner_uid),
            _ => Ok(false),
        }
    }
}

/// Bounded live candidate evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyCandidate {
    /// PID derived from the trusted record.
    pub pid: u32,
    /// Kernel UID of the live process.
    pub candidate_uid: u32,
    /// Linux boot id / macOS kern.boottime.
    pub boot_identity: String,
    /// Precise start identity encoding.
    pub start_identity: String,
    /// Canonical live executable path.
    pub executable_path: String,
    /// Digest of that path.
    pub executable_path_digest: String,
    /// Process-owned role endpoints.
    pub role: Vec<GatewayEndpoint>,
    /// Digest of the role endpoint set.
    pub role_digest: String,
    /// Generation digest over the bound fields.
    pub candidate_digest: String,
}

/// Capture the canonical legacy record pair.
pub fn capture_legacy_record(
    pid_path: &Path,
    operator_uid: u32,
) -> Result<LegacyCapture, GatewayError> {
    let snapshot = match identity::read_snapshot(pid_path) {
        Ok(snapshot) => snapshot,
        Err(GatewayError::Io(ref error))
            if error.kind() == std::io::ErrorKind::NotFound
                && gateway_directory_absent(pid_path) =>
        {
            return Ok(LegacyCapture::Absent);
        }
        Err(_) => {
            return Ok(LegacyCapture::Unknown {
                reason: "untrusted or malformed gateway record",
            });
        }
    };
    let version_path = identity::version_path(pid_path);
    let version_bytes =
        match identity::read_trusted_sidecar_bytes(pid_path, &version_path, MAX_VERSION_BYTES) {
            Ok(bytes) => bytes,
            Err(_) => {
                return Ok(LegacyCapture::Unknown {
                    reason: "untrusted gateway.version record",
                });
            }
        };

    match (snapshot, version_bytes) {
        (None, None) => Ok(LegacyCapture::Absent),
        (None, Some(_)) | (Some(_), None) => Ok(LegacyCapture::Unknown {
            reason: "partial gateway pid/version pair",
        }),
        (Some(snapshot), Some(version_bytes)) => {
            classify_snapshot(snapshot, version_bytes, operator_uid)
        }
    }
}

fn classify_snapshot(
    snapshot: GatewayRecordSnapshot,
    version_bytes: Vec<u8>,
    operator_uid: u32,
) -> Result<LegacyCapture, GatewayError> {
    if !snapshot.is_legacy_pid_only() {
        return Ok(LegacyCapture::Unknown {
            reason: "gateway identity sidecar is present or untrusted",
        });
    }
    let Some(target_digest) = snapshot.target_digest() else {
        return Ok(LegacyCapture::Unknown {
            reason: "canonical gateway path digest is unavailable",
        });
    };
    if version_bytes.is_empty() {
        return Ok(LegacyCapture::Unknown {
            reason: "gateway.version is empty",
        });
    }
    let version = std::str::from_utf8(&version_bytes)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    Ok(LegacyCapture::Legacy(LegacyRecordCapture {
        target_digest,
        pid: snapshot.pid(),
        record_digest: record_digest(snapshot.pid_bytes(), &version_bytes),
        pid_bytes: snapshot.pid_bytes().to_vec(),
        version_bytes,
        directory_owner_uid: snapshot.owner_uid(),
        operator_uid,
        version,
        pid_path: snapshot.pid_path().to_path_buf(),
    }))
}

fn gateway_directory_absent(pid_path: &Path) -> bool {
    let Some(parent) = pid_path.parent() else {
        return false;
    };
    if parent.as_os_str().is_empty() {
        return false;
    }
    matches!(
        fs::symlink_metadata(parent),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

fn record_digest(pid_bytes: &[u8], version_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(pid_bytes);
    hasher.update([0]);
    hasher.update(version_bytes);
    hex::encode(hasher.finalize())
}

fn path_digest(path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    hex::encode(hasher.finalize())
}

fn role_digest(endpoints: &[GatewayEndpoint]) -> String {
    let mut labels: Vec<String> = endpoints.iter().map(GatewayEndpoint::label).collect();
    labels.sort();
    let mut hasher = Sha256::new();
    for label in labels {
        hasher.update(label.as_bytes());
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

/// Digest over the adopted generation fields.
#[allow(clippy::too_many_arguments)]
pub fn candidate_digest(
    target_digest: &str,
    record_digest: &str,
    pid: u32,
    candidate_uid: u32,
    boot_identity: &str,
    start_identity: &str,
    executable_path_digest: &str,
    role_digest: &str,
) -> String {
    let mut hasher = Sha256::new();
    for field in [
        target_digest,
        record_digest,
        &pid.to_string(),
        &candidate_uid.to_string(),
        boot_identity,
        start_identity,
        executable_path_digest,
        role_digest,
    ] {
        hasher.update(field.as_bytes());
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

/// Inspect a live PID for legacy recovery evidence.
pub fn inspect_legacy_candidate(
    pid: u32,
    target_digest: &str,
    record_digest: &str,
) -> Option<LegacyCandidate> {
    inspect_legacy_candidate_with_endpoints(
        pid,
        target_digest,
        record_digest,
        &canonical_gateway_role_endpoints(),
    )
}

/// Inspect using an explicit expected role set. Production always passes
/// [`canonical_gateway_role_endpoints`].
pub fn inspect_legacy_candidate_with_endpoints(
    pid: u32,
    target_digest: &str,
    record_digest: &str,
    expected: &[GatewayEndpoint],
) -> Option<LegacyCandidate> {
    server::checked_gateway_pid(pid)?;
    match server::probe_gateway_process(pid) {
        GatewayProcessProbe::Running => {}
        GatewayProcessProbe::ConfirmedAbsent | GatewayProcessProbe::Unknown => return None,
    }
    let live = identity::read_live_process_identity(pid).ok()?;
    let executable_path = read_executable_path(pid)?;
    if executable_path.len() > MAX_EXECUTABLE_PATH || executable_path.is_empty() {
        return None;
    }
    let owned = process_listening_endpoints(pid).ok()?;
    if !role_matches(&owned, expected) {
        return None;
    }
    Some(build_candidate(
        pid,
        &live,
        &executable_path,
        expected,
        target_digest,
        record_digest,
    ))
}

fn build_candidate(
    pid: u32,
    live: &LiveProcessIdentity,
    executable_path: &str,
    role: &[GatewayEndpoint],
    target_digest: &str,
    record_digest_value: &str,
) -> LegacyCandidate {
    let executable_path_digest = path_digest(executable_path);
    let role_digest_value = role_digest(role);
    let start_identity = live.start_identity.digest_label();
    let digest = candidate_digest(
        target_digest,
        record_digest_value,
        pid,
        live.uid,
        &live.boot_identity,
        &start_identity,
        &executable_path_digest,
        &role_digest_value,
    );
    LegacyCandidate {
        pid,
        candidate_uid: live.uid,
        boot_identity: live.boot_identity.clone(),
        start_identity,
        executable_path: executable_path.to_owned(),
        executable_path_digest,
        role: role.to_vec(),
        role_digest: role_digest_value,
        candidate_digest: digest,
    }
}

fn role_matches(owned: &[GatewayEndpoint], expected: &[GatewayEndpoint]) -> bool {
    if expected.is_empty() {
        return false;
    }
    expected.iter().all(|need| owned.contains(need))
}

/// List TCP/UDP listening endpoints held by one live process.
///
/// This inspects kernel socket ownership on Linux and macOS. Unsupported
/// platforms and unreadable process state return an error so callers can fail
/// closed instead of inferring ownership from a connection probe.
pub fn process_listening_endpoints(pid: u32) -> Result<Vec<GatewayEndpoint>, std::io::Error> {
    #[cfg(target_os = "linux")]
    {
        crate::linux_net::process_listening_endpoints(pid)
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos_socket::process_listening_endpoints(pid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "legacy candidate socket inspection is unsupported on this platform",
        ))
    }
}

/// Find every same-user process whose TCP listener can accept connections for
/// `endpoint`. A managed route requires exactly one owner; `SO_REUSEPORT` or a
/// wildcard listener shared with a foreign process is refused.
pub fn listening_process_ids(endpoint: &GatewayEndpoint) -> Result<Vec<u32>, std::io::Error> {
    if endpoint.transport != GatewayTransport::Tcp {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "managed host routes require TCP endpoints",
        ));
    }
    let output = Command::new("ps").args(["-Ao", "pid=,uid="]).output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "cannot enumerate local process owners: ps exited with {}",
            output.status
        )));
    }
    let rendered = String::from_utf8(output.stdout).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("local process inventory is not UTF-8: {error}"),
        )
    })?;
    let effective_uid = nix::unistd::Uid::effective().as_raw();
    let mut owners = Vec::new();
    for row in rendered.lines() {
        let Some((pid, uid)) = parse_process_inventory_row(row) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "local process inventory contains an invalid PID or UID",
            ));
        };
        if pid == 0 || uid != effective_uid {
            continue;
        }
        let endpoints = match process_listening_endpoints(pid) {
            Ok(endpoints) => endpoints,
            // The process inventory can race with a same-user process exiting.
            // ESRCH proves that PID no longer owns a socket; permission and
            // inspection errors remain fail-closed.
            Err(error) if process_has_disappeared(&error) => continue,
            Err(error) => return Err(error),
        };
        if endpoints
            .iter()
            .any(|current| endpoints_overlap(current, endpoint))
        {
            owners.push(pid);
        }
    }
    owners.sort_unstable();
    owners.dedup();
    Ok(owners)
}

/// Verify that `pid` owns the kernel listener socket and that no second
/// listener socket can accept connections for the same endpoint.
///
/// Linux uses the kernel's socket inode table for the exclusivity check. This
/// avoids requiring permission to inspect unrelated same-user processes while
/// still detecting wildcard and `SO_REUSEPORT` collisions. macOS enumerates
/// listener owners through libproc.
pub fn verify_process_listener_exclusive(
    pid: u32,
    endpoint: &GatewayEndpoint,
) -> Result<(), std::io::Error> {
    if endpoint.transport != GatewayTransport::Tcp {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "managed host routes require TCP endpoints",
        ));
    }
    #[cfg(target_os = "linux")]
    {
        crate::linux_net::verify_process_listener_exclusive(pid, endpoint)
    }
    #[cfg(target_os = "macos")]
    {
        let owners = listening_process_ids(endpoint)?;
        if owners.as_slice() == [pid] {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "managed endpoint {} is shared or has unowned listeners: {owners:?}",
                    endpoint.addr
                ),
            ))
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        let _ = endpoint;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "managed listener exclusivity inspection is unsupported on this platform",
        ))
    }
}

/// Parses one `ps -Ao pid=,uid=` row into the PID and the owner UID as the
/// kernel's unsigned `uid_t`.
///
/// `ps` can print a UID in signed 32-bit form: macOS shows some system daemons
/// as `-2`. That presentation is an exact two's-complement encoding of the
/// unsigned UID (`-2` is `4294967294`), so it is mapped to that value and then
/// compared with the effective UID like any other row. A row whose owner is not
/// representable as `i32::MIN..=u32::MAX`, a PID or UID with a sign, non-digit
/// or missing field, and a row with trailing fields are rejected, so the whole
/// inventory fails closed rather than skipping a row it could not classify.
fn parse_process_inventory_row(row: &str) -> Option<(u32, u32)> {
    let mut fields = row.split_whitespace();
    let pid = u32::try_from(parse_ps_integer(fields.next()?, false)?).ok()?;
    let uid = parse_ps_integer(fields.next()?, true)?;
    if fields.next().is_some() {
        return None;
    }
    let owner = if (i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&uid) {
        u32::try_from(uid.rem_euclid(1 << 32)).ok()?
    } else {
        return None;
    };
    Some((pid, owner))
}

/// Parses a `ps` numeric column. Only ASCII digits are accepted, with a leading
/// `-` allowed for the signed UID presentation.
fn parse_ps_integer(token: &str, signed: bool) -> Option<i64> {
    let digits = match token.strip_prefix('-') {
        Some(rest) if signed => rest,
        Some(_) => return None,
        None => token,
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    token.parse::<i64>().ok()
}

fn process_has_disappeared(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH)
}

fn endpoints_overlap(left: &GatewayEndpoint, right: &GatewayEndpoint) -> bool {
    left.transport == right.transport
        && left.addr.port() == right.addr.port()
        && (left.addr.ip() == right.addr.ip()
            || left.addr.ip().is_unspecified()
            || right.addr.ip().is_unspecified())
}

fn read_executable_path(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let path = fs::read_link(format!("/proc/{pid}/exe")).ok()?;
        let text = path.to_str()?.to_owned();
        if text.len() > MAX_EXECUTABLE_PATH {
            return None;
        }
        Some(text)
    }
    #[cfg(target_os = "macos")]
    {
        let mut buf = vec![0u8; PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: buf is writable storage of PROC_PIDPATHINFO_MAXSIZE bytes;
        // pid is a checked positive signed PID.
        let len = unsafe {
            libc::proc_pidpath(
                pid as libc::c_int,
                buf.as_mut_ptr().cast(),
                PROC_PIDPATHINFO_MAXSIZE,
            )
        };
        if len <= 0 {
            return None;
        }
        let len = usize::try_from(len).ok()?;
        if len > MAX_EXECUTABLE_PATH {
            return None;
        }
        buf.truncate(len);
        let text = String::from_utf8(buf).ok()?;
        if text.is_empty() {
            return None;
        }
        Some(text)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        None
    }
}

/// Allowed daemon owner: authenticated operator or root.
pub fn candidate_uid_allowed(candidate_uid: u32, operator_uid: u32) -> bool {
    candidate_uid == operator_uid || candidate_uid == 0
}

/// Compare-and-remove a legacy PID/version pair after confirmed absence.
///
/// PID and version bytes are compared under the record lock so a substituted
/// or vanished captured pair is preserved.
pub fn remove_legacy_if_unchanged(capture: &LegacyRecordCapture) -> Result<bool, GatewayError> {
    identity::remove_legacy_pair_if_unchanged(
        capture.pid_path(),
        capture.directory_owner_uid,
        &capture.pid_bytes,
        &capture.version_bytes,
    )
}

/// Owner-only exclusive lock covering one `up`/`down`/`recover` command.
pub struct GatewayTransitionLock(File);

impl GatewayTransitionLock {
    /// Acquire `gateway.transition.lock` without waiting.
    pub fn try_acquire(pid_path: &Path) -> Result<Self, GatewayError> {
        let parent = pid_path
            .parent()
            .ok_or_else(|| invalid_record("gateway PID path has no parent"))?;
        let owner_uid = identity::ensure_trusted_gateway_parent(pid_path)?;
        acquire_transition_lock(parent, owner_uid, false)
    }

    /// Acquire `gateway.transition.lock`, waiting for an active transition.
    /// Certificate generation uses this form because multiple managed
    /// profiles can request different certificates from the same private CA.
    pub fn acquire(pid_path: &Path) -> Result<Self, GatewayError> {
        let parent = pid_path
            .parent()
            .ok_or_else(|| invalid_record("gateway PID path has no parent"))?;
        let owner_uid = identity::ensure_trusted_gateway_parent(pid_path)?;
        acquire_transition_lock(parent, owner_uid, true)
    }
}

#[cfg(unix)]
fn acquire_transition_lock(
    parent: &Path,
    owner_uid: u32,
    wait: bool,
) -> Result<GatewayTransitionLock, GatewayError> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let lock_path = parent.join("gateway.transition.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(&lock_path)
        .map_err(GatewayError::Io)?;
    let metadata = file.metadata().map_err(GatewayError::Io)?;
    let path_metadata = fs::symlink_metadata(&lock_path).map_err(GatewayError::Io)?;
    if path_metadata.file_type().is_symlink()
        || path_metadata.dev() != metadata.dev()
        || path_metadata.ino() != metadata.ino()
        || !metadata.is_file()
        || metadata.mode() & 0o077 != 0
        || (metadata.uid() != owner_uid && metadata.uid() != 0)
    {
        return Err(invalid_record("gateway transition lock is unsafe"));
    }
    if nix::unistd::Uid::effective().is_root() && metadata.uid() != owner_uid {
        nix::unistd::fchown(
            &file,
            Some(nix::unistd::Uid::from_raw(owner_uid)),
            Some(nix::unistd::Gid::from_raw(metadata.gid())),
        )
        .map_err(|error| GatewayError::Io(error.into()))?;
    }
    let lock_result = if wait {
        fs2::FileExt::lock_exclusive(&file)
    } else {
        fs2::FileExt::try_lock_exclusive(&file)
    };
    match lock_result {
        Ok(()) => Ok(GatewayTransitionLock(file)),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            Err(GatewayError::TransitionLockHeld)
        }
        Err(error) => Err(GatewayError::Io(error)),
    }
}

#[cfg(not(unix))]
fn acquire_transition_lock(
    _parent: &Path,
    _owner_uid: u32,
    _wait: bool,
) -> Result<GatewayTransitionLock, GatewayError> {
    Err(GatewayError::Io(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "gateway transition lock is unsupported on this platform",
    )))
}

#[cfg(unix)]
impl Drop for GatewayTransitionLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

fn invalid_record(message: &str) -> GatewayError {
    GatewayError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message.to_owned(),
    ))
}

/// Hex-64 binding used by hidden recovery commands.
pub fn is_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod legacy_recovery_tests {
    use super::*;

    /// Signed `-2` and unsigned `4294967294` are one kernel UID. A row printed
    /// in signed form must compare equal to an effective UID of that value, and
    /// an ordinary different UID must not match it.
    #[test]
    fn process_inventory_row_maps_signed_presentation_to_its_unsigned_uid() {
        let unsigned_minus_two = u32::MAX - 1;
        assert_eq!(
            parse_process_inventory_row("84795    -2"),
            Some((84795, unsigned_minus_two))
        );
        assert_eq!(
            parse_process_inventory_row("3 4294967294"),
            Some((3, unsigned_minus_two))
        );
        assert_eq!(parse_process_inventory_row("5 -1"), Some((5, u32::MAX)));
        assert_eq!(
            parse_process_inventory_row("4 -2147483648"),
            Some((4, 1 << 31))
        );
        assert_eq!(parse_process_inventory_row("1 0"), Some((1, 0)));
        assert_eq!(parse_process_inventory_row("  501 501"), Some((501, 501)));
        assert_eq!(
            parse_process_inventory_row("6 4294967295"),
            Some((6, u32::MAX))
        );
        assert_ne!(
            parse_process_inventory_row("84795 -2").map(|(_, owner)| owner),
            Some(501)
        );
    }

    /// Owners outside `i32::MIN..=u32::MAX` are not UIDs, and rows with missing,
    /// signed-PID, non-digit or trailing fields cannot be classified. None of
    /// them may be skipped as foreign.
    #[test]
    fn process_inventory_row_rejects_out_of_range_and_malformed_rows() {
        for row in [
            "6 4294967296",
            "6 -2147483649",
            "1",
            "",
            "x 501",
            "501 unknown",
            "501 501 extra",
            "-1 501",
            "+1 501",
            "1 +501",
            "1 -",
            "1 1e3",
        ] {
            assert_eq!(parse_process_inventory_row(row), None, "row {row:?}");
        }
    }

    #[test]
    fn process_inventory_row_numeric_columns_accept_only_ascii_digits() {
        assert_eq!(parse_ps_integer("42", false), Some(42));
        assert_eq!(parse_ps_integer("-42", true), Some(-42));
        assert_eq!(parse_ps_integer("-42", false), None);
        assert_eq!(parse_ps_integer("", true), None);
        assert_eq!(parse_ps_integer("٤٢", false), None);
    }
    use std::net::TcpListener;
    use std::net::UdpSocket;
    use std::os::unix::fs::PermissionsExt;
    use std::thread;
    use std::time::Duration;

    struct OwnedChild(std::process::Child);

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    fn private_gateway_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("fixture directory");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    fn write_legacy_pair(dir: &Path, pid: u32, version: &str) -> PathBuf {
        let pid_path = dir.join("gateway.pid");
        fs::write(&pid_path, format!("{pid}\n")).unwrap();
        fs::write(dir.join("gateway.version"), version).unwrap();
        pid_path
    }

    #[test]
    fn legacy_recovery_capture_absent_partial_and_legacy() {
        let dir = private_gateway_dir();
        let pid_path = dir.path().join("gateway.pid");
        let operator = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(dir.path()).unwrap().uid()
        };
        assert!(matches!(
            capture_legacy_record(&pid_path, operator).unwrap(),
            LegacyCapture::Absent
        ));
        fs::write(&pid_path, "4242\n").unwrap();
        assert!(matches!(
            capture_legacy_record(&pid_path, operator).unwrap(),
            LegacyCapture::Unknown { .. }
        ));
        fs::write(dir.path().join("gateway.version"), "v0.13.1+local.test").unwrap();
        match capture_legacy_record(&pid_path, operator).unwrap() {
            LegacyCapture::Legacy(capture) => {
                assert_eq!(capture.pid, 4242);
                assert_eq!(capture.operator_uid, operator);
                assert_eq!(capture.directory_owner_uid, operator);
                assert!(is_hex64(&capture.record_digest));
                assert!(is_hex64(&capture.target_digest));
            }
            other => panic!("expected legacy capture, got {other:?}"),
        }
        fs::write(dir.path().join("gateway.identity"), b"{broken").unwrap();
        fs::set_permissions(
            dir.path().join("gateway.identity"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert!(matches!(
            capture_legacy_record(&pid_path, operator).unwrap(),
            LegacyCapture::Unknown { .. }
        ));
    }

    #[test]
    fn legacy_recovery_missing_parent_directory_is_absent() {
        let dir = private_gateway_dir();
        let missing = dir.path().join("no-such-gateway").join("gateway.pid");
        assert!(matches!(
            capture_legacy_record(&missing, 501).unwrap(),
            LegacyCapture::Absent
        ));
    }

    #[test]
    fn legacy_recovery_root_uid_with_operator_directory_is_allowed() {
        assert!(candidate_uid_allowed(0, 501));
        assert!(candidate_uid_allowed(501, 501));
        assert!(!candidate_uid_allowed(502, 501));
    }

    #[test]
    fn legacy_recovery_canonical_endpoints_are_udp_dns_and_tcp_http_tls() {
        let endpoints = canonical_gateway_role_endpoints();
        assert_eq!(
            endpoints,
            vec![
                GatewayEndpoint {
                    transport: GatewayTransport::Udp,
                    addr: "127.0.0.1:15353".parse().unwrap(),
                },
                GatewayEndpoint {
                    transport: GatewayTransport::Tcp,
                    addr: "127.0.0.1:80".parse().unwrap(),
                },
                GatewayEndpoint {
                    transport: GatewayTransport::Tcp,
                    addr: "127.0.0.1:443".parse().unwrap(),
                },
            ]
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn legacy_recovery_role_predicate_owned_child_and_foreign_worker() {
        let udp = UdpSocket::bind("127.0.0.1:0").expect("udp");
        let tcp_a = TcpListener::bind("127.0.0.1:0").expect("tcp a");
        let tcp_b = TcpListener::bind("127.0.0.1:0").expect("tcp b");
        let expected = vec![
            GatewayEndpoint {
                transport: GatewayTransport::Udp,
                addr: udp.local_addr().unwrap(),
            },
            GatewayEndpoint {
                transport: GatewayTransport::Tcp,
                addr: tcp_a.local_addr().unwrap(),
            },
            GatewayEndpoint {
                transport: GatewayTransport::Tcp,
                addr: tcp_b.local_addr().unwrap(),
            },
        ];
        let pid = std::process::id();
        let candidate = inspect_legacy_candidate_with_endpoints(
            pid,
            &"a".repeat(64),
            &"b".repeat(64),
            &expected,
        )
        .expect("owned sockets satisfy the role");
        assert_eq!(candidate.pid, pid);
        assert!(is_hex64(&candidate.candidate_digest));
        assert!(!candidate.executable_path.is_empty());

        let foreign = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("foreign worker");
        let mut foreign = OwnedChild(foreign);
        thread::sleep(Duration::from_millis(50));
        assert!(
            inspect_legacy_candidate_with_endpoints(
                foreign.0.id(),
                &"a".repeat(64),
                &"b".repeat(64),
                &expected,
            )
            .is_none(),
            "same-host process without the role must fail"
        );
        let _ = foreign.0.kill();

        let tcp_dns = TcpListener::bind("127.0.0.1:0").expect("tcp dns");
        let udp_need = GatewayEndpoint {
            transport: GatewayTransport::Udp,
            addr: tcp_dns.local_addr().unwrap(),
        };
        assert!(
            inspect_legacy_candidate_with_endpoints(
                pid,
                &"a".repeat(64),
                &"b".repeat(64),
                &[udp_need],
            )
            .is_none(),
            "TCP must not satisfy the UDP DNS role"
        );

        drop(udp);
        drop(tcp_a);
        drop(tcp_b);
        drop(tcp_dns);
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn legacy_recovery_transition_lock_refuses_contention() {
        let dir = private_gateway_dir();
        let pid_path = dir.path().join("gateway.pid");
        fs::write(&pid_path, "4242\n").unwrap();
        let first = GatewayTransitionLock::try_acquire(&pid_path).expect("first lock");
        let second = GatewayTransitionLock::try_acquire(&pid_path);
        assert!(matches!(second, Err(GatewayError::TransitionLockHeld)));
        drop(first);
        GatewayTransitionLock::try_acquire(&pid_path).expect("lock after release");
    }

    #[cfg(unix)]
    #[test]
    fn legacy_recovery_transition_lock_can_wait_for_private_tls_generation() {
        use std::sync::mpsc;
        use std::time::Duration;

        let dir = private_gateway_dir();
        let pid_path = dir.path().join("gateway.pid");
        fs::write(&pid_path, "4242\n").unwrap();
        let held = GatewayTransitionLock::try_acquire(&pid_path).expect("held lock");
        let (started_tx, started_rx) = mpsc::channel();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let contender_path = pid_path.clone();
        let contender = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let _lock = GatewayTransitionLock::acquire(&contender_path).unwrap();
            acquired_tx.send(()).unwrap();
        });

        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(acquired_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(held);
        acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        contender.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn legacy_recovery_transition_lock_refuses_symlinked_effigy_without_mutating_target() {
        use std::os::unix::fs::{symlink, MetadataExt};

        let root = tempfile::tempdir().expect("fixture directory");
        let home = root.path().join("home");
        let outside = root.path().join("outside");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o700)).unwrap();
        let sentinel = outside.join("sentinel");
        fs::write(&sentinel, b"keep").unwrap();
        let outside_names = || {
            fs::read_dir(&outside)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<std::collections::BTreeSet<_>>()
        };
        let before_outside = outside_names();
        let before_sentinel = fs::read(&sentinel).unwrap();
        let before_ino = fs::symlink_metadata(&outside).unwrap().ino();
        symlink(&outside, home.join(".effigy")).unwrap();
        let pid_path = home.join(".effigy").join("gateway").join("gateway.pid");
        let operator = fs::metadata(&home).unwrap().uid();

        assert!(matches!(
            capture_legacy_record(&pid_path, operator).unwrap(),
            LegacyCapture::Absent
        ));
        match GatewayTransitionLock::try_acquire(&pid_path) {
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.contains("unsafe"),
                    "expected ancestor-trust refusal, got {message}"
                );
            }
            Ok(_) => panic!("symlink .effigy must refuse lock acquisition"),
        }

        assert_eq!(outside_names(), before_outside);
        assert_eq!(fs::read(&sentinel).unwrap(), before_sentinel);
        assert!(!outside.join("gateway").exists());
        assert_eq!(fs::symlink_metadata(&outside).unwrap().ino(), before_ino);
        assert!(home
            .join(".effigy")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn legacy_recovery_transition_lock_creates_genuine_absent_parent() {
        use std::os::unix::fs::MetadataExt;

        let root = tempfile::tempdir().expect("fixture directory");
        let home = root.path().join("home");
        fs::create_dir(&home).unwrap();
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
        let pid_path = home.join(".effigy").join("gateway").join("gateway.pid");

        let lock = GatewayTransitionLock::try_acquire(&pid_path).expect("genuine absent parent");
        drop(lock);

        let effigy = home.join(".effigy");
        let gateway = effigy.join("gateway");
        assert!(effigy.symlink_metadata().unwrap().is_dir());
        assert!(!effigy.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(
            fs::symlink_metadata(&effigy).unwrap().uid(),
            fs::metadata(&home).unwrap().uid()
        );
        assert!(gateway.is_dir());
        assert!(gateway.join("gateway.transition.lock").is_file());
        assert!(!pid_path.exists());
    }

    #[test]
    fn legacy_recovery_remove_preserves_changed_bytes() {
        let dir = private_gateway_dir();
        let operator = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(dir.path()).unwrap().uid()
        };
        let pid_path = write_legacy_pair(dir.path(), 4242, "v0.13.1");
        let LegacyCapture::Legacy(capture) = capture_legacy_record(&pid_path, operator).unwrap()
        else {
            panic!("legacy capture");
        };
        fs::write(dir.path().join("gateway.version"), "v0.13.1-changed").unwrap();
        assert!(!remove_legacy_if_unchanged(&capture).unwrap());
        assert!(pid_path.exists());
        fs::write(dir.path().join("gateway.version"), "v0.13.1").unwrap();
        fs::write(&pid_path, "9999\n").unwrap();
        assert!(!remove_legacy_if_unchanged(&capture).unwrap());
        assert!(pid_path.exists());
        fs::write(&pid_path, "8888\n").unwrap();
        fs::write(dir.path().join("gateway.version"), "v0.13.1-changed").unwrap();
        assert!(!remove_legacy_if_unchanged(&capture).unwrap());
        assert!(pid_path.exists());
        fs::write(&pid_path, "4242\n").unwrap();
        fs::write(dir.path().join("gateway.version"), "v0.13.1").unwrap();
        fs::remove_file(&pid_path).unwrap();
        assert!(!remove_legacy_if_unchanged(&capture).unwrap());
        assert!(dir.path().join("gateway.version").exists());
        fs::write(&pid_path, "4242\n").unwrap();
        fs::remove_file(dir.path().join("gateway.version")).unwrap();
        assert!(!remove_legacy_if_unchanged(&capture).unwrap());
        assert!(pid_path.exists());
        fs::write(dir.path().join("gateway.version"), "v0.13.1").unwrap();
        fs::remove_file(&pid_path).unwrap();
        fs::remove_file(dir.path().join("gateway.version")).unwrap();
        assert!(!remove_legacy_if_unchanged(&capture).unwrap());
        fs::write(&pid_path, "4242\n").unwrap();
        fs::write(dir.path().join("gateway.version"), "v0.13.1").unwrap();
        assert!(remove_legacy_if_unchanged(&capture).unwrap());
        assert!(!pid_path.exists());
        assert!(!dir.path().join("gateway.version").exists());
    }
}
