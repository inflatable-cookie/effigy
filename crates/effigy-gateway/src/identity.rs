//! Persisted gateway-generation identity and its trusted file pair.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::GatewayError;

const IDENTITY_FORMAT_VERSION: u32 = 1;
const MAX_PID_BYTES: usize = 32;
const MAX_IDENTITY_BYTES: usize = 1024;
const MAX_VERSION_BYTES: usize = 256;

/// Precise, platform-specific start data stored with the gateway PID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "platform", rename_all = "snake_case", deny_unknown_fields)]
pub enum GatewayStartIdentity {
    /// Linux process start ticks (field 22 of `/proc/<pid>/stat`).
    Linux { start_ticks: u64 },
    /// macOS kernel start timestamp, retaining both timeval components.
    Macos {
        start_seconds: u64,
        start_microseconds: u64,
    },
}

impl GatewayStartIdentity {
    /// Stable encoding used in candidate-generation digests.
    pub fn digest_label(&self) -> String {
        match self {
            Self::Linux { start_ticks } => format!("linux:{start_ticks}"),
            Self::Macos {
                start_seconds,
                start_microseconds,
            } => format!("macos:{start_seconds}:{start_microseconds}"),
        }
    }
}

/// A versioned sidecar record that identifies one gateway generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayIdentityRecord {
    format_version: u32,
    pid: u32,
    boot_identity: String,
    start_identity: GatewayStartIdentity,
}

impl GatewayIdentityRecord {
    /// Recorded process ID.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    fn is_valid(&self) -> bool {
        if self.format_version != IDENTITY_FORMAT_VERSION
            || crate::server::checked_gateway_pid(self.pid).is_none()
            || self.boot_identity.is_empty()
        {
            return false;
        }
        match &self.start_identity {
            GatewayStartIdentity::Linux { start_ticks } => *start_ticks > 0,
            GatewayStartIdentity::Macos {
                start_seconds,
                start_microseconds,
            } => *start_seconds > 0 && *start_microseconds < 1_000_000,
        }
    }

    fn digest_bytes(pid_bytes: &[u8], identity_bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(pid_bytes);
        hasher.update([0]);
        hasher.update(identity_bytes);
        hex::encode(hasher.finalize())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Result of comparing a persisted generation with the live process.
pub enum GatewayIdentityProbe {
    /// PID, boot identity, and precise process start identity all match.
    Matched,
    /// The live process has a different readable generation.
    Mismatch,
    /// The kernel denied this caller access to the live process identity.
    PermissionDenied,
    /// The identity could not be read or trusted.
    Unknown,
}

/// Snapshot of both persisted files, including the exact bytes used for
/// compare-and-remove and elevated-reader target binding.
#[derive(Debug, Clone)]
pub struct GatewayRecordSnapshot {
    pid_path: PathBuf,
    pid: u32,
    record: Option<GatewayIdentityRecord>,
    pid_bytes: Vec<u8>,
    identity_bytes: Option<Vec<u8>>,
    owner_uid: u32,
}

impl GatewayRecordSnapshot {
    /// The PID read from the decimal compatibility file.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// The parsed, versioned sidecar, if it is present and valid.
    pub fn record(&self) -> Option<&GatewayIdentityRecord> {
        self.record.as_ref()
    }

    /// True when the PID record has no identity sidecar at all.
    ///
    /// This distinguishes a legacy PID-only record from a present but invalid
    /// sidecar, which remains an unknown record.
    pub fn is_legacy_pid_only(&self) -> bool {
        self.identity_bytes.is_none()
    }

    /// Canonical PID path that produced this snapshot.
    pub fn pid_path(&self) -> &Path {
        &self.pid_path
    }

    /// Exact `gateway.pid` bytes, including any trailing newline.
    pub fn pid_bytes(&self) -> &[u8] {
        &self.pid_bytes
    }

    /// Digest of the exact numeric PID and sidecar bytes.
    pub fn digest(&self) -> Option<String> {
        Some(GatewayIdentityRecord::digest_bytes(
            &self.pid_bytes,
            self.identity_bytes.as_deref()?,
        ))
    }

    /// UID that owns the validated gateway directory and records.
    pub fn owner_uid(&self) -> u32 {
        self.owner_uid
    }

    /// Digest of the fixed PID path, used to detect HOME/target substitution
    /// across the bounded elevated reader boundary.
    pub fn target_digest(&self) -> Option<String> {
        gateway_target_digest(&self.pid_path)
    }
}

/// Stable digest for an exact gateway PID path without disclosing that path
/// in the elevated command response.
pub fn gateway_target_digest(pid_path: &Path) -> Option<String> {
    let parent = pid_path.parent()?;
    let canonical_parent = fs::canonicalize(parent).ok()?;
    let filename = pid_path.file_name()?;
    #[cfg(unix)]
    let mut path_bytes = {
        use std::os::unix::ffi::OsStrExt;
        canonical_parent.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let mut path_bytes = canonical_parent
        .as_os_str()
        .to_string_lossy()
        .as_bytes()
        .to_vec();
    path_bytes.push(std::path::MAIN_SEPARATOR as u8);
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path_bytes.extend_from_slice(filename.as_bytes());
    }
    #[cfg(not(unix))]
    path_bytes.extend_from_slice(filename.to_string_lossy().as_bytes());
    let mut hasher = Sha256::new();
    hasher.update(path_bytes);
    Some(hex::encode(hasher.finalize()))
}

/// Classify the current process generation for one persisted record.
///
/// The exact process start identity is mandatory. The boot component may be
/// either the stable kernel session identity or, for records written before
/// that change, the legacy macOS `kern.boottime` timeval whose microsecond
/// field drifts and whose seconds are wall-clock-adjusted within a boot. A
/// legacy value that cannot be proven to describe the current boot is
/// [`GatewayIdentityProbe::Unknown`], never a readable different generation, so
/// the record is preserved rather than cleaned up as replaced. The comparison
/// lives in [`effigy_process::compare_boot_identity`].
pub fn probe_live_identity(record: &GatewayIdentityRecord) -> GatewayIdentityProbe {
    match read_live_process_identity(record.pid) {
        Ok(live) if live.start_identity == record.start_identity => {
            match effigy_process::compare_boot_identity(&record.boot_identity) {
                effigy_process::BootIdentityComparison::Match => GatewayIdentityProbe::Matched,
                effigy_process::BootIdentityComparison::DifferentSession => {
                    GatewayIdentityProbe::Mismatch
                }
                effigy_process::BootIdentityComparison::Unknown => GatewayIdentityProbe::Unknown,
            }
        }
        Ok(_) => GatewayIdentityProbe::Mismatch,
        Err(error) if is_permission_denied(&error) => GatewayIdentityProbe::PermissionDenied,
        Err(_) => GatewayIdentityProbe::Unknown,
    }
}

/// Safely read the decimal PID and optional sidecar without following file
/// symlinks. A missing sidecar is retained as an unauthenticated legacy
/// snapshot; it never authenticates a live process. A version-only,
/// malformed, or symlink `gateway.version` is unknown and is not treated as
/// an empty gateway.
pub fn read_snapshot(pid_path: &Path) -> Result<Option<GatewayRecordSnapshot>, GatewayError> {
    let owner_uid = trusted_directory_owner(pid_path)?;
    read_snapshot_for_owner(pid_path, owner_uid)
}

/// Test seam for the elevated root context: resolves the directory owner
/// with a modeled caller (effective UID plus authenticated operator) and
/// then executes the real file validation rules. Production uses
/// [`read_snapshot`].
#[cfg(all(unix, test))]
fn read_snapshot_with(
    pid_path: &Path,
    effective_uid: u32,
    authenticated_operator: Option<u32>,
) -> Result<Option<GatewayRecordSnapshot>, GatewayError> {
    let owner_uid = trusted_directory_owner_with(pid_path, effective_uid, authenticated_operator)?;
    read_snapshot_for_owner(pid_path, owner_uid)
}

fn read_snapshot_for_owner(
    pid_path: &Path,
    owner_uid: u32,
) -> Result<Option<GatewayRecordSnapshot>, GatewayError> {
    // Inspect version through the same no-follow trusted reader used for PID
    // and identity so a version-only or symlink version cannot classify as
    // absent and later be overwritten, including by `std::fs::write`.
    let version_present =
        read_trusted_file(&version_path(pid_path), owner_uid, MAX_VERSION_BYTES)?.is_some();
    let Some((pid_bytes, pid_file_owner, pid_file_mode)) =
        read_trusted_file(pid_path, owner_uid, MAX_PID_BYTES)?
    else {
        // An identity-only pair is an interrupted publication, not an empty
        // gateway state. Preserve it and fail closed for status/start.
        if read_trusted_file(&identity_path(pid_path), owner_uid, MAX_IDENTITY_BYTES)?.is_some() {
            return Err(invalid_record("gateway identity exists without its PID"));
        }
        if version_present {
            return Err(invalid_record("gateway version exists without its PID"));
        }
        return Ok(None);
    };
    let pid_text =
        std::str::from_utf8(&pid_bytes).map_err(|_| invalid_record("PID file is not UTF-8"))?;
    let pid = pid_text
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|value| crate::server::checked_gateway_pid(*value).is_some())
        .ok_or_else(|| invalid_record("PID file is malformed"))?;

    let identity_path = identity_path(pid_path);
    let identity_result = read_trusted_file(&identity_path, owner_uid, MAX_IDENTITY_BYTES)?;
    let identity_bytes = identity_result.as_ref().map(|(bytes, _, _)| bytes.clone());
    let record = identity_bytes
        .as_deref()
        .and_then(|bytes| serde_json::from_slice::<GatewayIdentityRecord>(bytes).ok())
        .filter(GatewayIdentityRecord::is_valid)
        .filter(|record| record.pid == pid)
        .filter(|_| pid_file_owner == owner_uid)
        .filter(|_| {
            identity_result.as_ref().is_some_and(|(_, owner, mode)| {
                *owner == owner_uid && mode & 0o777 == 0o600 && pid_file_mode & 0o400 != 0
            })
        });

    Ok(Some(GatewayRecordSnapshot {
        pid_path: pid_path.to_path_buf(),
        pid,
        record,
        pid_bytes,
        identity_bytes,
        owner_uid,
    }))
}

