use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use effigy_core::shell::{shell_quote, with_local_node_bin_path};
use effigy_managed::run_spec::wrap_command_with_env;
use effigy_manifest::ManifestTestSuiteTeardownPolicy;
use effigy_process::ProcessSpec;

/// The explicit-effigy selector `effigy_core::effigy_invocation` reads first.
const EXECUTABLE_ENV: &str = "EFFIGY_EXECUTABLE";
use effigy_tui::multiprocess::{
    run_multiprocess_tui, run_multiprocess_tui_with_source, MultiProcessTuiControl,
    MultiProcessTuiOptions, SessionProcesses, SessionTab,
};

use super::planning::{BuiltinTargetRuntime, BuiltinTestRunnable};
use super::sweep::Sweep;
use super::{BuiltinError, BuiltinTestExecResult, BuiltinTestOutcome};
use crate::ports::BuiltinTestChildEvidence;
use crate::{BuiltinRuntimePorts, HostSuiteCustody};
#[path = "planning/runnable/cargo_env.rs"]
mod cargo_env;

static NEXT_TEST_INVOCATION_GENERATION: AtomicU64 = AtomicU64::new(1);
static NEXT_TEST_CHILD_GENERATION: AtomicU64 = AtomicU64::new(1);

pub(super) fn should_run_builtin_test_tui(
    force_tui: bool,
    suite_count: usize,
    output_json: bool,
) -> bool {
    // JSON output is a machine contract: the TUI would interleave terminal
    // control output with the payload on stdout.
    if output_json || !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return false;
    }
    force_tui || suite_count > 1
}

pub(super) fn run_builtin_test_targets_tui(
    ports: &dyn BuiltinRuntimePorts,
    runnable: Vec<BuiltinTestRunnable>,
) -> Result<Vec<BuiltinTestExecResult>, BuiltinError> {
    if runnable.is_empty() {
        return Ok(Vec::new());
    }
    let tab_order = runnable
        .iter()
        .map(|suite| suite.name.clone())
        .collect::<Vec<String>>();
    let mut host_env = suite_host_env(ports);
    let sweep = begin_sweep(&mut host_env, runnable.iter().any(runs_on_host));
    let specs = runnable
        .iter()
        .map(|suite| {
            let execution_command = render_suite_execution_command(ports, suite)?;
            let host = runs_on_host(suite);
            Ok(ProcessSpec {
                name: suite.name.clone(),
                run: match &sweep {
                    Some(sweep) if host => sweep.command(&execution_command),
                    _ => execution_command,
                },
                cwd: suite.root.clone(),
                start_after_ms: 0,
                shutdown_on_exit: false,
                pty: true,
                env: if host {
                    host_env.clone()
                } else {
                    std::collections::BTreeMap::new()
                },
            })
        })
        .collect::<Result<Vec<ProcessSpec>, BuiltinError>>()?;
    let options = MultiProcessTuiOptions {
        esc_quit_on_complete: true,
    };
    let outcome = if let Some(source) = ports.test_tui_source(specs.clone())? {
        let tabs = specs
            .iter()
            .map(|spec| SessionTab {
                name: spec.name.clone(),
                pty: spec.pty,
                shutdown_on_exit: spec.shutdown_on_exit,
            })
            .collect();
        let poll = |source: &dyn SessionProcesses| Ok(source.close_requested());
        let none = |_: &dyn SessionProcesses| Ok(());
        let control = MultiProcessTuiControl {
            poll: &poll,
            before_shutdown: &none,
            after_shutdown: &none,
        };
        run_multiprocess_tui_with_source(
            ports.current_working_dir()?,
            source,
            tabs,
            tab_order,
            options,
            Some(&control),
        )
    } else {
        run_multiprocess_tui(ports.current_working_dir()?, specs, tab_order, options)
    }
    .map_err(|error| BuiltinError::Ui(format!("builtin test tui runtime failed: {error}")))?;

    let results = builtin_test_results_from_tui(runnable, &outcome);
    finish_sweep(sweep)?;
    Ok(results)
}

/// One suite's result from the session state the TUI recorded for it.
///
/// The session reports what it observed; this is the only place that decides
/// what those facts mean for a suite. A suite the operator stopped is
/// cancelled and a suite that never started is not started; neither is a
/// failure invented by the projection, and neither is a pass.
fn suite_result(
    suite: BuiltinTestRunnable,
    state: Option<&effigy_tui::multiprocess::SessionEntryState>,
    stopped_by_user: bool,
) -> BuiltinTestExecResult {
    use effigy_tui::multiprocess::SessionEntryState;

    let (outcome, code) = match state {
        Some(SessionEntryState::Passed) => (BuiltinTestOutcome::Passed, Some(0)),
        Some(SessionEntryState::Failed { detail }) => {
            let code = detail
                .strip_prefix("exit=")
                .and_then(|value| value.parse::<i32>().ok());
            (BuiltinTestOutcome::Failed, code)
        }
        Some(SessionEntryState::Stopped { .. }) if stopped_by_user => {
            (BuiltinTestOutcome::Cancelled, None)
        }
        Some(SessionEntryState::Stopped { .. }) => (BuiltinTestOutcome::Failed, None),
        Some(SessionEntryState::NotStarted) | None => (BuiltinTestOutcome::NotStarted, None),
    };
    BuiltinTestExecResult {
        name: suite.name,
        runner: suite.runner,
        root: suite.root,
        command: suite.command,
        outcome,
        code,
    }
}

