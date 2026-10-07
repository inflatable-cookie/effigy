//! Legacy gateway record capture, candidate inspection, and transition lock.

use std::fs::{self, File, OpenOptions};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::GatewayError;
use crate::identity::{self, GatewayRecordSnapshot, LiveProcessIdentity};
use crate::server::{self, GatewayProcessProbe};

const MAX_VERSION_BYTES: usize = 256;
const MAX_EXECUTABLE_PATH: usize = 4096;
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

fn process_listening_endpoints(pid: u32) -> Result<Vec<GatewayEndpoint>, std::io::Error> {
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

fn read_executable_path(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let path = fs::read_link(format!("/proc/{pid}/exe")).ok()?;
        let text = path.to_str()?.to_owned();
        if text.as_bytes().len() > MAX_EXECUTABLE_PATH {
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
pub fn remove_legacy_if_unchanged(capture: &LegacyRecordCapture) -> Result<bool, GatewayError> {
    if !capture.bytes_unchanged()? {
        return Ok(false);
    }
    let snapshot = identity::read_snapshot(&capture.pid_path)?;
    let Some(snapshot) = snapshot else {
        let _ = fs::remove_file(identity::version_path(&capture.pid_path));
        return Ok(true);
    };
    if !snapshot.is_legacy_pid_only() || snapshot.pid_bytes() != capture.pid_bytes.as_slice() {
        return Ok(false);
    }
    identity::remove_if_unchanged(&snapshot)
}

/// Owner-only exclusive lock covering one `up`/`down`/`recover` command.
pub struct GatewayTransitionLock(File);

impl GatewayTransitionLock {
    /// Acquire `gateway.transition.lock` without waiting.
    pub fn try_acquire(pid_path: &Path) -> Result<Self, GatewayError> {
        let parent = pid_path
            .parent()
            .ok_or_else(|| invalid_record("gateway PID path has no parent"))?;
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(GatewayError::Io)?;
        }
        let owner_uid = identity::trusted_directory_owner(pid_path)?;
        acquire_transition_lock(parent, owner_uid)
    }
}

#[cfg(unix)]
fn acquire_transition_lock(
    parent: &Path,
    owner_uid: u32,
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
    match fs2::FileExt::try_lock_exclusive(&file) {
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
        assert!(remove_legacy_if_unchanged(&capture).unwrap());
        assert!(!pid_path.exists());
        assert!(!dir.path().join("gateway.version").exists());
    }
}
