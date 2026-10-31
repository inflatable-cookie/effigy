//! Private, deadline-bounded materialization of one frozen render closure.
//!
//! A preflight that must ask the backend about the render itself — Compose's own
//! `config --hash` — can only describe a closure it can read, while the one apply
//! publishes that closure through reconcile. This is the owner for putting the
//! frozen bytes in place for that read without any of the authority a Prepare has:
//!
//! * it never acquires `Gate(Env)` and never mutates desired state, so a caller
//!   that already holds the environment gate (the reserved publication's gated
//!   recheck) can call it;
//! * it stages the closure in a private directory with explicit modes, syncs it,
//!   and promotes it atomically into its content-addressed object directory, so a
//!   reader sees the whole object or none of it;
//! * it accepts an already-present byte-identical object unchanged and refuses
//!   bytes that differ, so it never replaces a published object;
//! * it is bounded by the caller's absolute deadline: an expired budget promotes
//!   nothing, a promotion is refused once the deadline has passed, and this
//!   function is synchronous, so no abandoned worker can publish later;
//! * it cleans up its own staging on every refusal, including a refused promotion
//!   and a promotion lost to a concurrent publisher.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::publication;
use crate::policy::model::ContainerPolicyError;
use crate::EffectiveContainerPolicy;

/// Materialize one policy's frozen render closure for a read-only preflight.
pub fn materialize_render_for_preflight(
    policy: &EffectiveContainerPolicy,
    deadline: Instant,
) -> Result<(), ContainerPolicyError> {
    let Some(render) = &policy.render else {
        return Ok(());
    };
    let destination = render.path.clone();
    let files = render
        .closure
        .files()
        .iter()
        .map(|(relative, bytes)| (relative.clone(), bytes.clone()))
        .collect::<Vec<_>>();
    // Already sealed: accept identical bytes, refuse anything else. Re-verified
    // even on the fast path, so a changed object never passes as this closure.
    if destination.exists() {
        return verify_object(&destination, &files);
    }
    ensure_live(deadline, "before staging the render")?;
    let objects = publication::object_directory(&policy.repo_root)?;
    // The kernel refuses a state directory other users can read, so every
    // component this preflight has to create is created private rather than with
    // the caller's umask.
    ensure_private_directory(&objects)?;
    let staging = staging_directory(&objects);
    std::fs::create_dir(&staging).map_err(|error| read_error(&staging, error))?;
    let staged = stage(&staging, &files, deadline);
    if let Err(error) = staged {
        remove_tree(&staging);
        return Err(error);
    }
    // Nothing is promoted after the deadline: the caller has already stopped
    // waiting, so a late object would be authority nobody asked for.
    if ensure_live(deadline, "before promoting the render").is_err() {
        remove_tree(&staging);
        return Err(timeout_error("promoting the render"));
    }
    match std::fs::rename(&staging, &destination) {
        Ok(()) => {
            sync_directory(&objects);
            Ok(())
        }
        Err(_) => {
            // A concurrent publisher may have sealed the same digest first. That
            // is the same object if its bytes match; anything else refuses.
            remove_tree(&staging);
            let verified = verify_object(&destination, &files);
            if verified.is_ok() {
                sync_directory(&objects);
            }
            verified
        }
    }
}

/// Every frozen file must be at `directory` with exactly its bytes.
fn verify_object(
    directory: &Path,
    files: &[(String, Vec<u8>)],
) -> Result<(), ContainerPolicyError> {
    for (relative, bytes) in files {
        let path = directory.join(relative);
        match std::fs::read(&path) {
            Ok(existing) if existing == *bytes => {}
            Ok(_) => {
                return Err(ContainerPolicyError::TaskInvocation(format!(
                    "the render object `{}` already holds other bytes; a preflight never \
                     replaces a published object",
                    path.display()
                )))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(ContainerPolicyError::TaskInvocation(format!(
                    "the render object `{}` is missing from the sealed render; refusing to mix \
                     a partial object with the preflight's closure",
                    path.display()
                )))
            }
            Err(error) => return Err(read_error(&path, error)),
        }
    }
    Ok(())
}

/// Write every frozen file into the private staging directory, with explicit
/// modes and a deadline check between files.
fn stage(
    staging: &Path,
    files: &[(String, Vec<u8>)],
    deadline: Instant,
) -> Result<(), ContainerPolicyError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    std::fs::set_permissions(staging, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| read_error(staging, error))?;
    for (relative, bytes) in files {
        ensure_live(deadline, "while staging the render")?;
        let path = staging.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| read_error(parent, error))?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| read_error(parent, error))?;
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| read_error(&path, error))?;
        file.write_all(bytes)
            .map_err(|error| read_error(&path, error))?;
        file.sync_all().map_err(|error| read_error(&path, error))?;
    }
    // Every directory and file this staging created carries an explicit private
    // mode, whatever the umask says: the kernel refuses a state directory other
    // users can read, and it validates the promoted tree.
    seal_modes(staging);
    sync_tree(staging);
    Ok(())
}

/// Mode 0700 on every directory and 0600 on every file under `root`.
fn seal_modes(root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700));
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => seal_modes(&path),
                Ok(_) => {
                    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                }
                Err(_) => {}
            }
        }
    }
}

/// Create `path` and any missing parents with mode 0700. An existing directory is
/// left as the kernel made it.
fn ensure_private_directory(path: &Path) -> Result<(), ContainerPolicyError> {
    use std::os::unix::fs::PermissionsExt;
    let mut missing = Vec::new();
    let mut current = Some(path);
    while let Some(directory) = current {
        if directory.exists() {
            break;
        }
        missing.push(directory.to_path_buf());
        current = directory.parent();
    }
    for directory in missing.into_iter().rev() {
        std::fs::create_dir(&directory).map_err(|error| read_error(&directory, error))?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| read_error(&directory, error))?;
    }
    Ok(())
}

fn ensure_live(deadline: Instant, phase: &str) -> Result<(), ContainerPolicyError> {
    let live = deadline
        .checked_duration_since(Instant::now())
        .is_some_and(|remaining| !remaining.is_zero());
    if live {
        Ok(())
    } else {
        Err(timeout_error(phase))
    }
}

fn timeout_error(phase: &str) -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation(format!(
        "the render preflight deadline expired {phase}; nothing was promoted"
    ))
}

fn staging_directory(objects: &Path) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    objects.join(format!(
        ".preflight-{pid}-{counter}",
        pid = std::process::id()
    ))
}

fn read_error(path: &Path, error: std::io::Error) -> ContainerPolicyError {
    ContainerPolicyError::Read {
        path: path.to_path_buf(),
        error,
    }
}

/// Remove this call's own staging directory. Only its own tree: another
/// publisher's staging belongs to that publisher.
fn remove_tree(staging: &Path) {
    let _ = std::fs::remove_dir_all(staging);
}

fn sync_tree(root: &Path) {
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                sync_tree(&path);
            }
        }
    }
    sync_directory(root);
}

fn sync_directory(path: &Path) {
    if let Ok(directory) = std::fs::File::open(path) {
        let _ = directory.sync_all();
    }
}

#[cfg(test)]
#[path = "preflight/tests.rs"]
mod tests;