fn builtin_test_results_from_tui(
    runnable: Vec<BuiltinTestRunnable>,
    outcome: &effigy_tui::multiprocess::MultiProcessTuiOutcome,
) -> Vec<BuiltinTestExecResult> {
    let states = outcome
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), &entry.state))
        .collect::<HashMap<&str, &effigy_tui::multiprocess::SessionEntryState>>();
    runnable
        .into_iter()
        .map(|suite| {
            let state = states.get(suite.name.as_str()).copied();
            suite_result(suite, state, outcome.stopped_by_user)
        })
        .collect()
}

pub(super) fn run_builtin_test_targets_parallel(
    ports: &dyn BuiltinRuntimePorts,
    runnable: Vec<BuiltinTestRunnable>,
    max_parallel: usize,
    capture_output: bool,
) -> Result<Vec<BuiltinTestExecResult>, BuiltinError> {
    if runnable.is_empty() {
        return Ok(Vec::new());
    }
    let capture_child_evidence = ports.builtin_test_child_evidence_enabled();
    let invocation_generation = if capture_child_evidence {
        NEXT_TEST_INVOCATION_GENERATION.fetch_add(1, Ordering::Relaxed)
    } else {
        0
    };
    // The suites run on worker threads, which cannot see the caller's thread
    // scope: read the run's host environment here and carry it across.
    let mut host_env = suite_host_env(ports);
    let sweep = begin_sweep(&mut host_env, runnable.iter().any(runs_on_host));
    let jobs = runnable
        .into_iter()
        .map(|job| {
            let routed_command = render_suite_execution_command(ports, &job)?;
            let host = runs_on_host(&job);
            let execution_command = match &sweep {
                Some(sweep) if host => sweep.command(&routed_command),
                _ => routed_command,
            };
            // A suite the caller's run owns runs under that run's custody: one
            // child run per suite, moved to the worker that runs it, so the run
            // settles the suite's whole process group. Child evidence is the
            // built-in's own diagnostic spawn, so it keeps the plain path.
            // Remote suites still have a host control client. Its process
            // group and inherited runtime capability belong to this run too.
            let custody = if !capture_child_evidence {
                ports.host_suite_custody(&job.name)?
            } else {
                None
            };
            Ok((
                job.name,
                job.root,
                job.runner,
                job.command,
                execution_command,
                host,
                custody,
            ))
        })
        .collect::<Result<
            Vec<(
                String,
                PathBuf,
                String,
                String,
                String,
                bool,
                Option<Box<dyn HostSuiteCustody>>,
            )>,
            BuiltinError,
        >>()?;
    let worker_count = max_parallel.min(jobs.len()).max(1);
    let queue = Arc::new(Mutex::new(VecDeque::from(jobs)));

    let results = std::thread::scope(
        |scope| -> Result<Vec<BuiltinTestExecResult>, BuiltinError> {
            let (child_evidence_sender, child_evidence_receiver) = if capture_child_evidence {
                let (sender, receiver) = std::sync::mpsc::channel::<BuiltinTestChildEvidence>();
                (Some(sender), Some(receiver))
            } else {
                (None, None)
            };
            let mut handles = Vec::with_capacity(worker_count);
            for _ in 0..worker_count {
                let queue_ref = Arc::clone(&queue);
                let host_env = host_env.clone();
                let child_evidence_sender = child_evidence_sender.clone();
                handles.push(scope.spawn(move || {
                    let mut local = Vec::<BuiltinTestExecResult>::new();
                    loop {
                        let job = {
                            let mut queue = queue_ref.lock().expect("test queue lock poisoned");
                            queue.pop_front()
                        };
                        let Some((name, root, runner, command, execution_command, host, custody)) =
                            job
                        else {
                            break;
                        };
                        let child_generation = if capture_child_evidence {
                            NEXT_TEST_CHILD_GENERATION.fetch_add(1, Ordering::Relaxed)
                        } else {
                            0
                        };
                        if let Some(mut custody) = custody {
                            // The suite stays in this run's process group, so
                            // custody still settles it.
                            let outcome =
                                custody.run(&execution_command, &root, &host_env, capture_output);
                            let result = outcome.map(|outcome| {
                                #[cfg(unix)]
                                let status = {
                                    use std::os::unix::process::ExitStatusExt;
                                    std::process::ExitStatus::from_raw(
                                        outcome.code.unwrap_or(1) << 8,
                                    )
                                };
                                #[cfg(not(unix))]
                                let status = {
                                    let _ = outcome;
                                    unreachable!("built-in test custody is a unix process group")
                                };
                                status
                            });
                            let succeeded = matches!(&result, Ok(status) if status.success());
                            // The suite's outcome is recorded and its whole
                            // process group stopped here, before the run that
                            // owns the scratch directory concludes.
                            custody.settle(succeeded);
                            let status =
                                result.map_err(|error| BuiltinError::TaskCommandLaunch {
                                    command: execution_command.clone(),
                                    error: std::io::Error::other(error.to_string()),
                                })?;
                            local.push(BuiltinTestExecResult {
                                name,
                                runner,
                                root,
                                command,
                                outcome: if status.success() {
                                    BuiltinTestOutcome::Passed
                                } else {
                                    BuiltinTestOutcome::Failed
                                },
                                code: status.code(),
                            });
                            continue;
                        }
                        let mut process = ProcessCommand::new("sh");
                        process.arg("-c").arg(&execution_command).current_dir(&root);
                        with_local_node_bin_path(&mut process, &root);
                        if host {
                            for (key, value) in &host_env {
                                process.env(key, value);
                            }
                        }
                        let observer_active_at_spawn =
                            effigy_process::process_group_observer_active();
                        if observer_active_at_spawn {
                            #[cfg(unix)]
                            {
                                use std::os::unix::process::CommandExt;
                                process.process_group(0);
                            }
                        }
                        if capture_output {
                            process.stdout(Stdio::piped()).stderr(Stdio::piped());
                        }
                        let child =
                            process
                                .spawn()
                                .map_err(|error| BuiltinError::TaskCommandLaunch {
                                    command: execution_command.clone(),
                                    error,
                                })?;
                        let child_pid = child.id();
                        let process_group_result = if capture_child_evidence {
                            Some(effigy_process::process_group_id(child_pid))
                        } else {
                            None
                        };
                        let start_observer_notified =
                            effigy_process::notify_process_group_started(child_pid);
                        let wait_result = if capture_output {
                            child
                                .wait_with_output()
                                .map(|output| output.status)
                                .map_err(|error| BuiltinError::TaskCommandLaunch {
                                    command: execution_command.clone(),
                                    error,
                                })
                        } else {
                            let mut child = child;
                            child
                                .wait()
                                .map_err(|error| BuiltinError::TaskCommandLaunch {
                                    command: execution_command.clone(),
                                    error,
                                })
                        };
                        let stop_observer_notified =
                            effigy_process::notify_process_group_stopped(child_pid);
                        if capture_child_evidence {
                            let (process_group, process_group_error) = match process_group_result {
                                Some(Ok(process_group)) => (Some(process_group), None),
                                Some(Err(error)) => (None, Some(error.to_string())),
                                None => (None, None),
                            };
                            let exit_code = wait_result
                                .as_ref()
                                .ok()
                                .and_then(std::process::ExitStatus::code);
                            let signal = wait_result.as_ref().ok().and_then(|status| {
                                #[cfg(unix)]
                                {
                                    use std::os::unix::process::ExitStatusExt;
                                    status.signal()
                                }
                                #[cfg(not(unix))]
                                {
                                    let _ = status;
                                    None
                                }
                            });
                            let wait_error = wait_result.as_ref().err().map(ToString::to_string);
                            child_evidence_sender
                                .as_ref()
                                .expect("evidence capture creates its receiver")
                                .send(BuiltinTestChildEvidence {
                                    invocation_generation,
                                    child_generation,
                                    suite_name: name.clone(),
                                    root: root.display().to_string(),
                                    pid: child_pid,
                                    parent_pid: std::process::id(),
                                    process_group,
                                    process_group_error,
                                    observer_active_at_spawn,
                                    start_observer_notified,
                                    stop_observer_notified,
                                    spawned: true,
                                    exit_code,
                                    signal,
                                    wait_error,
                                })
                                .expect(
                                    "child evidence receiver remains active until workers finish",
                                );
                        }
                        let status = wait_result?;
                        local.push(BuiltinTestExecResult {
                            name,
                            runner,
                            root,
                            command,
                            outcome: if status.success() {
                                BuiltinTestOutcome::Passed
                            } else {
                                BuiltinTestOutcome::Failed
                            },
                            code: status.code(),
                        });
                    }
                    Ok::<Vec<BuiltinTestExecResult>, BuiltinError>(local)
                }));
            }
            drop(child_evidence_sender);
            if let Some(child_evidence_receiver) = child_evidence_receiver {
                for evidence in child_evidence_receiver {
                    ports.record_builtin_test_child_evidence(evidence);
                }
            }

            let mut combined = Vec::<BuiltinTestExecResult>::new();
            for handle in handles {
                let mut part = handle
                    .join()
                    .expect("builtin test worker thread panicked unexpectedly")?;
                combined.append(&mut part);
            }
            Ok(combined)
        },
    )?;
    finish_sweep(sweep)?;
    Ok(results)
}

