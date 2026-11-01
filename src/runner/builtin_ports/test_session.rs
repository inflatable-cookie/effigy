//! The test TUI borrows its invoking run. It owns no separate supervisor or
//! journal: each attempt crosses that run's kernel spawn barrier and is
//! settled before a restart or before the borrowed run can close.
use std::cell::{RefCell, RefMut};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use effigy_builtin::BuiltinError;
use effigy_kernel::custody::{Lifetime, OwnedProcess, ProcessSpec as KernelSpec, Terminal};
use effigy_kernel::observe::Observed;
use effigy_process::{
    ProcessEvent, ProcessEventKind, ProcessManagerError, ProcessSpec, ShutdownProgress,
};
use effigy_tui::multiprocess::{SessionEntryState, SessionProcesses};

use crate::runner::task_engine::{self, EnvOverride, TaskRun};

const OUTPUT_LIMIT: usize = 8 << 20;
const CLEANUP: Duration = Duration::from_secs(30);

pub(super) fn source<'p, 'r>(
    parent: &'p RefCell<&'r mut TaskRun>,
    specs: Vec<ProcessSpec>,
) -> Result<Box<dyn SessionProcesses + 'p>, BuiltinError> {
    let run = parent
        .try_borrow_mut()
        .map_err(|_| BuiltinError::task_invocation("test custody is already borrowed"))?;
    let mut entries = BTreeMap::new();
    let mut pending = VecDeque::new();
    for spec in specs {
        if spec.start_after_ms != 0 || entries.contains_key(&spec.name) {
            return Err(BuiltinError::task_invocation(
                "test TUI requires unique suites without a start delay",
            ));
        }
        pending.push_back(spec.name.clone());
        entries.insert(
            spec.name.clone(),
            Entry {
                spec,
                process: None,
                exit: None,
            },
        );
    }
    Ok(Box::new(Source {
        state: RefCell::new(State {
            run,
            entries,
            pending,
            events: VecDeque::new(),
            output_bytes: 0,
            cancelled: false,
            startup_error: None,
            forced: BTreeSet::new(),
        }),
    }))
}

struct Entry {
    spec: ProcessSpec,
    process: Option<OwnedProcess>,
    exit: Option<EntryExit>,
}

/// How one attempt ended, as this session observed it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EntryExit {
    /// The suite itself reported this exit status.
    Reported(String),
    /// The session ended the suite: it never reported its own status, so the
    /// attempt is a cancellation rather than a failure.
    Stopped,
}

impl EntryExit {
    fn record(&self) -> &str {
        match self {
            Self::Reported(record) => record,
            Self::Stopped => "stopped by the session",
        }
    }

    fn state(&self) -> SessionEntryState {
        match self {
            Self::Reported(record) => SessionEntryState::from_record(record),
            Self::Stopped => SessionEntryState::Stopped {
                detail: "stopped by the session".into(),
            },
        }
    }
}
struct State<'p, 'r> {
    run: RefMut<'p, &'r mut TaskRun>,
    entries: BTreeMap<String, Entry>,
    pending: VecDeque<String>,
    events: VecDeque<ProcessEvent>,
    output_bytes: usize,
    cancelled: bool,
    startup_error: Option<ProcessManagerError>,
    forced: BTreeSet<String>,
}
struct Source<'p, 'r> {
    state: RefCell<State<'p, 'r>>,
}

fn control(
    name: &str,
    operation: &'static str,
    reason: impl std::fmt::Display,
) -> ProcessManagerError {
    ProcessManagerError::Control {
        process: name.into(),
        operation,
        reason: reason.to_string(),
    }
}
fn event(
    name: &str,
    kind: ProcessEventKind,
    payload: String,
    chunk: Option<Vec<u8>>,
) -> ProcessEvent {
    ProcessEvent {
        process: name.into(),
        kind,
        payload,
        chunk,
    }
}

