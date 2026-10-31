use std::path::PathBuf;

use effigy_core::build_info::{stale_repo_local_install, StaleLocalInstall};
use effigy_core::path_error_text::{
    failed_to_parse_path, failed_to_read_path, failed_to_render_path, failed_to_write_path,
};
use effigy_core::resolver::ResolveError;
use effigy_core::task_lock::TaskLockConflict;
use effigy_env::error::EnvSchemaError;
use effigy_managed::ManagedError;
use effigy_manifest::ManifestError;
use effigy_process::ProcessManagerError;
use effigy_routing::RoutingError;
use effigy_runtime::EffigyRuntimeError;
use effigy_scan::ScanError;
use effigy_tasks::TaskError;

#[path = "error/display.rs"]
mod display;
#[path = "error/rendered_output.rs"]
mod rendered_output;

#[derive(Debug)]
pub enum RunnerError {
    Cwd(std::io::Error),
    Resolve(ResolveError),
    Task(TaskError),
    Ui(String),
    TaskInvocation(String),
    /// Diagnostic contains only closed-vocabulary observations, never raw stdio.
    /// `observed_status` is the HTTP status a listener actually answered when
    /// its readiness declared another one, so a refusal names the exact
    /// observation that ended it (effigy#455).
    ManagedHostReadiness {
        process: String,
        diagnostic: String,
        observed_status: Option<u16>,
    },
    /// A detached activation settled the environment and published every route
    /// it verified, while one or more managed host listeners kept their own
    /// routes withheld. Exposure is partial and attributed per declared route
    /// target, so an unrelated ready container route is never withdrawn with a
    /// failing listener (effigy#455).
    ManagedListenerExposure {
        environment: String,
        /// `domain -> target` for each route this activation published.
        exposed: Vec<String>,
        /// The listeners that failed, with their bounded, redacted evidence.
        failures: Vec<crate::runner::host_process::ManagedHostListenerFailure>,
    },
    /// A container bring-up ended in a wait it could not finish: services that
    /// never reported ready, a declared health check, or a shared service step.
    /// The display sentence is the human diagnostic; `details_json` carries
    /// `failure_kind` and the machine-readable `waiting_on` list, one entry per
    /// wait the bring-up was still in (effigy#460, Q-017).
    ContainerWaitRefused {
        environment: String,
        diagnostic: String,
        details_json: String,
    },
    ContainerSurfaceRegistryMissing,
    ContainerSurfaceDefaultTargetMissing,
    ContainerSurfaceNotDefined {
        container: String,
    },
    ContainerSurfaceNotRunning {
        container: String,
    },
    ContainerSurfacePolicy {
        phase: &'static str,
        container: String,
        detail: String,
    },
    WorkspaceSessionCleanup {
        shell_error: String,
        cleanup_error: String,
    },
    /// An attached or followed run settled; its output was already written,
    /// so the caller exits with that run's own result and prints nothing.
    RunSettled {
        code: i32,
    },
    /// A `--run-id` key already names this run: the invocation attaches to
    /// it instead of starting another.
    RunKeyExists {
        run: String,
    },
    /// A run could not be admitted. `message` is the sentence a person reads
    /// and `category` is the kernel's own read of it, so a supervisor relaying
    /// this refusal never calls a busy store a refusal (K4, effigy#360).
    RunAdmission {
        category: effigy_kernel::human::Category,
        message: String,
    },
    /// The admission hook denied, failed or timed out: the heavy task did
    /// not run (Q-006). Details carry `effigy.admission.v1`.
    AdmissionRefused {
        detail: String,
        details_json: String,
    },
    /// A run-control read or request that could not be answered, with its
    /// `effigy.run.v1` details.
    RunControl {
        code: i32,
        detail: String,
        details_json: String,
    },
    GatewayRouteTable {
        phase: &'static str,
        path: PathBuf,
        detail: String,
    },
    GatewayRouteRegistration {
        phase: &'static str,
        domain: String,
        detail: String,
    },
    GatewayRouteShape {
        phase: &'static str,
        detail: String,
    },
    GatewayLoopback {
        phase: &'static str,
        detail: String,
    },
    GatewayRuntimeTarget {
        phase: &'static str,
        detail: String,
    },
    ContainerRuntimePolicy {
        phase: &'static str,
        detail: String,
    },
    TaskCatalogsMissing {
        root: PathBuf,
    },
    TaskManifestRead {
        path: PathBuf,
        error: std::io::Error,
    },
    TaskManifestParse {
        path: PathBuf,
        error: toml::de::Error,
        stale_local_install: Option<Box<StaleLocalInstall>>,
    },
    TaskManifestCompose {
        path: PathBuf,
        detail: String,
    },
    TaskCatalogAliasConflict {
        alias: String,
        first_path: PathBuf,
        second_path: PathBuf,
    },
    TaskCatalogPrefixNotFound {
        prefix: String,
        available: Vec<String>,
    },
    TaskNotFound {
        name: String,
        path: PathBuf,
    },
    TaskNotFoundAny {
        name: String,
        catalogs: Vec<String>,
    },
    TaskAmbiguous {
        name: String,
        candidates: Vec<String>,
    },
    TaskCommandLaunch {
        command: String,
        error: std::io::Error,
    },
    TaskCommandFailure {
        command: String,
        code: Option<i32>,
        stdout: String,
        stderr: String,
    },
    TaskLockConflict(Box<TaskLockConflict>),
    CommandJsonFailure {
        rendered: String,
    },
    /// An opt-in bounded container command did not produce a complete,
    /// in-budget capture. Carries a safe machine-readable reason/count
    /// diagnostic; never raw command output, arguments or credentials.
    BoundedExecFailure {
        code: i32,
        message: String,
        json_details: String,
    },
    DepsOperationNonZero {
        command: &'static str,
        outcome: &'static str,
        error_count: usize,
        rendered: String,
    },
    ManagedProcess(ProcessManagerError),
    TaskManagedUnsupportedMode {
        task: String,
        mode: String,
    },
    TaskManagedProfileNotFound {
        task: String,
        profile: String,
        available: Vec<String>,
    },
    TaskManagedProfileEmpty {
        task: String,
        profile: String,
    },
    TaskManagedProcessNotFound {
        task: String,
        profile: String,
        process: String,
    },
    TaskManagedProcessInvalidDefinition {
        task: String,
        process: String,
        detail: String,
    },
    TaskManagedProfileTabOrderInvalid {
        task: String,
        profile: String,
        detail: String,
    },
    TaskManagedTaskReferenceInvalid {
        task: String,
        process: String,
        reference: String,
        detail: String,
    },
    TaskManagedNonZeroExit {
        task: String,
        profile: String,
        processes: Vec<(String, String)>,
    },
    TaskMissingRunCommand {
        task: String,
        path: PathBuf,
    },
    BuiltinTestNonZero {
        failures: Vec<(String, Option<i32>)>,
        cancelled: bool,
        rendered: String,
    },
    BuiltinScanNonZero {
        finding_count: usize,
        rendered: String,
    },
    DoctorNonZero {
        error_count: usize,
        rendered: String,
    },
    DeferLoopDetected {
        depth: u8,
    },
    EnvSchema(EnvSchemaError),
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display::fmt_runner_error(self, f)
    }
}

