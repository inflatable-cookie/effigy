use super::super::support::parse_stdout_json;
use super::super::support::{temp_execution_workspace, FixtureCommandExt, OwnedCliChild};
use super::*;
use std::path::Path;
use std::process::{ExitStatus, Output, Stdio};
use std::time::Instant;

const LOCK_HOLDER_EXIT_TIMEOUT: Duration = Duration::from_secs(2);

struct HeldCliTask {
    child: Option<OwnedCliChild>,
    task_pid_path: std::path::PathBuf,
    ready_path: std::path::PathBuf,
    release_path: std::path::PathBuf,
    task_pid: Option<u32>,
    cli_group_pid: u32,
}

impl HeldCliTask {
    fn spawn(
        root: &Path,
        args: &[&str],
        task_pid_path: std::path::PathBuf,
        ready_path: std::path::PathBuf,
        release_path: std::path::PathBuf,
    ) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .arg("--repo")
            .arg(root)
            .args(args)
            .env("NO_COLOR", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Own a fresh group rooted at the exact CLI child so cleanup can safely
        // stop the shell and any helper process it starts.
        let child = command.spawn_owned().expect("spawn lock holder");
        let cli_group_pid = child.id();
        Self {
            child: Some(child),
            task_pid_path,
            ready_path,
            release_path,
            task_pid: None,
            cli_group_pid,
        }
    }