impl State<'_, '_> {
    fn push(&mut self, event: ProcessEvent) {
        self.output_bytes += event.chunk.as_ref().map_or(0, Vec::len);
        self.events.push_back(event);
        while self.output_bytes > OUTPUT_LIMIT {
            let Some(index) = self.events.iter().position(|event| event.chunk.is_some()) else {
                break;
            };
            if let Some(event) = self.events.remove(index) {
                self.output_bytes -= event.chunk.as_ref().map_or(0, Vec::len);
            }
        }
    }
    fn start(&mut self, name: &str) -> Result<(), ProcessManagerError> {
        let spec = &self.entries[name].spec;
        let env = task_engine::process_environment(
            &spec.cwd,
            spec.env
                .iter()
                .map(|(key, value)| (key.clone(), EnvOverride::Plain(value.clone())))
                .collect(),
        )
        .map_err(|error| control(name, "prepare", error))?;
        let command = if spec.pty {
            match crossterm::terminal::size() {
                Ok((cols, rows)) if cols > 0 && rows > 0 => {
                    format!("stty cols {cols} rows {rows} >/dev/null 2>&1; {}", spec.run)
                }
                _ => spec.run.clone(),
            }
        } else {
            spec.run.clone()
        };
        let spec = KernelSpec {
            argv: vec!["/bin/sh".into(), "-c".into(), command.into()],
            cwd: spec.cwd.clone(),
            env,
            grace: Duration::from_secs(3),
            lifetime: task_engine::remaining_lifetime().map_or(
                Lifetime::Open(effigy_kernel::ctx::OpenReason::Shell),
                Lifetime::Finite,
            ),
            terminal: if spec.pty {
                Terminal::Pty
            } else {
                Terminal::Inherit { capture: true }
            },
            label: name.into(),
        };
        self.push(event(name, ProcessEventKind::Starting, String::new(), None));
        // An operator may retry a failed attempt. Its recorded exit remains
        // evidence; the final suite verdict comes from the latest attempt.
        let process = self
            .run
            .spawn(spec, true)
            .map_err(|error| control(name, "start", error))?;
        let entry = self.entries.get_mut(name).expect("queued suite exists");
        entry.process = Some(process);
        entry.exit = None;
        self.push(event(name, ProcessEventKind::Started, String::new(), None));
        Ok(())
    }
    fn pump(&mut self) {
        if !self.cancelled
            && (self.run.verify_admission().is_err()
                || task_engine::pending_stop("test TUI").is_some())
        {
            self.stop_all();
        }
        if !self.cancelled {
            if let Some(name) = self.pending.pop_front() {
                if let Err(error) = self.start(&name) {
                    self.entries.get_mut(&name).expect("queued suite").exit =
                        Some(EntryExit::Reported("not started".into()));
                    self.push(event(
                        &name,
                        ProcessEventKind::StartupFailed,
                        error.to_string(),
                        None,
                    ));
                    self.startup_error = Some(error);
                    self.stop_all();
                }
            }
        }
        let mut events = Vec::new();
        for (name, entry) in &mut self.entries {
            let Some(process) = entry.process.as_mut() else {
                continue;
            };
            let (stdout, stderr) = process.take_stream_output();
            for (kind, bytes) in [
                (ProcessEventKind::StdoutChunk, stdout),
                (ProcessEventKind::StderrChunk, stderr),
            ] {
                if !bytes.is_empty() {
                    events.push(event(name, kind, String::new(), Some(bytes)));
                }
            }
            if entry.exit.is_none() {
                if process.expired() {
                    // Stop uses an independent cleanup context, even after a
                    // workload deadline or the invoking terminal is lost.
                    process.stop(&task_engine::cleanup_context(CLEANUP));
                    entry.exit = Some(EntryExit::Reported("timed out".into()));
                } else if let Observed::Verified(exit, _) = process.poll() {
                    entry.exit = Some(EntryExit::Reported(
                        match self.run.complete(process, exit) {
                            Ok(()) => format!("exit={}", exit.code),
                            Err(error) => format!("unrecorded exit: {error}"),
                        },
                    ));
                }
                if let Some(exit) = &entry.exit {
                    events.push(event(
                        name,
                        ProcessEventKind::Exit,
                        exit.record().to_owned(),
                        None,
                    ));
                }
            }
        }
        for event in events {
            self.push(event);
        }
    }
    fn stop_one(&mut self, name: &str) -> Result<(), ProcessManagerError> {
        self.stop_with(name, &task_engine::cleanup_context(CLEANUP))
    }
    /// Preserve an already-completed attempt before delivering any stop signal.
    fn stop_with(
        &mut self,
        name: &str,
        cleanup: &effigy_kernel::ctx::CleanupContext,
    ) -> Result<(), ProcessManagerError> {
        let entry =
            self.entries
                .get_mut(name)
                .ok_or_else(|| ProcessManagerError::ProcessNotFound {
                    process: name.into(),
                })?;
        if let Some(process) = entry.process.as_mut() {
            if entry.exit.is_none() {
                if let Observed::Verified(exit, _) = process.poll() {
                    entry.exit = Some(EntryExit::Reported(
                        match self.run.complete(process, exit) {
                            Ok(()) => format!("exit={}", exit.code),
                            Err(error) => format!("unrecorded exit: {error}"),
                        },
                    ));
                }
            }
            let report = process.stop(cleanup);
            if report
                .signals
                .iter()
                .any(|sent| sent.signal == libc::SIGKILL)
            {
                self.forced.insert(name.to_owned());
            }
            if entry.exit.is_none() {
                // Contract 052: a process a stop ended reports the interrupted
                // convention `128 + signal`. Only such an exit is this stop's
                // doing; any other exit status is the suite's own result, so a
                // completed failure is never relabelled as a cancellation.

                entry.exit = Some(match report.leader {
                    Observed::Verified(exit, _) => match self.run.complete(process, exit) {
                        Ok(()) if exit.code == 0 => {
                            EntryExit::Reported(format!("exit={}", exit.code))
                        }
                        Ok(())
                            if report
                                .signals
                                .iter()
                                .any(|sent| exit.code == 128 + sent.signal) =>
                        {
                            EntryExit::Stopped
                        }
                        Ok(()) => EntryExit::Reported(format!("exit={}", exit.code)),
                        Err(error) => EntryExit::Reported(format!("unrecorded exit: {error}")),
                    },
                    _ => EntryExit::Reported("unknown exit".into()),
                });
            }
            if !matches!(report.group, Observed::Verified(..) | Observed::Absent(_))
                || !report.residue.is_empty()
            {
                return Err(control(
                    name,
                    "settle",
                    format!(
                        "group closure: {:?}; residue: {:?}",
                        report.group, report.residue
                    ),
                ));
            }
        } else if entry.exit.is_none() {
            entry.exit = Some(EntryExit::Reported("not started".into()));
        }
        let exit = entry
            .exit
            .clone()
            .unwrap_or_else(|| EntryExit::Reported("unknown exit".into()));
        self.push(event(
            name,
            ProcessEventKind::Exit,
            exit.record().to_owned(),
            None,
        ));
        self.pending.retain(|pending| pending != name);
        Ok(())
    }
    fn stop_all(&mut self) {
        self.cancelled = true;
        self.pending.clear();
        let names = self.entries.keys().cloned().collect::<Vec<_>>();
        let cleanup = task_engine::cleanup_context(CLEANUP);
        for name in names {
            if let Err(error) = self.stop_with(&name, &cleanup) {
                self.entries.get_mut(&name).expect("known suite").exit =
                    Some(EntryExit::Reported(error.to_string()));
            }
        }
    }
}

