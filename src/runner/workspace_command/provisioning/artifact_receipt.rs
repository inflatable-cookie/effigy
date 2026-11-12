//! Authenticate produced artifacts and own their version, identity and receipt sidecars.

use super::artifact_plan::LinuxWorkspaceTarget;
use crate::runner::workspace_client::context::{
    render_digest, GuestBinaryIdentity, CHANNEL_PROTOCOL_VERSION, HOST_REPREPARE_COMMAND,
};
use crate::runner::RunnerError;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
/// Whether a computed lowercase hex digest is the one a `sha256:<hex>` pin
/// names.
pub(super) fn digest_matches(pinned: &str, computed_hex: &str) -> bool {
    pinned
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.eq_ignore_ascii_case(computed_hex.trim()))
}

/// Authenticate version, target, digest and protocol against the producer's
/// receipt (§8.9). This verifier never writes provenance or blesses bytes.
pub(in crate::runner) fn verify_guest_artifact(
    artifact: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<GuestBinaryIdentity, RunnerError> {
    verify_guest_artifact_until(artifact, target, None)
}

/// Authenticate on the publication deadline; hashing stops when it expires.
pub(in crate::runner) fn verify_guest_artifact_until(
    artifact: &Path,
    target: LinuxWorkspaceTarget,
    until: Option<std::time::Instant>,
) -> Result<GuestBinaryIdentity, RunnerError> {
    let version = read_trimmed_workspace_effigy_active_version(
        &workspace_effigy_active_version_file(artifact),
    )?
    .ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "linux workspace artifact `{}` records no version; run \
             `{HOST_REPREPARE_COMMAND}` on the host to rebuild it",
            artifact.display()
        ))
    })?;
    let host_version = effigy_core::build_info::active_version();
    if version != host_version {
        return Err(RunnerError::task_invocation(format!(
            "linux workspace artifact `{}` is version {} but this host runs {}; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            artifact.display(),
            version,
            host_version
        )));
    }
    let digest = linux_workspace_artifact_digest(artifact, || {
        until.is_none_or(|until| std::time::Instant::now() < until)
    })
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot read linux workspace artifact `{}`: {error}",
            artifact.display()
        ))
    })?
    .ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "linux workspace artifact `{}` is missing; run `{HOST_REPREPARE_COMMAND}` on the \
                 host to re-prepare the workspace",
            artifact.display()
        ))
    })?;
    let digest = render_digest(digest);
    let receipt = workspace_artifact_receipt(artifact)?;
    if receipt.get("release_triple").map(String::as_str) != Some(target.release_triple()) {
        return Err(RunnerError::task_invocation(format!(
            "linux workspace artifact `{}` has no receipt naming target `{}`; run \
             `EFFIGY_LINUX_RELEASE_TRIPLE={} effigy workspace:linux:artifact` on the host",
            artifact.display(),
            target.release_triple(),
            target.release_triple()
        )));
    }
    let recorded = receipt.get("sha256").ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "linux workspace artifact receipt for `{}` names no digest, so its bytes cannot be \
             authenticated; rebuild it with `EFFIGY_LINUX_RELEASE_TRIPLE={} effigy \
             workspace:linux:artifact`",
            artifact.display(),
            target.release_triple()
        ))
    })?;
    if !digest_matches(&digest, recorded) {
        return Err(RunnerError::task_invocation(format!(
            "linux workspace artifact `{}` is {digest} but its receipt records sha256:{recorded}; \
             rebuild it with `EFFIGY_LINUX_RELEASE_TRIPLE={} effigy workspace:linux:artifact`",
            artifact.display(),
            target.release_triple()
        )));
    }
    Ok(GuestBinaryIdentity {
        version,
        digest,
        protocol: CHANNEL_PROTOCOL_VERSION,
    })
}

/// Compare producer output in memory; this digest is never an install identity.
pub(super) fn linux_workspace_artifact_digest(
    artifact_path: &Path,
    mut within_deadline: impl FnMut() -> bool,
) -> std::io::Result<Option<[u8; 32]>> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = match std::fs::File::open(artifact_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut digest = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        if !within_deadline() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "artifact inspection deadline expired",
            ));
        }
        let read = file.read(&mut bytes)?;
        if read == 0 {
            return Ok(Some(digest.finalize().into()));
        }
        digest.update(&bytes[..read]);
    }
}

/// Keep each target's producer receipt beside its binary.
pub(in crate::runner) fn workspace_artifact_receipt_path(artifact: &Path) -> PathBuf {
    let mut path = artifact.as_os_str().to_owned();
    path.push(".receipt");
    PathBuf::from(path)
}

/// The `key=value` receipt an artifact producer writes beside its artifact.
fn workspace_artifact_receipt(artifact: &Path) -> Result<BTreeMap<String, String>, RunnerError> {
    let path = workspace_artifact_receipt_path(artifact);
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot read linux workspace artifact receipt `{}`: {error}",
            path.display()
        ))
    })?;
    let mut fields = BTreeMap::new();
    for line in raw.lines() {
        if let Some((key, value)) = line.trim().split_once('=') {
            fields.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    Ok(fields)
}

pub(super) fn read_trimmed_workspace_effigy_active_version(
    path: &Path,
) -> Result<Option<String>, RunnerError> {
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read workspace effigy active version file `{}`: {error}",
            path.display()
        ))
    })?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.to_owned()))
}

