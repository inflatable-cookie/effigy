//! One managed session, one vault question: a locked vault prompts once,
//! before any front end renders, and Effigy's own children use that access
//! without prompting while a user workload never receives it (contract 032,
//! Q-007 "Secrets before the TUI"; Tom 2026-10-23; effigy#338).
//!
//! These proofs drive the real binary on a private pseudo terminal, because
//! the prompt is the thing under test: the session must write it on the
//! terminal it owns, once, before a tab exists.

use std::fs;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use effigy_secrets::{
    local_unlock_credential_path, SecretValue, VaultPlaintextPayload, VaultSecretRecord,
};
use nix::pty::{openpty, Winsize};

const PROMPT: &str = "Vault passphrase";
const SECRET_VALUE: &str = "postgres://session-shared-value";
/// How long the session gets to reach each milestone. Kept well inside the
/// test runner's termination budget (180s for this selector): the fixture must
/// reach its own cleanup and stop its process group before the runner kills
/// the test process, or a hung session would outlive the test (effigy#338).
const WAIT: Duration = Duration::from_secs(60);

/// Every source a caller could set to answer without a terminal. Each child
/// starts without them so the proof is about the session's own resolution.
const PASSHRASE_SOURCES: [&str; 5] = [
    "EFFIGY_SECRET_PASSPHRASE",
    "EFFIGY_INTERNAL_SECRET_PASSPHRASE",
    "EFFIGY_TEST_SECRETS_PASSPHRASE",
    "EFFIGY_INTERNAL_LOCAL_DEV_SECRET_ACCESS",
    "EFFIGY_INTERNAL_SECRET_GRANT",
];

/// The session asks once, before it renders, and that access never reaches a
/// workload: the workload's own environment and its `ps -ww` argv view hold
/// neither the passphrase nor a session-access key, while the required secret
/// the session resolved for it does arrive (effigy#338).
#[test]
fn a_managed_session_prompts_once_before_it_renders_and_keeps_the_access_off_the_workload() {
    let fixture = SessionFixture::new("effigy-session-secrets-", "e319h.");
    let mut session = PtySession::spawn(&fixture.root, fixture.home.path());
    session.wait_for(PROMPT);
    // The question arrives before the front end has shown any of the run.
    assert!(
        !session.output().contains("workload saw"),
        "a workload started before the session asked for the vault: {}",
        session.output()
    );
    session.send_line("vault-passphrase");

    // The supervisor is Effigy's own child. It resolved the task's required
    // secret through the session's access, so the workload received the value
    // without the session asking a second time.
    session.wait_for("workload saw");
    let argv =
        fs::read_to_string(fixture.root.join("workload-argv")).expect("workload argv receipt");
    assert!(
        !argv.contains(SECRET_VALUE),
        "the task secret reached the host workload's argv"
    );
    let transcript = session.output();
    let dump: Vec<&str> = transcript
        .lines()
        .filter(|line| line.contains("[dump]"))
        .collect();
    assert!(
        dump.iter().any(|line| line.contains("workload saw")),
        "the workload's own output did not reach the session: {transcript}"
    );
    // The required secret was delivered (its value is redacted in the log),
    // and the passphrase is nowhere in the workload's environment or argv.
    assert!(
        dump.iter()
            .any(|line| line.contains("DATABASE_URL=[REDACTED]")),
        "the session's resolved secret did not reach the workload: {transcript}"
    );
    for forbidden in [
        "vault-passphrase",
        "EFFIGY_INTERNAL_SECRET_PASSPHRASE",
        "EFFIGY_INTERNAL_LOCAL_DEV_SECRET_ACCESS",
        "EFFIGY_INTERNAL_SECRET_GRANT",
        "EFFIGY_SECRET_PASSPHRASE",
    ] {
        assert!(
            dump.iter().all(|line| !line.contains(forbidden)),
            "`{forbidden}` reached the workload's environment or argv: {transcript}"
        );
    }
    assert_eq!(
        transcript.matches(PROMPT).count(),
        1,
        "the session must prompt exactly once: {transcript}"
    );

    // Nothing was persisted: sharing is not an implicit `secrets unlock`.
    let credential = fixture.credential_path();
    assert!(
        !credential.exists(),
        "the session must not seal a command credential"
    );
    session.stop();
    let status = session.finish();
    assert!(
        status.success(),
        "session exit: {status}: {}",
        session.output()
    );
    assert_session_stopped(&mut session);
    assert!(
        !credential.exists(),
        "the session wrote a sealed credential"
    );
}