impl SessionProcesses for Source<'_, '_> {
    fn close_requested(&self) -> bool {
        self.state.borrow().cancelled
    }
    fn next_event_timeout(&self, timeout: Duration) -> Option<ProcessEvent> {
        let until = Instant::now() + timeout;
        loop {
            {
                let mut state = self.state.borrow_mut();
                state.pump();
                if let Some(event) = state.events.pop_front() {
                    state.output_bytes -= event.chunk.as_ref().map_or(0, Vec::len);
                    return Some(event);
                }
            }
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            std::thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }
    fn send_input(&self, name: &str, input: &str) -> Result<(), ProcessManagerError> {
        let mut state = self.state.borrow_mut();
        if state.cancelled || task_engine::pending_stop("test TUI input").is_some() {
            return Err(control(name, "write input", "session is closing"));
        }
        state
            .run
            .verify_admission()
            .map_err(|error| control(name, "write input", error))?;
        let process = state
            .entries
            .get(name)
            .and_then(|entry| entry.process.as_ref())
            .ok_or_else(|| ProcessManagerError::ProcessNotFound {
                process: name.into(),
            })?;
        process
            .write_input(input.as_bytes())
            .map_err(|error| ProcessManagerError::InputWrite {
                process: name.into(),
                error,
            })
    }
    fn terminate_process(&self, name: &str) -> Result<(), ProcessManagerError> {
        self.state.borrow_mut().stop_one(name)
    }
    fn restart_process(&self, name: &str) -> Result<(), ProcessManagerError> {
        let mut state = self.state.borrow_mut();
        if state.cancelled {
            return Err(control(name, "restart", "session is closing"));
        }
        state
            .run
            .verify_admission()
            .map_err(|error| control(name, "restart", error))?;
        state.stop_one(name)?;
        let entry = state.entries.get_mut(name).expect("settled suite exists");
        entry.process = None;
        entry.exit = None;
        state.pending.push_back(name.into());
        Ok(())
    }
    fn startup_finished(&self) -> bool {
        self.state.borrow().pending.is_empty()
    }
    fn cancel_startup(&self) {
        self.state.borrow_mut().stop_all();
    }
    fn take_startup_error(&self) -> Option<ProcessManagerError> {
        self.state.borrow_mut().startup_error.take()
    }
    fn entry_states(&self) -> Vec<(String, SessionEntryState)> {
        self.state
            .borrow()
            .entries
            .iter()
            .map(|(name, entry)| {
                let state = match &entry.exit {
                    // The session never started this attempt, so it has no
                    // result to report.
                    None => SessionEntryState::NotStarted,
                    Some(exit) => exit.state(),
                };
                (name.clone(), state)
            })
            .collect()
    }
    fn terminate_all_graceful_with_progress(
        &self,
        _timeout: Duration,
        on_progress: &mut dyn FnMut(ShutdownProgress),
    ) {
        on_progress(ShutdownProgress::SendingTerm);
        on_progress(ShutdownProgress::Waiting);
        let mut state = self.state.borrow_mut();
        state.stop_all();
        if !state.forced.is_empty() {
            on_progress(ShutdownProgress::ForceKilling);
        }
        on_progress(ShutdownProgress::Complete {
            total: state
                .entries
                .values()
                .filter(|entry| entry.process.is_some())
                .count(),
            forced: state.forced.len(),
        });
    }
}
impl Drop for Source<'_, '_> {
    fn drop(&mut self) {
        self.state.get_mut().stop_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_kernel::outcome::WorkloadOutcome;

    fn spec(root: &std::path::Path, command: &str) -> ProcessSpec {
        ProcessSpec {
            name: "suite".into(),
            run: command.into(),
            cwd: root.into(),
            start_after_ms: 0,
            shutdown_on_exit: false,
            pty: true,
            env: BTreeMap::new(),
        }
    }
    fn wait(source: &dyn SessionProcesses, mut predicate: impl FnMut(&ProcessEvent) -> bool) {
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            if let Some(event) = source.next_event_timeout(Duration::from_millis(20)) {
                if predicate(&event) {
                    return;
                }
            }
        }
        panic!(
            "suite event not observed; entry states: {:?}",
            source.entry_states()
        );
    }
    #[test]
    fn input_exit_and_retry_use_the_same_memory_run_without_a_database() {
        let fixture = tempfile::tempdir().expect("private checkout");
        let mut run =
            TaskRun::standalone_foreground(fixture.path(), "effigy test").expect("memory owner");
        {
            let parent = RefCell::new(&mut run);
            let source = source(
                &parent,
                vec![spec(
                    fixture.path(),
                    "printf 'READY\\n'; read answer; printf '%s' \"$answer\" > answer",
                )],
            )
            .expect("source");
            wait(source.as_ref(), |event| {
                event
                    .chunk
                    .as_ref()
                    .is_some_and(|bytes| bytes.windows(5).any(|part| part == b"READY"))
            });
            source.send_input("suite", "first\n").expect("PTY input");
            wait(source.as_ref(), |event| {
                event.kind == ProcessEventKind::Exit
            });
            assert_eq!(
                source.entry_states(),
                vec![("suite".into(), SessionEntryState::Passed)]
            );
            assert_eq!(
                std::fs::read_to_string(fixture.path().join("answer")).expect("answer"),
                "first"
            );
            source
                .terminate_process("suite")
                .expect("completed group is closed");
            source
                .terminate_process("suite")
                .expect("empty group remains positively closed");
            source
                .restart_process("suite")
                .expect("verified old group permits retry");
            wait(source.as_ref(), |event| {
                event.kind == ProcessEventKind::Started
            });
            source.send_input("suite", "second\n").expect("retry input");
            wait(source.as_ref(), |event| {
                event.kind == ProcessEventKind::Exit
            });
            assert_eq!(
                source.entry_states(),
                vec![("suite".into(), SessionEntryState::Passed)]
            );
            assert_eq!(
                std::fs::read_to_string(fixture.path().join("answer")).expect("answer"),
                "second"
            );
        }
        assert!(run.close(WorkloadOutcome::Succeeded).is_empty());
        assert!(!fixture.path().join(".effigy/state/kernel.db").exists());
    }
    #[test]
    fn stopping_an_unfinished_suite_never_reports_success() {
        let fixture = tempfile::tempdir().expect("private checkout");
        let mut run =
            TaskRun::standalone_foreground(fixture.path(), "effigy test").expect("memory owner");
        {
            let parent = RefCell::new(&mut run);
            let source = source(
                &parent,
                vec![spec(fixture.path(), "printf 'READY\\n'; exec sleep 60")],
            )
            .expect("source");
            wait(source.as_ref(), |event| {
                event.kind == ProcessEventKind::Started
            });
            source.terminate_process("suite").expect("verified stop");
            // The session ended this suite itself. It is a stop the caller
            // reads as a cancellation, never a pass and never a failure the
            // suite reported.
            assert_eq!(
                source.entry_states(),
                vec![(
                    "suite".into(),
                    SessionEntryState::Stopped {
                        detail: "stopped by the session".into()
                    }
                )]
            );
        }
        assert!(run.close(WorkloadOutcome::Failed { code: 1 }).is_empty());
    }

