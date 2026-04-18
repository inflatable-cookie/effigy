use super::query::render_demo_run_command;
use super::*;

pub(super) fn load_active_attempt(
    repo_root: &Path,
    demo_id: &str,
) -> Result<DemoActiveAttempt, RunnerError> {
    load_demo_active_attempt(repo_root, demo_id, pid_is_alive).map_err(Into::into)
}

pub(super) fn execute_demo_attempt(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    match DemoEntrypoint::from_manifest(demo) {
        DemoEntrypoint::Task(task_name) => {
            execute_task_backed_demo(repo_root, demo_id, &task_name, demo.mode, output_json)
        }
        DemoEntrypoint::Run(run_spec) => {
            let entrypoint_value = crate_demo_run_preview(&run_spec);
            let rendered_command = render_demo_run_command(repo_root, loaded, demo_id, &run_spec)?;
            execute_run_backed_demo(
                repo_root,
                demo_id,
                demo.mode,
                &entrypoint_value,
                &rendered_command,
                output_json,
            )
        }
    }
}

fn execute_task_backed_demo(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    demo_mode: ManifestDemoMode,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    if let Some(selection) = demo_task_selection(repo_root, task_name)? {
        if task_is_concurrent_runner_backed(selection.task()?) {
            return execute_concurrent_runner_backed_demo(
                repo_root,
                demo_id,
                task_name,
                demo_mode,
                selection,
                output_json,
            );
        }
    }

    let active_record =
        PersistedDemoActiveAttempt::new_task_backed(build_attempt_id(demo_id), demo_id, task_name);
    let _active_guard = register_active_attempt(repo_root, demo_id, &active_record)?;

    if output_json {
        let task = TaskInvocation {
            name: task_name.to_owned(),
            args: vec!["--json".to_owned()],
        };
        return match run_manifest_task_with_cwd(&task, repo_root.to_path_buf()) {
            Ok(rendered) => {
                parse_task_backed_attempt_json(repo_root, demo_id, task_name, &rendered)
            }
            Err(RunnerError::CommandJsonFailure { rendered }) => {
                parse_task_backed_attempt_json(repo_root, demo_id, task_name, &rendered)
            }
            Err(error) => Ok(failed_demo_attempt(
                "task",
                task_name,
                task_name,
                None,
                format!("Demo `{demo_id}` failed to run task `{task_name}`: {error}"),
                String::new(),
                String::new(),
                DemoLogPaths::none(),
            )),
        };
    }

    let task = TaskInvocation {
        name: task_name.to_owned(),
        args: Vec::new(),
    };
    match run_manifest_task_with_cwd(&task, repo_root.to_path_buf()) {
        Ok(_) => Ok(successful_demo_attempt(
            "task",
            task_name,
            task_name,
            None,
            Some(format!(
                "Demo `{demo_id}` completed via task `{task_name}`."
            )),
            String::new(),
            String::new(),
            DemoLogPaths::none(),
        )),
        Err(RunnerError::TaskCommandFailure { code, .. }) => Ok(failed_demo_attempt(
            "task",
            task_name,
            task_name,
            code,
            format!("Demo `{demo_id}` failed via task `{task_name}`."),
            String::new(),
            String::new(),
            DemoLogPaths::none(),
        )),
        Err(error) => Ok(failed_demo_attempt(
            "task",
            task_name,
            task_name,
            None,
            format!("Demo `{demo_id}` failed to run task `{task_name}`: {error}"),
            String::new(),
            String::new(),
            DemoLogPaths::none(),
        )),
    }
}

pub(super) struct DemoTaskSelectionResolved {
    selector: TaskSelector,
    catalogs: Vec<LoadedCatalog>,
    selected_catalog_index: usize,
}

impl DemoTaskSelectionResolved {
    fn selection(&self) -> Result<TaskSelection<'_>, RunnerError> {
        select_catalog_and_task(
            &self.selector,
            &self.catalogs,
            &self.catalogs[self.selected_catalog_index].catalog_root,
        )
        .map_err(Into::into)
    }

    pub(super) fn task(&self) -> Result<&ManifestTask, RunnerError> {
        self.selection().map(|selection| selection.task)
    }
}

pub(super) fn demo_task_selection(
    repo_root: &Path,
    task_name: &str,
) -> Result<Option<DemoTaskSelectionResolved>, RunnerError> {
    let catalogs = effigy_routing::discover_catalogs_allow_missing(repo_root)?;
    if catalogs.is_empty() {
        return Ok(None);
    }
    let selector = parse_task_selector(task_name).map_err(RunnerError::task_invocation)?;
    let selection = select_catalog_and_task(&selector, &catalogs, repo_root)?;
    let selected_catalog_index = catalogs
        .iter()
        .position(|catalog| {
            catalog.alias == selection.catalog.alias
                && catalog.catalog_root == selection.catalog.catalog_root
                && catalog.manifest_path == selection.catalog.manifest_path
        })
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "failed to re-identify selected task catalog for demo task `{task_name}`"
            ))
        })?;
    Ok(Some(DemoTaskSelectionResolved {
        selector,
        catalogs,
        selected_catalog_index,
    }))
}