/// Without a terminal and without a source the session refuses before it
/// starts anything, and the refusal names the fix.
#[test]
fn a_managed_session_without_a_terminal_or_source_refuses_before_starting() {
    let fixture = SessionFixture::new("effigy-session-secrets-headless-", "e319i.");
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command
        .arg("--repo")
        .arg(&fixture.root)
        .args(["dev", "--headless"])
        .env("HOME", fixture.home.path())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in PASSHRASE_SOURCES {
        command.env_remove(key);
    }
    let output = command.output().expect("run headless session");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "the session should refuse: stdout={} stderr={stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        stderr.contains("managed task `dev`"),
        "the refusal names the session: {stderr}"
    );
    assert!(
        stderr.contains("is locked"),
        "the refusal names what is locked: {stderr}"
    );
    assert!(
        stderr.contains("effigy secrets unlock") && stderr.contains("EFFIGY_SECRET_PASSPHRASE"),
        "the refusal names what to do next: {stderr}"
    );
    assert!(
        !stderr.contains(SECRET_VALUE),
        "the refusal must not print a value: {stderr}"
    );
    assert!(
        !fixture.credential_path().exists(),
        "a refusal must not seal a command credential"
    );
}

/// A caller-supplied passphrase answers the session without a terminal, and
/// the nested run in its tab uses it too.
/// A caller-supplied passphrase answers the session without a terminal, and
/// the run's Effigy children resolve through it without asking.
#[test]
fn a_managed_session_uses_a_caller_supplied_passphrase() {
    let fixture = SessionFixture::new("effigy-session-secrets-env-", "e319j.");
    let mut session = PtySession::spawn_with_passphrase_source(
        &fixture.root,
        fixture.home.path(),
        Some("vault-passphrase"),
    );
    session.wait_for("workload saw");
    let transcript = session.output();
    let dump: Vec<&str> = transcript
        .lines()
        .filter(|line| line.contains("[dump]"))
        .collect();
    assert!(
        dump.iter()
            .any(|line| line.contains("DATABASE_URL=[REDACTED]")),
        "the run did not resolve the session's secret: {transcript}"
    );
    assert!(
        !transcript.contains(PROMPT),
        "a supplied passphrase must not be asked for again: {transcript}"
    );
    for forbidden in ["vault-passphrase", "EFFIGY_SECRET_PASSPHRASE"] {
        assert!(
            dump.iter().all(|line| !line.contains(forbidden)),
            "`{forbidden}` reached the workload: {transcript}"
        );
    }
    session.stop();
    let status = session.finish();
    assert!(
        status.success(),
        "session exit: {status}: {}",
        session.output()
    );
    assert_session_stopped(&mut session);
}

/// A nested `effigy` started from user code is a top-level invocation: it does
/// not inherit the session's access, so it asks for the passphrase itself
/// (effigy#338). The session's own prompt is the only one Effigy answers.
#[test]
fn a_nested_effigy_run_started_by_user_code_never_inherits_the_session_access() {
    let fixture = SessionFixture::new_user_code_nested_run();
    let mut session = PtySession::spawn_with_passphrase_source(
        &fixture.root,
        fixture.home.path(),
        Some("vault-passphrase"),
    );
    // The session itself resolved from the caller's source. Its tab's nested
    // `effigy probe` is user code: it inherits no access. It either reports a
    // refusal or waits on its own terminal for a passphrase nobody types, so
    // this observes a bounded window instead of waiting for its exit.
    session.settle_for(Duration::from_secs(20));
    let transcript = session.output();
    assert!(
        !transcript.contains("probe saw"),
        "a user-code nested run received the session's access: {transcript}"
    );
    assert!(
        !transcript.contains(SECRET_VALUE),
        "a user-code nested run received the session's secret: {transcript}"
    );
    session.stop();
    let status = session.finish();
    assert!(
        status.success(),
        "session exit: {status}: {}",
        session.output()
    );
    assert_session_stopped(&mut session);
}

