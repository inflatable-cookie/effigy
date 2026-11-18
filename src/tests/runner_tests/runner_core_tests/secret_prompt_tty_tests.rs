//! What reaches the terminal when a secret prompt asks a question
//! (effigy#1036).
//!
//! A passphrase prompt must be visible while a progress display is live: the
//! step lines are cleared, the answer is read on a quiet terminal, and the
//! steps come back afterwards. And a resolution whose declared keys are all
//! optional must never wait on a prompt at all.
//!
//! Both cases are proved on a real pseudo-terminal: the child is this test
//! binary, run again through `--exact` on the pty as its controlling terminal,
//! so `rpassword` asks the question the way it does under `effigy workspace`.

use std::fs;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const MARKER: &str = "EFFIGY_TEST_SECRET_PROMPT_CASE";
const PROMPT_CASE: &str =
    "runner::tests::runner_core_tests::secret_prompt_tty_tests::passphrase_prompt_pty_helper";
const OPTIONAL_CASE: &str =
    "runner::tests::runner_core_tests::secret_prompt_tty_tests::optional_only_resolution_pty_helper";
const FIXTURE_ROOT: &str = "EFFIGY_TEST_SECRET_PROMPT_ROOT";
const PASSPHRASE: &str = "tty-passphrase";
const PROMPT: &str = "Vault passphrase: ";
/// How long the parent gives a child to reach its prompt or its answer.
const BUDGET: Duration = Duration::from_secs(30);

/// A passphrase prompt under a live spinner clears the step lines, waits on a
/// quiet terminal, and the spinner returns (effigy#1036).
#[test]
fn a_passphrase_prompt_is_visible_under_a_live_progress_display() {
    let (mut child, mut master) = spawn_on_a_pty(PROMPT_CASE, None);
    let mut transcript = String::new();

    // The spinner drew frames before the prompt asked its question.
    if !wait_for(&mut master, &mut transcript, |text| {
        text.contains("Updating containers")
    }) || !wait_for(&mut master, &mut transcript, |text| text.contains(PROMPT))
    {
        stop(&mut child);
        panic!("the terminal never reached the prompt: {transcript:?}");
    }

    // Nothing is drawn under the prompt: a tick that kept redrawing would
    // arrive here as bytes while the read waits.
    let while_waiting = drain(&mut master, 600);
    transcript.push_str(&String::from_utf8_lossy(&while_waiting));
    if !while_waiting.is_empty() {
        stop(&mut child);
        panic!(
            "the progress display kept drawing under the prompt: {:?}",
            String::from_utf8_lossy(&while_waiting)
        );
    }

    master
        .write_all(format!("{PASSPHRASE}\n").as_bytes())
        .expect("answer the prompt");
    master.flush().expect("flush the answer");

    let status = wait_with_transcript(&mut child, &mut master, &mut transcript);
    assert!(
        status.success(),
        "the helper did not read the passphrase it was given: {transcript}"
    );
    // The step came back after the prompt: its ending line reaches the
    // terminal in spinner mode.
    assert!(
        transcript.contains("answered the vault prompt"),
        "the step after the prompt never drew: {transcript}"
    );
}

/// A container resolution whose declared keys are all optional never waits on
/// a prompt, and says why the local-dev key did not unlock this run
/// (effigy#1036).
#[test]
fn an_optional_only_resolution_never_prompts_on_a_terminal() {
    let fixture = effigy_test_support::Fixture::workspace("secret-prompt-optional-")
        .expect("fixture workspace");
    let root = fixture.path().to_path_buf();
    write_optional_only_fixture(&root);

    let (mut child, mut master) = spawn_on_a_pty(OPTIONAL_CASE, Some(&root));
    let transcript = read_to_exit(&mut child, &mut master);
    assert!(
        !transcript.contains(PROMPT),
        "an optional-only resolution must not ask for a passphrase: {transcript}"
    );
    assert!(
        child
            .try_wait()
            .expect("bounded helper exit")
            .is_some_and(|status| status.success()),
        "the helper refused the resolution: {transcript}"
    );
}

// ---- The two helper roles ---------------------------------------------------