impl std::error::Error for RunnerError {}

impl RunnerError {
    pub fn rendered_output(&self) -> Option<&str> {
        rendered_output::runner_error_rendered_output(self)
    }

    /// The process exit code for this error: an attached run's own result,
    /// a task command's code (128 + n when a signal ended it), 75 when the
    /// task's gate is held by a live run, otherwise 1.
    pub fn process_exit_code(&self) -> i32 {
        match self {
            RunnerError::RunSettled { code } | RunnerError::RunControl { code, .. } => *code,
            RunnerError::TaskCommandFailure {
                code: Some(code), ..
            } if (1..=255).contains(code) => *code,
            RunnerError::CommandJsonFailure { rendered }
                if serde_json::from_str::<serde_json::Value>(rendered)
                    .ok()
                    .is_some_and(|value| value["schema"] == "effigy.task.run.v1") =>
            {
                self.task_exit_status()
                    .filter(|code| (1..=255).contains(code))
                    .unwrap_or(1)
            }
            // An operator stop is the interrupted convention (128 + SIGINT),
            // the same code a cancelled QA-group member records, so text, JSON
            // and durable run records agree that the run was cancelled.
            RunnerError::BuiltinTestNonZero {
                cancelled: true, ..
            } => 130,
            RunnerError::TaskLockConflict(_) => 75,
            _ => 1,
        }
    }

