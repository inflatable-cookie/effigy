#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use nix::unistd::{setpgid, Pid};

use super::super::{ProcessEvent, ProcessManagerError, ProcessSpec};
use super::monitor::attach_child_stream_threads;

const DEMO_BROWSER_TERMINAL_COLS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_COLS";
const DEMO_BROWSER_TERMINAL_ROWS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_ROWS";

pub(super) fn spawn_process_instance(
    spec: &ProcessSpec,
    events_tx: &Sender<ProcessEvent>,
    honor_start_delay: bool,
) -> Result<Arc<Mutex<Child>>, ProcessManagerError> {
    if honor_start_delay && spec.start_after_ms > 0 {
        thread::sleep(Duration::from_millis(spec.start_after_ms));
    }
    let mut process = if spec.pty {
        spawn_with_pty_wrapper(spec)
    } else {
        spawn_plain_shell(spec)
    };
    let mut child = process
        .spawn()
        .map_err(|error| ProcessManagerError::Spawn {
            process: spec.name.clone(),
            command: spec.run.clone(),
            error,
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ProcessManagerError::MissingStdio {
            process: spec.name.clone(),
        })?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ProcessManagerError::MissingStdio {
            process: spec.name.clone(),
        })?;

    let child = Arc::new(Mutex::new(child));
    attach_child_stream_threads(spec.name.clone(), child.clone(), stdout, stderr, events_tx);
    Ok(child)
}

