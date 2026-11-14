//! The interactive session path: one PTY-backed tab per suite, and the one
//! place that decides what a session entry's state means for a suite.

use std::collections::HashMap;

use effigy_process::ProcessSpec;
use effigy_tui::multiprocess::{
    run_multiprocess_tui, run_multiprocess_tui_with_source, MultiProcessTuiControl,
    MultiProcessTuiOptions, SessionProcesses, SessionTab,
};

use super::command::{
    begin_sweep, finish_sweep, render_suite_execution_command, runs_on_host, suite_host_env,
};
use crate::test::planning::BuiltinTestRunnable;
use crate::test::{BuiltinError, BuiltinTestExecResult, BuiltinTestOutcome};
use crate::BuiltinRuntimePorts;

/// What one interactive session produced: every suite's result, and whether
/// the operator stopped the session before the suites finished. Both facts
/// belong to the run's outcome: a suite the stop never reached is not started,
/// and the run it was part of is cancelled rather than failed.
pub(in crate::test) struct BuiltinTestSession {
    pub(in crate::test) results: Vec<BuiltinTestExecResult>,
    pub(in crate::test) stopped_by_user: bool,
}

pub(in crate::test) fn run_builtin_test_targets_tui(
    ports: &dyn BuiltinRuntimePorts,
    runnable: Vec<BuiltinTestRunnable>,
) -> Result<BuiltinTestSession, BuiltinError> {
    if runnable.is_empty() {
        return Ok(BuiltinTestSession {
            results: Vec::new(),
            stopped_by_user: false,
        });
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
    Ok(BuiltinTestSession {
        results,
        stopped_by_user: outcome.stopped_by_user,
    })
}

/// One suite's result from the session state the TUI recorded for it.
///
/// The session reports what it observed; this is the only place that decides
/// what those facts mean for a suite. A suite the session ended is cancelled
/// (the suite never reported a result of its own), and a suite that never
/// started is not started; neither is a failure invented by the projection,
/// and neither is a pass.
pub(super) fn suite_result(
    suite: BuiltinTestRunnable,
    state: Option<&effigy_tui::multiprocess::SessionEntryState>,
) -> BuiltinTestExecResult {
    use effigy_tui::multiprocess::SessionEntryState;

    let (outcome, code) = match state {
        Some(SessionEntryState::Passed) => (BuiltinTestOutcome::Passed, Some(0)),
        // The session carries the exit the suite reported. A failure it could
        // not read a status for (a timeout, an unreadable wait) names none, and
        // the display record is never parsed back into a code.
        Some(SessionEntryState::Failed { exit, .. }) => (BuiltinTestOutcome::Failed, *exit),
        Some(SessionEntryState::Stopped { .. }) => (BuiltinTestOutcome::Cancelled, None),
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

pub(super) fn builtin_test_results_from_tui(
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
            suite_result(suite, state)
        })
        .collect()
}