    /// The kernel's category a person reads for this failure. An admission
    /// refusal keeps the category the kernel read for the run it could not
    /// start; every other failure belongs to one of the three kinds (K4, F16):
    /// a refusal is the request, its wiring or a holder; unavailable is a step
    /// or a service that did not answer; broken is Effigy's own invariant.
    pub fn category(&self) -> effigy_kernel::human::Category {
        use effigy_kernel::human::Category;
        match self {
            RunnerError::RunAdmission { category, .. } => *category,
            // A request, its manifest or catalog wiring, a holder, a policy or
            // a placement said no.
            RunnerError::Resolve(_)
            | RunnerError::Task(_)
            | RunnerError::Ui(_)
            | RunnerError::TaskInvocation(_)
            | RunnerError::ContainerWaitRefused { .. }
            | RunnerError::ContainerSurfaceRegistryMissing
            | RunnerError::ContainerSurfaceDefaultTargetMissing
            | RunnerError::ContainerSurfaceNotDefined { .. }
            | RunnerError::ContainerSurfaceNotRunning { .. }
            | RunnerError::ContainerSurfacePolicy { .. }
            | RunnerError::ContainerRuntimePolicy { .. }
            | RunnerError::GatewayRouteShape { .. }
            | RunnerError::RunSettled { .. }
            | RunnerError::RunKeyExists { .. }
            | RunnerError::AdmissionRefused { .. }
            | RunnerError::TaskLockConflict(_)
            | RunnerError::TaskCatalogsMissing { .. }
            | RunnerError::TaskManifestParse { .. }
            | RunnerError::TaskManifestCompose { .. }
            | RunnerError::TaskCatalogAliasConflict { .. }
            | RunnerError::TaskCatalogPrefixNotFound { .. }
            | RunnerError::TaskNotFound { .. }
            | RunnerError::TaskNotFoundAny { .. }
            | RunnerError::TaskAmbiguous { .. }
            | RunnerError::TaskMissingRunCommand { .. }
            | RunnerError::TaskManagedUnsupportedMode { .. }
            | RunnerError::TaskManagedProfileNotFound { .. }
            | RunnerError::TaskManagedProfileEmpty { .. }
            | RunnerError::TaskManagedProcessNotFound { .. }
            | RunnerError::TaskManagedProcessInvalidDefinition { .. }
            | RunnerError::TaskManagedProfileTabOrderInvalid { .. }
            | RunnerError::TaskManagedTaskReferenceInvalid { .. }
            | RunnerError::DeferLoopDetected { .. }
            // A command that reported its own outcome carries the next step in
            // its own document.
            | RunnerError::CommandJsonFailure { .. }
            | RunnerError::EnvSchema(_) => Category::Refused,
            // A step, a backend, a file, a service or a supervisor did not
            // answer, or the work it ran failed.
            RunnerError::Cwd(_)
            | RunnerError::ManagedHostReadiness { .. }
            | RunnerError::ManagedListenerExposure { .. }
            | RunnerError::WorkspaceSessionCleanup { .. }
            | RunnerError::RunControl { .. }
            | RunnerError::GatewayRouteTable { .. }
            | RunnerError::GatewayRouteRegistration { .. }
            | RunnerError::GatewayLoopback { .. }
            | RunnerError::GatewayRuntimeTarget { .. }
            | RunnerError::TaskManifestRead { .. }
            | RunnerError::TaskCommandLaunch { .. }
            | RunnerError::TaskCommandFailure { .. }
            | RunnerError::BoundedExecFailure { .. }
            | RunnerError::DepsOperationNonZero { .. }
            | RunnerError::ManagedProcess(_)
            | RunnerError::TaskManagedNonZeroExit { .. }
            | RunnerError::BuiltinTestNonZero { .. }
            | RunnerError::BuiltinScanNonZero { .. }
            | RunnerError::DoctorNonZero { .. } => Category::Unavailable,
        }
    }

    pub fn json_error_details(&self) -> Option<&str> {
        rendered_output::runner_error_json_details(self)
    }

