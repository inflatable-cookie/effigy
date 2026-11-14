//! The execution proofs: real host suites, a custody double, a container route
//! double and the session-state projection.

use super::*;
use crate::test::planning::{BuiltinTargetRuntime, BuiltinTestRunnable};
use crate::test::{BuiltinError, BuiltinTestExecResult, BuiltinTestOutcome};
use crate::{
    BuiltinContainerSuiteTarget, BuiltinLockGuards, BuiltinRuntimePorts, HostSuiteCustody,
    LockScope,
};
use effigy_cli::{Command, DoctorArgs, TaskInvocation, TasksArgs};
use effigy_core::shell::shell_quote;
use effigy_manifest::{LoadedCatalog, ManifestCargoEnvMatchMode, ManifestTestSuiteTeardownPolicy};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A ports double that answers the scratch question, and can hand out a
/// custody double that records the suite it was given.
struct ScratchPorts {
    env: Vec<(String, OsString)>,
    custody_marker: Option<PathBuf>,
    /// What the custody double reports the owning run recorded for the
    /// attempt it ran.
    custody_outcome: crate::HostSuiteOutcome,
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
            outcome: self.custody_outcome.clone(),
        })))
    }

    fn acquire_scopes(
        &self,
        _workspace_root: &Path,
        scopes: &[LockScope],
    ) -> Result<BuiltinLockGuards, BuiltinError> {
        Ok(BuiltinLockGuards::new(scopes.to_vec(), ()))
    }

    fn current_working_dir(&self) -> Result<PathBuf, BuiltinError> {
        std::env::current_dir().map_err(|error| BuiltinError::task_invocation(error.to_string()))
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
    /// What the run that owns the suite records for this attempt.
    outcome: crate::HostSuiteOutcome,
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
        Ok(self.outcome.clone())
    }

    fn settle(self: Box<Self>, _outcome: &crate::HostSuiteOutcome) {}
}

