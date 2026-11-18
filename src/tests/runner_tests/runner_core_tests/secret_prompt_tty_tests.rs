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
/// The workspace preparation case: the real apply against a running stack.
const PREPARE_CASE: &str =
    "runner::tests::runner_core_tests::secret_prompt_tty_tests::running_workspace_prepare_pty_helper";
/// The pane case: a managed pane with a real terminal on stdin.
const PANE_CASE: &str =
    "runner::tests::runner_core_tests::secret_prompt_tty_tests::pane_with_terminal_stdin_pty_helper";
/// The generic secret-input case: a value that is not a vault passphrase.
const GENERIC_CASE: &str =
    "runner::tests::runner_core_tests::secret_prompt_tty_tests::generic_secret_input_pty_helper";
const GENERIC_PROMPT: &str = "Secret value: ";
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

/// The reported hang, through the real preparation: a task's apply against an
/// already-running stack needs one required container secret, and the progress
/// display must not hide the question (effigy#1036).
#[test]
fn an_already_running_stack_asks_for_its_required_secret_visibly() {
    let fixture = effigy_test_support::Fixture::workspace("secret-prompt-prepare-")
        .expect("fixture workspace");
    let root = fixture.path().to_path_buf();
    write_running_stack_fixture(&root);

    let (mut child, mut master) =
        spawn_on_a_pty_with_env(PREPARE_CASE, &prepare_environment(&root));
    let mut transcript = String::new();
    if !wait_for(&mut master, &mut transcript, |text| text.contains(PROMPT)) {
        stop(&mut child);
        panic!("the apply never showed its passphrase prompt: {transcript:?}");
    }
    // Nothing is drawn under the prompt, however busy the apply is.
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
        "the running workspace's preparation did not finish after the answer: {transcript}"
    );
    assert!(
        transcript.contains("the running stack delivered its required secret"),
        "the apply delivered the secret it was given: {transcript}"
    );
}

/// A value read for `secrets set` is that command's own input, not this
/// session's vault passphrase: later work must not resolve with it
/// (effigy#1036 review).
#[test]
fn a_generic_secret_value_is_not_cached_as_the_vault_passphrase() {
    let (mut child, mut master) = spawn_on_a_pty(GENERIC_CASE, None);
    let mut transcript = String::new();
    if !wait_for(&mut master, &mut transcript, |text| {
        text.contains(GENERIC_PROMPT)
    }) {
        stop(&mut child);
        panic!("the value prompt never appeared: {transcript:?}");
    }
    master
        .write_all(b"a-stored-value\n")
        .expect("answer the value prompt");
    master.flush().expect("flush the value");
    let status = wait_with_transcript(&mut child, &mut master, &mut transcript);
    assert!(
        status.success(),
        "a generic secret value must not become the session passphrase: {transcript}"
    );
}