    fn named_spec(root: &std::path::Path, name: &str, command: &str) -> ProcessSpec {
        ProcessSpec {
            name: name.into(),
            run: command.into(),
            cwd: root.into(),
            start_after_ms: 0,
            shutdown_on_exit: false,
            pty: true,
            env: BTreeMap::new(),
        }
    }

    /// The production interactive stop: a suite that finished keeps its own
    /// pass and the suite the session ended is a stop. These are the states
    /// the builtin projects to `passed` / `cancelled`.
    #[test]
    fn a_production_session_stop_keeps_a_pass_and_records_the_stop() {
        let fixture = tempfile::tempdir().expect("private checkout");
        let mut run =
            TaskRun::standalone_foreground(fixture.path(), "effigy test").expect("memory owner");
        {
            let parent = RefCell::new(&mut run);
            let source = source(
                &parent,
                vec![
                    named_spec(fixture.path(), "finished", "printf 'READY\\n'; exit 0"),
                    named_spec(
                        fixture.path(),
                        "stopped",
                        "printf 'READY\\n'; exec sleep 60",
                    ),
                ],
            )
            .expect("source");
            // The custody source starts one suite per pump turn.
            let mut finished = false;
            for expected in ["finished", "stopped"] {
                wait(source.as_ref(), |event| {
                    finished |= event.kind == ProcessEventKind::Exit && event.process == "finished";
                    event.kind == ProcessEventKind::Started && event.process == expected
                });
            }
            if !finished {
                wait(source.as_ref(), |event| {
                    event.kind == ProcessEventKind::Exit && event.process == "finished"
                });
            }
            source.cancel_startup();
            assert_eq!(
                source.entry_states(),
                vec![
                    ("finished".into(), SessionEntryState::Passed),
                    (
                        "stopped".into(),
                        SessionEntryState::Stopped {
                            detail: "stopped by the session".into()
                        }
                    ),
                ]
            );
        }
        assert!(run
            .close(WorkloadOutcome::Cancelled {
                cause: effigy_kernel::ctx::StopCause::Requested,
                signal: None,
            })
            .is_empty());
    }