/// A private checkout, private HOME and a locked vault holding one value.
struct SessionFixture {
    _workspace: effigy_test_support::Fixture,
    root: PathBuf,
    home: effigy_test_support::Fixture,
}

impl SessionFixture {
    fn new(prefix: &str, home_prefix: &str) -> Self {
        Self::from_manifest(prefix, home_prefix, probe_manifest())
    }

    /// A session whose tab runs a nested `effigy probe` as user code, to prove
    /// that route is a top-level invocation rather than a shared session.
    fn new_user_code_nested_run() -> Self {
        Self::from_manifest(
            "effigy-session-secrets-user-code-",
            "e319k.",
            user_code_manifest(),
        )
    }

    fn from_manifest(prefix: &str, home_prefix: &str, manifest: String) -> Self {
        let workspace =
            effigy_test_support::Fixture::workspace(prefix).expect("private session fixture");
        let root = fs::canonicalize(workspace.path()).expect("canonical fixture root");
        let home =
            effigy_test_support::Fixture::short_home(home_prefix).expect("short private home");
        write_locked_fixture(&root, &manifest);
        Self {
            _workspace: workspace,
            root,
            home,
        }
    }

    fn credential_path(&self) -> PathBuf {
        local_unlock_credential_path(&self.root.join(".effigy/secrets/local.vault"))
    }
}

/// The session's workload dumps everything a `ps` listing and an environment
/// dump would show, then prints the required secret the session resolved.
fn probe_manifest() -> String {
    r#"
[catalog]
alias = "root"

[tasks.dev]
mode = "tui"
secrets = "required"
concurrent = [
  { name = "dump", run = "/usr/bin/env; /bin/ps -ww -o args= -p $$ > workload-argv; printf 'workload saw %s\\n' \"$DATABASE_URL\"", start = 1, tab = 1 },
]

[tasks.probe]
secrets = "required"
run = "printf 'probe saw %s\\n' \"$DATABASE_URL\""

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["tasks"]
"#
    .to_owned()
}

/// A session whose tab runs a nested `effigy probe` as user code.
fn user_code_manifest() -> String {
    format!(
        r#"
[catalog]
alias = "root"

[tasks.dev]
mode = "tui"
concurrent = [
  {{ name = "probe", run = "'{effigy}' probe; printf 'nested exit %s\\n' \"$?\"", start = 1, tab = 1 }},
]

[tasks.probe]
secrets = "required"
run = "printf 'probe saw %s\\n' \"$DATABASE_URL\""

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["tasks"]
"#,
        effigy = env!("CARGO_BIN_EXE_effigy")
    )
}

fn write_locked_fixture(root: &Path, manifest: &str) {
    fs::write(root.join("effigy.toml"), manifest).expect("write session manifest");
    write_locked_vault(root, "vault-passphrase", SECRET_VALUE);
}

fn write_locked_vault(root: &Path, passphrase: &str, value: &str) {
    let mut payload = VaultPlaintextPayload::empty();
    payload.records.insert(
        "database_url".to_owned(),
        VaultSecretRecord::new(SecretValue::new(value)),
    );
    let envelope = payload
        .encrypt_with_passphrase(passphrase)
        .expect("encrypt locked vault");
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
    fs::write(
        &vault_path,
        envelope.to_json_pretty().expect("serialize vault"),
    )
    .expect("write locked vault");
}

/// Opens descriptors until this process holds every slot below the kernel's
/// handoff descriptor, and leaves them open (not CLOEXEC) until the caller has
/// spawned the child that should inherit them.
fn occupy_below_handoff() -> Vec<std::os::fd::OwnedFd> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let slot = effigy_kernel::custody::HANDOFF_FD;
    let mut held = Vec::new();
    loop {
        // SAFETY: a plain open of a static path; ownership transfers.
        let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY) };
        assert!(fd >= 0, "open /dev/null");
        // SAFETY: the successful open returned a fresh owned descriptor.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let raw = fd.as_raw_fd();
        held.push(fd);
        if raw >= slot - 1 {
            return held;
        }
    }
}

