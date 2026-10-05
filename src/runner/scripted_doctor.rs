//! Deterministic, test-only capture runtime for the doctor behavior oracles.
//!
//! The doctor ownership path reaches the runtime through three runner-owned
//! boundaries: the preliminary liveness sequencing in `system_command`, the
//! service-name resolve in `exec_command::transport::colima`, and the
//! `run_command_capture_until` boundary every Compose access batch funnels
//! through. This module lets a test install a thread-local scripted runtime at
//! those boundaries so the production orchestration, argv construction and
//! parsers run unchanged while no subprocess is spawned.
//!
//! A scripted run also carries a *logical* clock: every executed script
//! operation costs [`SCRIPTED_OPERATION_COST`] of model time. Deadlines stay
//! real `Instant`s (the production code compares them unchanged); the runtime
//! only decides expiry against `install_time + logical_elapsed`, so a model
//! budget can expire without sleeping. Logical time never leaves this module
//! and never replaces a production `Instant`.
//!
//! This module compiles only under `#[cfg(test)]`; production builds contain no
//! scripted branch and keep their existing capture, deadlines and policies.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::process::{ExitStatus, Output};
use std::rc::Rc;
use std::time::{Duration, Instant};

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

/// Model cost charged for one executed script operation. This is the
/// controlled backend latency the pre-batching doctor measured per round trip;
/// it is a model, not a wall-clock or performance assertion.
pub(in crate::runner) const SCRIPTED_OPERATION_COST: Duration = Duration::from_millis(430);

/// The production phase a captured launch belongs to, derived from the argv
/// production actually built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) enum ScriptedPhase {
    ColimaStatus,
    ServiceResolve,
    LivenessComposePs,
    BunProbe,
    Identity,
    MetadataBatch,
    AccessBatch,
    Unexpected,
}

impl ScriptedPhase {
    pub(in crate::runner) fn label(self) -> &'static str {
        match self {
            Self::ColimaStatus => "colima-status",
            Self::ServiceResolve => "service-resolve",
            Self::LivenessComposePs => "compose-ps",
            Self::BunProbe => "bun",
            Self::Identity => "identity",
            Self::MetadataBatch => "metadata",
            Self::AccessBatch => "access",
            Self::Unexpected => "unexpected",
        }
    }
}

/// One executed script operation, in execution order. The fields are the
/// production argv and the classification derived from it, so an oracle can
/// assert exactly what the doctor asked the backend to do.
#[derive(Clone, Debug)]
pub(in crate::runner) struct ScriptedOperation {
    pub phase: ScriptedPhase,
    pub program: String,
    pub args: Vec<String>,
    /// Paths handed to a metadata/access batch, in argv order.
    pub paths: Vec<String>,
    /// The `-u` user the production argv requested, when present.
    pub user: Option<String>,
    /// True when the argv contains a mutating command (`chown`/`mkdir`/...),
    /// which the read-only doctor must never issue.
    pub mutating: bool,
    pub cost: Duration,
}

impl ScriptedOperation {
    pub(in crate::runner) fn rendered(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}

/// The answer the scripted backend gives for a request.
#[derive(Clone, Debug)]
pub(in crate::runner) struct ScriptedAnswer {
    pub success: bool,
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub timeout: bool,
}

impl ScriptedAnswer {
    pub(in crate::runner) fn ok(stdout: impl Into<String>) -> Self {
        Self {
            success: true,
            code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
            timeout: false,
        }
    }

    pub(in crate::runner) fn failed(code: i32, stderr: impl Into<String>) -> Self {
        Self {
            success: false,
            code,
            stdout: String::new(),
            stderr: stderr.into(),
            timeout: false,
        }
    }

    pub(in crate::runner) fn timed_out() -> Self {
        Self {
            success: false,
            code: 1,
            stdout: String::new(),
            stderr: String::new(),
            timeout: true,
        }
    }

    fn into_output(self) -> Output {
        Output {
            status: exit_status(self.success, self.code),
            stdout: self.stdout.into_bytes(),
            stderr: self.stderr.into_bytes(),
        }
    }
}

#[cfg(unix)]
fn exit_status(success: bool, code: i32) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    if success {
        ExitStatus::from_raw(0)
    } else {
        ExitStatus::from_raw((code & 0xff) << 8)
    }
}