/// A managed pane with a real terminal on stdin never prompts: the question is
/// captured, so the run refuses and names the unlock instead (effigy#1036).
#[test]
fn a_managed_pane_with_terminal_stdin_never_prompts() {
    let (mut child, mut master) = spawn_on_a_pty_with_env(
        PANE_CASE,
        &[("EFFIGY_MANAGED_PANE".to_owned(), "1".to_owned())],
    );
    let transcript = read_to_exit(&mut child, &mut master);
    assert!(
        !transcript.contains(PROMPT),
        "a captured pane must not ask for a passphrase: {transcript}"
    );
    assert!(
        child
            .try_wait()
            .expect("bounded helper exit")
            .is_some_and(|status| status.success()),
        "the helper expected the pane refusal: {transcript}"
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

/// The `PREPARE_CASE` child: the one apply of a task's stack preparation,
/// against a stack that is already running.
#[test]
fn running_workspace_prepare_pty_helper() {
    if std::env::var(MARKER).ok().as_deref() != Some(PREPARE_CASE) {
        return;
    }
    let root = PathBuf::from(std::env::var(FIXTURE_ROOT).expect("the fixture root"));
    effigy_ui::steps::configure(effigy_ui::steps::StepMode::Spinner);
    let prepared = effigy_containers::load_container_policy(&root, Some("web"));
    let policy = match prepared {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("cannot load the fixture container policy: {error}");
            finish(1);
        }
    };
    let start = crate::runner::container_runtime_prep::StartPolicy {
        retention: effigy_containers::environment::Retention::Persistent,
        holders: &[],
        reconcile_shared_services: true,
    };
    match crate::runner::container_runtime_prep::prepare_container_exec_runtime(
        &root,
        &policy,
        Some("web"),
        start,
    ) {
        Ok(()) => println!("the running stack delivered its required secret"),
        Err(error) => {
            eprintln!("the running workspace's preparation failed: {error}");
            finish(1);
        }
    }
    finish(0);
}

/// The `GENERIC_CASE` child: one value prompt that is not a passphrase.
#[test]
fn generic_secret_input_pty_helper() {
    if std::env::var(MARKER).ok().as_deref() != Some(GENERIC_CASE) {
        return;
    }
    match crate::runner::secret_session::prompt_secret_on_terminal(GENERIC_PROMPT) {
        Ok(value) if value.expose() == "a-stored-value" => {}
        Ok(_) => {
            eprintln!("the value prompt read a different value");
            finish(1);
        }
        Err(error) => {
            eprintln!("the value prompt failed: {error}");
            finish(1);
        }
    }
    if crate::runner::secret_session::session_passphrase().is_some() {
        eprintln!("a stored value became this session's vault passphrase");
        finish(1);
    }
    finish(0);
}

/// The `PANE_CASE` child: a managed pane whose stdin is a real terminal.
#[test]
fn pane_with_terminal_stdin_pty_helper() {
    if std::env::var(MARKER).ok().as_deref() != Some(PANE_CASE) {
        return;
    }
    assert!(
        std::io::IsTerminal::is_terminal(&std::io::stdin()),
        "this proof needs a real terminal on stdin"
    );
    match crate::runner::secret_session::read_secret_passphrase(
        false,
        PROMPT,
        "the prompt requires an interactive TTY",
    ) {
        Err(error) if error.to_string().contains("managed pane") => finish(0),
        other => {
            eprintln!("a captured pane must refuse the prompt, got {other:?}");
            finish(1);
        }
    }
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
    let mut environment = Vec::new();
    if let Some(root) = fixture_root {
        environment.push((FIXTURE_ROOT.to_owned(), root.display().to_string()));
    }
    spawn_on_a_pty_with_env(case, &environment)
}

/// [`spawn_on_a_pty`] with extra child environment on top of the pty setup.
fn spawn_on_a_pty_with_env(case: &str, environment: &[(String, String)]) -> (Child, fs::File) {
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
    for (key, value) in environment {
        command.env(key, value);
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

/// A running stack this checkout owns: a container with one required
/// container secret, a vault only a passphrase opens, and a fake runtime whose
/// `exec` lands on this host.
fn write_running_stack_fixture(root: &Path) {
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["containers"]

[containers]
default = "web"

[containers.web]
compose_file = "docker-compose.yml"
primary_service = "app"
project_name = "pty-prep"
working_dir = "/workspace"

[containers.web.secrets]
delivery = "compose-env"
"#,
    )
    .expect("write the fixture manifest");
    fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write the fixture compose file");

    // A vault with no local-dev key: this run is not a `dev` session, and its
    // required key has to be asked for.
    let mut payload = effigy_secrets::VaultPlaintextPayload::empty();
    payload.records.insert(
        "database_url".to_owned(),
        effigy_secrets::VaultSecretRecord::new(effigy_secrets::SecretValue::new(
            "postgres://fixture",
        )),
    );
    let envelope = payload
        .encrypt_with_passphrase(PASSPHRASE)
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

    write_fake_container_runtime(root);
}

/// The fake runtime a preparation can run against: a running container in the
/// inventory, a permissive compose, and an exec that runs here.
fn write_fake_container_runtime(root: &Path) {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("mkdir fake runtime bin");
    let docker = bin.join("docker");
    fs::write(
        &docker,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$EFFIGY_TEST_DOCKER_LOG"
for arg in "$@"; do
  if [ "$arg" = "--format" ]; then
    printf '%s\tUp 1 minute\t\t%s\t%s\tapp\tFalse\n' "$EFFIGY_TEST_CONTAINER" "$EFFIGY_TEST_PROJECT" "$EFFIGY_TEST_WORKDIR"
    exit 0
  fi
done
case "$1" in
  inspect)
    printf '[{"Id":"%s","Name":"/%s","State":{"Status":"running","Running":true,"ExitCode":0},"Config":{"Labels":{"com.docker.compose.project":"%s","com.docker.compose.service":"app","com.docker.compose.oneoff":"False","com.docker.compose.project.working_dir":"%s"}},"Mounts":[],"NetworkSettings":{"Ports":{}}}]\n' "$EFFIGY_TEST_ID" "$EFFIGY_TEST_CONTAINER" "$EFFIGY_TEST_PROJECT" "$EFFIGY_TEST_WORKDIR"
    exit 0
    ;;
esac
while [ $# -gt 0 ]; do
  case "$1" in
    -f|-p|-e|--env) shift 2 ;;
    exec) shift; break ;;
    *) shift ;;
  esac
done
while [ $# -gt 0 ]; do
  case "$1" in
    -T|-i|-t|--) shift ;;
    -w|--workdir|-u|--user) shift 2 ;;
    *) break ;;
  esac