/// Capture this process's kernel start identity and atomically publish the
/// sidecar and decimal PID pair. The directory owner is validated before any
/// ownership change so a privileged writer cannot chown toward an untrusted
/// path owner.
pub fn publish_current_gateway(
    pid_path: &Path,
    operator_uid: u32,
) -> Result<GatewayRecordSnapshot, GatewayError> {
    let owner_uid = trusted_directory_owner(pid_path)?;
    if owner_uid != operator_uid {
        return Err(invalid_record("gateway directory owner changed"));
    }
    let pid = std::process::id();
    let (boot_identity, start_identity) = read_process_identity(pid).map_err(GatewayError::Io)?;
    let record = GatewayIdentityRecord {
        format_version: IDENTITY_FORMAT_VERSION,
        pid,
        boot_identity,
        start_identity,
    };
    if !record.is_valid() {
        return Err(invalid_record("live gateway identity is malformed"));
    }
    let identity_bytes = serde_json::to_vec(&record)
        .map_err(|error| invalid_record(&format!("serialize gateway identity: {error}")))?;
    let pid_bytes = pid.to_string().into_bytes();

    let _lock = GatewayRecordLock::acquire(pid_path, owner_uid)?;
    if read_snapshot(pid_path)?.is_some() {
        return Err(invalid_record(
            "gateway record appeared after the existing-generation check",
        ));
    }
    validate_replace_target(pid_path, owner_uid)?;
    validate_replace_target(&identity_path(pid_path), owner_uid)?;
    atomic_publish(&identity_path(pid_path), &identity_bytes, owner_uid)?;
    atomic_publish(pid_path, &pid_bytes, owner_uid)?;

    read_snapshot(pid_path)?
        .filter(|snapshot| snapshot.record.as_ref() == Some(&record))
        .ok_or_else(|| invalid_record("published gateway record pair did not verify"))
}

/// Remove a record pair only if it is still byte-for-byte the snapshot that
/// was checked. Publishers and removers share a lock, so stale cleanup cannot
/// delete a concurrently published generation.
pub fn remove_if_unchanged(snapshot: &GatewayRecordSnapshot) -> Result<bool, GatewayError> {
    let _lock = GatewayRecordLock::acquire(&snapshot.pid_path, snapshot.owner_uid)?;
    let Some(current) = read_snapshot(&snapshot.pid_path)? else {
        return Ok(true);
    };
    if current.pid_bytes != snapshot.pid_bytes
        || current.identity_bytes != snapshot.identity_bytes
        || current.owner_uid != snapshot.owner_uid
    {
        return Ok(false);
    }
    fs::remove_file(identity_path(&snapshot.pid_path)).or_else(ignore_not_found)?;
    fs::remove_file(&snapshot.pid_path).or_else(ignore_not_found)?;
    fs::remove_file(snapshot.pid_path.with_extension("version")).or_else(ignore_not_found)?;
    Ok(true)
}

