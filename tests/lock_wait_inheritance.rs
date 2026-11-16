//! `--lock-wait-ms` reaches the nested Effigy invocations a task step starts,
//! and it is accepted before `skill run` (effigy#1018).
//!
//! A step that runs `effigy <selector>` gets its own process. Before the fix
//! the flag reached only the outer task, so the nested call failed fast with
//! exit 75 against a lock another run held. The flag now travels in
//! `EFFIGY_LOCK_WAIT_MS`, which every child process inherits.

#![cfg(unix)]

use effigy_test_support::process::{FixtureCommandExt, OwnedChild as Child};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

/// `setup` is the lock another run holds until the test releases it. `outer`
/// reaches the same `setup` through a nested Effigy process.
const MANIFEST: &str = r#"[tasks.setup]
run = "echo ready > ready.marker; while [ ! -f release.marker ]; do sleep 0.1; done"

[tasks.outer]
run = "\"$EFFIGY_TEST_BIN\" setup && touch nested.ok"
"#;

struct Fixture {
    _home: effigy_test_support::Fixture,
    home: PathBuf,
    _repo: effigy_test_support::Fixture,
    repo: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = effigy_test_support::Fixture::short_home("lwi-").expect("home");
        let repo =
            effigy_test_support::Fixture::workspace("effigy-lock-wait-inheritance-").expect("repo");
        fs::write(repo.path().join("effigy.toml"), MANIFEST).expect("manifest");
        Self {
            home: home.path().to_path_buf(),
            _home: home,
            repo: repo.path().to_path_buf(),
            _repo: repo,
        }
    }

    fn effigy(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .args(args)
            .current_dir(&self.repo)
            .env("HOME", &self.home)
            .env("EFFIGY_TEST_BIN", env!("CARGO_BIN_EXE_effigy"))
            .env("NO_COLOR", "1")
            .env_remove("EFFIGY_LOCK_WAIT_MS")
            .env_remove("EFFIGY_MANAGED_RUN_TOKEN")
            .env_remove("EFFIGY_CUSTODY_NONCE")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn holder(&self) -> Child {
        let holder = self
            .effigy(&["setup"])
            .spawn_owned()
            .expect("holder spawns");
        wait_until("the holder to take setup", || {
            self.repo.join("ready.marker").exists()
        });
        holder
    }

    fn release(&self, holder: &mut Child) {
        fs::write(self.repo.join("release.marker"), b"go").expect("release the holder");
        let status = wait_exit(holder, Duration::from_secs(60));
        assert!(status.is_some(), "the holder never released setup");
    }
}

fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let started = Instant::now();
    while !ready() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_exit(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return Some(status);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

#[test]
fn flag_reaches_a_nested_selector_that_waits_on_a_held_lock() {
    let fixture = Fixture::new();
    let mut holder = fixture.holder();

    let mut outer = fixture
        .effigy(&["--lock-wait-ms", "600000", "outer"])
        .spawn_owned()
        .expect("outer spawns");
    // Long enough for the nested call to reach the held gate. Without the
    // inherited wait it exits 75 here and `outer` finishes.
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        outer.try_wait().expect("try_wait").is_none(),
        "the nested call did not wait on the held lock"
    );

    fixture.release(&mut holder);
    let status = wait_exit(&mut outer, Duration::from_secs(60));
    if status.is_none() {
        let _ = outer.kill();
    }
    let output = outer.wait_with_output().expect("outer output");
    assert!(
        status.is_some_and(|status| status.success()),
        "{}",
        text(&output)
    );
    assert!(fixture.repo.join("nested.ok").exists(), "{}", text(&output));
}

#[test]
fn nested_selector_fails_fast_without_the_flag() {
    let fixture = Fixture::new();
    let mut holder = fixture.holder();

    let mut outer = fixture
        .effigy(&["outer"])
        .spawn_owned()
        .expect("outer spawns");
    let status = wait_exit(&mut outer, Duration::from_secs(60));
    if status.is_none() {
        let _ = outer.kill();
    }
    let output = outer.wait_with_output().expect("outer output");
    assert!(
        status.is_some_and(|status| !status.success()),
        "the nested call waited without a flag: {}",
        text(&output)
    );
    assert!(
        !fixture.repo.join("nested.ok").exists(),
        "{}",
        text(&output)
    );

    fixture.release(&mut holder);
}

#[test]
fn lock_wait_flag_is_accepted_before_skill_run_and_reaches_its_task() {
    let fixture = Fixture::new();
    let skill = fixture.repo.join("skill");
    fs::create_dir_all(&skill).expect("skill dir");
    fs::write(
        skill.join("effigy.toml"),
        "[catalog]\nalias = \"lockskill\"\n\n[tasks.echo]\nrun = \"printf 'lock=%s\\\\n' \\\"$EFFIGY_LOCK_WAIT_MS\\\"\"\nrun_in = \"host\"\n",
    )
    .expect("skill manifest");

    let output = fixture
        .effigy(&[
            "--lock-wait-ms",
            "4242",
            "skill",
            "run",
            "--path",
            "skill",
            "echo",
        ])
        .output_owned()
        .expect("skill run");
    assert!(output.status.success(), "{}", text(&output));
    assert!(
        text(&output).contains("lock=4242"),
        "the skill task did not see the wait: {}",
        text(&output)
    );
    assert!(
        !text(&output).contains("unknown argument"),
        "{}",
        text(&output)
    );
}

fn text(output: &Output) -> String {
    format!(
        "status: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
