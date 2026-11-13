//! The parallel runner: a bounded worker pool, the caller's per-suite custody,
//! and the child evidence a test-only caller can collect.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use effigy_core::shell::with_local_node_bin_path;

use super::command::{
    begin_sweep, finish_sweep, render_suite_execution_command, runs_on_host, suite_host_env,
};
use crate::ports::BuiltinTestChildEvidence;
use crate::test::planning::BuiltinTestRunnable;
use crate::test::{BuiltinError, BuiltinTestExecResult, BuiltinTestOutcome};
use crate::{BuiltinRuntimePorts, HostSuiteCustody};

static NEXT_TEST_INVOCATION_GENERATION: AtomicU64 = AtomicU64::new(1);
static NEXT_TEST_CHILD_GENERATION: AtomicU64 = AtomicU64::new(1);

/// One suite's terminal status before the run that owns it says whether the
/// run's stop ended it. Workers record only what they observed.
struct SuiteStatus {
    name: String,
    runner: String,
    root: PathBuf,
    command: String,
    success: bool,
    code: Option<i32>,
    /// The run stopped this suite: it never reported a result of its own.
    stopped: bool,
}

/// One suite's result from the status the process reported and the per-suite
/// stop fact the run that owns the suite recorded.
///
/// A cancellation is the run's fact, never a suite's exit status: a suite that
/// reports `130` or `143` on its own is a failure. Only a suite the run's stop
/// ended is cancelled.
pub(super) fn suite_outcome(success: bool, stopped: bool) -> BuiltinTestOutcome {
    if success {
        BuiltinTestOutcome::Passed
    } else if stopped {
        BuiltinTestOutcome::Cancelled
    } else {
        BuiltinTestOutcome::Failed
    }
}

pub(in crate::test) fn run_builtin_test_targets_parallel(
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

    let statuses = std::thread::scope(|scope| -> Result<Vec<SuiteStatus>, BuiltinError> {
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
                let mut local = Vec::<SuiteStatus>::new();
                loop {
                    let job = {
                        let mut queue = queue_ref.lock().expect("test queue lock poisoned");
                        queue.pop_front()
                    };
                    let Some((name, root, runner, command, execution_command, host, custody)) = job
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
                        let suite =
                            match custody.run(&execution_command, &root, &host_env, capture_output)
                            {
                                Ok(suite) => suite,
                                Err(error) => {
                                    // The attempt never reported: close its run as
                                    // a failed attempt before the outer run
                                    // concludes.
                                    custody.settle(&crate::HostSuiteOutcome {
                                        code: None,
                                        stopped: false,
                                    });
                                    return Err(BuiltinError::TaskCommandLaunch {
                                        command: execution_command.clone(),
                                        error: std::io::Error::other(error.to_string()),
                                    });
                                }
                            };
                        // The attempt's own outcome is recorded and its whole
                        // process group stopped here, before the run that owns
                        // the scratch directory concludes. The record is what
                        // the run observed: a stop stays a cancellation and a
                        // failure keeps its own code.
                        custody.settle(&suite);
                        local.push(SuiteStatus {
                            name,
                            runner,
                            root,
                            command,
                            success: suite.code == Some(0),
                            code: suite.code,
                            stopped: suite.stopped,
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
                                spawned: true,
                                exit_code,
                                signal,
                                wait_error,
                            })
                            .expect("child evidence receiver remains active until workers finish");
                    }
                    let status = wait_result?;
                    // These suites are the built-in's own children, not the
                    // run's: nothing the run records can say a stop ended one,
                    // so its status is its own result.
                    local.push(SuiteStatus {
                        name,
                        runner,
                        root,
                        command,
                        success: status.success(),
                        code: status.code(),
                        stopped: false,
                    });
                }
                Ok::<Vec<SuiteStatus>, BuiltinError>(local)
            }));
        }
        drop(child_evidence_sender);
        if let Some(child_evidence_receiver) = child_evidence_receiver {
            for evidence in child_evidence_receiver {
                ports.record_builtin_test_child_evidence(evidence);
            }
        }

        let mut combined = Vec::<SuiteStatus>::new();
        for handle in handles {
            let mut part = handle
                .join()
                .expect("builtin test worker thread panicked unexpectedly")?;
            combined.append(&mut part);
        }
        Ok(combined)
    })?;
    finish_sweep(sweep)?;
    // Every suite is classified by its own status and the stop fact the run
    // that owned it recorded: a suite's exit status is never evidence that the
    // operator cancelled the run.
    Ok(statuses
        .into_iter()
        .map(|status| BuiltinTestExecResult {
            outcome: suite_outcome(status.success, status.stopped),
            name: status.name,
            runner: status.runner,
            root: status.root,
            command: status.command,
            code: status.code,
        })
        .collect())
}