/// Require the expected host identity, version and target receipt.
/// Timestamps or file presence alone never establish currency.
pub(super) fn linux_workspace_effigy_artifact_is_current(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
    expected_identity: &str,
) -> bool {
    artifact_path.is_file()
        && linux_workspace_effigy_artifact_receipt_matches_target(artifact_path, target)
        && read_trimmed_identity_file(&workspace_effigy_active_version_file(artifact_path))
            .as_deref()
            == Some(effigy_core::build_info::active_version().as_str())
        && read_trimmed_identity_file(&workspace_effigy_install_identity_file(artifact_path))
            .as_deref()
            == Some(expected_identity.trim())
}

/// Require a rewritten artifact and current version/target receipt after the build.
pub(super) fn linux_workspace_artifact_produced_since(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
    started: std::time::SystemTime,
) -> bool {
    let written = std::fs::metadata(artifact_path)
        .and_then(|metadata| metadata.modified())
        .is_ok_and(|modified| modified >= started);
    written
        && read_trimmed_identity_file(&workspace_effigy_active_version_file(artifact_path))
            .as_deref()
            == Some(effigy_core::build_info::active_version().as_str())
        && linux_workspace_effigy_artifact_receipt_matches_target(artifact_path, target)
}

pub(super) fn read_trimmed_identity_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
}

/// Name what is stale and the supported rebuild. Reports host and artifact
/// versions and identities, with `unknown` where a sidecar is absent.
pub(super) fn describe_linux_workspace_artifact_refusal(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
    expected_identity: &str,
    effigy_repo_root: &Path,
) -> String {
    let artifact_version =
        read_trimmed_identity_file(&workspace_effigy_active_version_file(artifact_path))
            .unwrap_or_else(|| "unknown".to_owned());
    let artifact_identity =
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(artifact_path))
            .unwrap_or_else(|| "unknown".to_owned());
    let receipt = if linux_workspace_effigy_artifact_receipt_matches_target(artifact_path, target) {
        "matches target"
    } else {
        "missing or names another target"
    };
    format!(
        "linux workspace artifact `{path}` is not current for `{triple}`: host version `{host}`, host identity `{expected_identity}`, artifact version `{artifact_version}`, artifact identity `{artifact_identity}`, artifact receipt {receipt}; rebuild with `EFFIGY_LINUX_RELEASE_TRIPLE={triple} effigy workspace:linux:artifact` from `{root}`",
        path = artifact_path.display(),
        triple = target.release_triple(),
        host = effigy_core::build_info::active_version(),
        root = effigy_repo_root.display(),
    )
}

/// Write the producer receipt an artifact is authenticated against.
pub(super) fn write_workspace_artifact_receipt(
    artifact: &Path,
    target: LinuxWorkspaceTarget,
    digest: &str,
    build_kind: &str,
) -> Result<(), RunnerError> {
    let path = workspace_artifact_receipt_path(artifact);
    let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
    let body = format!(
        "release_triple={triple}\nsha256={hex}\nbuild_kind={build_kind}\n",
        triple = target.release_triple(),
    );
    // Written beside and renamed over the receipt, so a reader never sees a
    // half-written receipt that could name the wrong digest.
    let mut staged = path.as_os_str().to_owned();
    staged.push(".new");
    let staged = PathBuf::from(staged);
    std::fs::write(&staged, body)
        .and_then(|()| std::fs::rename(&staged, &path))
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write linux workspace artifact receipt `{}`: {error}",
                path.display()
            ))
        })
}

pub(in crate::runner) fn workspace_effigy_active_version_file(binary: &Path) -> PathBuf {
    binary.with_extension("active-version")
}

pub(super) fn workspace_effigy_install_identity_file(binary: &Path) -> PathBuf {
    binary.with_extension("install-identity")
}

pub(super) fn write_artifact_identity(binary: &Path, identity: &str) -> Result<(), RunnerError> {
    for (path, value, label) in [
        (
            workspace_effigy_active_version_file(binary),
            effigy_core::build_info::active_version(),
            "active version",
        ),
        (
            workspace_effigy_install_identity_file(binary),
            identity.to_owned(),
            "install identity",
        ),
    ] {
        std::fs::write(&path, format!("{value}\n")).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write workspace effigy {label} file `{}`: {error}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

pub(super) fn workspace_effigy_release_install_identity() -> String {
    format!("release:v{}", effigy_core::build_info::active_version())
}

pub(super) fn workspace_effigy_local_install_identity(
    freshness_anchor: &Path,
) -> Result<String, RunnerError> {
    let metadata = std::fs::metadata(freshness_anchor).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to stat workspace effigy freshness anchor `{}`: {error}",
            freshness_anchor.display()
        ))
    })?;
    let modified = metadata.modified().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read workspace effigy freshness anchor mtime `{}`: {error}",
            freshness_anchor.display()
        ))
    })?;
    let modified_nanos = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "workspace effigy freshness anchor mtime predates unix epoch `{}`: {error}",
                freshness_anchor.display()
            ))
        })?
        .as_nanos();
    Ok(format!(
        "local:v{}:{}:{}",
        effigy_core::build_info::active_version(),
        metadata.len(),
        modified_nanos
    ))
}

pub(super) fn linux_workspace_effigy_artifact_receipt_matches_target(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
) -> bool {
    let receipt_path = workspace_artifact_receipt_path(artifact_path);
    let Ok(raw) = std::fs::read_to_string(receipt_path) else {
        return false;
    };
    raw.lines()
        .any(|line| line.trim() == format!("release_triple={}", target.release_triple()))
}
