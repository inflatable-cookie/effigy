use super::*;
#[cfg(target_os = "macos")]
use effigy_demo::wrap_pty_shell_command;

pub(in crate::runner::demo_command) fn execute_run_backed_demo(
    repo_root: &Path,
    demo_id: &str,
    mode: ManifestDemoMode,
    entrypoint_value: &str,
    run_command: &str,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let launch_mode = resolve_demo_launch_mode(mode, output_json, run_command);
    let attached_terminal = launch_mode.attached_terminal();
    let initial_terminal_size = initial_terminal_size_for_launch_mode(launch_mode);
    let input_handoff_path = launch_mode
        .supports_input_forwarding()
        .then(|| prepare_demo_input_handoff(repo_root, demo_id))
        .transpose()?;
    let resize_handoff_path = launch_mode
        .supports_resize()
        .then(|| prepare_demo_resize_handoff(repo_root, demo_id))
        .transpose()?;
    let log_paths = if output_json || attached_terminal {
        demo_log_paths_for_launch_mode(repo_root, demo_id, launch_mode)?
    } else {
        DemoLogPaths::none()
    };
    let mut child = build_run_backed_process(repo_root, run_command, launch_mode)?
        .spawn()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "Demo `{demo_id}` failed to launch run entrypoint: {error}"
            ))
        })?;

    let active_record = PersistedDemoActiveAttempt::new_run_backed(
        build_attempt_id(demo_id),
        demo_id,
        entrypoint_value.to_owned(),
        run_command.to_owned(),
        child.id(),
        launch_mode.transport(),
        input_handoff_path
            .as_ref()
            .map(|path| display_repo_path(path, repo_root)),
        resize_handoff_path
            .as_ref()
            .map(|path| display_repo_path(path, repo_root)),
        log_paths.stdout.clone(),
        log_paths.stderr.clone(),
        initial_terminal_size,
    );
    let _active_guard = register_active_attempt(repo_root, demo_id, &active_record)?;

    if output_json || attached_terminal {
        let _stdin_forward = if launch_mode.forward_stdin() && io::stdin().is_terminal() {
            child.stdin.take().map(spawn_stdin_forward)
        } else {
            None
        };
        let input_forward = input_handoff_path.as_ref().and_then(|path| {
            child
                .stdin
                .take()
                .map(|stdin| spawn_input_handoff_forward(path.clone(), stdin))
        });
        let stdout_reader = child.stdout.take().ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "Demo `{demo_id}` launched without a stdout capture pipe."
            ))
        })?;
        let stderr_reader = child.stderr.take().ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "Demo `{demo_id}` launched without a stderr capture pipe."
            ))
        })?;
        let stdout_handle = spawn_output_capture(
            stdout_reader,
            log_paths.stdout_absolute.clone(),
            attached_terminal.then_some(OutputMirror::Stdout),
        );
        let stderr_handle = spawn_output_capture(
            stderr_reader,
            log_paths.stderr_absolute.clone(),
            attached_terminal.then_some(OutputMirror::Stderr),
        );
        let status = child.wait().map_err(|error| {
            RunnerError::task_invocation(format!(
                "Demo `{demo_id}` failed to wait for run entrypoint: {error}"
            ))
        })?;
        if let Some(forward) = input_forward {
            stop_input_handoff_forward(forward, input_handoff_path.as_deref());
        }
        clear_resize_handoff(resize_handoff_path.as_deref());
        let mut stdout = join_output_capture(stdout_handle, "stdout", demo_id)?;
        let mut stderr = join_output_capture(stderr_handle, "stderr", demo_id)?;
        if matches!(launch_mode, DemoLaunchMode::AttachedPty) {
            stdout = sanitize_pty_transcript(&stdout);
            stderr = sanitize_pty_transcript(&stderr);
            if let Some(path) = &log_paths.stdout_absolute {
                fs::write(path, &stdout)
                    .map_err(|error| RunnerError::task_invocation_failed_write(path, error))?;
            }
            if let Some(path) = &log_paths.stderr_absolute {
                fs::write(path, &stderr)
                    .map_err(|error| RunnerError::task_invocation_failed_write(path, error))?;
            }
        }
        let stop_requested = active_attempt_is_stop_requested(repo_root, demo_id);
        return Ok(demo_run_attempt_from_output(
            demo_id,
            entrypoint_value,
            run_command,
            status.code(),
            status.success(),
            stop_requested,
            stdout,
            stderr,
            log_paths,
        ));
    }

    let status = child.wait().map_err(|error| {
        RunnerError::task_invocation(format!(
            "Demo `{demo_id}` failed to wait for run entrypoint: {error}"
        ))
    })?;
    clear_resize_handoff(resize_handoff_path.as_deref());
    let stop_requested = active_attempt_is_stop_requested(repo_root, demo_id);
    Ok(demo_run_attempt_from_output(
        demo_id,
        entrypoint_value,
        run_command,
        status.code(),
        status.success(),
        stop_requested,
        String::new(),
        String::new(),
        DemoLogPaths::none(),
    ))
}

