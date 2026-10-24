//! Locate the repository's `kernel-custody-fixture` binary from a test build.
//!
//! Cargo runs test executables from its *build* directory. Oxide points that
//! build directory at a shared slot and uplifts final binaries to
//! `<workspace>/target/<profile>/` (Oxide #106), so the profile-directory
//! ancestor of the running test executable no longer holds a sibling binary.
//! `CARGO_TARGET_DIR` names the workspace target directory, so a candidate
//! there is preferred when it exists; the sibling path stays as the fallback
//! for a plain Cargo build without that variable.

use std::path::PathBuf;

/// The custody fixture binary: `EFFIGY_KERNEL_CUSTODY_FIXTURE` when the
/// selector sets it, else the built binary in the workspace target directory,
/// else the sibling of the running test executable's profile directory.
pub(crate) fn custody_fixture() -> PathBuf {
    if let Some(explicit) = std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE") {
        return PathBuf::from(explicit);
    }
    let executable = std::env::current_exe().expect("test executable");
    let profile_dir = executable
        .parent()
        .and_then(std::path::Path::parent)
        .expect("target profile directory");
    if let Some(target) = std::env::var_os("CARGO_TARGET_DIR") {
        let candidate = PathBuf::from(target)
            .join(profile_dir.file_name().expect("profile directory name"))
            .join("kernel-custody-fixture");
        if candidate.is_file() {
            return candidate;
        }
    }
    profile_dir.join("kernel-custody-fixture")
}
