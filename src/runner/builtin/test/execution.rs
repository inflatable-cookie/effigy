use std::collections::{HashMap, VecDeque};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::sync::{Arc, Mutex};

use crate::process_manager::ProcessSpec;
use crate::runner::managed::run_spec::wrap_command_with_env;
use crate::runner::manifest::ManifestCargoEnvMatchMode;
use crate::runner::manifest::ManifestTestSuiteTeardownPolicy;
use crate::runner::util::shell_quote;
use crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions};

use super::super::super::command_context::current_working_dir;
use super::super::super::util::with_local_node_bin_path;
use super::planning::BuiltinTestRunnable;
use super::{BuiltinTestExecResult, RunnerError};
#[path = "planning/runnable/cargo_env.rs"]
mod cargo_env;

pub(super) fn should_run_builtin_test_tui(force_tui: bool, suite_count: usize) -> bool {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return false;
    }
    force_tui || suite_count > 1
}

pub(super) fn run_builtin_test_targets_tui(
    runnable: Vec<BuiltinTestRunnable>,
) -> Result<Vec<BuiltinTestExecResult>, RunnerError> {
    if runnable.is_empty() {
        return Ok(Vec::new());
    }
    let tab_order = runnable
        .iter()
        .map(|suite| suite.name.clone())
        .collect::<Vec<String>>();
    let specs = runnable
        .iter()
        .map(|suite| ProcessSpec {
            name: suite.name.clone(),
            run: lifecycle_execution_command(suite),
            cwd: suite.root.clone(),
            start_after_ms: 0,
            pty: true,
        })
        .collect::<Vec<ProcessSpec>>();
    let outcome = run_multiprocess_tui(
        current_working_dir()?,
        specs,
        tab_order,
        MultiProcessTuiOptions {
            esc_quit_on_complete: true,
        },
    )
    .map_err(|error| RunnerError::Ui(format!("builtin test tui runtime failed: {error}")))?;
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
    runnable: Vec<BuiltinTestRunnable>,
    max_parallel: usize,
    capture_output: bool,
) -> Result<Vec<BuiltinTestExecResult>, RunnerError> {
    if runnable.is_empty() {
        return Ok(Vec::new());
    }
    let jobs = runnable
        .into_iter()
        .map(|job| {
            (
                job.name,
                job.root,
                job.runner,
                job.command,
                job.cargo_env,
                job.cargo_env_match,
                job.env,
                job.setup_command,
                job.teardown_command,
                job.teardown_policy,
            )
        })
        .collect::<Vec<(
            String,
            PathBuf,
            String,
            String,
            std::collections::BTreeMap<String, String>,
            ManifestCargoEnvMatchMode,
            std::collections::BTreeMap<String, String>,
            Option<String>,
            Option<String>,
            ManifestTestSuiteTeardownPolicy,
        )>>();
    let worker_count = max_parallel.min(jobs.len()).max(1);
    let queue = Arc::new(Mutex::new(VecDeque::from(jobs)));

    std::thread::scope(|scope| -> Result<Vec<BuiltinTestExecResult>, RunnerError> {
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
                    let Some((
                        name,
                        root,
                        runner,
                        command,
                        cargo_env,
                        cargo_env_match,
                        env,
                        setup_command,
                        teardown_command,
                        teardown_policy,
                    )) = job
                    else {
                        break;
                    };
                    let execution_command = lifecycle_execution_command_from_parts(
                        &wrap_command_with_env(
                            cargo_env::maybe_wrap_with_cargo_env(
                                command.clone(),
                                &cargo_env,
                                cargo_env_match,
                                &root,
                            ),
                            &env,
                            &root,
                        ),
                        setup_command.as_deref(),
                        teardown_command.as_deref(),
                        teardown_policy,
                    );
                    let mut process = ProcessCommand::new("sh");
                    process
                        .arg("-lc")
                        .arg(&execution_command)
                        .current_dir(&root);
                    with_local_node_bin_path(&mut process, &root);
                    let status = if capture_output {
                        process
                            .output()
                            .map_err(|error| RunnerError::TaskCommandLaunch {
                                command: execution_command.clone(),
                                error,
                            })?
                            .status
                    } else {
                        process
                            .status()
                            .map_err(|error| RunnerError::TaskCommandLaunch {
                                command: execution_command.clone(),
                                error,
                            })?
                    };
                    local.push(BuiltinTestExecResult {
                        name,
                        runner,
                        root,
                        command,
                        success: status.success(),
                        code: status.code(),
                    });
                }
                Ok::<Vec<BuiltinTestExecResult>, RunnerError>(local)
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
    })
}

fn lifecycle_execution_command(suite: &BuiltinTestRunnable) -> String {
    lifecycle_execution_command_from_parts(
        &render_wrapped_suite_command(suite),
        suite.setup_command.as_deref(),
        suite.teardown_command.as_deref(),
        suite.teardown_policy,
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
        script.push_str(&format!("sh -lc {}\n", shell_quote(setup_command)));
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
    format!("sh -lc {}", shell_quote(&script))
}

fn render_primary_command_block(command: &str) -> String {
    let mut block = String::new();
    block.push_str(&format!("sh -lc {}\n", shell_quote(command)));
    block.push_str("command_status=$?\n");
    block.push_str("if [ \"$command_status\" -ne 0 ]; then\n");
    block.push_str("  status=$command_status\n");
    block.push_str("fi\n");
    block
}

fn render_teardown_block(teardown_command: &str, preserve_existing_failure: bool) -> String {
    let mut block = String::new();
    block.push_str(&format!("sh -lc {}\n", shell_quote(teardown_command)));
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
