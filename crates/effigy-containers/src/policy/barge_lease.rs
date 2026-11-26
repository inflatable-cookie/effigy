//! The Barge lease an environment's generated Compose is labelled with.
//!
//! The lifecycle records the lease ID in checkout state before it renders, so
//! every later render of the same environment (a task's apply, a status read)
//! names the same `barge.lease` and one apply never recreates what another
//! created. No record means no label.

use std::io;
use std::path::{Path, PathBuf};

/// Container label naming the Barge lease that reserves a stack's slot.
pub const BARGE_LEASE_LABEL: &str = "barge.lease";

const DIRECTORY: &str = ".effigy/runtime/barge-lease";

fn record_path(repo_root: &Path, profile: &str, project: &str) -> Option<PathBuf> {
    let plain = |value: &str| !value.is_empty() && !value.contains(['/', '\\', '\0']);
    if !plain(profile) || !plain(project) {
        return None;
    }
    Some(
        effigy_core::worktree_scope::checkout_root(repo_root)
            .join(DIRECTORY)
            .join(format!("{profile}.{project}")),
    )
}

/// The lease ID recorded for `(profile, project)` in this checkout.
pub fn recorded_barge_lease(repo_root: &Path, profile: &str, project: &str) -> Option<String> {
    let text = std::fs::read_to_string(record_path(repo_root, profile, project)?).ok()?;
    let id = text.trim();
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then(|| id.to_owned())
}

/// Record the lease ID the next render labels with, or forget it with `None`.
pub fn record_barge_lease(
    repo_root: &Path,
    profile: &str,
    project: &str,
    id: Option<&str>,
) -> io::Result<()> {
    let path = record_path(repo_root, profile, project).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cannot record a Barge lease for `{profile}/{project}`"),
        )
    })?;
    let Some(id) = id else {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Renamed into place, so a concurrent render never reads half an ID.
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary, id)?;
    std::fs::rename(&temporary, &path)
}