#[cfg(not(unix))]
fn exit_status(_success: bool, _code: i32) -> ExitStatus {
    unreachable!("scripted doctor capture is unix-only")
}

/// A classified production request, before it is answered.
#[derive(Clone, Debug)]
pub(in crate::runner) struct ScriptedRequest {
    pub phase: ScriptedPhase,
    pub program: String,
    pub args: Vec<String>,
    pub paths: Vec<String>,
    pub user: Option<String>,
    pub mutating: bool,
}

pub(in crate::runner) type ScriptedHandler = dyn Fn(&ScriptedRequest) -> ScriptedAnswer;

thread_local! {
    static ACTIVE: RefCell<Option<Rc<ScriptedDoctor>>> = const { RefCell::new(None) };
}

/// Installed handle. Dropping it uninstalls the scripted runtime.
pub(in crate::runner) struct ScriptedGuard {
    runtime: Rc<ScriptedDoctor>,
}

impl ScriptedGuard {
    pub(in crate::runner) fn runtime(&self) -> &ScriptedDoctor {
        &self.runtime
    }

    /// A production-sized deadline on the model clock. `base + budget`, so the
    /// unchanged production `Instant` comparisons and the runtime's model
    /// expiry agree.
    pub(in crate::runner) fn deadline(&self, budget: Duration) -> Instant {
        self.runtime.base + budget
    }

    /// Run `body` while `phase` is answered by the real fake backend instead of
    /// the scripted one. Used only by the bounded real crosscheck.
    pub(in crate::runner) fn passthrough(&self, phases: impl IntoIterator<Item = ScriptedPhase>) {
        self.runtime.passthrough.borrow_mut().extend(phases);
    }
}

impl Drop for ScriptedGuard {
    fn drop(&mut self) {
        ACTIVE.with(|active| *active.borrow_mut() = None);
    }
}

/// Install a scripted runtime for the current test thread.
pub(in crate::runner) fn install(handler: Box<ScriptedHandler>) -> ScriptedGuard {
    let runtime = Rc::new(ScriptedDoctor::new(Instant::now(), handler));
    ACTIVE.with(|active| *active.borrow_mut() = Some(runtime.clone()));
    ScriptedGuard { runtime }
}

pub(in crate::runner) struct ScriptedDoctor {
    base: Instant,
    logical: Cell<Duration>,
    handler: Box<ScriptedHandler>,
    operations: RefCell<Vec<ScriptedOperation>>,
    passthrough: RefCell<Vec<ScriptedPhase>>,
    /// Negative-control switch: record a phase's `-u` user as this value even
    /// though production built the real argv. The oracle must reject it.
    recorded_user_override: RefCell<Option<(ScriptedPhase, String)>>,
}

impl ScriptedDoctor {
    fn new(base: Instant, handler: Box<ScriptedHandler>) -> Self {
        Self {
            base,
            logical: Cell::new(Duration::ZERO),
            handler,
            operations: RefCell::new(Vec::new()),
            passthrough: RefCell::new(Vec::new()),
            recorded_user_override: RefCell::new(None),
        }
    }

    /// Negative-control fixture switch. Used only by the bounded reliability
    /// controls to prove an oracle rejects a wrong recorded backend user.
    pub(in crate::runner) fn override_recorded_user(&self, phase: ScriptedPhase, user: &str) {
        *self.recorded_user_override.borrow_mut() = Some((phase, user.to_owned()));
    }

    /// Model time consumed by every executed script operation so far.
    pub(in crate::runner) fn logical_elapsed(&self) -> Duration {
        self.logical.get()
    }

    pub(in crate::runner) fn operations(&self) -> Vec<ScriptedOperation> {
        self.operations.borrow().clone()
    }

    /// Executed script operations, excluding the Colima `status` preliminary.
    /// These are the launches the pre-batching doctor issued per workload op.
    pub(in crate::runner) fn applicable_operations(&self) -> Vec<ScriptedOperation> {
        self.operations()
            .into_iter()
            .filter(|operation| operation.phase != ScriptedPhase::ColimaStatus)
            .collect()
    }

    pub(in crate::runner) fn operations_for(&self, phase: ScriptedPhase) -> Vec<ScriptedOperation> {
        self.operations()
            .into_iter()
            .filter(|operation| operation.phase == phase)
            .collect()
    }