fn demo_log_paths_for_launch_mode(
    repo_root: &Path,
    demo_id: &str,
    launch_mode: DemoLaunchMode,
) -> Result<DemoLogPaths, RunnerError> {
    match launch_mode {
        DemoLaunchMode::AttachedPty => DemoLogPaths::prepare_pty(repo_root, demo_id),
        DemoLaunchMode::DetachedJson | DemoLaunchMode::AttachedStream => {
            DemoLogPaths::prepare_split(repo_root, demo_id)
        }
    }
    .map_err(Into::into)
}

fn build_run_backed_process(
    repo_root: &Path,
    run_command: &str,
    launch_mode: DemoLaunchMode,
) -> Result<ProcessCommand, RunnerError> {
    let mut process = match launch_mode {
        DemoLaunchMode::AttachedPty => build_run_backed_pty_process(repo_root, run_command),
        DemoLaunchMode::DetachedJson | DemoLaunchMode::AttachedStream => {
            let mut process = ProcessCommand::new("sh");
            process.arg("-c").arg(run_command).current_dir(repo_root);
            process
        }
    };
    if launch_mode.capture_output() {
        process
            .stdin(
                if launch_mode.forward_stdin() || launch_mode.supports_input_forwarding() {
                    Stdio::piped()
                } else {
                    Stdio::inherit()
                },
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    } else {
        process
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    }
    #[cfg(unix)]
    unsafe {
        process.pre_exec(|| {
            setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|error| std::io::Error::other(error.to_string()))
        });
    }
    if let Some((cols, rows)) = current_terminal_size() {
        process
            .env("COLUMNS", cols.to_string())
            .env("LINES", rows.to_string());
    }
    with_local_node_bin_path(&mut process, repo_root);
    Ok(process)
}

#[cfg(target_os = "macos")]
fn build_run_backed_pty_process(repo_root: &Path, run_command: &str) -> ProcessCommand {
    let wrapped = wrap_pty_shell_command(run_command, current_terminal_size());
    let mut process = ProcessCommand::new("script");
    process
        .arg("-q")
        .arg("/dev/null")
        .arg("sh")
        .arg("-c")
        .arg(wrapped)
        .current_dir(repo_root);
    process
}

#[cfg(not(target_os = "macos"))]
fn build_run_backed_pty_process(repo_root: &Path, run_command: &str) -> ProcessCommand {
    let mut process = ProcessCommand::new("sh");
    process.arg("-c").arg(run_command).current_dir(repo_root);
    process
}

fn join_output_capture(
    handle: thread::JoinHandle<Result<String, effigy_demo::DemoStateError>>,
    stream_name: &str,
    demo_id: &str,
) -> Result<String, RunnerError> {
    handle
        .join()
        .map_err(|_| {
            RunnerError::task_invocation(format!(
                "demo `{demo_id}` {stream_name} capture thread panicked"
            ))
        })?
        .map_err(Into::into)
}
