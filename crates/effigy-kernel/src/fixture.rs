//! Locate the repository's `kernel-custody-fixture` binary from a test build.
//!
//! Cargo runs test executables from its *build* directory. Oxide points that
//! build directory at a shared slot and uplifts final binaries to
//! `<workspace>/target/<profile>/` (Oxide #106), so the profile-directory
//! ancestor of the running test executable no longer holds a sibling binary.
//! The lookup therefore tries, in order: the selector's explicit
//! `EFFIGY_KERNEL_CUSTODY_FIXTURE`; the workspace target directory named by
//! `CARGO_TARGET_DIR`; the workspace target directory derived from this crate's
//! manifest (so a focused `effigy test:focused` run finds the uplifted fixture
//! without the selector's export); and finally the sibling path for a plain
//! Cargo build.

use std::path::{Path, PathBuf};

/// The custody fixture binary: `EFFIGY_KERNEL_CUSTODY_FIXTURE` when the
/// selector sets it, else the first built candidate in a workspace target
/// directory, else the sibling of the running test executable's profile
/// directory.
pub(crate) fn custody_fixture() -> PathBuf {
    if let Some(explicit) = std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE") {
        return PathBuf::from(explicit);
    }
    let executable = std::env::current_exe().expect("test executable");
    let profile_dir = executable
        .parent()
        .and_then(std::path::Path::parent)
        .expect("target profile directory");
    let profile = profile_dir.file_name().expect("profile directory name");
    let workspace_targets = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .into_iter()
        .chain(workspace_target_dir());
    for target in workspace_targets {
        let candidate = target.join(profile).join("kernel-custody-fixture");
        if candidate.is_file() {
            return candidate;
        }
    }
    profile_dir.join("kernel-custody-fixture")
}

/// `<workspace>/target` for this crate's checkout: the crate manifest sits two
/// directories below the workspace root (`crates/effigy-kernel`).
fn workspace_target_dir() -> Option<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent()?.parent()?;
    Some(root.join("target"))
}