fn spawn_plain_shell(spec: &ProcessSpec) -> ProcessCommand {
    let mut process = ProcessCommand::new("sh");
    process
        .arg("-c")
        .arg(&spec.run)
        .current_dir(&spec.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    // SAFETY: `pre_exec` runs this closure in the forked child before `exec`,
    // where only async-signal-safe work is allowed. `setpgid` is
    // async-signal-safe and places the child in a new process group whose id
    // is the child's own pid; the error path uses the allocation-free
    // `io::Error::from` conversion. `terminate_process_tree` signals
    // `kill(-pid, ...)` for that same group, so shutdown owns the child and
    // every descendant still in its group.
    unsafe {
        process
            .pre_exec(|| setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(std::io::Error::from));
    }
    with_local_node_bin_path(&mut process, &spec.cwd);
    for (key, value) in &spec.env {
        process.env(key, value);
    }
    process
}

fn spawn_with_pty_wrapper(spec: &ProcessSpec) -> ProcessCommand {
    let terminal_size = terminal_size_override(spec);
    let wrapped_run = wrap_pty_shell_command(&spec.run, terminal_size);
    let mut process = ProcessCommand::new("script");
    apply_pty_wrapper_invocation(&mut process, &wrapped_run);
    process
        .current_dir(&spec.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    // SAFETY: same contract as `spawn_plain_shell`: `pre_exec` runs before
    // `exec` where only async-signal-safe calls are permitted, `setpgid` is
    // async-signal-safe, the error conversion allocates nothing, and the new
    // child-owned process group is what `terminate_process_tree` targets with
    // `kill(-pid, ...)`.
    unsafe {
        process
            .pre_exec(|| setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(std::io::Error::from));
    }
    with_local_node_bin_path(&mut process, &spec.cwd);
    if let Some((cols, rows)) = terminal_size {
        process
            .env("COLUMNS", cols.to_string())
            .env("LINES", rows.to_string());
    }
    for (key, value) in &spec.env {
        process.env(key, value);
    }
    process
}

fn with_local_node_bin_path(process: &mut ProcessCommand, cwd: &Path) {
    let local_bin = cwd.join("node_modules/.bin");
    if !local_bin.is_dir() {
        return;
    }
    let local_rendered = local_bin.display().to_string();
    let merged = match std::env::var("PATH") {
        Ok(path) if !path.is_empty() => format!("{local_rendered}:{path}"),
        _ => local_rendered,
    };
    process.env("PATH", merged);
}

fn terminal_size_override(spec: &ProcessSpec) -> Option<(u16, u16)> {
    if let Some(size) = terminal_size_override_from_env_map(&spec.env) {
        return Some(size);
    }
    browser_terminal_size_override()
}

fn terminal_size_override_from_env_map(
    env: &std::collections::BTreeMap<String, String>,
) -> Option<(u16, u16)> {
    let cols = env
        .get(DEMO_BROWSER_TERMINAL_COLS_ENV)?
        .parse::<u16>()
        .ok()?;
    let rows = env
        .get(DEMO_BROWSER_TERMINAL_ROWS_ENV)?
        .parse::<u16>()
        .ok()?;
    if cols == 0 || rows == 0 {
        return None;
    }
    Some((cols, rows))
}

fn browser_terminal_size_override() -> Option<(u16, u16)> {
    let cols = std::env::var(DEMO_BROWSER_TERMINAL_COLS_ENV)
        .ok()?
        .parse::<u16>()
        .ok()?;
    let rows = std::env::var(DEMO_BROWSER_TERMINAL_ROWS_ENV)
        .ok()?
        .parse::<u16>()
        .ok()?;
    if cols == 0 || rows == 0 {
        return None;
    }
    Some((cols, rows))
}

fn wrap_pty_shell_command(run_command: &str, terminal_size: Option<(u16, u16)>) -> String {
    let Some((cols, rows)) = terminal_size else {
        return run_command.to_owned();
    };
    format!("stty cols {cols} rows {rows} >/dev/null 2>&1; {run_command}")
}

#[cfg(target_os = "macos")]
fn apply_pty_wrapper_invocation(process: &mut ProcessCommand, wrapped_run: &str) {
    process
        .arg("-q")
        .arg("/dev/null")
        .arg("sh")
        .arg("-c")
        .arg(wrapped_run);
}

#[cfg(not(target_os = "macos"))]
fn apply_pty_wrapper_invocation(process: &mut ProcessCommand, wrapped_run: &str) {
    process.arg("-qefc").arg(wrapped_run).arg("/dev/null");
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{terminal_size_override_from_env_map, wrap_pty_shell_command};

    #[test]
    fn wrap_pty_shell_command_prefixes_stty_with_terminal_size() {
        let wrapped = wrap_pty_shell_command("printf demo", Some((96, 28)));
        assert_eq!(wrapped, "stty cols 96 rows 28 >/dev/null 2>&1; printf demo");
    }

    #[test]
    fn wrap_pty_shell_command_leaves_command_when_size_missing() {
        let wrapped = wrap_pty_shell_command("printf demo", None);
        assert_eq!(wrapped, "printf demo");
    }

    #[test]
    fn terminal_size_override_from_env_map_reads_process_specific_values() {
        let mut env = BTreeMap::new();
        env.insert("EFFIGY_BROWSER_TERMINAL_COLS".to_owned(), "132".to_owned());
        env.insert("EFFIGY_BROWSER_TERMINAL_ROWS".to_owned(), "41".to_owned());
        assert_eq!(terminal_size_override_from_env_map(&env), Some((132, 41)));
    }
}

#[cfg(all(test, unix))]
mod postfork_safety_tests {
    use std::collections::BTreeMap;
    use std::io::ErrorKind;
    use std::os::unix::process::CommandExt;
    use std::path::PathBuf;
    use std::process::{Command as ProcessCommand, Stdio};

    use nix::errno::Errno;
    use nix::unistd::{getpgid, Pid};

    use super::{setpgid, spawn_plain_shell};
    use crate::ProcessSpec;

    fn sample_spec(run: &str) -> ProcessSpec {
        ProcessSpec {
            name: "postfork-safety".to_owned(),
            run: run.to_owned(),
            cwd: PathBuf::from("/"),
            start_after_ms: 0,
            shutdown_on_exit: false,
            pty: false,
            env: BTreeMap::new(),
        }
    }

    #[test]
    fn setpgid_error_conversion_preserves_errno_without_allocating() {
        for errno in [Errno::EPERM, Errno::EINVAL, Errno::ESRCH] {
            let nix_error = nix::Error::from(errno);
            crate::postfork_test_alloc::begin();
            let converted = std::io::Error::from(nix_error);
            let allocations = crate::postfork_test_alloc::take();
            assert_eq!(
                allocations, 0,
                "allocation-free mapper allocated {allocations} times for {errno}"
            );
            assert_eq!(converted.raw_os_error(), Some(errno as i32));
        }
    }

    #[test]
    fn allocating_setpgid_error_mapper_fails_the_allocation_oracle() {
        let nix_error = nix::Error::from(Errno::EPERM);
        crate::postfork_test_alloc::begin();
        let converted = std::io::Error::other(nix_error.to_string());
        let allocations = crate::postfork_test_alloc::take();
        assert!(
            allocations > 0,
            "negative allocating mapper must be visible to the allocation oracle"
        );
        assert_eq!(converted.raw_os_error(), None);
        assert_eq!(converted.kind(), ErrorKind::Other);
    }

    #[test]
    fn spawn_plain_shell_runs_argv_and_owns_its_process_group() {
        let output = spawn_plain_shell(&sample_spec("printf postfork-ok"))
            .output()
            .expect("printf through production setpgid pre_exec");
        assert!(output.status.success());
        assert_eq!(output.stdout, b"postfork-ok");

        let mut child = spawn_plain_shell(&sample_spec("exec cat"))
            .spawn()
            .expect("cat through production setpgid pre_exec");
        let pid = Pid::from_raw(child.id() as i32);
        let pgid = getpgid(Some(pid)).expect("child process group");
        assert_eq!(pgid, pid);
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn setpgid_pre_exec_preserves_missing_binary_launch_error() {
        let mut command = ProcessCommand::new("/effigy-postfork-missing-binary");
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: same allocation-free `setpgid` conversion as production
        // spawn; this private launch only proves the missing-binary exec
        // error after that callback.
        unsafe {
            command.pre_exec(|| {
                setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(std::io::Error::from)
            });
        }
        let error = command
            .spawn()
            .expect_err("missing binary must fail to launch");
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }
}