/// The `PROMPT_CASE` child: a live spinner, then one passphrase read.
#[test]
fn passphrase_prompt_pty_helper() {
    if std::env::var(MARKER).ok().as_deref() != Some(PROMPT_CASE) {
        return;
    }
    effigy_ui::steps::configure(effigy_ui::steps::StepMode::Spinner);
    let source = effigy_ui::steps::ProgressSource::new(
        effigy_ui::steps::ProgressFrame::default()
            .label("Updating containers")
            .waiting_for("docker compose up to return"),
    );
    let step = effigy_ui::steps::live_step(source);
    // Let the steady tick draw a frame or two before the prompt asks anything.
    std::thread::sleep(Duration::from_millis(300));

    let read = crate::runner::secret_session::read_secret_passphrase(
        false,
        PROMPT,
        "the prompt requires an interactive TTY",
    );
    match read {
        Ok(Some(value)) if value.expose() == PASSPHRASE => {}
        Ok(Some(_)) => {
            eprintln!("the prompt read a different value");
            finish(1);
        }
        Ok(None) => {
            eprintln!("the prompt read nothing");
            finish(1);
        }
        Err(error) => {
            eprintln!("the prompt failed: {error}");
            finish(1);
        }
    }
    step.succeed("answered the vault prompt");
    finish(0);
}

/// The `OPTIONAL_CASE` child: one optional-only container resolution.
#[test]
fn optional_only_resolution_pty_helper() {
    if std::env::var(MARKER).ok().as_deref() != Some(OPTIONAL_CASE) {
        return;
    }
    let root = PathBuf::from(std::env::var(FIXTURE_ROOT).expect("the fixture root"));
    crate::runner::secret_session::forget_session_passphrase_for_tests();
    let policy = match effigy_containers::load_container_policy(&root, Some("web")) {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("cannot load the fixture container policy: {error}");
            finish(1);
        }
    };
    let runtime = crate::runner::container_command::secret_env::resolve_container_secret_runtime(
        &root, &policy, false,
    );
    match runtime {
        Ok(runtime) => {
            if !runtime.env.is_empty() {
                eprintln!("a direct command delivered {:?}", runtime.env.len());
                finish(1);
            }
            let named = runtime
                .warnings
                .iter()
                .all(|warning| warning.contains("the local-dev key only unlocks `effigy dev`"));
            if runtime.warnings.is_empty() || !named {
                eprintln!(
                    "the warnings do not name the local-dev scope: {:?}",
                    runtime.warnings
                );
                finish(1);
            }
        }
        Err(error) => {
            eprintln!("the resolution failed: {error}");
            finish(1);
        }
    }
    finish(0);
}

/// End a helper role without returning into the harness: the case already
/// decided its own outcome.
fn finish(code: i32) -> ! {
    std::process::exit(code)
}

// ---- The pty harness --------------------------------------------------------

/// Run `case` on a fresh pseudo-terminal as its controlling terminal. The
/// parent keeps the master, so the test reads exactly what the terminal
/// received.
fn spawn_on_a_pty(case: &str, fixture_root: Option<&Path>) -> (Child, fs::File) {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    // SAFETY: `openpty` fills two fresh descriptors; the optional terminal and
    // window arguments request the defaults.
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(opened, 0, "open a pseudo-terminal");
    // SAFETY: a successful `openpty` returned two owned descriptors.
    let master = unsafe { fs::File::from_raw_fd(master) };
    // A window of no columns is a terminal no progress display can draw into,
    // and `openpty`'s default is empty: give the pty the size a real one has.
    let window = libc::winsize {
        ws_row: 40,
        ws_col: 120,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: the master is live and `window` outlives the call.
    assert_eq!(
        unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &window) },
        0,
        "size the pseudo-terminal"
    );
    // SAFETY: as above; the child receives this end as its three streams.
    let slave = unsafe { fs::File::from_raw_fd(slave) };

    let mut command = Command::new(std::env::current_exe().expect("test image"));
    command
        .args(["--exact", case, "--nocapture"])
        .env(MARKER, case)
        .env(
            "PATH",
            std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
        )
        // A progress display draws only into a terminal it recognises: an
        // unset or `dumb` TERM is a terminal indicatif hides every bar in.
        .env("TERM", "xterm-256color")
        // Session access never answers a prompt here: the case is what the
        // terminal sees.
        .env_remove("EFFIGY_TEST_SECRETS_PASSPHRASE")
        .env_remove("EFFIGY_SECRET_PASSPHRASE")
        .env_remove("EFFIGY_INTERNAL_SECRET_GRANT")
        .env_remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE")
        .env_remove("EFFIGY_MANAGED_PANE")
        .env_remove("EFFIGY_INTERNAL_MANAGED_PANE")
        .stdin(Stdio::from(
            slave.try_clone().expect("clone the pty slave for stdin"),
        ))
        .stdout(Stdio::from(
            slave.try_clone().expect("clone the pty slave for stdout"),
        ))
        .stderr(Stdio::from(slave));
    if let Some(root) = fixture_root {
        command.env(FIXTURE_ROOT, root);
    }
    // A session leader with the pty as its controlling terminal is what makes
    // `/dev/tty` (where a passphrase prompt writes and reads) the pty itself.
    // SAFETY: the closure runs in the forked child before `exec`, where only
    // async-signal-safe syscalls are used.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY as libc::c_ulong, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().expect("spawn the helper on the pty");
    (child, master)
}