    /// All paths requested across metadata batches, in order.
    pub(in crate::runner) fn requested_metadata_paths(&self) -> Vec<String> {
        self.operations_for(ScriptedPhase::MetadataBatch)
            .into_iter()
            .flat_map(|operation| operation.paths)
            .collect()
    }

    pub(in crate::runner) fn requested_access_paths(&self) -> Vec<String> {
        self.operations_for(ScriptedPhase::AccessBatch)
            .into_iter()
            .flat_map(|operation| operation.paths)
            .collect()
    }

    fn is_passthrough(&self, phase: ScriptedPhase) -> bool {
        self.passthrough.borrow().contains(&phase)
    }

    fn charge(&self) {
        self.logical
            .set(self.logical.get() + SCRIPTED_OPERATION_COST);
    }

    fn expired(&self, deadline: Option<Instant>) -> bool {
        deadline.is_some_and(|deadline| self.base + self.logical.get() >= deadline)
    }

    fn record(&self, request: &ScriptedRequest) {
        let user = self
            .recorded_user_override
            .borrow()
            .as_ref()
            .filter(|(phase, _)| *phase == request.phase)
            .map(|(_, user)| user.clone())
            .or_else(|| request.user.clone());
        self.operations.borrow_mut().push(ScriptedOperation {
            phase: request.phase,
            program: request.program.clone(),
            args: request.args.clone(),
            paths: request.paths.clone(),
            user,
            mutating: request.mutating,
            cost: SCRIPTED_OPERATION_COST,
        });
    }

    fn run(
        &self,
        request: ScriptedRequest,
        deadline: Option<Instant>,
    ) -> Result<Output, RunnerError> {
        let rendered = format!("{} {}", request.program, request.args.join(" "));
        if request.phase == ScriptedPhase::Unexpected {
            self.record(&request);
            return Err(RunnerError::TaskInvocation(format!(
                "scripted doctor captured an unexpected launch: {rendered}"
            )));
        }
        if self.expired(deadline) {
            return Err(RunnerError::TaskInvocation(format!("{rendered} timed out")));
        }
        let answer = (self.handler)(&request);
        self.record(&request);
        self.charge();
        if answer.timeout {
            return Err(RunnerError::TaskInvocation(format!("{rendered} timed out")));
        }
        Ok(answer.into_output())
    }
}

fn with_runtime<R>(body: impl FnOnce(&ScriptedDoctor) -> Option<R>) -> Option<R> {
    ACTIVE.with(|active| {
        let runtime = active.borrow();
        runtime.as_ref().and_then(|runtime| body(runtime))
    })
}

/// Classify a production launch into a scripted phase and its salient argv.
pub(in crate::runner) fn classify(program: &str, args: &[String]) -> ScriptedRequest {
    let mutating = args.iter().any(|arg| {
        matches!(arg.as_str(), "chown" | "mkdir" | "chmod")
            || (arg == "-execdir" && args.iter().any(|value| value == "chown"))
    });
    let user = args
        .iter()
        .position(|arg| arg == "-u")
        .and_then(|index| args.get(index + 1))
        .cloned();
    let marker_paths = |marker: &str| -> Vec<String> {
        args.iter()
            .position(|arg| arg == marker)
            .map(|index| args[index + 1..].to_vec())
            .unwrap_or_default()
    };

    let phase = if program.contains("colima") || program.contains("docker") {
        match args.first().map(String::as_str) {
            Some("status") => ScriptedPhase::ColimaStatus,
            _ if args.iter().any(|arg| arg == "effigy-workspace-identity") => {
                ScriptedPhase::Identity
            }
            _ if args
                .iter()
                .any(|arg| arg == "effigy-workspace-doctor-inspect-batch") =>
            {
                ScriptedPhase::MetadataBatch
            }
            _ if args
                .iter()
                .any(|arg| arg == "effigy-workspace-doctor-access-batch") =>
            {
                ScriptedPhase::AccessBatch
            }
            _ if args.iter().any(|arg| arg.contains("BUN_INSTALL")) => ScriptedPhase::BunProbe,
            _ if args.iter().any(|arg| arg == "ps") => {
                if args.iter().any(|arg| arg == "-q") {
                    ScriptedPhase::ServiceResolve
                } else {
                    ScriptedPhase::LivenessComposePs
                }
            }
            _ => ScriptedPhase::Unexpected,
        }
    } else {
        ScriptedPhase::Unexpected
    };

    let paths = match phase {
        ScriptedPhase::MetadataBatch => marker_paths("effigy-workspace-doctor-inspect-batch"),
        ScriptedPhase::AccessBatch => marker_paths("effigy-workspace-doctor-access-batch"),
        _ => Vec::new(),
    };

    ScriptedRequest {
        phase,
        program: program.to_owned(),
        args: args.to_vec(),
        paths,
        user,
        mutating,
    }
}