/// Remove a legacy PID/version pair only if both files still match the
/// captured bytes. Comparison and deletion share the record lock so a
/// substituted PID or version cannot be deleted. A vanished captured PID or
/// version is a changed record and returns `Ok(false)`.
pub(crate) fn remove_legacy_pair_if_unchanged(
    pid_path: &Path,
    owner_uid: u32,
    pid_bytes: &[u8],
    version_bytes: &[u8],
) -> Result<bool, GatewayError> {
    let _lock = GatewayRecordLock::acquire(pid_path, owner_uid)?;
    let version_path = version_path(pid_path);
    let current_version =
        read_trusted_file(&version_path, owner_uid, MAX_VERSION_BYTES)?.map(|(bytes, _, _)| bytes);
    let snapshot = match read_snapshot(pid_path) {
        Ok(snapshot) => snapshot,
        Err(GatewayError::Io(ref error)) if error.kind() == std::io::ErrorKind::InvalidData => {
            // Version-only, symlink, or other untrusted remainder is a changed
            // record. Preserve it; do not treat classifier refusal as cleanup.
            return Ok(false);
        }
        Err(error) => return Err(error),
    };
    match (snapshot, current_version.as_deref()) {
        (Some(snapshot), Some(current))
            if snapshot.is_legacy_pid_only()
                && snapshot.pid_bytes() == pid_bytes
                && current == version_bytes
                && snapshot.owner_uid() == owner_uid =>
        {
            fs::remove_file(identity_path(pid_path)).or_else(ignore_not_found)?;
            fs::remove_file(pid_path).or_else(ignore_not_found)?;
            fs::remove_file(&version_path).or_else(ignore_not_found)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Read and verify the only authorized target for the hidden elevated reader.
/// It accepts a digest, never an arbitrary PID, and returns no live identity.
pub fn read_only_elevated_check(
    pid_path: &Path,
    expected_digest: &str,
    expected_target_digest: &str,
    expected_owner_uid: u32,
) -> GatewayIdentityProbe {
    if gateway_target_digest(pid_path).as_deref() != Some(expected_target_digest) {
        return GatewayIdentityProbe::Unknown;
    }
    let Ok(Some(snapshot)) = read_snapshot(pid_path) else {
        return GatewayIdentityProbe::Unknown;
    };
    if snapshot.owner_uid != expected_owner_uid
        || snapshot.digest().as_deref() != Some(expected_digest)
    {
        return GatewayIdentityProbe::Unknown;
    }
    let Some(record) = snapshot.record() else {
        return GatewayIdentityProbe::Unknown;
    };
    match probe_live_identity(record) {
        GatewayIdentityProbe::PermissionDenied => GatewayIdentityProbe::Unknown,
        other => other,
    }
}

pub(crate) fn identity_path(pid_path: &Path) -> PathBuf {
    pid_path.with_extension("identity")
}

pub(crate) fn version_path(pid_path: &Path) -> PathBuf {
    pid_path.with_extension("version")
}

/// Publish `gateway.version` beside a trusted PID path. Refuses a symlink or
/// non-file replacement target so the writer cannot follow an untrusted path,
/// including one that appeared after a vacant preflight.
pub(crate) fn publish_gateway_version(pid_path: &Path, version: &str) -> Result<(), GatewayError> {
    let owner_uid = trusted_directory_owner(pid_path)?;
    let path = version_path(pid_path);
    validate_version_replace_target(&path, owner_uid)?;
    atomic_publish(&path, version.as_bytes(), owner_uid)
}

/// Read a trusted sidecar next to the PID file (version, identity, …).
pub(crate) fn read_trusted_sidecar_bytes(
    pid_path: &Path,
    sidecar: &Path,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, GatewayError> {
    let owner_uid = trusted_directory_owner(pid_path)?;
    Ok(read_trusted_file(sidecar, owner_uid, max_bytes)?.map(|(bytes, _, _)| bytes))
}

fn invalid_record(message: &str) -> GatewayError {
    GatewayError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message.to_owned(),
    ))
}

fn ignore_not_found(error: std::io::Error) -> std::io::Result<()> {
    if error.kind() == std::io::ErrorKind::NotFound {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(unix)]
fn escalated_marker_present() -> bool {
    std::env::var("EFFIGY_GATEWAY_ESCALATED")
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
}

#[cfg(unix)]
fn operator_claim_uid() -> Option<u32> {
    std::env::var("EFFIGY_GATEWAY_OPERATOR_UID")
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
}

#[cfg(unix)]
fn passwd_home_for(uid: u32) -> Option<PathBuf> {
    nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
        .ok()??
        .dir
        .into()
}

/// Authenticate the forwarded operator claim for an elevated root caller.
///
/// The claim is accepted only when the caller is root behind the existing
/// administrator-elevation marker, the claimed UID is a non-root UID, an
/// ambient `SUDO_UID` (when present) names the same UID, and `HOME` is
/// byte-identical to that UID's passwd home. Any mismatch, missing value,
/// or non-Unicode-safe comparison input yields `None` (fail closed). The
/// env UID alone never authenticates an owner.
#[cfg(unix)]
fn authenticate_operator_claim(
    effective_uid: u32,
    escalated: bool,
    operator: Option<u32>,
    sudo_present: bool,
    sudo_uid: Option<u32>,
    home: Option<PathBuf>,
    passwd_home: Option<PathBuf>,
) -> Option<u32> {
    if effective_uid != 0 || !escalated {
        return None;
    }
    let operator = operator.filter(|uid| *uid != 0)?;
    if sudo_present && sudo_uid != Some(operator) {
        return None;
    }
    let home = home?;
    let expected = passwd_home?;
    (home == expected).then_some(operator)
}

#[cfg(unix)]
fn authenticated_elevated_operator() -> Option<u32> {
    let effective_uid = nix::unistd::Uid::effective().as_raw();
    let escalated = escalated_marker_present();
    let operator = operator_claim_uid();
    let (sudo_present, sudo_uid) = match std::env::var("SUDO_UID") {
        Err(std::env::VarError::NotPresent) => (false, None),
        Ok(value) => (true, value.trim().parse::<u32>().ok()),
        Err(_) => (true, None),
    };
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let passwd_home = operator.and_then(passwd_home_for);
    authenticate_operator_claim(
        effective_uid,
        escalated,
        operator,
        sudo_present,
        sudo_uid,
        home,
        passwd_home,
    )
}

#[cfg(unix)]
pub(crate) fn trusted_directory_owner(pid_path: &Path) -> Result<u32, GatewayError> {
    let effective_uid = nix::unistd::Uid::effective().as_raw();
    let authenticated_operator = if nix::unistd::Uid::effective().is_root() {
        authenticated_elevated_operator()
    } else {
        None
    };
    trusted_directory_owner_with(pid_path, effective_uid, authenticated_operator)
}

/// Core directory trust check with an explicit caller context. The filesystem
/// rules (absolute path, no symlink, directory, no group/other write, parent
/// safety) always execute; only the accepted-owner set varies. An
/// authenticated elevated operator adds exactly that UID to the root owner;
/// an unauthenticated root accepts only root ownership, so an operator-owned
/// directory stays unsafe without its authenticated context.
#[cfg(unix)]
fn trusted_directory_owner_with(
    pid_path: &Path,
    effective_uid: u32,
    authenticated_operator: Option<u32>,
) -> Result<u32, GatewayError> {
    use std::os::unix::fs::MetadataExt;
    let parent = pid_path
        .parent()
        .ok_or_else(|| invalid_record("gateway PID path has no parent"))?;
    if !pid_path.is_absolute() {
        return Err(invalid_record("gateway PID path must be absolute"));
    }
    let metadata = fs::symlink_metadata(parent).map_err(GatewayError::Io)?;
    let accepted = metadata.uid() == 0
        || (effective_uid != 0 && metadata.uid() == effective_uid)
        || authenticated_operator.is_some_and(|uid| metadata.uid() == uid);
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.mode() & 0o022 != 0
        || !accepted
    {
        return Err(invalid_record("gateway directory is unsafe"));
    }
    let gateway_owner_uid = metadata.uid();
    check_effigy_ancestors(parent, gateway_owner_uid)?;
    Ok(metadata.uid())
}

/// Validate `.effigy` ancestors without creating directories. Creation paths
/// must call this before `create_dir_all` so a symlink `.effigy` cannot receive
/// a gateway child in an untrusted location.
#[cfg(unix)]
fn ensure_gateway_parent_creatable(pid_path: &Path) -> Result<(), GatewayError> {
    if !pid_path.is_absolute() {
        return Err(invalid_record("gateway PID path must be absolute"));
    }
    let parent = pid_path
        .parent()
        .ok_or_else(|| invalid_record("gateway PID path has no parent"))?;
    let effective_uid = nix::unistd::Uid::effective().as_raw();
    let authenticated_operator = if nix::unistd::Uid::effective().is_root() {
        authenticated_elevated_operator()
    } else {
        None
    };
    let expected_owner = authenticated_operator.unwrap_or(effective_uid);
    check_effigy_ancestors(parent, expected_owner)
}

#[cfg(unix)]
fn check_effigy_ancestors(parent: &Path, gateway_owner_uid: u32) -> Result<(), GatewayError> {
    use std::os::unix::fs::MetadataExt;
    for ancestor in parent
        .ancestors()
        .filter(|path| path.file_name().is_some_and(|name| name == ".effigy"))
    {
        match fs::symlink_metadata(ancestor) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(GatewayError::Io(error)),
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || !metadata.is_dir()
                    || metadata.mode() & 0o022 != 0
                    || (metadata.uid() != 0 && metadata.uid() != gateway_owner_uid)
                {
                    return Err(invalid_record("gateway state parent is unsafe"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn trusted_directory_owner(_pid_path: &Path) -> Result<u32, GatewayError> {
    Ok(0)
}

#[cfg(not(unix))]
fn ensure_gateway_parent_creatable(_pid_path: &Path) -> Result<(), GatewayError> {
    Ok(())
}

/// Validate `.effigy` ancestor trust, create a missing gateway parent, then
/// re-check directory trust. Every create/open/lock path on an absent canonical
/// gateway directory must call this before mutating that path.
pub fn ensure_trusted_gateway_parent(pid_path: &Path) -> Result<u32, GatewayError> {
    ensure_gateway_parent_creatable(pid_path)?;
    let parent = pid_path
        .parent()
        .ok_or_else(|| invalid_record("gateway PID path has no parent"))?;
    let parent_missing = match fs::symlink_metadata(parent) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => return Err(GatewayError::Io(error)),
        Ok(_) => false,
    };
    if parent_missing {
        fs::create_dir_all(parent).map_err(GatewayError::Io)?;
    }
    trusted_directory_owner(pid_path)
}

#[cfg(unix)]
fn read_trusted_file(
    path: &Path,
    directory_owner: u32,
    max_bytes: usize,
) -> Result<Option<(Vec<u8>, u32, u32)>, GatewayError> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(GatewayError::Io(error)),
    };
    if before.file_type().is_symlink() || !before.is_file() || before.mode() & 0o022 != 0 {
        return Err(invalid_record("gateway record file is unsafe"));
    }
    if before.uid() != directory_owner && before.uid() != 0 {
        return Err(invalid_record("gateway record owner is untrusted"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(GatewayError::Io)?;
    let metadata = file.metadata().map_err(GatewayError::Io)?;
    if metadata.dev() != before.dev()
        || metadata.ino() != before.ino()
        || !metadata.is_file()
        || metadata.mode() & 0o022 != 0
        || (metadata.uid() != directory_owner && metadata.uid() != 0)
    {
        return Err(invalid_record("gateway record changed while opening"));
    }
    let mut bytes = Vec::new();
    file.take((max_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(GatewayError::Io)?;
    if bytes.len() > max_bytes {
        return Err(invalid_record("gateway record exceeds the size limit"));
    }
    Ok(Some((bytes, metadata.uid(), metadata.mode())))
}

#[cfg(not(unix))]
fn read_trusted_file(
    path: &Path,
    _directory_owner: u32,
    max_bytes: usize,
) -> Result<Option<(Vec<u8>, u32, u32)>, GatewayError> {
    match fs::read(path) {
        Ok(bytes) if bytes.len() <= max_bytes => Ok(Some((bytes, 0, 0o600))),
        Ok(_) => Err(invalid_record("gateway record exceeds the size limit")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(GatewayError::Io(error)),
    }
}

#[cfg(unix)]
fn validate_replace_target(path: &Path, directory_owner: u32) -> Result<(), GatewayError> {
    use std::os::unix::fs::MetadataExt;
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.mode() & 0o077 != 0
                || (metadata.uid() != directory_owner && metadata.uid() != 0) =>
        {
            Err(invalid_record(
                "gateway record replacement target is unsafe",
            ))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(GatewayError::Io(error)),
    }
}

#[cfg(not(unix))]
fn validate_replace_target(_path: &Path, _directory_owner: u32) -> Result<(), GatewayError> {
    Ok(())
}

/// Refuse a symlink or non-file version target. Existing regular files may be
/// 0644 from older writers; replacement still publishes owner-only bytes.
#[cfg(unix)]
fn validate_version_replace_target(path: &Path, directory_owner: u32) -> Result<(), GatewayError> {
    use std::os::unix::fs::MetadataExt;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
            invalid_record("gateway record replacement target is unsafe"),
        ),
        Ok(metadata) if metadata.uid() != directory_owner && metadata.uid() != 0 => {
            Err(invalid_record("gateway record owner is untrusted"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(GatewayError::Io(error)),
    }
}

#[cfg(not(unix))]
fn validate_version_replace_target(
    _path: &Path,
    _directory_owner: u32,
) -> Result<(), GatewayError> {
    Ok(())
}

#[cfg(unix)]
fn atomic_publish(path: &Path, bytes: &[u8], owner_uid: u32) -> Result<(), GatewayError> {
    use nix::unistd::{fchown, Gid, Uid};

    atomic_publish_with_owner(
        path,
        bytes,
        owner_uid,
        Uid::effective().is_root(),
        |file, uid, gid| {
            fchown(file, Some(Uid::from_raw(uid)), Some(Gid::from_raw(gid)))
                .map_err(std::io::Error::from)
        },
    )
}

#[cfg(unix)]
fn atomic_publish_with_owner(
    path: &Path,
    bytes: &[u8],
    owner_uid: u32,
    root_writer: bool,
    chown: impl FnOnce(&File, u32, u32) -> std::io::Result<()>,
) -> Result<(), GatewayError> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

    let temporary = crate::atomic_write::temp_path(path, "gateway-record");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&temporary)
        .map_err(GatewayError::Io)?;
    let write_result = file.write_all(bytes).and_then(|()| file.sync_all());
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(GatewayError::Io(error));
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(GatewayError::Io)?;
    fs::rename(&temporary, path).map_err(GatewayError::Io)?;
    if root_writer {
        let metadata = file.metadata().map_err(GatewayError::Io)?;
        if metadata.uid() != owner_uid {
            chown(&file, owner_uid, metadata.gid()).map_err(GatewayError::Io)?;
        }
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(GatewayError::Io)?;
    Ok(())
}

#[cfg(not(unix))]
fn atomic_publish(path: &Path, bytes: &[u8], _owner_uid: u32) -> Result<(), GatewayError> {
    let temporary = crate::atomic_write::temp_path(path, "gateway-record");
    fs::write(&temporary, bytes).map_err(GatewayError::Io)?;
    fs::rename(temporary, path).map_err(GatewayError::Io)
}

#[cfg(unix)]
struct GatewayRecordLock(File);

#[cfg(unix)]
impl GatewayRecordLock {
    fn acquire(pid_path: &Path, owner_uid: u32) -> Result<Self, GatewayError> {
        use nix::unistd::{fchown, Gid, Uid};
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

        let lock_path = pid_path.with_extension("lock");
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
            return Err(invalid_record("gateway record lock is unsafe"));
        }
        if nix::unistd::Uid::effective().is_root() && metadata.uid() != owner_uid {
            fchown(
                &file,
                Some(Uid::from_raw(owner_uid)),
                Some(Gid::from_raw(metadata.gid())),
            )
            .map_err(|error| GatewayError::Io(error.into()))?;
        }
        fs2::FileExt::lock_exclusive(&file).map_err(GatewayError::Io)?;
        Ok(Self(file))
    }
}

#[cfg(unix)]
impl Drop for GatewayRecordLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

#[cfg(not(unix))]
struct GatewayRecordLock;

#[cfg(not(unix))]
impl GatewayRecordLock {
    fn acquire(_pid_path: &Path, _owner_uid: u32) -> Result<Self, GatewayError> {
        Ok(Self)
    }
}

fn is_permission_denied(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
        || matches!(error.raw_os_error(), Some(libc::EACCES) | Some(libc::EPERM))
}

/// Live kernel identity used by both the sidecar matcher and legacy recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveProcessIdentity {
    /// Boot generation for this host.
    pub boot_identity: String,
    /// Precise process start identity.
    pub start_identity: GatewayStartIdentity,
    /// Kernel UID of the live process.
    pub uid: u32,
}

pub(crate) fn read_process_identity(
    pid: u32,
) -> Result<(String, GatewayStartIdentity), std::io::Error> {
    let live = read_live_process_identity(pid)?;
    Ok((live.boot_identity, live.start_identity))
}

/// Read boot, precise start, and kernel UID for one PID.
pub fn read_live_process_identity(pid: u32) -> Result<LiveProcessIdentity, std::io::Error> {
    if crate::server::checked_gateway_pid(pid).is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "gateway PID is outside the supported domain",
        ));
    }
    let boot = effigy_process::boot_identity()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| std::io::Error::other("boot identity is unavailable"))?;
    #[cfg(target_os = "linux")]
    {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
        let start_ticks = linux_start_ticks(&stat).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Linux process start ticks are malformed",
            )
        })?;
        let uid = linux_proc_euid(pid)?;
        Ok(LiveProcessIdentity {
            boot_identity: boot,
            start_identity: GatewayStartIdentity::Linux { start_ticks },
            uid,
        })
    }
    #[cfg(target_os = "macos")]
    {
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        // SAFETY: the pointer is aligned writable storage for exactly one
        // proc_bsdinfo, and pid has been checked as a positive signed PID.
        let read = unsafe {
            libc::proc_pidinfo(
                pid as libc::c_int,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int,
            )
        };
        if read != std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: proc_pidinfo returned exactly the size of the initialized
        // destination structure.
        let info = unsafe { info.assume_init() };
        if info.pbi_pid != pid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "macOS process identity returned a different PID",
            ));
        }
        Ok(LiveProcessIdentity {
            boot_identity: boot,
            start_identity: GatewayStartIdentity::Macos {
                start_seconds: info.pbi_start_tvsec,
                start_microseconds: info.pbi_start_tvusec,
            },
            uid: info.pbi_uid,
        })
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = boot;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "gateway process identity is unsupported on this platform",
        ))
    }
}