/// Whether the terminal showed `needle` before the budget expired.
fn wait_for(master: &mut fs::File, transcript: &mut String, needle: impl Fn(&str) -> bool) -> bool {
    let deadline = Instant::now() + BUDGET;
    while Instant::now() < deadline {
        if needle(transcript) {
            return true;
        }
        transcript.push_str(&String::from_utf8_lossy(&drain(master, 100)));
    }
    needle(transcript)
}

/// Whatever the terminal received within `timeout_ms` of wall clock, however
/// busy it is: a display that keeps drawing must not keep this read alive.
fn drain(master: &mut fs::File, timeout_ms: i32) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(0) as u64);
    let mut collected = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let wait = i32::try_from(remaining.as_millis()).unwrap_or(i32::MAX);
        let mut poll = libc::pollfd {
            fd: master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one `pollfd` describing this live descriptor.
        let ready = unsafe { libc::poll(&mut poll, 1, wait) };
        if ready <= 0 || Instant::now() >= deadline {
            return collected;
        }
        match master.read(&mut buffer) {
            Ok(0) => return collected,
            Ok(read) => collected.extend_from_slice(&buffer[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return collected,
        }
        if collected.len() > 1 << 20 {
            return collected;
        }
    }
}

/// Stop a helper this test started, so a failing proof leaves no process
/// behind for the suite guard to find.
fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn wait_with_transcript(
    child: &mut Child,
    master: &mut fs::File,
    transcript: &mut String,
) -> std::process::ExitStatus {
    let deadline = Instant::now() + BUDGET;
    loop {
        transcript.push_str(&String::from_utf8_lossy(&drain(master, 100)));
        if let Some(status) = child.try_wait().expect("bounded helper exit") {
            transcript.push_str(&String::from_utf8_lossy(&drain(master, 100)));
            return status;
        }
        if Instant::now() >= deadline {
            stop(child);
            panic!("the helper never finished: {transcript:?}");
        }
    }
}

fn read_to_exit(child: &mut Child, master: &mut fs::File) -> String {
    let mut transcript = String::new();
    wait_with_transcript(child, master, &mut transcript);
    transcript
}

// ---- Fixtures ---------------------------------------------------------------

/// A repository whose one container key is optional, with a vault this machine
/// holds a local-dev key for - and a run that is not a `dev` session.
fn write_optional_only_fixture(root: &Path) {
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.openai_api_key]
required = false
targets = ["containers"]

[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.secrets]
delivery = "runtime-files"
runtime_dir = "/run/effigy/secrets"

[containers.web.services.app]
catalog = "php-fpm"
"#,
    )
    .expect("write the fixture manifest");

    let mut payload = effigy_secrets::VaultPlaintextPayload::empty();
    payload.records.insert(
        "openai_api_key".to_owned(),
        effigy_secrets::VaultSecretRecord::new(effigy_secrets::SecretValue::new("sk-dev-value")),
    );
    let key = effigy_secrets::LocalDevUnlockKey::generate().expect("generate a local-dev key");
    let envelope = payload
        .encrypt_with_passphrase_and_local_dev_key(PASSPHRASE, &key)
        .expect("encrypt the fixture vault");
    let vault_path = root.join(".effigy/secrets/local.vault");
    effigy_test_support::write_private_fixture(
        &vault_path,
        envelope
            .to_json_pretty()
            .expect("serialize the fixture vault")
            .as_bytes(),
    )
    .expect("write the fixture vault");
    let key_path = effigy_secrets::local_dev_unlock_key_path(&vault_path);
    fs::write(&key_path, key.expose()).expect("write the local-dev key");
    fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
        .expect("restrict the local-dev key");
}
