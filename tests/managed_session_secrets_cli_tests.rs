//! One managed session, one vault question: a locked vault prompts once,
//! before any front end renders, and everything the session starts uses that
//! access without prompting (contract 032, Q-007 "Secrets before the TUI";
//! Tom 2026-10-23).
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
/// How long the session gets to reach each milestone.
const WAIT: Duration = Duration::from_secs(180);

/// Every source a caller could set to answer without a terminal. Each child
/// starts without them so the proof is about the session's own resolution.
const PASSHRASE_SOURCES: [&str; 3] = [
    "EFFIGY_SECRET_PASSPHRASE",
    "EFFIGY_INTERNAL_SECRET_PASSPHRASE",
    "EFFIGY_TEST_SECRETS_PASSPHRASE",
];

#[test]
fn a_managed_session_prompts_once_before_it_renders_and_shares_that_access() {
    let fixture = SessionFixture::new("effigy-session-secrets-", "e319h.");
    let mut session = PtySession::spawn(&fixture.root, fixture.home.path());
    session.wait_for(PROMPT);
    // The question arrives before the front end has shown any of the run.
    assert!(
        !session.output().contains("probe saw"),
        "a workload started before the session asked for the vault: {}",
        session.output()
    );
    session.send_line("vault-passphrase");

    // The tab's nested `effigy probe` — a non-`dev` task with
    // `secrets = "required"` — resolves the stored value through the access
    // the session shared, and never asks for the passphrase itself.
    session.wait_for("probe saw");
    let transcript = session.output();
    assert!(
        transcript.contains(SECRET_VALUE),
        "the nested run did not resolve the shared secret: {transcript}"
    );
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
#[test]
fn a_managed_session_uses_a_caller_supplied_passphrase() {
    let fixture = SessionFixture::new("effigy-session-secrets-env-", "e319j.");
    let mut session = PtySession::spawn_with_passphrase_source(
        &fixture.root,
        fixture.home.path(),
        Some("vault-passphrase"),
    );
    session.wait_for("probe saw");
    let transcript = session.output();
    assert!(
        transcript.contains(SECRET_VALUE),
        "the nested run did not resolve the shared secret: {transcript}"
    );
    assert!(
        !transcript.contains(PROMPT),
        "a supplied passphrase must not be asked for again: {transcript}"
    );
    session.stop();
    let status = session.finish();
    assert!(
        status.success(),
        "session exit: {status}: {}",
        session.output()
    );
}

/// A private checkout, private HOME and a locked vault holding one value.
struct SessionFixture {
    _workspace: effigy_test_support::Fixture,
    root: PathBuf,
    home: effigy_test_support::Fixture,
}

impl SessionFixture {
    fn new(prefix: &str, home_prefix: &str) -> Self {
        let workspace =
            effigy_test_support::Fixture::workspace(prefix).expect("private session fixture");
        let root = fs::canonicalize(workspace.path()).expect("canonical fixture root");
        let home =
            effigy_test_support::Fixture::short_home(home_prefix).expect("short private home");
        write_locked_fixture(&root);
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

fn write_locked_fixture(root: &Path) {
    let effigy = env!("CARGO_BIN_EXE_effigy");
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"
[catalog]
alias = "root"

[tasks.dev]
mode = "tui"
concurrent = [
  {{ name = "probe", run = "'{effigy}' probe", start = 1, tab = 1 }},
]

[tasks.probe]
secrets = "required"
run = "printf 'probe saw %s\n' \"$DATABASE_URL\""

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["tasks"]
"#
        ),
    )
    .expect("write session manifest");
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
        let child = command.spawn().expect("spawn session on a PTY");
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
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {needle:?}: {}",
                self.output()
            );
        }
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
                // SAFETY: only this test's own child is signalled, then reaped.
                unsafe {
                    libc::kill(self.child.id() as i32, libc::SIGKILL);
                }
                let status = self.child.wait().expect("reap session");
                self.drain();
                panic!(
                    "session did not settle after TERM: {} ({status})",
                    self.output()
                );
            }
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
        if self.child.try_wait().ok().flatten().is_none() {
            // SAFETY: only this test's own child, which it started on the PTY.
            unsafe {
                libc::kill(self.child.id() as i32, libc::SIGKILL);
            }
            let _ = self.child.wait();
        }
    }
}