#[cfg(target_os = "linux")]
fn linux_proc_euid(pid: u32) -> Result<u32, std::io::Error> {
    let status = fs::read_to_string(format!("/proc/{pid}/status"))?;
    for line in status.lines() {
        let Some(rest) = line.strip_prefix("Uid:") else {
            continue;
        };
        let euid = rest.split_whitespace().nth(1).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Linux process Uid line is malformed",
            )
        })?;
        return euid.parse::<u32>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Linux process euid is malformed",
            )
        });
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Linux process status has no Uid line",
    ))
}

#[cfg(target_os = "linux")]
fn linux_start_ticks(stat: &str) -> Option<u64> {
    let command_end = stat.rfind(')')?;
    stat[command_end + 1..]
        .split_whitespace()
        .nth(19)?
        .parse::<u64>()
        .ok()
}

#[cfg(test)]
pub(crate) fn write_test_record(pid_path: &Path, pid: u32) {
    let (boot_identity, start_identity) = read_process_identity(pid).unwrap_or_else(|_| {
        let boot = effigy_process::boot_identity().unwrap_or_else(|| "test-boot".to_owned());
        #[cfg(target_os = "linux")]
        let start = GatewayStartIdentity::Linux { start_ticks: 1 };
        #[cfg(target_os = "macos")]
        let start = GatewayStartIdentity::Macos {
            start_seconds: 1,
            start_microseconds: 0,
        };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let start = GatewayStartIdentity::Linux { start_ticks: 1 };
        (boot, start)
    });
    let record = GatewayIdentityRecord {
        format_version: IDENTITY_FORMAT_VERSION,
        pid,
        boot_identity,
        start_identity,
    };
    fs::write(pid_path, pid.to_string()).expect("write private gateway PID fixture");
    let sidecar = identity_path(pid_path);
    fs::write(
        &sidecar,
        serde_json::to_vec(&record).expect("serialize fixture"),
    )
    .expect("write private gateway identity fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(sidecar, fs::Permissions::from_mode(0o600))
            .expect("protect private gateway identity fixture");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish_current_fixture(dir: &Path) -> (PathBuf, GatewayRecordSnapshot) {
        let pid_path = dir.join("gateway.pid");
        #[cfg(unix)]
        let owner_uid = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(dir).expect("fixture metadata").uid()
        };
        #[cfg(not(unix))]
        let owner_uid = 0;
        let snapshot = publish_current_gateway(&pid_path, owner_uid).expect("publish identity");
        (pid_path, snapshot)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn gateway_identity_publication_is_atomic_private_and_operator_readable() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().expect("fixture directory");
        let (pid_path, published) = publish_current_fixture(dir.path());
        let snapshot = read_snapshot(&pid_path)
            .expect("trusted read")
            .expect("record pair");
        assert_eq!(snapshot.pid(), std::process::id());
        assert_eq!(snapshot.record(), published.record());
        assert_eq!(snapshot.identity_bytes, published.identity_bytes);
        assert_eq!(
            fs::metadata(identity_path(&pid_path)).unwrap().mode() & 0o777,
            0o600
        );
        assert!(snapshot.digest().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_root_writer_validates_owner_before_chown_or_publish() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().expect("fixture directory");
        let pid_path = dir.path().join("gateway.pid");
        let owner_uid = fs::metadata(dir.path()).unwrap().uid();
        let wrong_uid = owner_uid.wrapping_add(1);
        assert!(publish_current_gateway(&pid_path, wrong_uid).is_err());
        assert!(!pid_path.exists());
        assert!(!identity_path(&pid_path).exists());
        assert!(!pid_path.with_extension("lock").exists());

        let staged = dir.path().join("owner-adapter.pid");
        let observed = std::cell::Cell::new(None);
        atomic_publish_with_owner(
            &staged,
            b"operator-record",
            wrong_uid,
            true,
            |file, requested_uid, gid| {
                assert!(file.metadata().unwrap().is_file());
                observed.set(Some((requested_uid, gid)));
                Ok(())
            },
        )
        .expect("record-only root ownership adapter");
        assert_eq!(observed.get().map(|(uid, _)| uid), Some(wrong_uid));
        assert_eq!(fs::read(staged).unwrap(), b"operator-record");
    }

    #[test]
    fn gateway_identity_legacy_and_malformed_bytes_remain_unmodified() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let pid_path = dir.path().join("gateway.pid");
        fs::write(&pid_path, "4242\n").unwrap();
        let legacy_before = fs::read(&pid_path).unwrap();
        let legacy = read_snapshot(&pid_path).unwrap().unwrap();
        assert!(legacy.record().is_none());
        assert_eq!(fs::read(&pid_path).unwrap(), legacy_before);

        let sidecar = identity_path(&pid_path);
        fs::write(&sidecar, b"{broken").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&sidecar, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let sidecar_before = fs::read(&sidecar).unwrap();
        let malformed = read_snapshot(&pid_path).unwrap().unwrap();
        assert!(malformed.record().is_none());
        assert_eq!(fs::read(&pid_path).unwrap(), legacy_before);
        assert_eq!(fs::read(&sidecar).unwrap(), sidecar_before);
    }

    #[test]
    fn gateway_identity_interrupted_pair_is_unknown_and_preserved() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let pid_path = dir.path().join("gateway.pid");
        let sidecar = identity_path(&pid_path);
        fs::write(&sidecar, br#"{"format_version":1}"#).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&sidecar, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let bytes = fs::read(&sidecar).unwrap();
        assert!(read_snapshot(&pid_path).is_err());
        assert_eq!(fs::read(&sidecar).unwrap(), bytes);
    }

    #[test]
    fn legacy_recovery_version_without_pid_is_unknown_and_preserved() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let pid_path = dir.path().join("gateway.pid");
        let version = version_path(&pid_path);
        fs::write(&version, b"v0.13.1+local.test\n").unwrap();
        let before = fs::read(&version).unwrap();
        let error = read_snapshot(&pid_path).expect_err("version-only is unknown");
        assert!(error
            .to_string()
            .contains("gateway version exists without its PID"));
        assert_eq!(fs::read(&version).unwrap(), before);
        assert!(!pid_path.exists());
        assert!(!identity_path(&pid_path).exists());
    }

    #[cfg(unix)]
    #[test]
    fn legacy_recovery_version_symlink_is_rejected_without_following() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().expect("fixture directory");
        let pid_path = dir.path().join("gateway.pid");
        let version = version_path(&pid_path);
        let outside = dir.path().join("outside-version");
        fs::write(&outside, b"sentinel").unwrap();
        symlink(&outside, &version).unwrap();
        assert!(read_snapshot(&pid_path).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"sentinel");
        assert!(fs::symlink_metadata(&version)
            .unwrap()
            .file_type()
            .is_symlink());

        fs::write(&pid_path, "4242\n").unwrap();
        fs::set_permissions(&pid_path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read_snapshot(&pid_path).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"sentinel");
        assert_eq!(fs::read(&pid_path).unwrap(), b"4242\n");
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_symlink_and_unsafe_mode_are_rejected_without_following() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().expect("fixture directory");
        let pid_path = dir.path().join("gateway.pid");
        fs::write(&pid_path, "4242").unwrap();
        let sidecar = identity_path(&pid_path);
        let target = dir.path().join("outside-record");
        fs::write(&target, b"sentinel").unwrap();
        symlink(&target, &sidecar).unwrap();
        assert!(read_snapshot(&pid_path).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"sentinel");
        fs::remove_file(&sidecar).unwrap();
        fs::write(&sidecar, b"{}").unwrap();
        fs::set_permissions(&sidecar, fs::Permissions::from_mode(0o666)).unwrap();
        let before = fs::read(&sidecar).unwrap();
        assert!(read_snapshot(&pid_path).is_err());
        assert_eq!(fs::read(&sidecar).unwrap(), before);
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn gateway_identity_boot_and_precise_start_mismatches_are_not_matches() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let (_, snapshot) = publish_current_fixture(dir.path());
        let record = snapshot.record().expect("published identity");
        // A different canonical session identity proves another boot.
        let mut wrong_boot = record.clone();
        wrong_boot.boot_identity = "00000000-0000-0000-0000-000000000000".to_owned();
        assert_eq!(
            probe_live_identity(&wrong_boot),
            GatewayIdentityProbe::Mismatch
        );

        // Unparseable recorded evidence is Unknown, not a claimed different
        // generation, so a live record is preserved rather than cleaned up.
        let mut unknown_boot = record.clone();
        unknown_boot.boot_identity.push_str("-other");
        assert_eq!(
            probe_live_identity(&unknown_boot),
            GatewayIdentityProbe::Unknown
        );
        let mut wrong_start = record.clone();
        match &mut wrong_start.start_identity {
            GatewayStartIdentity::Linux { start_ticks } => *start_ticks += 1,
            GatewayStartIdentity::Macos {
                start_seconds,
                start_microseconds,
            } => {
                *start_microseconds = (*start_microseconds + 1) % 1_000_000;
                let original_seconds = match &record.start_identity {
                    GatewayStartIdentity::Macos { start_seconds, .. } => *start_seconds,
                    _ => unreachable!("record platform remains unchanged"),
                };
                assert_eq!(*start_seconds, original_seconds);
            }
        }
        assert_eq!(
            probe_live_identity(&wrong_start),
            GatewayIdentityProbe::Mismatch
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn gateway_identity_legacy_boot_time_record_matches_across_microsecond_drift() {
        let live = read_live_process_identity(std::process::id()).expect("live identity");
        let legacy = effigy_process::legacy_boot_time_identity().expect("kernel kern.boottime");
        let (prefix, _) = legacy.split_once("usec").expect("kern.boottime usec field");
        let drifted = format!("{prefix}usec = 999999 }}");

        let mut record = GatewayIdentityRecord {
            format_version: IDENTITY_FORMAT_VERSION,
            pid: std::process::id(),
            boot_identity: drifted.clone(),
            start_identity: live.start_identity.clone(),
        };
        assert!(record.is_valid());
        // An already-upgraded v0.14.0 macOS sidecar stored the drifting
        // `kern.boottime` microsecond field. With the exact process start
        // identity intact it must still match this boot.
        assert_eq!(
            probe_live_identity(&record),
            GatewayIdentityProbe::Matched,
            "legacy boot-time record must survive usec drift"
        );

        // A changed boot-time seconds value is ambiguous: same-boot clock
        // adjustment and a real reboot are indistinguishable, so it is
        // Unknown and the record is preserved, never classified as replaced.
        record.boot_identity = "{ sec = 1, usec = 2 }".to_owned();
        assert_eq!(probe_live_identity(&record), GatewayIdentityProbe::Unknown);

        // A different canonical session identity is a different generation.
        record.boot_identity = "00000000-0000-0000-0000-000000000000".to_owned();
        assert_eq!(probe_live_identity(&record), GatewayIdentityProbe::Mismatch);

        // The exact start identity stays mandatory within the same boot.
        record.boot_identity = drifted;
        record.start_identity = perturbed_start_identity(&live.start_identity);
        assert_eq!(probe_live_identity(&record), GatewayIdentityProbe::Mismatch);

        // The new stable session identity matches exactly.
        record.start_identity = live.start_identity;
        record.boot_identity = effigy_process::boot_identity().expect("session identity");
        assert_eq!(probe_live_identity(&record), GatewayIdentityProbe::Matched);

        // Unknown or malformed recorded boot evidence is Unknown, never a
        // readable different generation.
        record.boot_identity = "not-a-boot-identity".to_owned();
        assert_eq!(probe_live_identity(&record), GatewayIdentityProbe::Unknown);
    }

    #[cfg(target_os = "macos")]
    fn perturbed_start_identity(identity: &GatewayStartIdentity) -> GatewayStartIdentity {
        match identity {
            GatewayStartIdentity::Linux { .. } => unreachable!("macOS fixture"),
            GatewayStartIdentity::Macos {
                start_seconds,
                start_microseconds,
            } => GatewayStartIdentity::Macos {
                start_seconds: *start_seconds,
                start_microseconds: (*start_microseconds + 1) % 1_000_000,
            },
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn gateway_identity_compare_remove_preserves_new_generation_and_is_idempotent() {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("fixture directory");
        let (pid_path, old) = publish_current_fixture(dir.path());
        let mut newer = old.record().expect("record").clone();
        newer.boot_identity.push_str("-new-generation");
        let sidecar = identity_path(&pid_path);
        fs::write(&sidecar, serde_json::to_vec(&newer).unwrap()).unwrap();
        #[cfg(unix)]
        fs::set_permissions(&sidecar, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!remove_if_unchanged(&old).unwrap());
        assert!(pid_path.exists());
        let current = read_snapshot(&pid_path).unwrap().unwrap();
        assert!(remove_if_unchanged(&current).unwrap());
        assert!(remove_if_unchanged(&current).unwrap());
        assert!(!pid_path.exists());
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn gateway_identity_elevated_reader_binds_record_and_canonical_target() {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        let source = tempfile::tempdir().expect("source fixture");
        let target = tempfile::tempdir().expect("target fixture");
        let (source_path, snapshot) = publish_current_fixture(source.path());
        let target_path = target.path().join("gateway.pid");
        fs::copy(&source_path, &target_path).unwrap();
        fs::copy(identity_path(&source_path), identity_path(&target_path)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                identity_path(&target_path),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }
        #[cfg(unix)]
        let owner_uid = fs::metadata(source.path()).unwrap().uid();
        #[cfg(not(unix))]
        let owner_uid = 0;
        let result = read_only_elevated_check(
            &target_path,
            snapshot.digest().as_deref().unwrap(),
            &snapshot.target_digest().unwrap(),
            owner_uid,
        );
        assert_eq!(result, GatewayIdentityProbe::Unknown);
        let result = read_only_elevated_check(
            &source_path,
            snapshot.digest().as_deref().unwrap(),
            &snapshot.target_digest().unwrap(),
            owner_uid,
        );
        assert_eq!(result, GatewayIdentityProbe::Matched);
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_operator_claim_authentication_is_bound() {
        use std::path::PathBuf;
        let home = PathBuf::from("/home/operator");
        // Valid sudo elevation: invoking UID, claimed UID, and passwd HOME agree.
        assert_eq!(
            authenticate_operator_claim(
                0,
                true,
                Some(501),
                true,
                Some(501),
                Some(home.clone()),
                Some(home.clone()),
            ),
            Some(501)
        );
        // No sudo ambient (osascript path): HOME/passwd agreement still binds.
        assert_eq!(
            authenticate_operator_claim(
                0,
                true,
                Some(501),
                false,
                None,
                Some(home.clone()),
                Some(home.clone()),
            ),
            Some(501)
        );
        // Root may never claim itself as the operator.
        assert_eq!(
            authenticate_operator_claim(
                0,
                true,
                Some(0),
                false,
                None,
                Some(home.clone()),
                Some(PathBuf::from("/root")),
            ),
            None
        );
        // Missing elevation marker, non-root caller, forged SUDO_UID, and
        // substituted HOME/passwd pairs all refuse.
        assert_eq!(
            authenticate_operator_claim(
                0,
                false,
                Some(501),
                false,
                None,
                Some(home.clone()),
                Some(home.clone()),
            ),
            None
        );
        assert_eq!(
            authenticate_operator_claim(
                501,
                true,
                Some(501),
                false,
                None,
                Some(home.clone()),
                Some(home.clone()),
            ),
            None
        );
        assert_eq!(
            authenticate_operator_claim(
                0,
                true,
                Some(501),
                true,
                Some(502),
                Some(home.clone()),
                Some(home.clone()),
            ),
            None
        );
        assert_eq!(
            authenticate_operator_claim(
                0,
                true,
                Some(501),
                false,
                None,
                Some(PathBuf::from("/home/attacker")),
                Some(home.clone()),
            ),
            None
        );
        assert_eq!(
            authenticate_operator_claim(0, true, None, false, None, Some(home.clone()), Some(home)),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_operator_dir_trust_across_modeled_elevation() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().expect("fixture directory");
        let pid_path = dir.path().join("gateway.pid");
        fs::write(&pid_path, "4242\n").unwrap();
        let owner = fs::metadata(dir.path()).unwrap().uid();
        let unrelated = owner.wrapping_add(1);
        // Ordinary operator trusts its own safe directory with real rules.
        assert_eq!(
            trusted_directory_owner_with(&pid_path, owner, None).unwrap(),
            owner
        );
        // Unauthenticated root rejects the operator-owned directory: the
        // v0.14.0 failure.
        assert!(trusted_directory_owner_with(&pid_path, 0, None).is_err());
        // The same directory with the same mode/owner checks accepts the
        // authenticated operator UID.
        assert_eq!(
            trusted_directory_owner_with(&pid_path, 0, Some(owner)).unwrap(),
            owner
        );
        // An unrelated owner is never accepted through the operator slot.
        assert!(trusted_directory_owner_with(&pid_path, 0, Some(unrelated)).is_err());
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn gateway_identity_modeled_root_reads_operator_publication() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().expect("fixture directory");
        // Ordinary operator publishes; the modeled elevated root with the
        // same authenticated context reads the identical trusted snapshot.
        let (pid_path, published) = publish_current_fixture(dir.path());
        let owner = fs::metadata(dir.path()).unwrap().uid();
        assert_eq!(published.owner_uid(), owner);
        assert_eq!(
            fs::metadata(identity_path(&pid_path)).unwrap().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::metadata(&pid_path).unwrap().uid(), owner);
        let elevated = read_snapshot_with(&pid_path, 0, Some(owner))
            .expect("trusted elevated read")
            .expect("record pair");
        assert_eq!(elevated.record(), published.record());
        assert_eq!(elevated.digest(), published.digest());
        assert_eq!(
            read_only_elevated_check(
                &pid_path,
                published.digest().as_deref().unwrap(),
                &published.target_digest().unwrap(),
                owner,
            ),
            GatewayIdentityProbe::Matched
        );
        // Ordinary verification of the same generation still matches.
        assert_eq!(
            probe_live_identity(published.record().expect("record")),
            GatewayIdentityProbe::Matched
        );
        // Without the authenticated context the same bytes stay untrusted.
        assert!(read_snapshot_with(&pid_path, 0, None).is_err());
        assert!(read_snapshot_with(&pid_path, 0, Some(owner.wrapping_add(1))).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_modeled_root_preserves_legacy_and_rejects_unsafe_dirs() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().expect("fixture directory");
        let owner = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(dir.path()).unwrap().uid()
        };
        // Legacy numeric-only pair remains record-less (fail closed) under
        // the authenticated elevated context, with bytes preserved.
        let legacy_path = dir.path().join("gateway.pid");
        fs::write(&legacy_path, "4242\n").unwrap();
        let before = fs::read(&legacy_path).unwrap();
        let legacy = read_snapshot_with(&legacy_path, 0, Some(owner))
            .unwrap()
            .unwrap();
        assert!(legacy.record().is_none());
        assert_eq!(fs::read(&legacy_path).unwrap(), before);
        // Writable directory, symlinked parent, and absolute-path violations
        // refuse even with the authenticated context.
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(read_snapshot_with(&legacy_path, 0, Some(owner)).is_err());
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let link_parent = tempfile::tempdir().expect("link parent");
        let linked = link_parent.path().join("linked-gateway");
        symlink(dir.path(), &linked).unwrap();
        assert!(read_snapshot_with(&linked.join("gateway.pid"), 0, Some(owner)).is_err());
        assert!(
            trusted_directory_owner_with(Path::new("relative/gateway.pid"), 0, Some(owner))
                .is_err()
        );
    }
}
