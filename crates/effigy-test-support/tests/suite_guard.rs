//! Negative controls for the suite sweep.
//!
//! Each control runs this binary under `tests/scripts/suite_guard.py` as a
//! deliberately leaking runner, and requires the guard to name the leak and
//! leave nothing behind.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn guard() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scripts/suite_guard.py")
}

fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 only probes for the exact PID this proof started.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn reaped(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !alive(pid)
}

fn reported(output: &std::process::Output) -> (u32, String) {
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let pid = stdout
        .lines()
        .find_map(|line| line.strip_prefix("leaked-child="))
        .unwrap_or_else(|| panic!("runner did not name its child; {stdout}; {stderr}"))
        .trim()
        .parse()
        .expect("child PID");
    (pid, stderr)
}

/// A child that leaves the runner's session and refuses cooperative signals.
/// It is deliberately never waited on: the suite sweep must end it.
#[allow(clippy::zombie_processes)]
fn leaked_child() -> u32 {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "trap '' TERM HUP; while :; do /bin/sleep 0.05; done"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: `setsid` is async-signal-safe and runs in the forked child.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().expect("detached child");
    println!("leaked-child={}", child.id());
    std::io::stdout().flush().expect("flush child PID");
    child.id()
}

#[test]
fn selector_fails_and_names_a_setsid_child_that_traps_term_and_hup() {
    let output = Command::new("python3")
        .arg(guard())
        .arg("--")
        .arg(std::env::current_exe().expect("test binary"))
        .args([
            "suite_guard_leaks_and_panics",
            "--ignored",
            "--exact",
            "--nocapture",
        ])
        .output()
        .expect("suite guard runs");
    let (pid, stderr) = reported(&output);
    assert!(!output.status.success(), "the leak was hidden: {stderr}");
    assert!(
        stderr.contains(&format!("leaked test process: {pid} (")),
        "the report does not name {pid}: {stderr}"
    );
    assert!(reaped(pid), "{pid} outlived the sweep");
}

#[test]
fn selector_sweeps_after_the_runner_is_killed_mid_test() {
    let output = Command::new("python3")
        .arg(guard())
        .args(["--timeout", "5", "--"])
        .arg(std::env::current_exe().expect("test binary"))
        .args([
            "suite_guard_child_outlives_runner",
            "--ignored",
            "--exact",
            "--nocapture",
        ])
        .output()
        .expect("guarded runner");
    let (pid, stderr) = reported(&output);
    assert_eq!(output.status.code(), Some(124), "{stderr}");
    assert!(
        stderr.contains(&format!("leaked test process: {pid} (")),
        "the report does not name {pid}: {stderr}"
    );
    assert!(reaped(pid), "{pid} outlived the killed runner");
}

#[test]
fn a_parent_bound_fixture_exits_when_its_runner_is_killed() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let mut runner = Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--ignored",
            "--exact",
            "parent_bound_fixture_runner",
            "--nocapture",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("start fixture runner");
    let mut line = String::new();
    let mut output = BufReader::new(runner.stdout.take().expect("runner stdout"));
    let mut fixture_pid = None;
    let mut read_error = None;
    loop {
        line.clear();
        match output.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Some(pid) = line.trim().strip_prefix("parent-bound-fixture=") {
                    fixture_pid = Some(pid.to_owned());
                    break;
                }
            }
            Err(error) => {
                read_error = Some(error);
                break;
            }
        }
    }
    // SAFETY: this is the runner process this test just spawned. Kill and
    // reap it before asserting on its output so every path settles the child.
    let kill_result = unsafe { libc::kill(runner.id() as i32, libc::SIGKILL) };
    let runner_status = runner.wait().expect("reap fixture runner");
    assert_eq!(kill_result, 0, "kill fixture runner");
    assert!(!runner_status.success(), "runner should have been killed");
    assert!(read_error.is_none(), "read runner output: {read_error:?}");
    let pid = fixture_pid
        .expect("runner exited before publishing fixture PID")
        .parse::<u32>()
        .expect("fixture pid");
    let gone = reaped(pid);
    if !gone {
        // SAFETY: this exact fixture PID was printed by the runner.
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    }
    assert!(gone, "parent-bound fixture {pid} outlived its runner");
}

/// Inner runner used by the killed-runner proof above.
#[test]
#[ignore = "runner killed by its parent proof"]
fn parent_bound_fixture_runner() {
    use std::process::Stdio;

    let mut fixture = Command::new("/bin/sh")
        .args(["-c", effigy_test_support::process::parent_exit_guard()])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start parent-bound fixture");
    println!("parent-bound-fixture={}", fixture.id());
    std::io::stdout().flush().expect("publish fixture PID");
    std::thread::sleep(Duration::from_secs(120));
    let _ = fixture.wait();
}

/// The leaking runner: a detached child, then an unwind.
#[test]
#[ignore = "leaking runner; launched only by the suite sweep proof"]
fn suite_guard_leaks_and_panics() {
    assert!(
        std::env::var("EFFIGY_TEST_SUITE").is_ok_and(|suite| !suite.is_empty()),
        "the runner must carry the suite marker"
    );
    leaked_child();
    panic!("controlled runner failure after the child detached");
}

/// The surviving runner: a detached child, then a bounded wait the guard ends.
#[test]
#[ignore = "leaking runner; launched only by the suite sweep proof"]
fn suite_guard_child_outlives_runner() {
    leaked_child();
    std::thread::sleep(Duration::from_secs(120));
}