/// Route one rendered suite command to its owning catalog's declared
/// runtime target.
///
/// The command is the suite's lifecycle (setup, primary, teardown). Host
/// targets run it unchanged. Container targets execute through the runner's
/// container machinery via the runtime ports; nested effigy re-entries keep
/// their own routing and are never wrapped again.
fn route_suite_command(
    ports: &dyn BuiltinRuntimePorts,
    runtime: &BuiltinTargetRuntime,
    nested_invocation: bool,
    root: &std::path::Path,
    command: &str,
) -> Result<String, BuiltinError> {
    let BuiltinTargetRuntime::Container { container } = runtime else {
        return Ok(command.to_owned());
    };
    if nested_invocation {
        return Ok(command.to_owned());
    }
    let target = ports.prepare_container_suite_target(root, container)?;
    ports.render_container_suite_command(&target, command)
}

/// The run's host environment, as the caller's run provides it.
fn host_scratch_env(ports: &dyn BuiltinRuntimePorts) -> std::collections::BTreeMap<String, String> {
    ports
        .host_scratch_env()
        .into_iter()
        .map(|(key, value)| (key, value.to_string_lossy().into_owned()))
        .collect()
}

/// Begin the one suite sweep a host run owns, or none when every suite runs
/// elsewhere. The token joins the suite environment so a descendant that keeps
/// its environment still names the suite.
fn begin_sweep(host_env: &mut BTreeMap<String, String>, host_suites: bool) -> Option<Sweep> {
    if !host_suites {
        return None;
    }
    let scratch = host_env.get("TMPDIR").map(PathBuf::from);
    let sweep = Sweep::begin(scratch.as_deref())?;
    let (key, value) = sweep.token_env();
    host_env.insert(key.to_owned(), value);
    Some(sweep)
}