done
shift || exit 0
if [ "$1" = sh ] && [ "$2" = -lc ]; then
  sh -lc "$3"
  exit $?
fi
exit 0
"#,
    )
    .expect("write fake docker");
    let colima = bin.join("colima");
    fs::write(
        &colima,
        "#!/bin/sh\ncase \"$1\" in status) echo 'status: Running';; esac\nexit 0\n",
    )
    .expect("write fake colima");
    for program in [&docker, &colima] {
        fs::set_permissions(program, fs::Permissions::from_mode(0o755))
            .expect("chmod fake runtime");
    }
}

/// What the preparation case runs with: the fake runtime, a private home, and
/// no session access, so its required key must be asked for.
fn prepare_environment(root: &Path) -> Vec<(String, String)> {
    let bin = root.join("bin");
    let home = root.join("home");
    fs::create_dir_all(&home).expect("mkdir fixture home");
    vec![
        (FIXTURE_ROOT.to_owned(), root.display().to_string()),
        (
            "PATH".to_owned(),
            format!("{}:/bin:/usr/bin", bin.display()),
        ),
        ("HOME".to_owned(), home.display().to_string()),
        ("TERM".to_owned(), "xterm-256color".to_owned()),
        ("DOCKER_HOST".to_owned(), "tcp://127.0.0.1:1".to_owned()),
        ("EFFIGY_COMPOSE_BACKEND".to_owned(), "docker".to_owned()),
        (
            "EFFIGY_TEST_CONTAINERS_HOME".to_owned(),
            root.join(".effigy").display().to_string(),
        ),
        (
            "EFFIGY_TEST_DOCKER_LOG".to_owned(),
            root.join("docker.log").display().to_string(),
        ),
        ("EFFIGY_TEST_ID".to_owned(), "d".repeat(64)),
        (
            "EFFIGY_TEST_CONTAINER".to_owned(),
            "pty-prep-app-1".to_owned(),
        ),
        ("EFFIGY_TEST_PROJECT".to_owned(), "pty-prep".to_owned()),
        ("EFFIGY_TEST_WORKDIR".to_owned(), root.display().to_string()),
    ]
}