fn execute_concurrent_runner_backed_demo(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    demo_mode: ManifestDemoMode,
    resolved: DemoTaskSelectionResolved,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let selection = resolved.selection()?;
    let runtime_args = TaskRuntimeArgs {
        repo_override: None,
        verbose_root: false,
        env_schema_override: None,
        passthrough: Vec::new(),
    };
    let plan = resolve_managed_task_plan(
        &resolved.selector,
        selection.catalog,
        selection.task,
        &runtime_args,
        &resolved.catalogs,
        &selection.catalog.catalog_root,
        &effigy_routing::resolve_task_selection,
    )?
    .ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "demo `{demo_id}` task `{task_name}` does not resolve to a managed concurrent runtime"
        ))
    })?;
    let managed_process_names = plan
        .processes
        .iter()
        .map(|process| process.name.clone())
        .collect::<Vec<_>>();
    let detached_interaction_projection = output_json;
    let browser_live_attach_supported =
        effigy_demo::concurrent_runner_supports_browser_live_attach(&managed_process_names);
    let input_target_process = if detached_interaction_projection || browser_live_attach_supported {
        concurrent_runner_input_target_process(&managed_process_names)
    } else {
        None
    };
    let input_handoff_path = input_target_process
        .as_ref()
        .map(|_| prepare_demo_input_handoff(repo_root, demo_id))
        .transpose()?;
    let resize_handoff_path = detached_interaction_projection
        .then(|| prepare_demo_resize_handoff(repo_root, demo_id))
        .transpose()?;
    let log_paths = DemoLogPaths::prepare_split(repo_root, demo_id)?;
    let initial_terminal_size = if demo_mode_prefers_attached_terminal(demo_mode) {
        current_terminal_size()
    } else {
        Some((DEMO_DEFAULT_TERMINAL_COLS, DEMO_DEFAULT_TERMINAL_ROWS))
    };
    let active_record = PersistedDemoActiveAttempt::new_concurrent_runner_backed(
        build_attempt_id(demo_id),
        demo_id,
        task_name,
        format!("<managed:{task_name} profile:{}>", plan.profile),
        browser_live_attach_supported,
        concurrent_runner_projection_shape(managed_process_names.len())
            .kind
            .clone(),
        plan.processes.len(),
        managed_process_names.clone(),
        concurrent_runner_projected_output_provenance(managed_process_names.len())
            .kind
            .clone(),
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

    run_concurrent_runner_demo_runtime(
        repo_root,
        demo_id,
        task_name,
        plan,
        log_paths,
        input_target_process,
        input_handoff_path,
        resize_handoff_path,
        output_json,
    )
}