/// Fail the route with every process that outlived its suites.
fn finish_sweep(sweep: Option<Sweep>) -> Result<(), BuiltinError> {
    let leaked = sweep.map(Sweep::end).unwrap_or_default();
    if leaked.is_empty() {
        return Ok(());
    }
    Err(BuiltinError::task_invocation(format!(
        "{} process(es) outlived the test suite: {}",
        leaked.len(),
        leaked.join("; ")
    )))
}

/// A host suite's environment: its scratch variables plus the invoking Effigy.
///
/// A task step that invokes Effigy falls back to `cargo run --bin effigy` from
/// a test harness, and every one of those builds can replace the uplifted
/// binary while another suite's process is still running. A suite shares the
/// stable binary that started the test run instead, so one build cannot swap
/// the binary under another suite. An operator's explicit value wins.
fn suite_host_env(ports: &dyn BuiltinRuntimePorts) -> std::collections::BTreeMap<String, String> {
    let mut env = host_scratch_env(ports);
    if let Some(executable) = suite_effigy() {
        env.insert(EXECUTABLE_ENV.to_owned(), executable);
    }
    env
}

/// The Effigy binary a suite's nested invocations use: the operator's
/// inherited choice, else this process's binary when it is not a library test
/// harness (which re-execs itself only for `__kernel-*` entries).
fn suite_effigy() -> Option<String> {
    if let Some(explicit) = std::env::var_os(EXECUTABLE_ENV) {
        if !explicit.is_empty() {
            return Some(explicit.to_string_lossy().into_owned());
        }
    }
    let executable = std::env::current_exe().ok()?;
    let is_test_harness = executable
        .parent()
        .and_then(std::path::Path::file_name)
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "deps");
    (!is_test_harness).then(|| executable.display().to_string())
}

/// Whether this suite runs as a host process of the calling run: a host-scoped
/// target that is not already a nested invocation inside someone else's scope.
fn runs_on_host(suite: &BuiltinTestRunnable) -> bool {
    matches!(suite.runtime, BuiltinTargetRuntime::Host) && !suite.nested_invocation
}

/// One suite command: env wrap, then setup/command/teardown, then container
/// routing. Lifecycle is assembled before routing so a named-container target
/// runs those hooks inside the selected container (CHANGELOG, guide 022).
fn render_suite_execution_command(
    ports: &dyn BuiltinRuntimePorts,
    suite: &BuiltinTestRunnable,
) -> Result<String, BuiltinError> {
    let wrapped = render_wrapped_suite_command(suite);
    let lifecycle = lifecycle_execution_command_from_parts(
        &wrapped,
        suite.setup_command.as_deref(),
        suite.teardown_command.as_deref(),
        suite.teardown_policy,
    );
    route_suite_command(
        ports,
        &suite.runtime,
        suite.nested_invocation,
        &suite.root,
        &lifecycle,
    )
}

fn lifecycle_execution_command_from_parts(
    command: &str,
    setup_command: Option<&str>,
    teardown_command: Option<&str>,
    teardown_policy: ManifestTestSuiteTeardownPolicy,
) -> String {
    if setup_command.is_none() && teardown_command.is_none() {
        return command.to_owned();
    }

    let mut script = String::from("status=0\n");

    if let Some(setup_command) = setup_command {
        script.push_str(&format!("sh -c {}\n", shell_quote(setup_command)));
        script.push_str("setup_status=$?\n");
        script.push_str("if [ \"$setup_status\" -ne 0 ]; then\n");
        script.push_str("  status=$setup_status\n");
        script.push_str("else\n");
        script.push_str(&render_primary_command_block(command));
        script.push_str("fi\n");
    } else {
        script.push_str(&render_primary_command_block(command));
    }

    match (teardown_command, teardown_policy) {
        (Some(teardown_command), ManifestTestSuiteTeardownPolicy::Always) => {
            script.push_str(&render_teardown_block(teardown_command, true));
        }
        (Some(teardown_command), ManifestTestSuiteTeardownPolicy::OnSuccess) => {
            script.push_str("if [ \"$status\" -eq 0 ]; then\n");
            script.push_str(&indent_block(&render_teardown_block(
                teardown_command,
                false,
            )));
            script.push_str("fi\n");
        }
        (None, _) => {}
    }

    script.push_str("exit \"$status\"");
    format!("sh -c {}", shell_quote(&script))
}

fn render_primary_command_block(command: &str) -> String {
    let mut block = String::new();
    block.push_str(&format!("sh -c {}\n", shell_quote(command)));
    block.push_str("command_status=$?\n");
    block.push_str("if [ \"$command_status\" -ne 0 ]; then\n");
    block.push_str("  status=$command_status\n");
    block.push_str("fi\n");
    block
}

