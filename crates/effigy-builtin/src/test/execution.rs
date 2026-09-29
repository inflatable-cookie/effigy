use std::collections::{HashMap, VecDeque};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::{Arc, Mutex};

use effigy_core::shell::{shell_quote, with_local_node_bin_path};
use effigy_managed::run_spec::wrap_command_with_env;
use effigy_manifest::ManifestTestSuiteTeardownPolicy;
use effigy_process::ProcessSpec;
use effigy_tui::multiprocess::{run_multiprocess_tui, MultiProcessTuiOptions};

use super::planning::{BuiltinTargetRuntime, BuiltinTestRunnable};
use super::{BuiltinError, BuiltinTestExecResult};
use crate::BuiltinRuntimePorts;
#[path = "planning/runnable/cargo_env.rs"]
mod cargo_env;

pub(super) fn should_run_builtin_test_tui(force_tui: bool, suite_count: usize) -> bool {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
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
    let specs = runnable
        .iter()
        .map(|suite| {
            let execution_command = render_routed_suite_command(ports, suite)?;
            Ok(ProcessSpec {
                name: suite.name.clone(),
                run: lifecycle_execution_command_from_parts(
                    &execution_command,
                    suite.setup_command.as_deref(),
                    suite.teardown_command.as_deref(),
                    suite.teardown_policy,
                ),
                cwd: suite.root.clone(),
                start_after_ms: 0,
                shutdown_on_exit: false,
                pty: true,
                env: std::collections::BTreeMap::new(),
            })
        })
        .collect::<Result<Vec<ProcessSpec>, BuiltinError>>()?;
    let outcome = run_multiprocess_tui(
        ports.current_working_dir()?,
        specs,
        tab_order,
        MultiProcessTuiOptions {
            esc_quit_on_complete: true,
        },
    )
    .map_err(|error| BuiltinError::Ui(format!("builtin test tui runtime failed: {error}")))?;
    let failures = outcome
        .non_zero_exits
        .into_iter()
        .collect::<HashMap<String, String>>();

    Ok(runnable
        .into_iter()
        .map(|suite| {
            let diagnostic = failures.get(&suite.name);
            let code = diagnostic
                .and_then(|value| value.strip_prefix("exit="))
                .and_then(|value| value.parse::<i32>().ok());
            BuiltinTestExecResult {
                name: suite.name,
                runner: suite.runner,
                root: suite.root,
                command: suite.command,
                success: diagnostic.is_none(),
                code,
            }
        })
        .collect::<Vec<BuiltinTestExecResult>>())
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
    let jobs = runnable
        .into_iter()
        .map(|job| {
            let env_wrapped_command = wrap_command_with_env(
                cargo_env::maybe_wrap_with_cargo_env(
                    job.command.clone(),
                    &job.cargo_env,
                    job.cargo_env_match,
                    &job.root,
                ),
                &job.env,
                &job.root,
            );
            let routed_command = route_suite_command(
                ports,
                &job.runtime,
                job.nested_invocation,
                &job.root,
                &env_wrapped_command,
            )?;
            let execution_command = lifecycle_execution_command_from_parts(
                &routed_command,
                job.setup_command.as_deref(),
                job.teardown_command.as_deref(),
                job.teardown_policy,
            );
            Ok((
                job.name,
                job.root,
                job.runner,
                job.command,
                execution_command,
            ))
        })
        .collect::<Result<Vec<(String, PathBuf, String, String, String)>, BuiltinError>>()?;
    let worker_count = max_parallel.min(jobs.len()).max(1);
    let queue = Arc::new(Mutex::new(VecDeque::from(jobs)));

    std::thread::scope(
        |scope| -> Result<Vec<BuiltinTestExecResult>, BuiltinError> {
            let mut handles = Vec::with_capacity(worker_count);
            for _ in 0..worker_count {
                let queue_ref = Arc::clone(&queue);
                handles.push(scope.spawn(move || {
                    let mut local = Vec::<BuiltinTestExecResult>::new();
                    loop {
                        let job = {
                            let mut queue = queue_ref.lock().expect("test queue lock poisoned");
                            queue.pop_front()
                        };
                        let Some((name, root, runner, command, execution_command)) = job else {
                            break;
                        };
                        let mut process = ProcessCommand::new("sh");
                        process.arg("-c").arg(&execution_command).current_dir(&root);
                        with_local_node_bin_path(&mut process, &root);
                        if effigy_process::process_group_observer_active() {
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
                        effigy_process::notify_process_group_started(child_pid);
                        let status = if capture_output {
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
                        effigy_process::notify_process_group_stopped(child_pid);
                        let status = status?;
                        local.push(BuiltinTestExecResult {
                            name,
                            runner,
                            root,
                            command,
                            success: status.success(),
                            code: status.code(),
                        });
                    }
                    Ok::<Vec<BuiltinTestExecResult>, BuiltinError>(local)
                }));
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
    )
}

/// Route one rendered suite command to its owning catalog's declared
/// runtime target.
///
/// Host targets run the command unchanged. Container targets execute through
/// the runner's container machinery via the runtime ports; nested effigy
/// re-entries keep their own routing and are never wrapped again.
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

fn render_routed_suite_command(
    ports: &dyn BuiltinRuntimePorts,
    suite: &BuiltinTestRunnable,
) -> Result<String, BuiltinError> {
    let wrapped = render_wrapped_suite_command(suite);
    route_suite_command(
        ports,
        &suite.runtime,
        suite.nested_invocation,
        &suite.root,
        &wrapped,
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
