//! Ordinary test runs prepare the kernel's custody fixture binary.
//!
//! Proofs in several crates re-exec `kernel-custody-fixture` as their kernel
//! helper (contract 052 §5.7): the release Run's custody and git proof takes it
//! from `EFFIGY_KERNEL_CUSTODY_FIXTURE` when its own selector builds it, and
//! otherwise looks for the built binary. Cargo builds a package's binary
//! targets when one of that package's integration tests is selected, so this
//! target makes a plain `cargo test`/`effigy test` run - which selects every
//! integration test - build the fixture, and it proves the prepared binary is
//! both a capturable kernel helper and a running one that serves kernel entries.
//!
//! `EFFIGY_KERNEL_CUSTODY_FIXTURE` is deliberately not consulted here: this is
//! the proof that the binary is *built*, not that a selector named one.

use std::path::Path;
use std::process::{Command, Stdio};

/// The custody fixture binary this package's ordinary test run builds.
fn prepared_fixture() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_kernel-custody-fixture"))
}

#[test]
fn an_ordinary_test_run_prepares_the_custody_fixture_binary() {
    let path = prepared_fixture();
    assert!(
        path.is_file(),
        "ordinary test runs must build `{}`",
        path.display()
    );
    effigy_kernel::supervise::Executable::capture(path)
        .expect("the prepared custody fixture is a capturable helper binary");
}

#[test]
fn the_prepared_custody_fixture_serves_kernel_helper_entries() {
    // `__kernel-anchor` requires the custody nonce, so an invocation without
    // one must exit non-zero and name it: the binary dispatches kernel entries
    // rather than merely existing.
    let output = Command::new(prepared_fixture())
        .arg("__kernel-anchor")
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("run the prepared custody fixture");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "an `__kernel-anchor` entry without an image frame must fail closed: {stderr}"
    );
    assert!(
        stderr.contains("anchor nonce missing"),
        "the fixture must serve the kernel anchor entry, got: {stderr}"
    );
}
