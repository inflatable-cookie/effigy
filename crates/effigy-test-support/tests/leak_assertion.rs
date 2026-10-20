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