fn run_concurrent_runner_demo_runtime(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    plan: effigy_managed::ManagedTaskPlan,
    log_paths: DemoLogPaths,
    input_target_process: Option<String>,
    input_handoff_path: Option<PathBuf>,
    resize_handoff_path: Option<PathBuf>,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let shutdown_on_exit_processes = plan
        .processes
        .iter()
        .filter(|process| process.shutdown_on_exit)
        .map(|process| process.name.clone())
        .collect();
    let specs = plan
        .processes
        .iter()
        .cloned()
        .map(|process| ProcessSpec {
            name: process.name,
            run: process.run,
            cwd: process.cwd,
            start_after_ms: process.start_after_ms,
            shutdown_on_exit: process.shutdown_on_exit,
            pty: true,
            env: BTreeMap::new(),
        })
        .collect::<Vec<_>>();
    let expected = specs.len();
    let supervisor = ProcessSupervisor::spawn(repo_root.to_path_buf(), specs)?;
    let input_forwarding_available = input_target_process.is_some();
    let mut state = DemoConcurrentRuntimeState::new(
        log_paths.stdout_absolute.as_deref(),
        log_paths.stderr_absolute.as_deref(),
        shutdown_on_exit_processes,
        input_target_process,
        input_handoff_path.clone(),
        !output_json,
    )?;
    let _stdin_handoff = if !output_json {
        input_handoff_path
            .as_ref()
            .filter(|_| input_forwarding_available)
            .map(|path| spawn_stdin_handoff_capture(path.clone()))
    } else {
        None
    };

    while state.exit_count < expected
        || state.drained_after_exit < DEMO_STREAM_DRAIN_POLLS_AFTER_EXIT
    {
        if !state.stop_requested && active_attempt_is_stop_requested(repo_root, demo_id) {
            state.stop_requested = true;
            supervisor.terminate_all();
        }
        if let Some((process, rendered, new_len)) = state.pending_input_chunk()? {
            supervisor.send_input(&process, &rendered)?;
            state.mark_input_forwarded(new_len);
        }
        if let Some(event) = supervisor
            .next_event_timeout(Duration::from_millis(DEMO_MANAGED_EVENT_POLL_INTERVAL_MS))
        {
            state.reset_drain_after_activity();
            match event.kind {
                ProcessEventKind::Stdout => state.record_stdout(&event.process, &event.payload)?,
                ProcessEventKind::Stderr => state.record_stderr(&event.process, &event.payload)?,
                ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {}
                ProcessEventKind::Exit => {
                    if state.record_exit(&event.process, &event.payload) {
                        supervisor.terminate_all();
                    }
                }
            }
        } else {
            state.record_idle_tick(expected);
        }
    }

    supervisor.terminate_all();
    if let Some(path) = input_handoff_path.as_deref() {
        let _ = fs::remove_file(path);
    }
    clear_resize_handoff(resize_handoff_path.as_deref());

    let command = format!("<managed:{task_name} profile:{}>", plan.profile);
    let summary = if state.stop_requested {
        format!(
            "Demo `{demo_id}` terminated after stop request while projecting managed task `{task_name}`."
        )
    } else if plan.fail_on_non_zero && !state.non_zero_exits.is_empty() {
        format!(
            "Demo `{demo_id}` failed via managed task `{task_name}`: {}",
            render_non_zero_exits(&state.non_zero_exits)
        )
    } else {
        format!(
            "Demo `{demo_id}` completed via managed task `{task_name}` profile `{}`.",
            plan.profile
        )
    };

    if state.stop_requested {
        Ok(terminated_demo_attempt(
            "task",
            task_name,
            &command,
            None,
            summary,
            state.stdout,
            state.stderr,
            log_paths,
        ))
    } else if plan.fail_on_non_zero && !state.non_zero_exits.is_empty() {
        Ok(failed_demo_attempt(
            "task",
            task_name,
            &command,
            None,
            summary,
            state.stdout,
            state.stderr,
            log_paths,
        ))
    } else {
        Ok(successful_demo_attempt(
            "task",
            task_name,
            &command,
            None,
            Some(summary),
            state.stdout,
            state.stderr,
            log_paths,
        ))
    }
}

pub(super) fn concurrent_runner_task_process_names(
    repo_root: &Path,
    task_name: &str,
) -> Option<Vec<String>> {
    let Ok(Some(resolved)) = demo_task_selection(repo_root, task_name) else {
        return None;
    };
    let Ok(selection) = resolved.selection() else {
        return None;
    };
    let runtime_args = TaskRuntimeArgs {
        repo_override: None,
        verbose_root: false,
        env_schema_override: None,
        passthrough: Vec::new(),
    };
    resolve_managed_task_plan(
        &resolved.selector,
        selection.catalog,
        selection.task,
        &runtime_args,
        &resolved.catalogs,
        &selection.catalog.catalog_root,
        &effigy_routing::resolve_task_selection,
    )
    .ok()
    .flatten()
    .map(|plan| {
        plan.processes
            .iter()
            .map(|process| process.name.clone())
            .collect()
    })
}

fn parse_task_backed_attempt_json(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    rendered: &str,
) -> Result<DemoExecutionAttempt, RunnerError> {
    demo_parse_task_backed_attempt_json(repo_root, demo_id, task_name, rendered).map_err(Into::into)
}

fn execute_run_backed_demo(
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

pub(super) fn request_demo_termination(target_pid: u32) -> Result<(), RunnerError> {
    #[cfg(unix)]
    {
        let raw = target_pid as i32;
        match signal::kill(Pid::from_raw(-raw), Signal::SIGTERM) {
            Ok(()) => Ok(()),
            Err(error) => Err(RunnerError::task_invocation(format!(
                "failed to send stop signal to demo process group `{target_pid}`: {error}"
            ))),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = target_pid;
        Err(RunnerError::task_invocation(
            "demo stop is not supported on this platform in the current runtime".to_owned(),
        ))
    }
}

pub(super) fn write_latest_attempt_receipt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    attempt: &DemoExecutionAttempt,
) -> Result<(), RunnerError> {
    persist_latest_demo_attempt_receipt(repo_root, demo_id, demo, attempt).map_err(Into::into)
}

#[cfg(unix)]
pub(super) fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let raw = pid as i32;
    match signal::kill(Pid::from_raw(raw), None) {
        Ok(()) => true,
        Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

#[cfg(not(unix))]
pub(super) fn pid_is_alive(pid: u32) -> bool {
    pid != 0
}