    /// A suite that reported its own failure is a failed check even when the
    /// session's stop arrives before the session observed that exit: the stop
    /// never produced the exit, so the result keeps its own code.
    #[test]
    fn a_suite_that_reported_its_own_failure_keeps_the_failure_across_a_stop() {
        let fixture = tempfile::tempdir().expect("private checkout");
        let mut run =
            TaskRun::standalone_foreground(fixture.path(), "effigy test").expect("memory owner");
        {
            let parent = RefCell::new(&mut run);
            let source = source(
                &parent,
                vec![named_spec(
                    fixture.path(),
                    "failed",
                    "touch failed.done; exit 143",
                )],
            )
            .expect("source");
            wait(source.as_ref(), |event| {
                event.kind == ProcessEventKind::Started
            });
            // The suite finishes on its own; the session is not pumped, so the
            // stop is its first observation of the attempt.
            let done = fixture.path().join("failed.done");
            let deadline = Instant::now() + Duration::from_secs(10);
            while !done.exists() {
                assert!(Instant::now() < deadline, "suite never finished");
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(250));
            source.cancel_startup();
            assert_eq!(
                source.entry_states(),
                vec![(
                    "failed".into(),
                    SessionEntryState::Failed {
                        detail: "exit=143".into()
                    }
                )],
                "a completed failure is not a cancellation"
            );
        }
        assert!(run.close(WorkloadOutcome::Failed { code: 143 }).is_empty());
    }