    /// The three facts a person needs from this refusal: what failed, the
    /// evidence, and the next action. The top-level printer renders this as
    /// text, and `--json` carries the same value as `error.human`.
    ///
    /// Every refusal already renders its own sentence through `Display`, so
    /// this lifts that sentence: its first line says what failed, an explicit
    /// `next:` line becomes the next action, and the rest is evidence. A
    /// command failure additionally names its exit code and output tail.
    pub fn human_error(&self) -> effigy_ui::HumanError {
        match self {
            RunnerError::TaskCommandFailure {
                command,
                code,
                stdout,
                stderr,
            } => task_command_failure_human(command, *code, stdout, stderr),
            // A lock conflict's remediation is already the sentence a person
            // reads: what holds the lock, and the commands that stop it. The
            // structured details are for `--json`; lifting them by key lost the
            // advice entirely (effigy#313).
            RunnerError::TaskLockConflict(details) => lock_conflict_human(details),
            // An admission refusal's display sentence already names the task,
            // the run, the hook's own reason and the next command. Its
            // `effigy.admission.v1` details carry `decision`/`reason` but no
            // `cause` or `message`, so lifting them by key reduced a deny or a
            // timeout to ``task `heavy` failed`` and hid why the heavy task
            // never ran (effigy#328).
            RunnerError::AdmissionRefused { detail, .. } => human_error_from_display(detail),
            _ => match self
                .json_error_details()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                .filter(|details| details.is_object())
            {
                Some(details) => {
                    let human = human_error_from_details(&self.to_string(), &details);
                    // A failed task run's payload names no next step; its text
                    // form does. Both carry the same one (failed task commands).
                    if details["schema"] == "effigy.task.run.v1" {
                        human.next_action(TASK_COMMAND_FAILURE_NEXT)
                    } else {
                        human
                    }
                }
                None => match self {
                    RunnerError::CommandJsonFailure { rendered } => text_payload_human(rendered),
                    _ => human_error_from_display(&self.to_string()),
                },
            },
        }
    }

    pub(in crate::runner) fn task_invocation(message: impl Into<String>) -> Self {
        Self::TaskInvocation(message.into())
    }

    /// A container wait that ended unfinished: its human `diagnostic`, and the
    /// machine-readable `details` (`failure_kind`, `waiting_on`) for `--json`.
    pub(in crate::runner) fn container_wait_refused(
        environment: &str,
        diagnostic: String,
        mut details: serde_json::Value,
    ) -> Self {
        details["environment"] = environment.into();
        Self::ContainerWaitRefused {
            environment: environment.to_owned(),
            diagnostic,
            details_json: details.to_string(),
        }
    }

