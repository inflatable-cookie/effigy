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
#[ignore = "detached descendant leak; only run under the negative-control wrapper"]
fn force_exit_after_detached_leader_reap() {
    use effigy_test_support::process::FixtureCommandExt;
    use std::process::Stdio;
    let fixture = Fixture::new("effigy-detached-nonce-").unwrap();
    let output = Command::new("python3")
        .arg("-c")
        .arg(r#"
import json, os, pathlib, subprocess, sys, time
root = pathlib.Path(sys.argv[1])
control = pathlib.Path(os.environ['EFFIGY_TEST_DETACHED_CONTROL'])
(control / 'metadata.json').write_text(json.dumps(dict(
    fixture=str(root), ledger=os.environ['EFFIGY_TEST_FIXTURE_LEDGER'])))
nonce = os.urandom(16).hex()
descendant = '''trap 'touch "$1/signalled"' TERM INT HUP CONT
touch "$1/ready"
while read command; do touch "$1/alive"; done
'''
child = subprocess.Popen(['sh', '-c',
                         'exec 3<&0; env -i PATH=/bin:/usr/bin sh -c "$1" sh "$2" <&3 & '
                         'while [ ! -f release ]; do sleep 0.01; done',
                         'sh', descendant, str(control)],
                         cwd=root, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         env=dict(os.environ, EFFIGY_CUSTODY_NONCE=nonce))
try:
    subprocess.run([os.environ['EFFIGY_TEST_PROCESS_HELPER'], 'register', str(child.pid), str(root), nonce], check=True)
    deadline = time.monotonic() + 5
    while not (control / 'ready').exists():
        assert time.monotonic() < deadline, 'descendant readiness deadline'
        time.sleep(0.01)
    (root / 'release').touch()
    assert child.wait(timeout=10) == 0
except BaseException:
    import signal
    os.killpg(child.pid, signal.SIGKILL)
    child.wait()
    raise
"#)
        .arg(fixture.path())
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_owned()
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    std::process::exit(0);
}

#[test]
fn suite_assertion_rejects_leaked_process_forced_exit_and_timeout() {
    for (helper, timeout, exit) in [
        ("deliberately_leak_process", "30", 1),
        ("force_exit_with_process", "30", 1),
        ("timeout_with_process", "3", 124),
    ] {
        assert_process_leak_is_settled(helper, timeout, exit);
    }
}

#[test]
fn suite_assertion_refuses_nonce_free_descendant_after_detached_leader_reap() {
    use effigy_test_support::process::FixtureCommandExt;
    let control = Fixture::new("effigy-detached-refusal-").unwrap();
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scripts/fixture_hygiene.py");
    let output = Command::new("python3")
        .arg("-c")
        .arg(
            r#"
import json, os, pathlib, shutil, subprocess, sys, time
control, script, binary, helper = sys.argv[1:]
control = pathlib.Path(control)
env = dict(os.environ, EFFIGY_TEST_DETACHED_CONTROL=str(control),
           EFFIGY_TEST_PROCESS_HELPER=helper)
# The writer remains here after the forced-exit child and wrapper have ended.
# Closing it in finally lets the nonce-free descendant exit without signalling.
wrapper = subprocess.Popen([sys.executable, script, '--timeout', '30', '--',
                            binary, '--ignored', '--exact',
                            'force_exit_after_detached_leader_reap'],
                           env=env, stdin=subprocess.PIPE,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
try:
    result = wrapper.wait(timeout=60)
    stdout, stderr = wrapper.stdout.read(), wrapper.stderr.read().decode()
    assert result == 2, (result, stdout, stderr)
    assert 'fixture process residue:' in stderr, stderr
    assert 'group contains members without generation evidence' in stderr, stderr
    assert 'unresolved fixture ownership; retained ledger:' in stderr, stderr
    metadata = json.loads((control / 'metadata.json').read_text())
    ledger, fixture = pathlib.Path(metadata['ledger']), pathlib.Path(metadata['fixture'])
    assert any((ledger / 'processes').iterdir()), 'Unknown lost its process receipt'
    assert any((ledger / 'directories').iterdir()), 'Unknown lost its directory receipt'
    assert (fixture / 'release').exists(), 'Unknown cleaned the fixture'
    wrapper.stdin.write(b'probe\n')
    wrapper.stdin.flush()
    deadline = time.monotonic() + 5
    while not (control / 'alive').exists():
        assert time.monotonic() < deadline, 'descendant was killed or stopped'
        time.sleep(0.01)
    assert not (control / 'signalled').exists(), 'Unknown signalled the descendant'
finally:
    wrapper.stdin.close()
    if wrapper.poll() is None:
        wrapper.kill()  # This exact retained direct child, never a discovered PID.
        wrapper.wait()
    metadata_file = control / 'metadata.json'
    if metadata_file.exists():
        metadata = json.loads(metadata_file.read_text())
        ledger = pathlib.Path(metadata['ledger'])
        # Reobserve closure after cooperative EOF; remove only this control's
        # retained private ledger once no process registration remains.
        cleanup = subprocess.run([helper, 'sweep', str(ledger)],
                                 capture_output=True, timeout=30)
        assert cleanup.returncode in (0, 1), cleanup.stderr
        assert not any((ledger / 'processes').iterdir()), cleanup.stderr
        shutil.rmtree(ledger)
"#,
        )
        .arg(control.path())
        .arg(script)
        .arg(std::env::current_exe().unwrap())
        .arg(env!("CARGO_BIN_EXE_effigy-fixture-process"))
        .output_owned()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn assert_process_leak_is_settled(helper: &str, timeout: &str, exit: i32) {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scripts/fixture_hygiene.py");
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("leaked test process:"),
        "{helper}: {output:?}"
    );
    assert!(
        !stderr.contains("unresolved fixture ownership"),
        "{helper}: {output:?}"
    );
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
    assert!(matches!(
        effigy_kernel::ident::process_identity(child.id()),
        effigy_kernel::observe::Observed::Absent(_)
    ));
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

#[test]
fn unknown_kernel_state_preserves_fixture_files() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new("effigy-unknown-kernel-state-").unwrap();
    let root = fixture.path().to_path_buf();
    let state = root.join(".effigy/state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
    let database = state.join("kernel.db");
    std::fs::write(&database, b"deliberately corrupt private fixture database").unwrap();
    std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(std::panic::catch_unwind(|| drop(fixture)).is_err());
    assert_eq!(
        std::fs::read(database).unwrap(),
        b"deliberately corrupt private fixture database"
    );
    // Only this proof's exact retained tree; its ledger observes absence later.
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn immediate_exit_and_failed_start_preserve_reusable_command_custody() {
    use effigy_test_support::process::FixtureCommandExt;
    let fixture = Fixture::new("effigy-fast-child-").unwrap();
    let mut command = Command::new("sh");
    command.args(["-c", "printf registered; exit 23"]);
    command.current_dir(fixture.path().join("missing"));
    assert_eq!(
        command.spawn_owned().unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    command.current_dir(fixture.path());
    for _ in 0..16 {
        let output = command.output_owned().unwrap();
        assert_eq!(output.status.code(), Some(23));
        assert_eq!(output.stdout, b"registered");
    }
}

#[test]
fn fixture_removal_retains_native_authority_over_an_exited_leader() {
    use effigy_test_support::process::FixtureCommandExt;
    let fixture = Fixture::new("effigy-native-root-cleanup-").unwrap();
    let root = fixture.path().to_path_buf();
    let mut child = Command::new("sh")
        .args(["-c", "env -i PATH=/bin:/usr/bin sleep 60 >/dev/null 2>&1 &"])
        .current_dir(&root)
        .spawn_owned()
        .unwrap();
    assert!(child.wait().unwrap().success());
    assert!(!child.group_closed());
    // There is no descendant nonce: only the retained native Child can settle
    // this group. Fixture removal must share it rather than detach the receipt.
    drop(fixture);
    assert!(!root.exists());
    assert!(child.group_closed());
    assert!(child.try_wait().unwrap().unwrap().success());
}
