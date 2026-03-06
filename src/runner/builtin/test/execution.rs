use std::collections::{HashMap, VecDeque};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::sync::{Arc, Mutex};

use crate::process_manager::ProcessSpec;
use crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions};

use super::super::super::command_context::current_working_dir;
use super::super::super::util::with_local_node_bin_path;
use super::planning::BuiltinTestRunnable;
use super::{BuiltinTestExecResult, RunnerError};

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
            run: suite.command.clone(),
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
        .map(|job| (job.name, job.root, job.runner, job.command))
        .collect::<Vec<(String, PathBuf, String, String)>>();
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
                    let Some((name, root, runner, command)) = job else {
                        break;
                    };
                    let mut process = ProcessCommand::new("sh");
                    process.arg("-lc").arg(&command).current_dir(&root);
                    with_local_node_bin_path(&mut process, &root);
                    let status = if capture_output {
                        process
                            .output()
                            .map_err(|error| RunnerError::TaskCommandLaunch {
                                command: command.clone(),
                                error,
                            })?
                            .status
                    } else {
                        process
                            .status()
                            .map_err(|error| RunnerError::TaskCommandLaunch {
                                command: command.clone(),
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
