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
fn suite_assertion_rejects_leaked_process_and_forced_exit() {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scripts/fixture_hygiene.py");
    for helper in ["deliberately_leak_process", "force_exit_with_process"] {
        let output = Command::new("python3")
            .arg(&script)
            .args(["--timeout", "30", "--"])
            .env(
                "EFFIGY_TEST_PROCESS_HELPER",
                env!("CARGO_BIN_EXE_effigy-fixture-process"),
            )
            .arg(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", helper])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{helper}: {output:?}");
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