/// A private pseudo terminal: the child gets the slave as its stdin, stdout
/// and stderr *and* adopts it as its controlling terminal, which is what
/// makes the passphrase prompt (an `/dev/tty` read) ask here rather than
/// fail.
struct PtySession {
    child: Child,
    master: fs::File,
    output: Vec<u8>,
}

impl PtySession {
    fn spawn(root: &Path, home: &Path) -> Self {
        Self::spawn_with_passphrase_source(root, home, None)
    }

    fn spawn_with_passphrase_source(root: &Path, home: &Path, source: Option<&str>) -> Self {
        let window = Winsize {
            ws_row: 40,
            ws_col: 120,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = openpty(Some(&window), None).expect("open private PTY");
        let master = fs::File::from(pty.master);
        let slave = fs::File::from(pty.slave);
        let child_stdin = slave.try_clone().expect("clone PTY stdin");
        let child_stdout = slave.try_clone().expect("clone PTY stdout");
        let child_stderr = slave.try_clone().expect("clone PTY stderr");
        let slave_fd = slave.as_raw_fd();
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .arg("--repo")
            .arg(root)
            .args(["dev", "--headless"])
            .env("HOME", home)
            .env("NO_COLOR", "1");
        for key in PASSHRASE_SOURCES {
            command.env_remove(key);
        }
        if let Some(source) = source {
            command.env("EFFIGY_SECRET_PASSPHRASE", source);
        }
        command
            .stdin(Stdio::from(child_stdin))
            .stdout(Stdio::from(child_stdout))
            .stderr(Stdio::from(child_stderr));
        // SAFETY: `pre_exec` runs in the forked child before `exec`, where
        // `setsid` and `ioctl` are async-signal-safe. The child becomes a
        // session leader and adopts the slave as its controlling terminal, so
        // `/dev/tty` inside the session is this PTY.
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        // Hand the session descriptors 3..=10, so its own one-shot grant pipe
        // lands on the kernel's handoff slot itself (11). That is the shape
        // where `dup2` onto the same number must still clear CLOEXEC for the
        // supervisor to find its grant after exec (effigy#338).
        let inherited = occupy_below_handoff();
        let child = command.spawn().expect("spawn session on a PTY");
        drop(inherited);
        Self {
            child,
            master,
            output: Vec::new(),
        }
    }

    fn output(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }

    fn wait_for(&mut self, needle: &str) {
        let deadline = Instant::now() + WAIT;
        loop {
            self.read_available(Duration::from_millis(50));
            if self.output().contains(needle) {
                return;
            }
            if self.child.try_wait().expect("poll session").is_some() {
                self.drain();
                assert!(
                    self.output().contains(needle),
                    "session exited before {needle:?}: {}",
                    self.output()
                );
                return;
            }
            if Instant::now() >= deadline {
                // Never panic while the session is still running: this test
                // process's death must not leave a `dev --headless` behind.
                self.kill_group();
                self.drain();
                panic!("timed out waiting for {needle:?}: {}", self.output());
            }
        }
    }

    /// Stops the fixture and everything it started in this session. The child
    /// is its own session leader (`setsid` in `pre_exec`), so its group is the
    /// pid it recorded at spawn.
    fn kill_group(&mut self) {
        if self.child.try_wait().ok().flatten().is_some() {
            return;
        }
        let pid = self.child.id() as i32;
        // SAFETY: only this test's own child, its own process group, recorded
        // at spawn. Both signals are idempotent here.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
            libc::kill(pid, libc::SIGKILL);
        }
        let _ = self.child.wait();
    }

    fn send_line(&mut self, line: &str) {
        self.master
            .write_all(format!("{line}\n").as_bytes())
            .expect("write to PTY");
        self.master.flush().expect("flush PTY");
    }

    /// The run's client stops and settles the run on TERM, which is also what
    /// stops every tab this session started.
    fn stop(&mut self) {
        // SAFETY: the pid is this test's own child, recorded at spawn.
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
    }

    fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + WAIT;
        loop {
            self.read_available(Duration::from_millis(50));
            if let Some(status) = self.child.try_wait().expect("poll session") {
                self.drain();
                return status;
            }
            if Instant::now() >= deadline {
                self.kill_group();
                self.drain();
                panic!("session did not settle after TERM: {}", self.output());
            }
        }
    }

    /// Reads output for a fixed window without waiting for any milestone.
    fn settle_for(&mut self, window: Duration) {
        let deadline = Instant::now() + window;
        while Instant::now() < deadline {
            self.read_available(Duration::from_millis(100));
        }
    }

    /// Reads whatever the terminal has now, waiting at most `timeout` for the
    /// first byte.
    fn read_available(&mut self, timeout: Duration) {
        let mut poll_fd = libc::pollfd {
            fd: self.master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let wait_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        // SAFETY: one initialized descriptor record and its valid count.
        if unsafe { libc::poll(&mut poll_fd, 1, wait_ms) } <= 0
            || poll_fd.revents & libc::POLLIN == 0
        {
            return;
        }
        let mut buffer = [0_u8; 4096];
        loop {
            match self.master.read(&mut buffer) {
                Ok(0) => return,
                Ok(read) => self.output.extend_from_slice(&buffer[..read]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return,
            }
            let mut poll_fd = libc::pollfd {
                fd: self.master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: as above; a zero timeout reads what is already buffered.
            if unsafe { libc::poll(&mut poll_fd, 1, 0) } <= 0 {
                return;
            }
        }
    }

    fn drain(&mut self) {
        for _ in 0..20 {
            self.read_available(Duration::from_millis(20));
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        // Every exit path, panics included, stops the fixture's own session
        // before this test process can die and orphan it.
        self.kill_group();
    }
}

/// The run's own settle stops everything; this only proves the fixture process
/// itself is gone, so a leaked session fails the test that made it.
fn assert_session_stopped(session: &mut PtySession) {
    assert!(
        session.child.try_wait().expect("poll session").is_some(),
        "the session fixture is still running after the test stopped it"
    );
}

#[test]
fn invocation_and_doctor_sweep_only_non_live_managed_secret_files() {
    use effigy_kernel::ident::{process_identity, ProcessStartIdentity};
    use effigy_test_support::process::FixtureCommandExt;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};

    for args in [vec!["--version"], vec!["doctor", "--json"]] {
        let workspace =
            effigy_test_support::Fixture::workspace("managed-secret-sweep-cli-").unwrap();
        let root = workspace.path().canonicalize().unwrap();
        let home = effigy_test_support::Fixture::short_home("esw-").unwrap();
        fs::write(root.join("effigy.toml"), "[tasks.ok]\nrun = \"true\"\n").unwrap();
        let namespace = home.path().join(".effigy/managed-secrets");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&namespace)
            .unwrap();
        let identity = process_identity(std::process::id())
            .verified()
            .unwrap()
            .clone();
        for live in [true, false] {
            let mut owner = identity.clone();
            if !live {
                owner.start = ProcessStartIdentity::LinuxTicks(0);
            }
            let directory = namespace.join(if live { "live" } else { "stale" });
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&directory)
                .unwrap();
            let meta = fs::metadata(&directory).unwrap();
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(directory.join("owner"))
                .unwrap();
            file.write_all(
                serde_json::json!({"identity": owner, "device": meta.dev(), "inode": meta.ino()})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(directory.join("env.delivery"))
                .unwrap();
            file.write_all(b"CLI-sweep-private-value").unwrap();
        }
        let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .args(&args)
            .current_dir(&root)
            .env("HOME", home.path())
            .output_owned()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!namespace.join("stale").exists());
        assert_eq!(
            fs::read_to_string(namespace.join("live/env.delivery")).unwrap(),
            "CLI-sweep-private-value"
        );
        for bytes in [&output.stdout, &output.stderr] {
            let output = String::from_utf8_lossy(bytes);
            assert!(!output.contains("CLI-sweep-private-value"));
            assert!(!output.contains(&namespace.display().to_string()));
        }
    }
}