    pub(in crate::runner) fn container_runtime_policy(
        phase: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::ContainerRuntimePolicy {
            phase,
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn container_surface_not_defined(container: impl Into<String>) -> Self {
        Self::ContainerSurfaceNotDefined {
            container: container.into(),
        }
    }

    pub(in crate::runner) fn container_surface_not_running(container: impl Into<String>) -> Self {
        Self::ContainerSurfaceNotRunning {
            container: container.into(),
        }
    }

    pub(in crate::runner) fn container_surface_policy(
        phase: &'static str,
        container: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::ContainerSurfacePolicy {
            phase,
            container: container.into(),
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn workspace_session_cleanup(
        shell_error: impl Into<String>,
        cleanup_error: impl Into<String>,
    ) -> Self {
        Self::WorkspaceSessionCleanup {
            shell_error: shell_error.into(),
            cleanup_error: cleanup_error.into(),
        }
    }

    pub(in crate::runner) fn gateway_route_table(
        phase: &'static str,
        path: impl Into<PathBuf>,
        detail: impl Into<String>,
    ) -> Self {
        Self::GatewayRouteTable {
            phase,
            path: path.into(),
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn gateway_route_registration(
        phase: &'static str,
        domain: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::GatewayRouteRegistration {
            phase,
            domain: domain.into(),
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn gateway_route_shape(
        phase: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::GatewayRouteShape {
            phase,
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn gateway_loopback(
        phase: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::GatewayLoopback {
            phase,
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn gateway_runtime_target(
        phase: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::GatewayRuntimeTarget {
            phase,
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn task_invocation_failed_read(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_read_path(path, error))
    }

    pub(in crate::runner) fn task_invocation_failed_parse(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_parse_path(path, error))
    }

    pub(in crate::runner) fn task_invocation_failed_write(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_write_path(path, error))
    }

    pub(in crate::runner) fn task_invocation_failed_render(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_render_path(path, error))
    }

    // The task owns raw stdio in passthrough mode; the only exit-status signal
    // Effigy preserves is the launched child's status.
    pub fn task_exit_status(&self) -> Option<i32> {
        match self {
            RunnerError::RunSettled { code } | RunnerError::RunControl { code, .. } => Some(*code),
            RunnerError::TaskCommandFailure { code, .. } => Some(code.unwrap_or(1)),
            RunnerError::CommandJsonFailure { rendered } => Some(
                serde_json::from_str::<serde_json::Value>(rendered)
                    .ok()
                    .and_then(|value| value["exit_code"].as_i64())
                    .map(|code| code as i32)
                    .filter(|code| *code != 0)
                    .unwrap_or(1),
            ),
            RunnerError::BoundedExecFailure { code, .. } => Some(*code),
            _ => None,
        }
    }
}

/// A child command's failure, as the three facts a person needs: the command
/// and its exit code, the bounded tail of its own output, and the next action.
/// Both the `Display` sentence and the human renderer use this, so the text a
/// wrapper quotes and the text a person reads cannot drift apart.
pub(in crate::runner) fn task_command_failure_human(
    command: &str,
    code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> effigy_ui::HumanError {
    let mut error = effigy_ui::HumanError::new(format!("command `{command}` failed"));
    error = error.evidence(match code {
        Some(code) => format!("exit code {code}"),
        None => "a signal stopped it before it reported an exit code".to_owned(),
    });
    // The failing output is the evidence: its first lines name the failure and
    // its last lines say how it ended. Both streams are shown when both said
    // something, because a runner that prints progress to stderr and failures
    // to stdout would otherwise hide its own cause (effigy#364).
    error = error.evidence_output("stderr", stderr);
    if !stdout.trim().is_empty() {
        error = error.evidence_output("stdout", stdout);
    }
    error.next_action(TASK_COMMAND_FAILURE_NEXT)
}

/// The next step for a failed task command. The text `next:` line and the
/// `--json` `error.human.next_action` both take it from here.
pub(in crate::runner) const TASK_COMMAND_FAILURE_NEXT: &str =
    "read the output above, fix the cause, then run the command again";

/// A wrapper's sentence around a child failure: what the wrapper was doing,
/// the child's own failure as evidence, and the next action. Sections stay on
/// their own lines so a reader (and `human_error`) can tell them apart.
pub(in crate::runner) fn child_failure_text(
    context: &str,
    child: &effigy_ui::HumanError,
    next: &str,
) -> String {
    let mut text = format!("{context}: {}", child.what_failed());
    for line in child.evidence_lines() {
        text.push_str("\n  ");
        text.push_str(line);
    }
    text.push_str("\nnext: ");
    text.push_str(next);
    text
}

/// A refusal that carries structured details says more than its message: the
/// details name the phase, the cause and the recovery. Lift those into the
/// three facts, and keep the message's own sentence as what failed when the
/// details do not improve on it.
/// The lock refusal as three facts: the holder and the wait, then the next
/// commands. `remediation` is the one rendering both paths share, so the text a
/// person reads can never drift from the text `Display` produces.
fn lock_conflict_human(
    details: &effigy_core::task_lock::TaskLockConflict,
) -> effigy_ui::HumanError {
    match details.remediation.split_once(". ") {
        Some((what_failed, next)) => effigy_ui::HumanError::new(what_failed).next_action(next),
        None => effigy_ui::HumanError::new(&details.remediation),
    }
}

fn human_error_from_details(rendered: &str, details: &serde_json::Value) -> effigy_ui::HumanError {
    // A detail is a string or a number (`exit_code` is a JSON number).
    let scalar = |key: &str| {
        details.get(key).and_then(|value| match value {
            serde_json::Value::String(text) => Some(text.trim().to_owned()),
            serde_json::Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
    };
    let text = |key: &str| scalar(key).filter(|value| !value.is_empty());
    let base = human_error_from_display(rendered);
    let mut error = match (text("cause"), text("message"), text("task")) {
        (Some(cause), Some(message), _) if cause != message => {
            effigy_ui::HumanError::new(cause).evidence(message)
        }
        (Some(cause), _, _) => effigy_ui::HumanError::new(cause),
        (None, Some(message), _) => effigy_ui::HumanError::new(message),
        (None, None, Some(task)) => effigy_ui::HumanError::new(format!("task `{task}` failed")),
        (None, None, None) => base.clone(),
    };
    for key in [
        "failure_kind",
        "phase",
        "preparation_step",
        "activation_stage",
        "state",
        "readiness",
        "exposure",
        "exit_code",
    ] {
        if let Some(value) = text(key) {
            error = error.evidence(format!("{key}: {value}"));
        }
    }
    // The failing output is the evidence: both streams when both said
    // something, so the cause cannot hide in the stream that was not read.
    for (label, key) in [("stderr", "stderr"), ("stdout", "stdout")] {
        if let Some(output) = text(key) {
            error = error.evidence_output(label, &output);
        }
    }
    if let Some(action) = ["next_action", "next", "stop", "recovery"]
        .iter()
        .find_map(|key| text(key))
        .or_else(|| base.next_action_text().map(str::to_owned))
    {
        error = error.next_action(action);
    }
    error
}

/// A command payload that is text, not a JSON object. The text prints as
/// written, so the first line says what failed and the last `next:` line is
/// the action. The `--json` form carries that same action, not `null`.
fn text_payload_human(rendered: &str) -> effigy_ui::HumanError {
    let what_failed = rendered
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("the command failed");
    let human = effigy_ui::HumanError::new(what_failed);
    let next = rendered
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("next:"))
        .map(str::trim)
        .filter(|action| !action.is_empty());
    match next {
        Some(action) => human.next_action(action),
        None => human,
    }
}

/// Lift a rendered refusal into the three facts, without inventing any: the
/// first line is what failed, a `next:` line is the next action, and the rest
/// is evidence.
fn human_error_from_display(rendered: &str) -> effigy_ui::HumanError {
    let mut lines = rendered.lines();
    let what_failed = lines
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .unwrap_or("the command failed");
    let mut error = effigy_ui::HumanError::new(what_failed);
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match line.strip_prefix("next:") {
            Some(action) => error = error.next_action(action.trim()),
            None => error = error.evidence(line),
        }
    }
    error
}

impl From<TaskError> for RunnerError {
    fn from(value: TaskError) -> Self {
        Self::Task(value)
    }
}

impl From<effigy_ui::UiError> for RunnerError {
    fn from(value: effigy_ui::UiError) -> Self {
        Self::Ui(value.to_string())
    }
}

impl From<ResolveError> for RunnerError {
    fn from(value: ResolveError) -> Self {
        Self::Resolve(value)
    }
}

impl From<ProcessManagerError> for RunnerError {
    fn from(value: ProcessManagerError) -> Self {
        Self::ManagedProcess(value)
    }
}

impl From<EnvSchemaError> for RunnerError {
    fn from(value: EnvSchemaError) -> Self {
        Self::EnvSchema(value)
    }
}

impl From<EffigyRuntimeError> for RunnerError {
    fn from(value: EffigyRuntimeError) -> Self {
        match value {
            EffigyRuntimeError::Cwd(error) => Self::Cwd(error),
            EffigyRuntimeError::Ui(message) => Self::Ui(message),
            EffigyRuntimeError::TaskInvocation(message) => Self::TaskInvocation(message),
            EffigyRuntimeError::TaskCommandLaunch { command, error } => {
                Self::TaskCommandLaunch { command, error }
            }
        }
    }
}

impl From<ManagedError> for RunnerError {
    fn from(value: ManagedError) -> Self {
        match value {
            ManagedError::Cwd(error) => Self::Cwd(error),
            ManagedError::TaskInvocation(message) => Self::TaskInvocation(message),
            ManagedError::Ui(message) => Self::Ui(message),
            ManagedError::Process(error) => Self::ManagedProcess(error),
            ManagedError::TaskManagedUnsupportedMode { task, mode } => {
                Self::TaskManagedUnsupportedMode { task, mode }
            }
            ManagedError::TaskHasConcurrentWithoutMode { task } => Self::TaskInvocation(format!(
                "task '{task}' declares `concurrent = [...]` but does not set `mode = \"tui\"`; \
                     either add `mode = \"tui\"` (TUI runs concurrent entries as tabs) or move \
                     the concurrent entries to `[[containers.<name>.host_processes]]` for non-TUI \
                     lifecycle-bound supervisors"
            )),
            ManagedError::TaskManagedProfileNotFound {
                task,
                profile,
                available,
            } => Self::TaskManagedProfileNotFound {
                task,
                profile,
                available,
            },
            ManagedError::TaskManagedProfileEmpty { task, profile } => {
                Self::TaskManagedProfileEmpty { task, profile }
            }
            ManagedError::TaskManagedProcessInvalidDefinition {
                task,
                process,
                detail,
            } => Self::TaskManagedProcessInvalidDefinition {
                task,
                process,
                detail,
            },
            ManagedError::TaskManagedTaskReferenceInvalid {
                task,
                process,
                reference,
                detail,
            } => Self::TaskManagedTaskReferenceInvalid {
                task,
                process,
                reference,
                detail,
            },
            ManagedError::TaskManagedNonZeroExit {
                task,
                profile,
                processes,
            } => Self::TaskManagedNonZeroExit {
                task,
                profile,
                processes,
            },
        }
    }
}

impl From<effigy_containers::ContainerPolicyError> for RunnerError {
    fn from(value: effigy_containers::ContainerPolicyError) -> Self {
        match value {
            effigy_containers::ContainerPolicyError::Manifest(error) => {
                Self::task_invocation(error.to_string())
            }
            effigy_containers::ContainerPolicyError::Catalog(error) => {
                Self::task_invocation(error.to_string())
            }
            effigy_containers::ContainerPolicyError::TaskInvocation(message) => {
                Self::task_invocation(message)
            }
            effigy_containers::ContainerPolicyError::Read { path, error } => {
                Self::task_invocation_failed_read(&path, error)
            }
        }
    }
}

impl From<effigy_release::ReleaseError> for RunnerError {
    fn from(value: effigy_release::ReleaseError) -> Self {
        Self::task_invocation(value.to_string())
    }
}

/// Lifts `RoutingError` from the `catalog/**` routing surface into the
/// existing `RunnerError::Task*` variants. Shapes are kept identical so
/// this is a mechanical variant-to-variant map; the `Manifest` wrapper
/// variant bridges catalog's own load path (card `245` Part B) by
/// delegating through the existing `ManifestError` → `RunnerError`
/// mapping. Card `246` will move this impl to a shim once `catalog/**`
/// extracts into `effigy-routing`.
impl From<RoutingError> for RunnerError {
    fn from(value: RoutingError) -> Self {
        match value {
            RoutingError::TaskCatalogsMissing { root } => Self::TaskCatalogsMissing { root },
            RoutingError::TaskCatalogAliasConflict {
                alias,
                first_path,
                second_path,
            } => Self::TaskCatalogAliasConflict {
                alias,
                first_path,
                second_path,
            },
            RoutingError::TaskCatalogMemberInvalid { .. } => {
                Self::TaskInvocation(value.to_string())
            }
            RoutingError::TaskCatalogPrefixNotFound { prefix, available } => {
                Self::TaskCatalogPrefixNotFound { prefix, available }
            }
            RoutingError::TaskNotFound { name, path } => Self::TaskNotFound { name, path },
            RoutingError::TaskNotFoundAny { name, catalogs } => {
                Self::TaskNotFoundAny { name, catalogs }
            }
            RoutingError::TaskAmbiguous { name, candidates } => {
                Self::TaskAmbiguous { name, candidates }
            }
            RoutingError::Manifest(error) => map_manifest_error(error),
        }
    }
}

/// Lifts `ScanError` from `effigy-scan` into `RunnerError` at the
/// runner's edge. Job-8 pattern — see also `effigy-process`,
/// `effigy-ui`, `effigy-managed`, `effigy-env`, `effigy-routing`.
/// The `Invocation` variant covers the 20 call sites that previously
/// constructed `RunnerError::task_invocation(...)` directly. The
/// `Manifest` variant bridges scan's option-loading path through the
/// existing `ManifestError` → `RunnerError` mapping.
impl From<ScanError> for RunnerError {
    fn from(value: ScanError) -> Self {
        match value {
            ScanError::Invocation(message) => Self::TaskInvocation(message),
            ScanError::Manifest(error) => map_manifest_error(error),
        }
    }
}

fn map_manifest_error(error: ManifestError) -> RunnerError {
    match error {
        ManifestError::Read { path, error } => RunnerError::TaskManifestRead { path, error },
        ManifestError::Parse { path, error } => {
            let stale_local_install = stale_repo_local_install(&path).map(Box::new);
            RunnerError::TaskManifestParse {
                path,
                error,
                stale_local_install,
            }
        }
        ManifestError::Compose { path, detail } => {
            RunnerError::TaskManifestCompose { path, detail }
        }
        ManifestError::Render { path, detail } => {
            RunnerError::task_invocation_failed_render(&path, detail)
        }
    }
}

/// Lifts `BuiltinError` from `effigy-builtin` into `RunnerError` at
/// the runner's edge. Variant shapes mirror one-for-one (card 250),
/// with the `Manifest` variant bridging through
/// `map_manifest_error` to match the pattern used by `ScanError` and
/// `RoutingError`.
impl From<effigy_builtin::BuiltinError> for RunnerError {
    fn from(value: effigy_builtin::BuiltinError) -> Self {
        use effigy_builtin::BuiltinError as B;
        match value {
            B::TaskInvocation(message) => Self::TaskInvocation(message),
            B::Ui(message) => Self::Ui(message),
            B::TaskManifestCompose { path, detail } => Self::TaskManifestCompose { path, detail },
            B::TaskCommandLaunch { command, error } => Self::TaskCommandLaunch { command, error },
            B::TaskLockConflict(details) => Self::TaskLockConflict(details),
            B::BuiltinTestNonZero {
                failures,
                cancelled,
                rendered,
            } => Self::BuiltinTestNonZero {
                failures,
                cancelled,
                rendered,
            },
            B::BuiltinScanNonZero {
                finding_count,
                rendered,
            } => Self::BuiltinScanNonZero {
                finding_count,
                rendered,
            },
            B::DoctorNonZero {
                error_count,
                rendered,
            } => Self::DoctorNonZero {
                error_count,
                rendered,
            },
            B::CommandJsonFailure { rendered } => Self::CommandJsonFailure { rendered },
            B::Manifest(error) => map_manifest_error(error),
            B::Managed(error) => Self::from(error),
            B::Routing(error) => Self::from(error),
            B::Scan(error) => Self::from(error),
        }
    }
}

/// Lifts `DoctorError` from `effigy-doctor` into `RunnerError` at the
/// runner's edge (card 254). Mirrors the `From<BuiltinError>` pattern.
impl From<effigy_doctor::DoctorError> for RunnerError {
    fn from(value: effigy_doctor::DoctorError) -> Self {
        use effigy_doctor::DoctorError as D;
        match value {
            D::DoctorNonZero {
                error_count,
                rendered,
            } => Self::DoctorNonZero {
                error_count,
                rendered,
            },
            D::TaskInvocation(message) => Self::TaskInvocation(message),
            D::Ui(message) => Self::Ui(message),
            D::CommandJsonFailure { rendered } => Self::CommandJsonFailure { rendered },
            D::Manifest(error) => map_manifest_error(error),
            D::Scan(error) => Self::from(error),
            D::Routing(error) => Self::from(error),
            D::BudgetExhausted { phase } => {
                Self::TaskInvocation(format!("doctor budget exhausted during {phase}"))
            }
        }
    }
}

impl From<effigy_containers::exec::ContainerExecError> for RunnerError {
    fn from(value: effigy_containers::exec::ContainerExecError) -> Self {
        match value {
            effigy_containers::exec::ContainerExecError::Launch { command, error } => {
                Self::TaskCommandLaunch { command, error }
            }
            effigy_containers::exec::ContainerExecError::Failure {
                command,
                code,
                stdout,
                stderr,
            } => map_container_exec_failure(command, code, stdout, stderr),
        }
    }
}

fn map_container_exec_failure(
    command: String,
    code: Option<i32>,
    stdout: String,
    stderr: String,
) -> RunnerError {
    let lowered = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if lowered.contains("error retrieving current runtime: empty value") {
        return RunnerError::TaskInvocation(format!(
            "Colima profile runtime state is corrupted and Effigy could not recover it automatically.\nprofile command: `{command}`\nnext: restart or delete the affected Colima profile, then retry the Effigy command.\ndetails:\n{stderr}"
        ));
    }
    if lowered.contains("no space left on device") {
        return RunnerError::TaskInvocation(format!(
            "Container runtime storage is out of space or inodes while running `{command}`.\nnext: run `effigy container profile resize` to restart the managed Colima profile without deleting runtime data, then retry the Effigy command. If disk pressure remains, inspect `effigy container cache list --global` and prune purge-safe caches with `effigy container cache prune --global --yes`.\nstdout:\n{stdout}\nstderr:\n{stderr}"
        ));
    }

    RunnerError::TaskCommandFailure {
        command,
        code,
        stdout,
        stderr,
    }
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;

impl From<effigy_gateway::GatewayError> for RunnerError {
    fn from(error: effigy_gateway::GatewayError) -> Self {
        Self::task_invocation(error.to_string())
    }
}