/// Intercept the shared `run_command_capture_until` boundary. Returns `None`
/// when no scripted runtime is installed, or when the phase is passing through
/// to the real backend for the bounded crosscheck.
pub(in crate::runner) fn intercept_capture(
    program: &OsStr,
    args: &[OsString],
    deadline: Option<Instant>,
) -> Option<Result<Output, RunnerError>> {
    let program = program.to_string_lossy().into_owned();
    let args = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    with_runtime(|runtime| {
        let request = classify(&program, &args);
        if runtime.is_passthrough(request.phase) {
            return None;
        }
        Some(runtime.run(request, deadline))
    })
}

/// Intercept the preliminary Colima `status` probe.
pub(in crate::runner) fn intercept_colima_status(
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Option<Result<Output, RunnerError>> {
    with_runtime(|runtime| {
        if runtime.is_passthrough(ScriptedPhase::ColimaStatus) {
            return None;
        }
        let request = ScriptedRequest {
            phase: ScriptedPhase::ColimaStatus,
            program: "colima".to_owned(),
            args: vec![
                "status".to_owned(),
                "--profile".to_owned(),
                policy.profile.clone(),
            ],
            paths: Vec::new(),
            user: None,
            mutating: false,
        };
        Some(runtime.run(request, deadline))
    })
}

/// Intercept the primary-service compose `ps` probe that follows a running
/// Colima status.
pub(in crate::runner) fn intercept_liveness_ps(
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Option<Result<Output, RunnerError>> {
    with_runtime(|runtime| {
        if runtime.is_passthrough(ScriptedPhase::LivenessComposePs) {
            return None;
        }
        let request = ScriptedRequest {
            phase: ScriptedPhase::LivenessComposePs,
            program: "colima".to_owned(),
            args: vec![
                "nerdctl".to_owned(),
                "--profile".to_owned(),
                policy.profile.clone(),
                "--".to_owned(),
                "compose".to_owned(),
                "ps".to_owned(),
            ],
            paths: Vec::new(),
            user: None,
            mutating: false,
        };
        Some(runtime.run(request, deadline))
    })
}

/// Intercept the service-container-name resolve that the Colima direct-exec
/// path performs before the first exec. Returns the resolved container. The
/// caller's deadline is honored so the scripted boundary cannot claim a
/// resolution that production would have refused once the budget expired.
pub(in crate::runner) fn intercept_service_name(
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Option<Result<Option<String>, RunnerError>> {
    with_runtime(|runtime| {
        if runtime.is_passthrough(ScriptedPhase::ServiceResolve) {
            return None;
        }
        let request = ScriptedRequest {
            phase: ScriptedPhase::ServiceResolve,
            program: "colima".to_owned(),
            args: vec![
                "nerdctl".to_owned(),
                "--profile".to_owned(),
                policy.profile.clone(),
                "--".to_owned(),
                "compose".to_owned(),
                "ps".to_owned(),
                "-q".to_owned(),
            ],
            paths: Vec::new(),
            user: None,
            mutating: false,
        };
        Some(match runtime.run(request, deadline) {
            Ok(output) => {
                let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                Ok((!name.is_empty()).then_some(name))
            }
            Err(error) => Err(error),
        })
    })
}

/// How the scripted Colima backend answers the liveness probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) enum ScriptedStatus {
    Running,
    Stopped,
    Timeout,
}

/// A private fake-VM fixture. The default answers model a healthy owned Rust
/// workspace; switches model wrong ownership and the bounded negative controls
/// without touching production code.
#[derive(Clone, Debug)]
pub(in crate::runner) struct OwnershipFixture {
    pub status: ScriptedStatus,
    pub primary_running: bool,
    pub identity: (u32, u32),
    /// Answer this path as `file 0 0 644` and its access as unwritable.
    pub bad_path: Option<String>,
    pub bun_install: Option<String>,
    /// Per-path metadata record overrides (`"1 2 755"`, `missing`, `symlink`).
    pub path_records: BTreeMap<String, String>,
    /// Per-path access overrides (`read-write` or `unwritable`).
    pub access_records: BTreeMap<String, String>,
    pub service_name: Option<String>,
    /// Negative control: force one phase to answer as a timeout.
    pub timeout_phase: Option<ScriptedPhase>,
}

impl Default for OwnershipFixture {
    fn default() -> Self {
        Self {
            status: ScriptedStatus::Running,
            primary_running: true,
            identity: (501, 20),
            bad_path: None,
            bun_install: None,
            path_records: BTreeMap::new(),
            access_records: BTreeMap::new(),
            service_name: Some("demo-stack-1".to_owned()),
            timeout_phase: None,
        }
    }
}

impl OwnershipFixture {
    fn metadata_record(&self, path: &str) -> String {
        if let Some(record) = self.path_records.get(path) {
            if record == "missing" || record == "symlink" {
                return record.clone();
            }
            return format!("dir {record}");
        }
        if self.bad_path.as_deref() == Some(path) {
            return "file 0 0 644".to_owned();
        }
        "dir 501 20 755".to_owned()
    }

    fn access_record(&self, path: &str) -> String {
        if let Some(record) = self.access_records.get(path) {
            return record.clone();
        }
        if self.bad_path.as_deref() == Some(path) {
            return "unwritable".to_owned();
        }
        "read-write".to_owned()
    }

    fn compose_ps_row(&self, project_name: &str, working_dir: &str, service: &str) -> String {
        format!("{project_name}-{service}-1\tUp 2 minutes\t\t{project_name}\t{working_dir}\t{service}\t0")
    }
}

/// Build the handler for a private fake-VM fixture. It answers only what the
/// production argv asks for, so an unexpected launch fails the test loudly.
pub(in crate::runner) fn ownership_handler(
    fixture: OwnershipFixture,
    project_name: String,
    primary_service: String,
    working_dir: String,
) -> Box<ScriptedHandler> {
    Box::new(move |request: &ScriptedRequest| {
        if fixture.timeout_phase == Some(request.phase) {
            return ScriptedAnswer::timed_out();
        }
        match request.phase {
            ScriptedPhase::ColimaStatus => match fixture.status {
                ScriptedStatus::Running => ScriptedAnswer::ok("status: Running\n"),
                ScriptedStatus::Stopped => ScriptedAnswer::ok("status: Stopped\n"),
                ScriptedStatus::Timeout => ScriptedAnswer::timed_out(),
            },
            ScriptedPhase::LivenessComposePs => {
                if fixture.primary_running {
                    ScriptedAnswer::ok(format!(
                        "{}\n",
                        fixture.compose_ps_row(&project_name, &working_dir, &primary_service)
                    ))
                } else {
                    ScriptedAnswer::ok("")
                }
            }
            ScriptedPhase::ServiceResolve => ScriptedAnswer::ok(format!(
                "{}\n",
                fixture.service_name.clone().unwrap_or_default()
            )),
            ScriptedPhase::BunProbe => ScriptedAnswer::ok(match fixture.bun_install.as_deref() {
                Some(path) => format!("{path}/install\n"),
                None => String::new(),
            }),
            ScriptedPhase::Identity => {
                ScriptedAnswer::ok(format!("{}\n{}\n", fixture.identity.0, fixture.identity.1))
            }
            ScriptedPhase::MetadataBatch => {
                let records = request
                    .paths
                    .iter()
                    .map(|path| fixture.metadata_record(path))
                    .collect::<Vec<_>>()
                    .join("\n");
                ScriptedAnswer::ok(format!("{records}\n"))
            }
            ScriptedPhase::AccessBatch => {
                let records = request
                    .paths
                    .iter()
                    .map(|path| fixture.access_record(path))
                    .collect::<Vec<_>>()
                    .join("\n");
                ScriptedAnswer::ok(format!("{records}\n"))
            }
            ScriptedPhase::Unexpected => ScriptedAnswer::failed(127, "unexpected scripted launch"),
        }
    })
}
