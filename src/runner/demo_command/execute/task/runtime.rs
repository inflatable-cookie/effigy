use super::*;

pub(super) fn execute_concurrent_runner_backed_demo(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    demo_mode: ManifestDemoMode,
    resolved: DemoTaskSelectionResolved,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let selection = resolved.selection()?;
    let plan = selection::resolve_concurrent_runner_plan(&resolved, selection, demo_id, task_name)?;
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