    /// The holder is ready once its task wrote the readiness marker and still
    /// lives: a running task holds its gate for exactly that long (K2).
    fn wait_until_ready(&mut self, timeout: Duration) -> Result<u32, String> {
        let startup_deadline = self.child.as_ref().expect("holder child").deadline();
        let mut task_started = None;
        loop {
            self.refresh_task_pid();
            if let Some(status) = self.try_wait()? {
                let output = self.take_output()?;
                return Err(format!(
                    "lock holder exited before readiness (ready={}): {}",
                    self.ready_path.display(),
                    describe_output(&output, status)
                ));
            }

            if let Some(task_pid) = self.task_pid {
                task_started.get_or_insert_with(Instant::now);
                if self.ready_path.exists() && process_is_live(task_pid)? {
                    return Ok(task_pid);
                }
            }

            // The unchanged readiness budget belongs to the started task.
            // Cold CLI/metadata/admission entry has its existing absolute
            // fixture deadline; neither phase can reset that outer deadline.
            let readiness_expired =
                task_started.is_some_and(|started: Instant| started.elapsed() >= timeout);
            if readiness_expired || Instant::now() >= startup_deadline {
                let readiness = fs::read_to_string(&self.ready_path)
                    .unwrap_or_else(|error| format!("unavailable ({error})"));
                let child_result = self.release_and_reap();
                let child_diagnostic = match child_result {
                    Ok(output) => describe_output(&output, output.status),
                    Err(error) => error,
                };
                let phase = if readiness_expired {
                    "task_readiness"
                } else {
                    "cli_startup"
                };
                return Err(format!(
                    "lock holder did not reach readiness (phase={phase}) within {timeout:?} (ready={} contents={readiness:?}); {child_diagnostic}",
                    self.ready_path.display(),
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn assert_holding(&mut self, task_pid: u32) -> Result<(), String> {
        self.refresh_task_pid();
        if !self.ready_path.exists() {
            return Err(format!(
                "holder readiness disappeared: {}",
                self.ready_path.display()
            ));
        }
        if self.task_pid != Some(task_pid) || !process_is_live(task_pid)? {
            return Err(format!("task owner process {task_pid} is no longer alive"));
        }
        if let Some(status) = self.try_wait()? {
            let output = self.take_output()?;
            return Err(format!(
                "CLI holder exited while lock was expected live: {}",
                describe_output(&output, status)
            ));
        }
        Ok(())
    }

    fn release_and_reap(&mut self) -> Result<Output, String> {
        if let Err(error) = self.signal_release() {
            self.force_cleanup();
            return Err(error);
        }

        let deadline = Instant::now() + LOCK_HOLDER_EXIT_TIMEOUT;
        loop {
            match self.try_wait() {
                Ok(Some(status)) => {
                    let output = self.take_output()?;
                    if let Some(task_pid) = self.task_pid {
                        if !wait_for_process_gone(task_pid, Duration::from_millis(300)) {
                            self.force_cleanup();
                            return Err(format!(
                                "task process {task_pid} remained after CLI exit: {}",
                                describe_output(&output, status)
                            ));
                        }
                    }
                    if !wait_for_process_group_gone(self.cli_group_pid, Duration::from_millis(300))
                    {
                        self.force_cleanup();
                        return Err(format!(
                            "owned CLI process group {} remained after holder exit: {}",
                            self.cli_group_pid,
                            describe_output(&output, status)
                        ));
                    }
                    return Ok(output);
                }
                Ok(None) => {}
                Err(error) => {
                    self.force_cleanup();
                    return Err(error);
                }
            }

            if Instant::now() >= deadline {
                self.force_cleanup();
                let output = self.take_output();
                return Err(format!(
                    "lock holder did not exit after release within {LOCK_HOLDER_EXIT_TIMEOUT:?}; {}",
                    output
                        .map(|output| format!(
                            "status={} stdout={:?} stderr={:?}",
                            output.status,
                            String::from_utf8_lossy(&output.stdout),
                            String::from_utf8_lossy(&output.stderr)
                        ))
                        .unwrap_or_else(|error| error)
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn signal_release(&mut self) -> Result<(), String> {
        fs::write(&self.release_path, b"release\n").map_err(|error| {
            format!(
                "could not release task through its private file {}: {error}",
                self.release_path.display()
            )
        })
    }

    fn refresh_task_pid(&mut self) {
        if let Ok(contents) = fs::read_to_string(&self.task_pid_path) {
            if let Ok(pid) = contents.trim().parse::<u32>() {
                if pid > 0 {
                    self.task_pid = Some(pid);
                }
            }
        }
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>, String> {
        self.child
            .as_mut()
            .map(OwnedCliChild::try_wait)
            .transpose()
            .map_err(|error| format!("could not inspect lock holder child: {error}"))
            .map(Option::flatten)
    }

    fn take_output(&mut self) -> Result<Output, String> {
        self.child
            .take()
            .ok_or_else(|| "lock holder output was already collected".to_owned())?
            .wait_with_output()
            .map_err(|error| format!("could not collect lock holder output: {error}"))
    }

    fn force_cleanup(&mut self) {
        // The private release file is cooperative task cleanup; a reaped CLI
        // or stale PID file never authorizes a process-group signal.
        let _ = self.signal_release();
        if let Some(child) = self.child.as_mut() {
            let _ = child.terminate_and_wait();
        }
        if let Some(task_pid) = self.task_pid {
            let _ = wait_for_process_gone(task_pid, Duration::from_secs(2));
        }
    }
}

impl Drop for HeldCliTask {
    fn drop(&mut self) {
        if self.child.is_none() {
            return;
        }
        let _ = self.signal_release();
        if self.release_and_reap().is_err() {
            self.force_cleanup();
        }
    }
}

fn write_held_task_manifest(root: &Path, task: &str, run: &str) {
    fs::write(
        root.join("effigy.toml"),
        format!("[tasks.{task}]\nrun = {run:?}\n"),
    )
    .expect("write lock-holder manifest");
}

fn held_task_command(task_pid_path: &Path, ready_path: &Path, release_path: &Path) -> String {
    format!(
        "printf '%s\\n' \"$$\" > {}; printf 'ready\\n' > {}; while [ ! -f {} ]; do sleep 0.01; done",
        shell_quote(task_pid_path),
        shell_quote(ready_path),
        shell_quote(release_path),
    )
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn describe_output(output: &Output, status: ExitStatus) -> String {
    format!(
        "status={status} stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn process_is_live(pid: u32) -> Result<bool, String> {
    #[cfg(unix)]
    {
        use nix::errno::Errno;
        match nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None) {
            Ok(()) | Err(Errno::EPERM) => Ok(true),
            Err(Errno::ESRCH) => Ok(false),
            Err(error) => Err(format!("could not inspect owned process {pid}: {error}")),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Ok(true)
    }
}

fn process_group_is_live(pid: u32) -> Result<bool, String> {
    #[cfg(unix)]
    {
        use nix::errno::Errno;
        match nix::sys::signal::kill(nix::unistd::Pid::from_raw(-(pid as i32)), None) {
            Ok(()) | Err(Errno::EPERM) => Ok(true),
            Err(Errno::ESRCH) => Ok(false),
            Err(error) => Err(format!(
                "could not inspect owned process group {pid}: {error}"
            )),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Ok(true)
    }
}

fn wait_for_process_group_gone(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !process_group_is_live(pid).unwrap_or(true) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_process_gone(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !process_is_live(pid).unwrap_or(true) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn held_task_paths(
    root: &Path,
    suffix: &str,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    (
        root.join(format!("{suffix}-task-pid")),
        root.join(format!("{suffix}-ready")),
        root.join(format!("{suffix}-release")),
    )
}

fn assert_json_lock_conflict_is_held(
    root_name: &str,
    manifest_task: &str,
    owner_args: &[&str],
    contender_args: &[&str],
    command_name: &str,
    message_fragments: &[&str],
) {
    let root = temp_workspace(root_name);
    let (task_pid_path, ready_path, release_path) = held_task_paths(&root, "holder");
    write_held_task_manifest(
        &root,
        manifest_task,
        &held_task_command(&task_pid_path, &ready_path, &release_path),
    );

    let mut holder = HeldCliTask::spawn(&root, owner_args, task_pid_path, ready_path, release_path);
    let task_pid = holder
        .wait_until_ready(Duration::from_secs(5))
        .unwrap_or_else(|diagnostic| panic!("{diagnostic}"));
    holder
        .assert_holding(task_pid)
        .unwrap_or_else(|diagnostic| panic!("holder was not ready: {diagnostic}"));

    let output = run_json_cli_command(&root, contender_args);
    holder
        .assert_holding(task_pid)
        .unwrap_or_else(|diagnostic| {
            panic!("holder ended before contender assertion: {diagnostic}")
        });

    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], command_name);
    assert_eq!(parsed["error"]["kind"], "refused");
    let message = parsed["error"]["message"]
        .as_str()
        .expect("lock conflict error message");
    for fragment in message_fragments {
        assert!(
            message.contains(fragment),
            "lock conflict message missed {fragment:?}: {message}"
        );
    }

    holder
        .assert_holding(task_pid)
        .unwrap_or_else(|diagnostic| {
            panic!("holder did not stay alive through assertion: {diagnostic}")
        });
    let holder_output = holder
        .release_and_reap()
        .unwrap_or_else(|diagnostic| panic!("could not release/reap holder: {diagnostic}"));
    assert!(
        holder_output.status.success(),
        "released holder failed: {}",
        describe_output(&holder_output, holder_output.status)
    );

    let released = run_json_cli_command(&root, contender_args);
    assert!(
        released.status.success(),
        "released lock still conflicted: {}",
        String::from_utf8_lossy(&released.stdout)
    );
    let released_json = parse_stdout_json(&released);
    assert_eq!(released_json["schema"], "effigy.command.v1");
    assert_eq!(released_json["ok"], true);
    assert_eq!(released_json["command"]["name"], command_name);
}

#[test]
fn cli_json_mode_task_wraps_task_run_payload() {
    let parsed = run_json_task_success("cli-json-task-success", "build", "printf build-ok");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "build");
    assert_eq!(parsed["result"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["result"]["task"], "build");
    assert_eq!(parsed["result"]["stdout"], "build-ok");
}

#[test]
fn cli_json_mode_parse_error_wraps_error_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("tasks")
        .arg("--repo")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "cli");
    assert_eq!(parsed["command"]["name"], "parse");
    assert_eq!(parsed["error"]["kind"], "refused");
    assert_eq!(parsed["error"]["type"], "CliParseError");
}

#[test]
fn cli_json_mode_runner_error_wraps_runner_failure() {
    let root = temp_workspace("cli-json-runner-error-envelope");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .arg("missing-task")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "missing-task");
    assert_eq!(parsed["error"]["kind"], "refused");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("missing-task")));
}

#[test]
fn cli_json_mode_lock_conflict_wraps_runner_failure() {
    assert_json_lock_conflict_is_held(
        "cli-json-lock-conflict",
        "dev",
        &["dev"],
        &["dev"],
        "dev",
        // The refusal names the holding run and only commands the CLI
        // accepts (`effigy tasks status` does not resolve a managed profile).
        &[
            "`dev` is already running",
            "`effigy run status ",
            "`effigy run stop ",
            "`effigy runs`",
        ],
    );
}

#[test]
fn cli_json_lock_conflict_holder_exit_before_readiness_reports_child_output() {
    let root = temp_workspace("cli-json-lock-conflict-holder-exit");
    let (task_pid_path, ready_path, release_path) = held_task_paths(&root, "early-exit");
    write_held_task_manifest(
        &root,
        "dev",
        &format!(
            "printf '%s\\n' \"$$\" > {}; printf holder-stdout; printf holder-stderr >&2; exit 17",
            shell_quote(&task_pid_path)
        ),
    );

    let mut holder = HeldCliTask::spawn(&root, &["dev"], task_pid_path, ready_path, release_path);
    let diagnostic = holder
        .wait_until_ready(Duration::from_secs(5))
        .expect_err("an exited holder must not count as lock readiness");
    assert!(diagnostic.contains("status=exit status:"), "{diagnostic}");
    assert!(diagnostic.contains("holder-stdout"), "{diagnostic}");
    assert!(diagnostic.contains("holder-stderr"), "{diagnostic}");
}

#[test]
fn cli_lock_readiness_budget_starts_at_task_entry_without_rebasing_outer_deadline() {
    let root = temp_execution_workspace("lock-delayed-task-entry");
    let (task_pid, ready, release) = held_task_paths(&root, "delayed");
    // A real task delays its entry marker; this is not a warmed metadata or
    // scheduler path. The original pre-entry five-second check fails here.
    write_held_task_manifest(
        &root,
        "dev",
        &format!(
            "sleep 6; {}",
            held_task_command(&task_pid, &ready, &release)
        ),
    );
    let mut holder = HeldCliTask::spawn(&root, &["dev"], task_pid, ready, release);
    let outer = holder.child.as_ref().expect("holder child").deadline();
    let pid = holder
        .wait_until_ready(Duration::from_secs(5))
        .expect("actual task entry followed by timely readiness");
    assert!(pid > 0);
    assert_eq!(
        holder.child.as_ref().expect("holder child").deadline(),
        outer,
        "readiness observation must not rebase the absolute fixture bound"
    );
    let output = holder
        .release_and_reap()
        .expect("release delayed-entry holder");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn cli_json_lock_conflict_readiness_timeout_reaps_owned_processes() {
    let root = temp_workspace("cli-json-lock-conflict-readiness-timeout");
    let (task_pid_path, ready_path, release_path) = held_task_paths(&root, "timeout");
    write_held_task_manifest(
        &root,
        "dev",
        &format!(
            "printf '%s\\n' \"$$\" > {}; printf timeout-stdout; printf timeout-stderr >&2; exec sleep 10",
            shell_quote(&task_pid_path)
        ),
    );

    let mut holder = HeldCliTask::spawn(&root, &["dev"], task_pid_path, ready_path, release_path);
    let diagnostic = holder
        .wait_until_ready(Duration::from_secs(5))
        .expect_err("a task without readiness must time out");
    assert!(
        diagnostic.contains("did not reach readiness"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains("timeout-stdout"), "{diagnostic}");
    assert!(diagnostic.contains("timeout-stderr"), "{diagnostic}");
    assert!(
        holder.child.is_none(),
        "timeout path must reap the CLI child"
    );
    if let Some(task_pid) = holder.task_pid {
        assert!(
            wait_for_process_gone(task_pid, Duration::from_secs(3)),
            "timed-out task process {task_pid} survived cleanup"
        );
    }
    assert!(
        wait_for_process_group_gone(holder.cli_group_pid, Duration::from_secs(3)),
        "timed-out owned process group {} survived cleanup",
        holder.cli_group_pid
    );
}

#[test]
fn cli_json_lock_conflict_holder_drop_releases_and_reaps_on_panic() {
    let root = temp_workspace("cli-json-lock-conflict-holder-panic");
    let (task_pid_path, ready_path, release_path) = held_task_paths(&root, "panic");
    write_held_task_manifest(
        &root,
        "dev",
        &held_task_command(&task_pid_path, &ready_path, &release_path),
    );
    let mut holder = HeldCliTask::spawn(&root, &["dev"], task_pid_path, ready_path, release_path);
    let task_pid = holder
        .wait_until_ready(Duration::from_secs(5))
        .unwrap_or_else(|diagnostic| panic!("{diagnostic}"));
    let cli_pid = holder.child.as_ref().expect("child").id();
    let cli_group_pid = holder.cli_group_pid;

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _holder = holder;
        panic!("simulate an assertion failure while the owner is held");
    }));
    assert!(panic.is_err());
    assert!(
        wait_for_process_gone(task_pid, Duration::from_secs(3)),
        "holder task process {task_pid} survived panic cleanup"
    );
    assert!(
        wait_for_process_gone(cli_pid, Duration::from_secs(3)),
        "holder CLI process {cli_pid} survived panic cleanup"
    );
    assert!(
        wait_for_process_group_gone(cli_group_pid, Duration::from_secs(3)),
        "holder process group {cli_group_pid} survived panic cleanup"
    );
}

#[test]
fn cli_lock_wait_timeout_keeps_owner_on_status_and_omits_json_from_text_stdout() {
    let root = temp_workspace("cli-lock-wait-timeout-status");
    fs::write(root.join("effigy.toml"), "[tasks.dev]\nrun = \"sleep 8\"\n")
        .expect("write manifest");

    let mut owner = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("dev")
        .env("NO_COLOR", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn_owned()
        .expect("spawn holding command");

    // The owner holds the gate once its run reads `running`.
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let probe = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("--json")
            .arg("--repo")
            .arg(&root)
            .args(["tasks", "status", "dev"])
            .env("NO_COLOR", "1")
            .output()
            .expect("probe owner status");
        let state = serde_json::from_slice::<Value>(&probe.stdout)
            .ok()
            .and_then(|parsed| parsed["result"]["state"].as_str().map(str::to_owned));
        if state.as_deref() == Some("running") {
            break;
        }
        assert!(Instant::now() < deadline, "owner never held `dev`");
        std::thread::sleep(Duration::from_millis(50));
    }

    let text = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--lock-wait-ms")
        .arg("150")
        .arg("dev")
        .env("NO_COLOR", "1")
        .output()
        .expect("run timed-out waiter");
    assert!(!text.status.success());
    let stdout = String::from_utf8(text.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(text.stderr).expect("utf8 stderr");
    assert!(
        !stdout.contains("effigy.lock-wait.v2"),
        "text mode dumped lock-wait JSON: {stdout}"
    );
    assert!(stderr.contains("`dev` is already running"), "{stderr}");
    assert!(stderr.contains("waited"), "{stderr}");
    assert!(
        stderr.contains("`effigy run status ") && stderr.contains("`effigy run stop "),
        "{stderr}"
    );
    assert!(!stderr.contains("effigy tasks status"), "{stderr}");
    assert_eq!(text.status.code(), Some(75), "a lock conflict exits 75");

    let json = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .arg("--lock-wait-ms")
        .arg("150")
        .arg("dev")
        .env("NO_COLOR", "1")
        .output()
        .expect("run json timed-out waiter");
    assert!(!json.status.success());
    let parsed: Value =
        serde_json::from_str(&String::from_utf8(json.stdout).expect("utf8 json")).expect("json");
    assert_eq!(parsed["error"]["details"]["schema"], "effigy.lock-wait.v2");
    assert_eq!(parsed["error"]["details"]["wait_timeout_ms"], 150);
    assert!(parsed["error"]["details"]["holder"]["pid"].is_u64());
    assert!(
        parsed["error"]["details"]["status_command"]
            .as_str()
            .is_some_and(|command| command.starts_with("effigy run status ")),
        "status_command must be a command the CLI accepts: {parsed}"
    );

    let status = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("tasks")
        .arg("status")
        .arg("dev")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run tasks status");
    // The waiters never started a run of their own: the owner's run stays
    // current.
    assert!(status.status.success(), "status failed: {status:?}");
    let status_parsed: Value =
        serde_json::from_str(&String::from_utf8(status.stdout).expect("utf8 status"))
            .expect("json");
    assert_eq!(status_parsed["result"]["state"], "running");
    assert!(
        status_parsed["result"]["current"].is_object(),
        "live owner missing: {status_parsed}"
    );
    assert_ne!(status_parsed["result"]["current"]["pid"], Value::Null);

    let _ = owner.kill();
    let _ = owner.wait();
}

fn write_failing_rhai_fixture(root: &Path, script: &str) {
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");
    fs::write(root.join("scripts/fail.rhai"), script).expect("write rhai script");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.fail-rhai]\nrhai = \"scripts/fail.rhai\"\nrun_in = \"host\"\n",
    )
    .expect("write manifest");
}

#[test]
fn cli_json_mode_failing_rhai_task_preserves_script_output_in_error_details() {
    let root = temp_workspace("cli-json-failing-rhai-task");
    write_failing_rhai_fixture(
        &root,
        "log(\"diagnostic line one\");\nlog_warn(\"warn diagnostic\");\nthrow(\"boom: deliberate failure\");\n",
    );

    let output = run_json_cli_command(&root, &["fail-rhai"]);

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "fail-rhai");
    assert_eq!(parsed["result"], Value::Null);
    assert_eq!(parsed["error"]["kind"], "unavailable");
    let details = &parsed["error"]["details"];
    assert_eq!(details["schema"], "effigy.task.run.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(details["task"], "fail-rhai");
    assert_eq!(details["exit_code"], 1);
    assert_eq!(details["stdout"], "diagnostic line one\n");
    assert!(details["stderr"]
        .as_str()
        .is_some_and(|stderr| stderr.contains("warn diagnostic")));
    assert!(details["stderr"]
        .as_str()
        .is_some_and(|stderr| stderr.contains("boom: deliberate failure")));
}

#[test]
fn cli_json_mode_failing_rhai_task_without_output_still_carries_details() {
    let root = temp_workspace("cli-json-failing-rhai-silent");
    write_failing_rhai_fixture(&root, "throw(\"boom: silent failure\");\n");

    let output = run_json_cli_command(&root, &["fail-rhai"]);

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["ok"], false);
    let details = &parsed["error"]["details"];
    assert_eq!(details["schema"], "effigy.task.run.v1");
    assert_eq!(details["stdout"], "");
    assert!(details["stderr"]
        .as_str()
        .is_some_and(|stderr| stderr.contains("boom: silent failure")));
}

#[test]
fn cli_text_mode_failing_rhai_task_streams_output_without_envelope() {
    let root = temp_workspace("cli-text-failing-rhai-task");
    write_failing_rhai_fixture(
        &root,
        "log(\"diagnostic line one\");\nlog_warn(\"warn diagnostic\");\nthrow(\"boom: deliberate failure\");\n",
    );

    let output = run_cli_command(&root, &["fail-rhai"]);

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stdout.contains("diagnostic line one"));
    assert!(stderr.contains("warn diagnostic"));
    assert!(stderr.contains("boom: deliberate failure"));
    assert!(
        !stdout.contains("effigy.command.v1"),
        "text mode leaked a JSON envelope: {stdout}"
    );
}

/// v0.14 task state is never read or deleted; `doctor` lists what remains so
/// the operator knows it is safe to remove (contract 052 §11). Only retired
/// cache files are listed: `cache/sources` and `cache/bundles` are live.
#[test]
fn cli_doctor_lists_v014_task_state_families_in_json_and_text() {
    let root = temp_workspace("cli-doctor-legacy-task-state");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build\"\n",
    )
    .expect("write manifest");
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(root.join(".effigy/locks/task-build.lock"), "{}").expect("write v0.14 lock");
    for directory in [".effigy/cache/sources/generation", ".effigy/cache/bundles"] {
        fs::create_dir_all(root.join(directory)).expect("mkdir live cache");
    }
    for file in ["task-cache-v1.json", "catalog-discovery-v1.json"] {
        fs::write(root.join(".effigy/cache").join(file), "{}").expect("write v0.14 cache");
    }

    let output = run_json_cli_command(&root, &["doctor"]);
    let parsed = parse_stdout_json(&output);
    let rendered = parsed.to_string();
    for family in [
        "tasks.legacy-state.task-lease-locks",
        "tasks.legacy-state.task-cache",
        "tasks.legacy-state.cache-catalog-discovery-v1",
    ] {
        assert!(
            rendered.contains(family),
            "json doctor misses {family}: {rendered}"
        );
    }
    assert!(
        !rendered.contains("delete the directory"),
        "doctor advises deleting live cache directories: {rendered}"
    );

    let text = run_cli_command(&root, &["doctor"]);
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(
        stdout.contains("tasks.legacy-state.task-lease-locks")
            && stdout.contains("tasks.legacy-state.task-cache"),
        "text doctor misses the legacy families: {stdout}"
    );
    assert!(
        root.join(".effigy/locks/task-build.lock").exists()
            && root.join(".effigy/cache/task-cache-v1.json").exists(),
        "doctor lists, never deletes"
    );
}

#[test]
fn cli_json_mode_tasks_unlock_refuses_with_the_explanation() {
    let root = temp_workspace("cli-json-retired-unlock");
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(root.join(".effigy/locks/task-watch-build.lock"), "{}").expect("write v0.14 lock");

    let output = run_json_cli_command(&root, &["tasks", "unlock", "task:watch:build"]);

    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["ok"], false);
    let message = parsed["error"]["message"].as_str().expect("message");
    assert!(message.contains("was removed in v0.15"), "{message}");
    assert!(
        root.join(".effigy/locks/task-watch-build.lock").exists(),
        "the refusal changes nothing"
    );
}

#[test]
fn cli_json_mode_missing_task_wraps_runner_failure() {
    let (_root, output, parsed) = run_json_cli_command_with_manifest(
        "cli-json-missing-task",
        "[tasks.build]\nrun = \"printf build\"\n",
        &["does-not-exist"],
    );

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "does-not-exist");
    assert_eq!(parsed["error"]["kind"], "refused");
}

#[test]
fn cli_json_mode_deploy_execution_refuses_with_nonzero_exit() {
    let root = temp_workspace("cli-json-deploy-refusal");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    for operation in ["apply", "redeploy"] {
        for yes in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
            command
                .args(["--json", "deploy", operation, "uat", "--repo"])
                .arg(&root);
            if operation == "redeploy" {
                command.args(["--deployment", "missing"]);
            }
            if yes {
                command.arg("--yes");
            }
            let output = command
                .env("NO_COLOR", "1")
                .output()
                .expect("run deploy refusal");
            assert!(!output.status.success());
            let parsed: Value = serde_json::from_slice(&output.stdout).expect("json envelope");
            assert_eq!(parsed["schema"], "effigy.command.v1");
            assert_eq!(parsed["ok"], false);
            assert_eq!(
                parsed["error"]["details"]["schema"],
                "effigy.deploy.apply.v1"
            );
            assert_eq!(parsed["error"]["details"]["reason"], "not_supported");
            assert_eq!(parsed["error"]["details"]["operation"], operation);
        }
    }
    assert!(!root.join(".effigy/reports/deploy").exists());
    assert!(!root.join(".effigy/runtime/deploy").exists());
}

#[test]
fn cli_json_mode_deploy_model_wraps_deploy_payload() {
    let root = temp_workspace("cli-json-deploy-model");
    setup_workspace_app_path_bundle(&root);
    fs::write(
        root.join("effigy.toml"),
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme", "acme_test"]

[bundle.dirs]
front = "acme-front"
admin = "acme-admin"
api = "acme-api"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("acme-front")).expect("mkdir front");
    fs::create_dir_all(root.join("acme-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("acme-api")).expect("mkdir api");
    fs::write(
        root.join("acme-front/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write front manifest");
    fs::write(
        root.join("acme-admin/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write admin manifest");
    fs::write(
        root.join("acme-api/effigy.toml"),
        "[tasks.build]\nrun = \"cargo build --release\"\n[tasks.api]\nrun = \"cargo run -p acme-api\"\n[tasks.jobs]\nrun = \"cargo run -p acme-jobs {args}\"\n",
    )
    .expect("write api manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("deploy")
        .arg("model")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run deploy model");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "deploy");
    assert_eq!(parsed["command"]["name"], "deploy");
    assert_eq!(parsed["result"]["schema"], "deploy.model.v1");
    assert_eq!(parsed["result"]["app"]["bundle"], "workspace-app");
    assert_eq!(parsed["result"]["backing_services"][0]["name"], "postgres");
}

#[test]
fn cli_json_mode_deploy_export_render_wraps_export_payload() {
    let root = temp_workspace("cli-json-deploy-export-render");
    setup_workspace_app_path_bundle(&root);
    write_test_deploy_export_provider(&root, "render", "app-api", "app-jobs");
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[bundle]\nbase = {{ type = \"path\", dir = \"bundles/workspace-app\" }}\nhost = \"acme.test\"\nproject_name = \"acme-dev\"\nworkspace_subdir = \"acme\"\ndatabases = [\"acme\"]\n{}",
            deploy_provider_source("render")
        ),
    )
    .expect("write root manifest");
    fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("app-api")).expect("mkdir api");
    fs::write(
        root.join("app-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front svelte config");
    fs::write(
        root.join("app-admin/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: 'index.html' }) } };\n",
    )
    .expect("write admin svelte config");
    fs::write(
        root.join("app-front/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write front manifest");
    fs::write(
        root.join("app-admin/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write admin manifest");
    fs::write(
        root.join("app-api/effigy.toml"),
        "[tasks.build]\nrun = \"cargo build --release\"\n[tasks.api]\nrun = \"cargo run -p app-api\"\n",
    )
    .expect("write api manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("deploy")
        .arg("export")
        .arg("render")
        .arg("--path")
        .arg(root.join("infra/render"))
        .arg("--plan")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run deploy export render");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "deploy");
    assert_eq!(parsed["command"]["name"], "deploy");
    assert_eq!(parsed["result"]["schema"], "effigy.deploy.export.v1");
    assert_eq!(parsed["result"]["provider"], "render");
    assert_eq!(parsed["result"]["plan"], true);
}

#[test]
fn cli_json_mode_deploy_export_railway_wraps_export_payload() {
    let root = temp_workspace("cli-json-deploy-export-railway");
    setup_workspace_app_path_bundle(&root);
    write_test_deploy_export_provider(&root, "railway", "app-api", "app-jobs");
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[bundle]\nbase = {{ type = \"path\", dir = \"bundles/workspace-app\" }}\nhost = \"acme.test\"\nproject_name = \"acme-dev\"\nworkspace_subdir = \"acme\"\ndatabases = [\"acme\"]\n{}",
            deploy_provider_source("railway")
        ),
    )
    .expect("write root manifest");
    fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("app-api")).expect("mkdir api");
    fs::write(
        root.join("app-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front svelte config");
    fs::write(
        root.join("app-admin/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: 'index.html' }) } };\n",
    )
    .expect("write admin svelte config");
    fs::write(
        root.join("app-front/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write front manifest");
    fs::write(
        root.join("app-admin/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write admin manifest");
    fs::write(
        root.join("app-api/effigy.toml"),
        "[tasks.build]\nrun = \"cargo build --release\"\n[tasks.api]\nrun = \"cargo run -p app-api\"\n",
    )
    .expect("write api manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("deploy")
        .arg("export")
        .arg("railway")
        .arg("--path")
        .arg(root.join("infra/railway"))
        .arg("--plan")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run deploy export railway");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "deploy");
    assert_eq!(parsed["command"]["name"], "deploy");
    assert_eq!(parsed["result"]["schema"], "effigy.deploy.export.v1");
    assert_eq!(parsed["result"]["provider"], "railway");
    assert_eq!(parsed["result"]["plan"], true);
}
