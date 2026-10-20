use effigy_test_support::Fixture;
use std::path::Path;
use std::process::Command;

#[test]
#[ignore = "deliberate leak; only run under the negative-control wrapper"]
fn deliberately_leak_directory() {
    let fixture = Fixture::new("effigy-deliberate-leak-").unwrap();
    std::mem::forget(fixture);
}

#[test]
fn suite_assertion_rejects_a_deliberately_leaking_test() {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scripts/fixture_hygiene.py");
    let output = Command::new("python3")
        .arg(script)
        .args(["--timeout", "30", "--"])
        .arg(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "deliberately_leak_directory"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("leaked test fixture:"),
        "{output:?}"
    );
}

#[test]
#[ignore = "deliberate process leak; only run under the negative-control wrapper"]
fn deliberately_leak_process() {
    use effigy_test_support::process::FixtureCommandExt;
    let child = Command::new("sleep").arg("60").spawn_owned().unwrap();
    std::mem::forget(child);
}

#[test]
#[ignore = "forced exit bypasses destructors; only run under the negative-control wrapper"]
fn force_exit_with_process() {
    use effigy_test_support::process::FixtureCommandExt;
    let _child = Command::new("sleep").arg("60").spawn_owned().unwrap();
    std::process::exit(0);
}

#[test]
#[ignore = "timeout bypasses destructors; only run under the negative-control wrapper"]
fn timeout_with_process() {
    use effigy_test_support::process::FixtureCommandExt;
    let _child = Command::new("sleep").arg("60").spawn_owned().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(60));
}

#[test]
fn suite_assertion_rejects_leaked_process_forced_exit_and_timeout() {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scripts/fixture_hygiene.py");
    for (helper, timeout, exit) in [
        ("deliberately_leak_process", "30", 1),
        ("force_exit_with_process", "30", 1),
        ("timeout_with_process", "3", 124),
    ] {
        let output = Command::new("python3")
            .arg(&script)
            .args(["--timeout", timeout, "--"])
            .env(
                "EFFIGY_TEST_PROCESS_HELPER",
                env!("CARGO_BIN_EXE_effigy-fixture-process"),
            )
            .arg(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", helper])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(exit), "{helper}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("leaked test process:"),
            "{helper}: {output:?}"
        );
    }
}

#[test]
fn process_guard_reaps_on_unwind_and_keeps_foreign_child() {
    use effigy_kernel::ident;
    use effigy_kernel::observe::Observed;
    use effigy_test_support::process::FixtureCommandExt;
    let mut foreign = Command::new("sleep").arg("60").spawn_owned().unwrap();
    let mut owned_pid = 0;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let child = Command::new("sleep").arg("60").spawn_owned().unwrap();
        owned_pid = child.id();
        panic!("controlled fixture unwind");
    }));
    assert!(result.is_err());
    assert!(foreign.try_wait().unwrap().is_none());
    assert!(matches!(
        ident::process_identity(owned_pid),
        Observed::Absent(_)
    ));
}

#[test]
fn natural_exit_with_background_group_is_reported_before_cleanup() {
    use effigy_test_support::process::FixtureCommandExt;
    use std::process::Stdio;
    let error = Command::new("sh")
        .args(["-c", "sleep 60 >/dev/null 2>&1 &"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_owned()
        .unwrap()
        .wait_with_output()
        .unwrap_err();
    assert!(
        error.to_string().contains("surviving group members"),
        "{error}"
    );
}

#[test]
#[ignore = "fallback atexit leak; only run from the owning proof"]
fn fallback_exit_with_process_fixture() {
    use effigy_test_support::process::FixtureCommandExt;
    let path = Fixture::process_lifetime("effigy-fallback-exit-").unwrap();
    println!("fallback-fixture={}", path.display());
    let child = Command::new("sleep").arg("60").spawn_owned().unwrap();
    std::mem::forget(child);
}

#[test]
fn fallback_atexit_drains_files_before_reporting_process_leak() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "fallback_exit_with_process_fixture",
            "--nocapture",
        ])
        .env_remove("EFFIGY_TEST_FIXTURE_LEDGER")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let path = stdout
        .lines()
        .find_map(|line| line.split_once("fallback-fixture=").map(|(_, path)| path))
        .expect("fixture path");
    assert!(!Path::new(path).exists(), "fallback left {path}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("leaked test process:"));
}

#[test]
fn wait_reaps_a_closed_group_before_the_guard_drops() {
    use effigy_test_support::process::FixtureCommandExt;
    let mut child = Command::new("sh")
        .args(["-c", "exit 0"])
        .spawn_owned()
        .unwrap();
    assert!(child.wait().unwrap().success());
    assert!(child.group_closed());
    let error = effigy_kernel::custody::test_support::poll(&child).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::ECHILD));
}

#[test]
fn unknown_custody_keeps_fixture_files_and_never_signals_a_foreign_generation() {
    use effigy_kernel::{custody::test_support as custody, ident};
    use effigy_test_support::process::{self, FixtureCommandExt};
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new("effigy-unknown-cleanup-").unwrap();
    let root = fixture.path().to_path_buf();
    let marker = root.join("preserve");
    std::fs::write(&marker, b"preserved").unwrap();
    std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o444)).unwrap();
    let foreign = Fixture::new("effigy-foreign-cleanup-").unwrap();
    let mut child = Command::new("sleep")
        .arg("60")
        .current_dir(foreign.path())
        .spawn_owned()
        .unwrap();
    let mut receipt = custody::register(child.id(), "").unwrap();
    receipt.identity = ident::process_identity(std::process::id())
        .verified()
        .unwrap()
        .clone();
    receipt.identity.pid = child.id();
    receipt.nonce = "deliberately-foreign-generation".into();
    let ledger = process::ledger().unwrap();
    let pending = tempfile::NamedTempFile::new_in(&ledger).unwrap();
    serde_json::to_writer(
        pending.as_file(),
        &serde_json::json!({"owner": root, "receipt": receipt}),
    )
    .unwrap();
    let destination = ledger
        .join("processes")
        .join(pending.path().file_name().unwrap());
    let registration = pending.persist_noclobber(&destination).unwrap();
    let mut owned = Command::new("sleep")
        .arg("60")
        .current_dir(&root)
        .spawn_owned()
        .unwrap();
    assert!(std::panic::catch_unwind(|| drop(fixture)).is_err());
    assert!(
        child.try_wait().unwrap().is_none(),
        "foreign generation was signalled"
    );
    assert!(
        owned.try_wait().unwrap().is_some(),
        "known-owned cleanup stopped at the unknown receipt"
    );
    assert_eq!(std::fs::read(&marker).unwrap(), b"preserved");
    assert_eq!(
        std::fs::metadata(&marker).unwrap().permissions().mode() & 0o222,
        0
    );
    // Remove only the injected receipt and this proof's exact retained tree.
    drop(registration);
    std::fs::remove_file(destination).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