fn render_teardown_block(teardown_command: &str, preserve_existing_failure: bool) -> String {
    let mut block = String::new();
    block.push_str(&format!("sh -c {}\n", shell_quote(teardown_command)));
    block.push_str("teardown_status=$?\n");
    block.push_str("if [ \"$teardown_status\" -ne 0 ]; then\n");
    if preserve_existing_failure {
        block.push_str("  if [ \"$status\" -eq 0 ]; then\n");
        block.push_str("    status=$teardown_status\n");
        block.push_str("  fi\n");
    } else {
        block.push_str("  status=$teardown_status\n");
    }
    block.push_str("fi\n");
    block
}

fn indent_block(block: &str) -> String {
    let mut indented = String::new();
    for line in block.lines() {
        indented.push_str("  ");
        indented.push_str(line);
        indented.push('\n');
    }
    indented
}

fn render_wrapped_suite_command(suite: &BuiltinTestRunnable) -> String {
    wrap_command_with_env(
        cargo_env::maybe_wrap_with_cargo_env(
            suite.command.clone(),
            &suite.cargo_env,
            suite.cargo_env_match,
            &suite.root,
        ),
        &suite.env,
        &suite.root,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuiltinContainerSuiteTarget, BuiltinLockGuards, LockScope};
    use effigy_cli::{Command, DoctorArgs, TaskInvocation, TasksArgs};
    use effigy_manifest::{LoadedCatalog, ManifestCargoEnvMatchMode};
    use std::collections::{BTreeMap, BTreeSet};
    use std::ffi::OsString;
    use std::path::Path;

    /// A ports double that answers the scratch question, and can hand out a
    /// custody double that records the suite it was given.
    struct ScratchPorts {
        env: Vec<(String, OsString)>,
        custody_marker: Option<PathBuf>,
        /// `(container, lifecycle command)` pairs handed to container
        /// routing. Empty unless a non-nested container suite actually
        /// routed.
        routed_commands: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl BuiltinRuntimePorts for ScratchPorts {
        fn host_scratch_env(&self) -> Vec<(String, OsString)> {
            self.env.clone()
        }

        fn custody_holds_host_suites(&self) -> bool {
            self.custody_marker.is_some()
        }

        fn host_suite_custody(
            &self,
            label: &str,
        ) -> Result<Option<Box<dyn HostSuiteCustody>>, BuiltinError> {
            let Some(marker) = &self.custody_marker else {
                return Ok(None);
            };
            Ok(Some(Box::new(RecordingCustody {
                marker: marker.clone(),
                label: label.to_owned(),
            })))
        }

        fn acquire_scopes(
            &self,
            _workspace_root: &Path,
            _scopes: &[LockScope],
        ) -> Result<BuiltinLockGuards, BuiltinError> {
            Ok(BuiltinLockGuards::new(()))
        }

        fn current_working_dir(&self) -> Result<PathBuf, BuiltinError> {
            std::env::current_dir()
                .map_err(|error| BuiltinError::task_invocation(error.to_string()))
        }

        fn run_manifest_task_with_cwd(
            &self,
            _task: &TaskInvocation,
            _cwd: PathBuf,
        ) -> Result<String, BuiltinError> {
            unreachable!("the execution proof runs suites directly")
        }

        fn run_doctor(&self, _args: DoctorArgs) -> Result<String, BuiltinError> {
            unreachable!("the execution proof runs suites directly")
        }

        fn run_tasks(&self, _args: TasksArgs) -> Result<String, BuiltinError> {
            unreachable!("the execution proof runs suites directly")
        }

        fn run_command(&self, _command: Command) -> Result<String, BuiltinError> {
            unreachable!("the execution proof runs suites directly")
        }

        fn deferred_builtins_from_catalogs(
            &self,
            _catalogs: &[LoadedCatalog],
            _resolved_root: &Path,
        ) -> BTreeSet<String> {
            BTreeSet::new()
        }

        fn prepare_container_suite_target(
            &self,
            target_root: &Path,
            container: &str,
        ) -> Result<BuiltinContainerSuiteTarget, BuiltinError> {
            Ok(BuiltinContainerSuiteTarget {
                container: container.to_owned(),
                service: "app".to_owned(),
                root: target_root.to_path_buf(),
            })
        }

        fn render_container_suite_command(
            &self,
            target: &BuiltinContainerSuiteTarget,
            suite_command: &str,
        ) -> Result<String, BuiltinError> {
            // The observation is which container the lifecycle was rendered
            // for and which command was routed; printf still has to run as a
            // host script.
            self.routed_commands
                .lock()
                .expect("routed command lock")
                .push((target.container.clone(), suite_command.to_owned()));
            Ok(suite_command.to_owned())
        }
    }

    fn suite(root: &Path, command: &str, runtime: BuiltinTargetRuntime) -> BuiltinTestRunnable {
        BuiltinTestRunnable {
            name: "proof".to_owned(),
            runner: "proof".to_owned(),
            root: root.to_path_buf(),
            command: command.to_owned(),
            auto_workspace_scope: false,
            cargo_env: BTreeMap::new(),
            cargo_env_match: ManifestCargoEnvMatchMode::default(),
            env: BTreeMap::new(),
            setup_command: None,
            teardown_command: None,
            teardown_policy: ManifestTestSuiteTeardownPolicy::Always,
            is_default: true,
            runtime,
            nested_invocation: false,
        }
    }

    /// Records the suite it is given and reports the code the suite's own
    /// script would have produced, without spawning: the dispatch proof.
    struct RecordingCustody {
        marker: PathBuf,
        label: String,
    }

    impl HostSuiteCustody for RecordingCustody {
        fn run(
            &mut self,
            command: &str,
            cwd: &Path,
            env: &BTreeMap<String, String>,
            _capture: bool,
        ) -> Result<crate::HostSuiteOutcome, BuiltinError> {
            // Every scratch variable by name, then whatever else the custody
            // environment carries: a count alone cannot say which variable
            // appeared, and the invoking Effigy is added when the harness has
            // one (EFFIGY_EXECUTABLE, set by an outer Effigy run).
            let scratch = ["TMPDIR", "TMP", "TEMP"]
                .iter()
                .map(|key| env.get(*key).cloned().unwrap_or_default())
                .collect::<Vec<_>>();
            let extra = env
                .keys()
                .filter(|key| !["TMPDIR", "TMP", "TEMP"].contains(&key.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            // Unit separator: a suite command can contain `|` (the sweep
            // prologue is `exec 9>>marker || exit 1`) and newlines.
            std::fs::write(
                &self.marker,
                format!(
                    "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
                    self.label,
                    command,
                    cwd.display(),
                    scratch[0],
                    scratch[1],
                    scratch[2],
                    extra.join(",")
                ),
            )
            .expect("custody marker");
            Ok(crate::HostSuiteOutcome { code: Some(0) })
        }

        fn settle(self: Box<Self>, _succeeded: bool) {}
    }

    fn ports(dir: &Path) -> ScratchPorts {
        ScratchPorts {
            env: ["TMPDIR", "TMP", "TEMP"]
                .iter()
                .map(|key| ((*key).to_owned(), dir.as_os_str().to_owned()))
                .collect(),
            custody_marker: None,
            routed_commands: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[test]
    fn a_host_suite_runs_in_the_caller_runs_scratch_directory() {
        let fixture = tempfile::tempdir().expect("fixture");
        let scratch = tempfile::tempdir().expect("scratch");
        let seen = fixture.path().join("tmpdir.txt");
        let command = format!(
            "printf '%s' \"$TMPDIR\" > {}",
            shell_quote(&seen.display().to_string())
        );
        let runnable = suite(fixture.path(), &command, BuiltinTargetRuntime::Host);

        let results =
            run_builtin_test_targets_parallel(&ports(scratch.path()), vec![runnable], 1, true)
                .expect("suite ran");
        assert!(results[0].success(), "{:?}", results[0]);
        assert_eq!(
            std::fs::read_to_string(&seen).expect("suite output"),
            scratch.path().display().to_string(),
            "a host suite sees the run's scratch directory as TMPDIR"
        );
    }

    #[test]
    fn a_host_suite_is_dispatched_through_the_callers_run_custody() {
        let fixture = tempfile::tempdir().expect("fixture");
        let scratch = tempfile::tempdir().expect("scratch");
        let marker = fixture.path().join("custody.txt");
        let runnable = suite(fixture.path(), "printf suite", BuiltinTargetRuntime::Host);
        let mut ports = ports(scratch.path());
        ports.custody_marker = Some(marker.clone());

        let results =
            run_builtin_test_targets_parallel(&ports, vec![runnable], 1, true).expect("suite ran");
        assert!(results[0].success(), "{:?}", results[0]);
        let recorded = std::fs::read_to_string(&marker).expect("custody marker");
        let fields = recorded.split('\u{1f}').collect::<Vec<_>>();
        assert_eq!(fields.len(), 7, "custody marker fields: {recorded}");
        assert_eq!(fields[0], "proof", "the suite reached its own custody");
        // A host sweep prefixes `exec 9>>marker || exit 1\n`; the suite text
        // stays. Sweep::begin can also return none, leaving the command bare.
        assert!(
            fields[1] == "printf suite" || fields[1].ends_with("\nprintf suite"),
            "the suite runs its own command: {}",
            fields[1]
        );
        assert_eq!(
            &fields[3..6],
            [
                scratch.path().display().to_string(),
                scratch.path().display().to_string(),
                scratch.path().display().to_string(),
            ],
            "custody ran the suite with the run's temp environment: {recorded}"
        );
        // Scratch plus the invoking Effigy, plus the suite sweep token when
        // a marker could be created.
        assert!(
            fields[6].split(',').all(|key| {
                key.is_empty() || key == "EFFIGY_EXECUTABLE" || key == "EFFIGY_TEST_SUITE"
            }),
            "no variable beyond the run's temp environment, the invoking Effigy, and the suite sweep token: {recorded}"
        );
    }

    #[test]
    fn a_nested_or_container_suite_never_receives_the_host_scratch() {
        let fixture = tempfile::tempdir().expect("fixture");
        let scratch = tempfile::tempdir().expect("scratch");
        let seen = fixture.path().join("tmpdir.txt");
        let command = format!(
            "printf '%s' \"$TMPDIR\" > {}",
            shell_quote(&seen.display().to_string())
        );
        // A nested invocation renders its container command itself and runs
        // inside the surrounding scope, so the outer host scratch stays out.
        let mut runnable = suite(
            fixture.path(),
            &command,
            BuiltinTargetRuntime::Container {
                container: "web".to_owned(),
            },
        );
        runnable.nested_invocation = true;

        let results =
            run_builtin_test_targets_parallel(&ports(scratch.path()), vec![runnable], 1, true)
                .expect("suite ran");
        assert!(results[0].success(), "{:?}", results[0]);
        assert_ne!(
            std::fs::read_to_string(&seen).expect("suite output"),
            scratch.path().display().to_string(),
            "a container suite keeps the container's own temp directory"
        );
    }

    #[test]
    fn a_container_suite_routes_setup_and_teardown_inside_the_selected_container() {
        let fixture = tempfile::tempdir().expect("fixture");
        let scratch = tempfile::tempdir().expect("scratch");
        let mut runnable = suite(
            fixture.path(),
            "printf primary",
            BuiltinTargetRuntime::Container {
                container: "web".to_owned(),
            },
        );
        runnable.setup_command = Some("printf setup".to_owned());
        runnable.teardown_command = Some("printf teardown".to_owned());

        let ports = ports(scratch.path());
        let results =
            run_builtin_test_targets_parallel(&ports, vec![runnable], 1, true).expect("suite ran");
        assert!(results[0].success(), "{:?}", results[0]);

        let routed = ports.routed_commands.lock().expect("routed lock").clone();
        assert_eq!(
            routed.len(),
            1,
            "one container route for the suite: {routed:?}"
        );
        let (container, command) = &routed[0];
        assert_eq!(
            container, "web",
            "every hook is rendered for the selected container: {routed:?}"
        );
        assert!(
            command.contains("printf setup"),
            "setup must be inside the container exec, got {command}"
        );
        assert!(
            command.contains("printf primary"),
            "primary must be inside the container exec, got {command}"
        );
        assert!(
            command.contains("printf teardown"),
            "teardown must be inside the container exec, got {command}"
        );
    }

    #[test]
    fn a_nested_container_suite_does_not_wrap_lifecycle_into_the_outer_container() {
        let fixture = tempfile::tempdir().expect("fixture");
        let scratch = tempfile::tempdir().expect("scratch");
        let mut runnable = suite(
            fixture.path(),
            "printf nested",
            BuiltinTargetRuntime::Container {
                container: "web".to_owned(),
            },
        );
        runnable.nested_invocation = true;
        runnable.setup_command = Some("printf setup".to_owned());

        let ports = ports(scratch.path());
        let results =
            run_builtin_test_targets_parallel(&ports, vec![runnable], 1, true).expect("suite ran");
        assert!(results[0].success(), "{:?}", results[0]);
        assert!(
            ports
                .routed_commands
                .lock()
                .expect("routed lock")
                .is_empty(),
            "a nested re-entry keeps its own routing"
        );
    }

    #[test]
    fn a_cancelled_suite_does_not_project_success() {
        let fixture = tempfile::tempdir().expect("fixture");
        let scratch = tempfile::tempdir().expect("scratch");
        let runnable = suite(fixture.path(), "exit 130", BuiltinTargetRuntime::Host);

        let results =
            run_builtin_test_targets_parallel(&ports(scratch.path()), vec![runnable], 1, true)
                .expect("suite recorded");
        assert!(
            !results[0].success(),
            "exit 130 must not project success: {:?}",
            results[0]
        );
        assert_eq!(results[0].code, Some(130));
    }

    #[test]
    fn a_host_suite_never_calls_container_routing() {
        let fixture = tempfile::tempdir().expect("fixture");
        let scratch = tempfile::tempdir().expect("scratch");
        let mut runnable = suite(fixture.path(), "printf host", BuiltinTargetRuntime::Host);
        runnable.setup_command = Some("printf setup".to_owned());

        let ports = ports(scratch.path());
        let results =
            run_builtin_test_targets_parallel(&ports, vec![runnable], 1, true).expect("suite ran");
        assert!(results[0].success(), "{:?}", results[0]);
        assert!(
            ports
                .routed_commands
                .lock()
                .expect("routed lock")
                .is_empty(),
            "a host suite must not reach container routing"
        );
    }

    use effigy_tui::multiprocess::{MultiProcessTuiOutcome, SessionEntry, SessionEntryState};

    fn named_suite(root: &Path, name: &str) -> BuiltinTestRunnable {
        let mut runnable = suite(root, "printf suite", BuiltinTargetRuntime::Host);
        runnable.name = name.to_owned();
        runnable
    }

    fn tui_outcome(
        entries: Vec<(&str, SessionEntryState)>,
        stopped_by_user: bool,
    ) -> MultiProcessTuiOutcome {
        MultiProcessTuiOutcome {
            entries: entries
                .into_iter()
                .map(|(name, state)| SessionEntry {
                    name: name.to_owned(),
                    state,
                })
                .collect(),
            stopped_by_user,
        }
    }

    /// The interactive projection: the session's recorded states decide every
    /// suite's result, and only a suite that ran and passed is a success.
    #[test]
    fn interactive_session_states_project_to_suite_outcomes() {
        let fixture = tempfile::tempdir().expect("fixture");
        let stopped = SessionEntryState::Stopped {
            detail: "signal=15".to_owned(),
        };
        let cases = [
            (
                "stopped before the first member started",
                vec![
                    ("one", SessionEntryState::NotStarted),
                    ("two", SessionEntryState::NotStarted),
                ],
                true,
                vec![
                    BuiltinTestOutcome::NotStarted,
                    BuiltinTestOutcome::NotStarted,
                ],
            ),
            (
                "stopped after one member passed",
                vec![("one", SessionEntryState::Passed), ("two", stopped.clone())],
                true,
                vec![BuiltinTestOutcome::Passed, BuiltinTestOutcome::Cancelled],
            ),
            (
                "no member started",
                vec![
                    ("one", SessionEntryState::NotStarted),
                    ("two", SessionEntryState::NotStarted),
                ],
                false,
                vec![
                    BuiltinTestOutcome::NotStarted,
                    BuiltinTestOutcome::NotStarted,
                ],
            ),
            (
                "every member passed",
                vec![
                    ("one", SessionEntryState::Passed),
                    ("two", SessionEntryState::Passed),
                ],
                true,
                vec![BuiltinTestOutcome::Passed, BuiltinTestOutcome::Passed],
            ),
            (
                "one member failed",
                vec![
                    ("one", SessionEntryState::Passed),
                    (
                        "two",
                        SessionEntryState::Failed {
                            detail: "exit=7".to_owned(),
                        },
                    ),
                ],
                false,
                vec![BuiltinTestOutcome::Passed, BuiltinTestOutcome::Failed],
            ),
        ];

        for (case, entries, stopped_by_user, expected) in cases {
            let runnable = vec![
                named_suite(fixture.path(), "one"),
                named_suite(fixture.path(), "two"),
            ];
            let outcome = tui_outcome(entries, stopped_by_user);
            let results = builtin_test_results_from_tui(runnable, &outcome);
            assert_eq!(
                results
                    .iter()
                    .map(|result| result.outcome)
                    .collect::<Vec<BuiltinTestOutcome>>(),
                expected,
                "{case}"
            );
            assert_eq!(
                results.iter().filter(|result| result.success()).count(),
                expected.iter().filter(|outcome| outcome.success()).count(),
                "only a passed suite is a success: {case}"
            );
        }
    }

    #[test]
    fn a_suite_the_session_never_supervised_never_reports_success() {
        let fixture = tempfile::tempdir().expect("fixture");
        let runnable = vec![named_suite(fixture.path(), "one")];
        let outcome = tui_outcome(Vec::new(), true);

        let results = builtin_test_results_from_tui(runnable, &outcome);

        assert_eq!(results[0].outcome, BuiltinTestOutcome::NotStarted);
        assert!(!results[0].success());
    }

    /// Text, JSON and the task's own outcome agree on a stopped run: the
    /// passed suite stays a pass, the stopped suite is cancelled, and the run
    /// carries the interrupted exit, never a success exit.
    #[test]
    fn a_stopped_interactive_run_projects_one_outcome_everywhere() {
        let fixture = tempfile::tempdir().expect("fixture");
        let runnable = vec![
            named_suite(fixture.path(), "one"),
            named_suite(fixture.path(), "two"),
        ];
        let outcome = tui_outcome(
            vec![
                ("one", SessionEntryState::Passed),
                (
                    "two",
                    SessionEntryState::Stopped {
                        detail: "signal=15".to_owned(),
                    },
                ),
            ],
            true,
        );
        let results = builtin_test_results_from_tui(runnable, &outcome);

        let text = cancelled_run(&results, false);
        assert!(
            text.contains("ok") && text.contains("cancelled"),
            "text keeps the pass and names the cancelled suite: {text}"
        );
        assert!(
            !text.contains("two  ok") && !text.contains("two ok"),
            "a stopped suite is never rendered as ok: {text}"
        );

        let payload: serde_json::Value =
            serde_json::from_str(&cancelled_run(&results, true)).expect("cancelled run JSON");
        assert_eq!(payload["schema"], "effigy.test.results.v1");
        let targets = payload["targets"].as_array().expect("targets");
        assert_eq!(targets[0]["outcome"], "passed");
        assert_eq!(targets[1]["outcome"], "cancelled");
        assert_eq!(targets[1]["success"], serde_json::Value::Bool(false));
        assert_eq!(targets[1]["code"], serde_json::Value::Null);
        assert_eq!(payload["failures"][0]["target"], "two");
    }

    /// Finalize the stopped run and return the rendered payload: the same
    /// projection every front end reads, proving the cancellation reaches the
    /// text and the machine contract from one place.
    fn cancelled_run(results: &[BuiltinTestExecResult], output_json: bool) -> String {
        let error = crate::test::render::finalize_builtin_test_outcome(
            results,
            &[],
            None,
            &[],
            false,
            output_json,
        )
        .expect_err("a cancelled run is not a success");
        match error {
            BuiltinError::BuiltinTestNonZero {
                cancelled,
                failures,
                rendered,
            } => {
                assert!(cancelled, "the run reports cancellation");
                assert_eq!(failures, vec![("two".to_owned(), None)]);
                rendered
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn an_unstarted_suite_keeps_the_ordinary_failure_exit() {
        let fixture = tempfile::tempdir().expect("fixture");
        let runnable = vec![named_suite(fixture.path(), "one")];
        let outcome = tui_outcome(vec![("one", SessionEntryState::NotStarted)], false);
        let results = builtin_test_results_from_tui(runnable, &outcome);

        let error = crate::test::render::finalize_builtin_test_outcome(
            &results,
            &[],
            None,
            &[],
            false,
            false,
        )
        .expect_err("an unstarted suite proves nothing");
        match error {
            BuiltinError::BuiltinTestNonZero { cancelled, .. } => {
                assert!(!cancelled, "an unstarted suite is not an operator stop")
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