fn ports(dir: &Path) -> ScratchPorts {
    ScratchPorts {
        env: ["TMPDIR", "TMP", "TEMP"]
            .iter()
            .map(|key| ((*key).to_owned(), dir.as_os_str().to_owned()))
            .collect(),
        custody_marker: None,
        custody_outcome: crate::HostSuiteOutcome {
            code: Some(0),
            stopped: false,
        },
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

/// A cancellation needs the per-suite stop fact the owning run recorded.
/// A suite's own exit status — even `130` or `143` — is never one.
#[test]
fn a_noninteractive_suite_outcome_needs_the_runs_own_stop_record() {
    let cases = [
        (true, false, BuiltinTestOutcome::Passed),
        (true, true, BuiltinTestOutcome::Passed),
        (false, false, BuiltinTestOutcome::Failed),
        (false, true, BuiltinTestOutcome::Cancelled),
    ];

    for (success, stopped, expected) in cases {
        assert_eq!(
            suite_outcome(success, stopped),
            expected,
            "status {success:?} with stopped={stopped}"
        );
    }
}

/// A run that owns the suite records the per-suite stop fact: a suite it
/// stopped is cancelled, while a suite it did not stop keeps its own
/// status even when that status looks like the interrupted convention.
#[test]
fn a_custody_suite_reports_the_stop_the_owning_run_recorded() {
    let fixture = tempfile::tempdir().expect("fixture");
    let scratch = tempfile::tempdir().expect("scratch");

    let stopped = ports_with_custody(
        scratch.path(),
        fixture.path().join("stopped.marker"),
        crate::HostSuiteOutcome {
            code: Some(143),
            stopped: true,
        },
    );
    let runnable = suite(fixture.path(), "printf suite", BuiltinTargetRuntime::Host);
    let results = run_builtin_test_targets_parallel(&stopped, vec![runnable], 1, true)
        .expect("suite recorded");
    assert_eq!(results[0].outcome, BuiltinTestOutcome::Cancelled);
    assert!(!results[0].success(), "a cancelled suite is not a pass");

    let failed = ports_with_custody(
        scratch.path(),
        fixture.path().join("failed.marker"),
        crate::HostSuiteOutcome {
            code: Some(130),
            stopped: false,
        },
    );
    let runnable = suite(fixture.path(), "printf suite", BuiltinTargetRuntime::Host);
    let results = run_builtin_test_targets_parallel(&failed, vec![runnable], 1, true)
        .expect("suite recorded");
    assert_eq!(results[0].outcome, BuiltinTestOutcome::Failed);
    assert_eq!(results[0].code, Some(130));

    let passed = ports_with_custody(
        scratch.path(),
        fixture.path().join("passed.marker"),
        crate::HostSuiteOutcome {
            code: Some(0),
            stopped: false,
        },
    );
    let runnable = suite(fixture.path(), "printf suite", BuiltinTargetRuntime::Host);
    let results = run_builtin_test_targets_parallel(&passed, vec![runnable], 1, true)
        .expect("suite recorded");
    assert_eq!(results[0].outcome, BuiltinTestOutcome::Passed);
    assert!(results[0].success());
}

fn ports_with_custody(
    scratch: &Path,
    marker: PathBuf,
    outcome: crate::HostSuiteOutcome,
) -> ScratchPorts {
    let mut ports = ports(scratch);
    ports.custody_marker = Some(marker);
    ports.custody_outcome = outcome;
    ports
}

/// A suite the built-in spawns itself is not the run's child: nothing the
/// run records can say a stop ended it, so a signal death is its own
/// failure rather than a cancellation.
#[test]
fn a_signal_death_of_the_builtins_own_child_is_a_failure() {
    let fixture = tempfile::tempdir().expect("fixture");
    let scratch = tempfile::tempdir().expect("scratch");
    // SIGKILL cannot be caught: the suite dies from the signal itself.
    let runnable = suite(fixture.path(), "kill -KILL $$", BuiltinTargetRuntime::Host);

    let results =
        run_builtin_test_targets_parallel(&ports(scratch.path()), vec![runnable], 1, true)
            .expect("suite recorded");
    assert_eq!(results[0].outcome, BuiltinTestOutcome::Failed);
    assert!(!results[0].success(), "a dead suite is not a pass");
}

#[test]
fn an_ordinary_noninteractive_failure_stays_failed() {
    let fixture = tempfile::tempdir().expect("fixture");
    let scratch = tempfile::tempdir().expect("scratch");
    for command in ["exit 7", "exit 130", "exit 143"] {
        let runnable = suite(fixture.path(), command, BuiltinTargetRuntime::Host);
        let results =
            run_builtin_test_targets_parallel(&ports(scratch.path()), vec![runnable], 1, true)
                .expect("suite recorded");
        let code = command
            .strip_prefix("exit ")
            .and_then(|value| value.parse::<i32>().ok());
        assert_eq!(
            results[0].outcome,
            BuiltinTestOutcome::Failed,
            "`{command}` is the suite's own failure, not a cancellation"
        );
        assert_eq!(results[0].code, code);
    }
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

/// A failed suite's code is the exit the session observed, not something
/// re-read from the display record: a verified exit whose journal record
/// failed still names its code, and a failure with no observed status names
/// none.
#[test]
fn a_failed_suite_carries_the_exit_the_session_observed() {
    let fixture = tempfile::tempdir().expect("fixture");

    let unrecorded = suite_result(
        named_suite(fixture.path(), "one"),
        Some(&SessionEntryState::Failed {
            detail: "unrecorded exit: journal unavailable".to_owned(),
            exit: Some(7),
        }),
    );
    assert_eq!(unrecorded.outcome, BuiltinTestOutcome::Failed);
    assert_eq!(unrecorded.code, Some(7));

    let unread = suite_result(
        named_suite(fixture.path(), "two"),
        Some(&SessionEntryState::Failed {
            detail: "timed out".to_owned(),
            exit: None,
        }),
    );
    assert_eq!(unread.outcome, BuiltinTestOutcome::Failed);
    assert_eq!(unread.code, None);
}

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
/// suite's result, and only a suite that ran and passed is a success. A
/// suite the session ended is a cancellation whether the operator quit the
/// session, stopped that one tab, or the run itself was stopped.
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
            "the session ended a member without the operator quitting",
            vec![("one", SessionEntryState::Passed), ("two", stopped.clone())],
            false,
            vec![BuiltinTestOutcome::Passed, BuiltinTestOutcome::Cancelled],
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
                        exit: Some(7),
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

    let text = cancelled_run(&results, false, true);
    assert!(
        text.contains("ok") && text.contains("cancelled"),
        "text keeps the pass and names the cancelled suite: {text}"
    );
    assert!(
        !text.contains("two  ok") && !text.contains("two ok"),
        "a stopped suite is never rendered as ok: {text}"
    );

    let payload: serde_json::Value =
        serde_json::from_str(&cancelled_run(&results, true, true)).expect("cancelled run JSON");
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
fn cancelled_run(
    results: &[BuiltinTestExecResult],
    output_json: bool,
    stopped_by_user: bool,
) -> String {
    let error = crate::test::render::finalize_builtin_test_outcome(
        results,
        &[],
        None,
        &[],
        false,
        output_json,
        stopped_by_user,
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

/// Finalize and report the run's own attribution: success, cancelled, or
/// failed. A completed failure stays a failed check; a stop before a suite
/// started is still the run's cancellation; a run where every suite passed
/// is a success even if the operator quit the session afterwards.
#[test]
fn the_run_outcome_attributes_cancellation_to_the_session_not_to_a_quiet_suite() {
    use crate::test::render::finalize_builtin_test_outcome;

    #[derive(Debug)]
    enum Expected {
        Success,
        Cancelled,
        Failed,
    }

    let fixture = tempfile::tempdir().expect("fixture");
    let stopped = SessionEntryState::Stopped {
        detail: "signal=15".to_owned(),
    };
    let failed = SessionEntryState::Failed {
        detail: "exit=7".to_owned(),
        exit: Some(7),
    };
    let cases = [
        (
            "all passed, quit after completion",
            vec![
                ("one", SessionEntryState::Passed),
                ("two", SessionEntryState::Passed),
            ],
            true,
            Expected::Success,
        ),
        (
            "stopped before any suite started",
            vec![
                ("one", SessionEntryState::NotStarted),
                ("two", SessionEntryState::NotStarted),
            ],
            true,
            Expected::Cancelled,
        ),
        (
            "stopped with a suite never started",
            vec![
                ("one", SessionEntryState::Passed),
                ("two", SessionEntryState::NotStarted),
            ],
            true,
            Expected::Cancelled,
        ),
        (
            "a suite the session ended without a quit",
            vec![("one", SessionEntryState::Passed), ("two", stopped)],
            false,
            Expected::Cancelled,
        ),
        (
            "a completed failure during a stop",
            vec![("one", SessionEntryState::Passed), ("two", failed)],
            true,
            Expected::Failed,
        ),
        (
            "never started without a stop",
            vec![("one", SessionEntryState::NotStarted)],
            false,
            Expected::Failed,
        ),
    ];

    for (case, entries, stopped_by_user, expected) in cases {
        let runnable = entries
            .iter()
            .map(|(name, _)| named_suite(fixture.path(), name))
            .collect::<Vec<BuiltinTestRunnable>>();
        let outcome = tui_outcome(entries, stopped_by_user);
        let session = BuiltinTestSession {
            results: builtin_test_results_from_tui(runnable, &outcome),
            stopped_by_user,
        };
        let finalized = finalize_builtin_test_outcome(
            &session.results,
            &[],
            None,
            &[],
            false,
            false,
            session.stopped_by_user,
        );
        match (expected, finalized) {
            (Expected::Success, Ok(Some(_))) => {}
            (Expected::Success, other) => panic!("{case}: expected success, got {other:?}"),
            (expected, Err(BuiltinError::BuiltinTestNonZero { cancelled, .. })) => {
                assert_eq!(
                    cancelled,
                    matches!(expected, Expected::Cancelled),
                    "run attribution for `{case}`"
                );
            }
            (expected, other) => panic!("{case}: expected {expected:?}, got {other:?}"),
        }
    }
}
