//! Locate the repository's `kernel-custody-fixture` binary from a test build.
//!
//! Cargo may run a test executable from a layout-specific build-unit directory.
//! The lookup therefore tries, in order: the selector's explicit
//! `EFFIGY_KERNEL_CUSTODY_FIXTURE`; the workspace target directory named by
//! `CARGO_TARGET_DIR`; the workspace target directory derived from this crate's
//! manifest (so a focused `effigy test:focused` run finds the uplifted fixture
//! without the selector's export); and finally the Cargo profile directory.

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
    let mut target_dirs = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .into_iter()
        .chain(workspace_target_dir());
    custody_fixture_from(
        &executable,
        target_dirs.by_ref(),
        effigy_core::effigy_invocation::CARGO_PROFILE,
    )
}

fn custody_fixture_from(
    executable: &Path,
    target_dirs: impl IntoIterator<Item = PathBuf>,
    profile: &str,
) -> PathBuf {
    let profile_dir = effigy_core::effigy_invocation::cargo_profile_dir(executable);
    let candidates = target_dirs
        .into_iter()
        .map(|target| target.join(profile).join("kernel-custody-fixture"))
        .chain(profile_dir.map(|dir| dir.join("kernel-custody-fixture")));
    for candidate in candidates {
        if candidate.is_file() {
            return candidate;
        }
    }
    profile_dir
        .map(|dir| dir.join("kernel-custody-fixture"))
        .unwrap_or_else(|| {
            executable
                .parent()
                .expect("test executable directory")
                .join("kernel-custody-fixture")
        })
}

/// `<workspace>/target` for this crate's checkout: the crate manifest sits two
/// directories below the workspace root (`crates/effigy-kernel`).
fn workspace_target_dir() -> Option<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent()?.parent()?;
    Some(root.join("target"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_unit_layout_finds_workspace_profile_fixture() {
        let fixture = tempfile::tempdir().expect("fixture");
        let profile = effigy_core::effigy_invocation::CARGO_PROFILE;
        let executable = fixture
            .path()
            .join(profile)
            .join("build/effigy-kernel/0123456789abcdef/kernel-test");
        std::fs::create_dir_all(executable.parent().expect("unit directory"))
            .expect("create unit directory");
        std::fs::write(&executable, "test harness").expect("write harness");

        let target = fixture.path().join("workspace-target");
        let binary = target.join(profile).join("kernel-custody-fixture");
        std::fs::create_dir_all(binary.parent().expect("profile directory"))
            .expect("create profile directory");
        std::fs::write(&binary, "fixture").expect("write fixture");

        assert_eq!(custody_fixture_from(&executable, [target], profile), binary);
    }
}