    /// A stop before the session started an attempt leaves it never started,
    /// not failed and not passed.
    #[test]
    fn a_production_session_that_never_starts_an_attempt_reports_not_started() {
        let fixture = tempfile::tempdir().expect("private checkout");
        let mut run =
            TaskRun::standalone_foreground(fixture.path(), "effigy test").expect("memory owner");
        {
            let parent = RefCell::new(&mut run);
            let source = source(
                &parent,
                vec![named_spec(fixture.path(), "queued", "exec sleep 60")],
            )
            .expect("source");
            source.cancel_startup();
            assert_eq!(
                source.entry_states(),
                vec![("queued".into(), SessionEntryState::NotStarted)]
            );
        }
        assert!(run
            .close(WorkloadOutcome::Cancelled {
                cause: effigy_kernel::ctx::StopCause::Requested,
                signal: None,
            })
            .is_empty());
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    use effigy_kernel::outcome::WorkloadOutcome;

    #[test]
    fn custody_cannot_be_borrowed_twice_and_cancelled_input_is_refused() {
        let fixture = tempfile::tempdir().expect("private checkout");
        let mut run =
            TaskRun::standalone_foreground(fixture.path(), "effigy test").expect("memory owner");
        {
            let parent = RefCell::new(&mut run);
            let view = source(&parent, Vec::new()).expect("first view");
            assert!(
                source(&parent, Vec::new()).is_err(),
                "a second view must not panic"
            );
            view.cancel_startup();
            assert!(matches!(
                view.send_input("suite", "forbidden\n"),
                Err(ProcessManagerError::Control { .. })
            ));
        }
        assert!(run.close(WorkloadOutcome::Succeeded).is_empty());
    }
}
