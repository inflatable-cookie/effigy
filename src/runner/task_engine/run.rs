//! One task invocation as one kernel `Run` (contract 052 §5.6, §10.1).
//!
//! Admission takes `Gate(Task)` for the resolved identity, settles any
//! interrupted predecessor (K5) and commits `Admitted` before the first
//! effect (K3). The gate is the run's liveness (K2): there is no heartbeat,
//! no staleness and no unlock. Closing records the workload outcome, settles
//! every effect and releases the gate; journal failures become warnings and
//! never change the task's own result (K10).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use effigy_kernel::ctx::{EffectClass, Placement};
use effigy_kernel::gate::{GateError, HolderDiag, Wait};
use effigy_kernel::id::{
    Digest, ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId, Route, RunId, TaskId,
};
use effigy_kernel::ident::ProcessState;
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::{CleanupOutcome, WorkloadOutcome};
use effigy_kernel::run::{BeginOpts, Run, RunError};
use effigy_kernel::store::Store;
use effigy_kernel::supervise::{LifecyclePlan, Placement as LifecyclePlacement};

use super::identity::TaskTarget;
use super::kernel::{self, TaskSettlement};
use super::{checkout, holders};
use crate::runner::error::RunnerError;

pub(in crate::runner) const LOCK_WAIT_ENV: &str = "EFFIGY_LOCK_WAIT_MS";
/// Marks a built-in's private exclusive scope, distinct from any task's
/// `lock = "<name>"`.
pub(in crate::runner) const BUILTIN_LOCK_PREFIX: &str = "@";

/// `shared:<name>` for a task-declared lock, the bare scope for a built-in's.
pub(in crate::runner) fn shared_scope_label(lock: &str) -> String {
    match lock.strip_prefix(BUILTIN_LOCK_PREFIX) {
        Some(scope) => scope.to_owned(),
        None => format!("shared:{lock}"),
    }
}
/// Provenance key under which a managed run's token is recorded, as a digest.
pub(in crate::runner) const MANAGED_RUN_PROVENANCE: &str = "managed-run-token";
/// Retention per close: bounded, Mutate-only, never an unsettled run (K15).
const RETENTION_PER_CLOSE: usize = 8;

/// `--lock-wait-ms`, else `EFFIGY_LOCK_WAIT_MS`, else fail fast.
pub(in crate::runner) fn resolve_lock_wait_ms(explicit: Option<u64>) -> Result<u64, RunnerError> {
    if let Some(value) = explicit {
        return Ok(value);
    }
    match std::env::var(LOCK_WAIT_ENV) {
        Ok(raw) if raw.trim().is_empty() => Ok(0),
        Ok(raw) => raw.trim().parse::<u64>().map_err(|_| {
            RunnerError::task_invocation(format!(
                "`{LOCK_WAIT_ENV}` must be a non-negative integer millisecond count"
            ))
        }),
        Err(_) => Ok(0),
    }
}

pub(in crate::runner) struct TaskRunRequest<'a> {
    pub(in crate::runner) target: &'a TaskTarget,
    pub(in crate::runner) lock_wait_ms: u64,
    /// The run will spawn under kernel custody, so it gets a sentinel.
    pub(in crate::runner) custody: bool,
    pub(in crate::runner) managed_run_token: Option<&'a str>,
    pub(in crate::runner) args: &'a [String],
    /// A caller's `--run-id` key: it names this run and no other.
    pub(in crate::runner) key: Option<&'a RunKey>,
    /// The task's declared admission class (Q-006).
    pub(in crate::runner) admission: Option<effigy_manifest::ManifestTaskAdmission>,
}

/// A caller-chosen `--run-id` key and the one run id it derives. Equal keys
/// derive equal ids on every host, so the journal's unique run id makes a
/// repeat or a racing second invocation find the first run instead of
/// starting another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct RunKey {
    key: String,
    id: RunId,
}

impl RunKey {
    pub(in crate::runner) const MAX: usize = 128;

    pub(in crate::runner) fn parse(key: &str) -> Result<Self, RunnerError> {
        if key.is_empty()
            || key.len() > Self::MAX
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
        {
            return Err(RunnerError::task_invocation(format!(
                "`--run-id` must be 1-{} characters of letters, digits, `.`, `_`, `:` or `-`",
                Self::MAX
            )));
        }
        let digest = Digest::of(format!("effigy-run-key\0{key}").as_bytes());
        let mut random = [0u8; 10];
        random.copy_from_slice(&digest.0[..10]);
        // Time 0 marks a keyed id; the key's digest is its entropy.
        let id = RunId::new(0, random)
            .map_err(|error| RunnerError::task_invocation(format!("run id: {error}")))?;
        Ok(Self {
            key: key.to_owned(),
            id,
        })
    }

    pub(in crate::runner) fn key(&self) -> &str {
        &self.key
    }

    pub(in crate::runner) fn id(&self) -> RunId {
        self.id
    }
}

#[derive(Debug)]
pub(in crate::runner) struct TaskRun {
    run: Option<Run<'static>>,
    shared: Option<Run<'static>>,
    target: TaskTarget,
    /// Held for the run's life; the run's settle stops the hook.
    admission: Option<super::admission::Admission>,
    /// Named by a caller's `--run-id` key: its output is logged for
    /// `effigy run logs`.
    keyed: bool,
}

impl TaskRun {
    pub(in crate::runner) fn begin(request: TaskRunRequest<'_>) -> Result<Self, RunnerError> {
        Self::begin_with(request, true)
    }

    /// A checkout-wide exclusive section, held by a run of its own (I-G4):
    /// built-ins and helpers that serialise work no task owns. `shared:<name>`
    /// is the same lock a task declares with `lock = "<name>"`; any other
    /// scope is private to built-ins. The section closes when dropped.
    pub(in crate::runner) fn exclusive(
        root: &std::path::Path,
        scope: &str,
        lock_wait_ms: u64,
    ) -> Result<ExclusiveSection, RunnerError> {
        let lock = match scope.strip_prefix("shared:") {
            Some(name) => name.to_owned(),
            None => format!("{BUILTIN_LOCK_PREFIX}{scope}"),
        };
        let target = TaskTarget {
            root: root.to_path_buf(),
            catalog: String::new(),
            task: format!("lock:{lock}"),
            surface: effigy_core::task_selection::TaskSurface::Published,
            profile: None,
            shared_lock: Some(lock),
            label: scope.to_owned(),
        };
        Self::begin_with(
            TaskRunRequest {
                target: &target,
                lock_wait_ms,
                custody: false,
                managed_run_token: None,
                args: &[],
                key: None,
                admission: None,
            },
            false,
        )
        .map(|run| ExclusiveSection(Some(run)))
    }

    /// A run for an operation no single task owns, such as one QA-group run.
    /// Its owner is fresh, so concurrent runs stay independent; the kernel
    /// run id addresses it.
    pub(in crate::runner) fn standalone(
        root: &std::path::Path,
        label: &str,
        admission: Option<effigy_manifest::ManifestTaskAdmission>,
    ) -> Result<Self, RunnerError> {
        let nonce = new_run_id()?;
        let lock = format!("{BUILTIN_LOCK_PREFIX}run:{nonce}");
        let target = TaskTarget {
            root: root.to_path_buf(),
            catalog: String::new(),
            task: label.to_owned(),
            surface: effigy_core::task_selection::TaskSurface::Published,
            profile: None,
            shared_lock: Some(lock),
            label: label.to_owned(),
        };
        Self::begin_with(
            TaskRunRequest {
                target: &target,
                lock_wait_ms: 0,
                custody: false,
                managed_run_token: None,
                args: &[],
                key: None,
                admission,
            },
            false,
        )
    }

    fn begin_with(request: TaskRunRequest<'_>, task_gate: bool) -> Result<Self, RunnerError> {
        let root = kernel::effects_root()?;
        let checkout = checkout::mint(&request.target.root)?;
        let owner = request.target.owner(checkout)?;
        let task_id = request.target.task_id(checkout)?;
        let admission = super::admission::plan(request.admission)?;
        let run_id = match request.key {
            Some(key) => {
                refuse_bound_key(key, &task_id)?;
                key.id()
            }
            None => new_run_id()?,
        };
        let custody = if request.custody || admission.needs_custody() {
            kernel::install_router()?;
            Some(LifecyclePlan {
                executable: kernel::executable()?,
                placement: LifecyclePlacement::Inline,
            })
        } else {
            None
        };
        let started = Instant::now();
        let wait = if request.lock_wait_ms == 0 {
            Wait::FailFast
        } else {
            Wait::Until(started + Duration::from_millis(request.lock_wait_ms))
        };
        let next = kernel::next_invocation(&request.target.label)?;
        let cx = kernel::effects();
        let cleanup = kernel::cleanup()?;
        let mut provenance = BTreeMap::new();
        if let Some(token) = request.managed_run_token {
            provenance.insert(
                MANAGED_RUN_PROVENANCE.to_owned(),
                Digest::of(token.as_bytes()),
            );
        }
        let argv = std::iter::once(request.target.label.clone())
            .chain(request.args.iter().cloned())
            .map(OsString::from)
            .collect::<Vec<_>>();

        // A shared lock is its own resource with its own run, held around
        // the task's run and released after it, always in that order.
        let shared = match request.target.shared_lock_owner(checkout)? {
            Some(lock_owner) => {
                let invocation =
                    invocation(&task_id, lock_owner.clone(), argv.clone(), &provenance)?;
                let run = Run::begin(
                    &cx,
                    root,
                    &invocation,
                    BeginOpts {
                        id: new_run_id()?,
                        wait,
                        acknowledge: Vec::new(),
                        next: next.clone(),
                        custody: None,
                    },
                    &cleanup,
                    &mut TaskSettlement,
                )
                .map_err(|error| {
                    begin_error(
                        error,
                        request.target,
                        &lock_owner,
                        request.lock_wait_ms,
                        started,
                    )
                })?;
                Some(run)
            }
            None => None,
        };
        if !task_gate {
            let mut run = Self {
                run: shared,
                shared: None,
                target: request.target.clone(),
                admission: None,
                keyed: false,
            };
            run.admit(request.admission, admission)?;
            return Ok(run);
        }
        let invocation = invocation(&task_id, owner.clone(), argv, &provenance)?;
        let run = Run::begin(
            &cx,
            root,
            &invocation,
            BeginOpts {
                id: run_id,
                wait,
                acknowledge: Vec::new(),
                next,
                custody,
            },
            &cleanup,
            &mut TaskSettlement,
        )
        .map_err(|error| match request.key {
            // The racing first invocation with this key holds the gate, or
            // finished between our check and our insert: attach to it.
            Some(key) if key_run_exists(key) => RunnerError::RunKeyExists {
                run: key.id().to_string(),
            },
            _ => begin_error(error, request.target, &owner, request.lock_wait_ms, started),
        });
        let run = match run {
            Ok(run) => run,
            Err(error) => {
                if let Some(shared) = shared {
                    let _ = close_run(shared, WorkloadOutcome::NotStarted);
                }
                return Err(error);
            }
        };
        let mut run = Self {
            run: Some(run),
            shared,
            target: request.target.clone(),
            admission: None,
            keyed: request.key.is_some(),
        };
        run.admit(request.admission, admission)?;
        Ok(run)
    }

    /// Calls the admission hook before any workload effect. A refusal
    /// closes the run as not started, so no task command ever runs.
    fn admit(
        &mut self,
        class: Option<effigy_manifest::ManifestTaskAdmission>,
        plan: super::admission::Plan,
    ) -> Result<(), RunnerError> {
        let (Some(class), super::admission::Plan::Hook { command, timeout }) = (class, plan) else {
            return Ok(());
        };
        let target = self.target.clone();
        let admitted = match self.run.as_mut() {
            Some(run) => super::admission::admit(run, &target, class, &command, timeout),
            None => return Ok(()),
        };
        match admitted {
            Ok(admission) => {
                self.admission = Some(admission);
                Ok(())
            }
            Err(error) => {
                let run = std::mem::replace(
                    self,
                    Self {
                        run: None,
                        shared: None,
                        target,
                        admission: None,
                        keyed: false,
                    },
                );
                for warning in run.close(WorkloadOutcome::NotStarted) {
                    eprintln!("warning: {warning}");
                }
                Err(error)
            }
        }
    }

    /// How long the admission hook took to admit this run, when one did.
    pub(in crate::runner) fn admission_wait_ms(&self) -> Option<u64> {
        self.admission
            .as_ref()
            .map(|admission| admission.waited.as_millis().min(u128::from(u64::MAX)) as u64)
    }

    pub(in crate::runner) fn run_mut(&mut self) -> &mut Run<'static> {
        self.run.as_mut().expect("open run")
    }

    pub(in crate::runner) fn id(&self) -> RunId {
        self.run.as_ref().expect("open run").id()
    }

    pub(in crate::runner) fn target(&self) -> &TaskTarget {
        &self.target
    }

    /// Whether a caller's `--run-id` key names this run.
    pub(in crate::runner) fn keyed(&self) -> bool {
        self.keyed
    }

    /// Records the workload outcome, settles every effect and releases the
    /// gates. Returns warnings only: the task's result is already decided.
    pub(in crate::runner) fn close(mut self, workload: WorkloadOutcome) -> Vec<String> {
        let mut warnings = Vec::new();
        // Settle stops the admission hook; the marker goes with it.
        let admission = self.admission.take();
        if let Some(run) = self.run.take() {
            warnings.extend(close_run(run, workload.clone()));
        }
        if let Some(shared) = self.shared.take() {
            let mirrored = match workload {
                WorkloadOutcome::Failed { .. } | WorkloadOutcome::Unknown(_) => {
                    WorkloadOutcome::Succeeded
                }
                other => other,
            };
            warnings.extend(close_run(shared, mirrored));
        }
        drop(admission);
        retain();
        warnings
    }

    /// A live run of root task `task`, held by this process.
    #[cfg(test)]
    pub(in crate::runner) fn hold_for_tests(
        root: &std::path::Path,
        task: &str,
        managed_run_token: Option<&str>,
    ) -> Self {
        let target = TaskTarget {
            root: root.to_path_buf(),
            catalog: String::new(),
            task: task.to_owned(),
            surface: effigy_core::task_selection::TaskSurface::Published,
            profile: None,
            shared_lock: None,
            label: task.to_owned(),
        };
        Self::begin(TaskRunRequest {
            target: &target,
            lock_wait_ms: 0,
            custody: false,
            managed_run_token,
            args: &[],
            key: None,
            admission: None,
        })
        .expect("live task fixture")
    }

    /// Drops the gates without closing the row: what a killed owner leaves.
    #[cfg(test)]
    pub(in crate::runner) fn abandon_for_tests(mut self) {
        drop(self.run.take());
        drop(self.shared.take());
    }
}

/// An exclusive section's run, recorded as a clean finish when dropped.
#[derive(Debug)]
pub(in crate::runner) struct ExclusiveSection(Option<TaskRun>);
impl Drop for ExclusiveSection {
    fn drop(&mut self) {
        if let Some(run) = self.0.take() {
            run.close(WorkloadOutcome::Succeeded);
        }
    }
}

impl Drop for TaskRun {
    /// An unclosed run (an early return or panic) still settles; the
    /// journal records it interrupted rather than claiming an outcome.
    fn drop(&mut self) {
        let cleanup = kernel::cleanup();
        for run in [self.run.take(), self.shared.take()].into_iter().flatten() {
            if let Ok(cleanup) = cleanup.as_ref() {
                let _ = run.settle(cleanup, &mut TaskSettlement);
            }
        }
    }
}

fn close_run(mut run: Run<'static>, workload: WorkloadOutcome) -> Vec<String> {
    let mut warnings = Vec::new();
    let cleanup = match kernel::cleanup() {
        Ok(cleanup) => cleanup,
        Err(error) => return vec![error.to_string()],
    };
    let recorded = match workload {
        WorkloadOutcome::Cancelled { .. }
        | WorkloadOutcome::TimedOut { .. }
        | WorkloadOutcome::NotStarted => run.interrupt(&cleanup, workload),
        other => run.finish(other),
    };
    if let Err(error) = recorded {
        warnings.push(format!(
            "run outcome was not recorded: {}",
            run_error(&error)
        ));
    }
    match run.settle(&cleanup, &mut TaskSettlement) {
        Ok(outcome) => {
            if outcome.cleanup() == CleanupOutcome::Incomplete {
                warnings.push(
                    "cleanup is incomplete: a process of this run could not be proven stopped; \
                     rerunning the task retries its settle"
                        .to_owned(),
                );
            }
        }
        Err(error) => warnings.push(format!("run settle failed: {}", run_error(&error))),
    }
    warnings
}

fn retain() {
    let Ok(root) = kernel::effects_root() else {
        return;
    };
    let cx = kernel::effects();
    let Ok(mut store) = Store::open(&cx.child(Duration::from_secs(5)), root) else {
        return;
    };
    let _ = effigy_kernel::retention::retain(
        &cx.child(Duration::from_secs(5)),
        root,
        &mut store,
        effigy_kernel::retention::RetentionBudget {
            max_removals: RETENTION_PER_CLOSE,
        },
    );
}

fn invocation(
    task: &TaskId,
    owner: ResourceId,
    argv: Vec<OsString>,
    provenance: &BTreeMap<String, Digest>,
) -> Result<ResolvedInvocation, RunnerError> {
    ResolvedInvocation::new(InvocationSpec {
        task: task.clone(),
        route: Route::Task,
        argv,
        cwd: task.catalog().to_owned(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner,
        gates: Vec::new(),
        attachments: Vec::new(),
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: provenance.clone(),
    })
    .map_err(|error| RunnerError::task_invocation(format!("task invocation is invalid: {error}")))
}

/// A key already naming a run of another task is refused: a key names
/// exactly one run. A key naming a run of this task attaches to it.
fn refuse_bound_key(key: &RunKey, task: &TaskId) -> Result<(), RunnerError> {
    let cx = kernel::observe();
    let root = match kernel::observe_root(&cx) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(()),
        Observed::Unknown(reason) => return Err(unverifiable_key(key, &format!("{reason:?}"))),
    };
    let reader = match Store::read(&cx, &root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Ok(()),
        Observed::Unknown(reason) => return Err(unverifiable_key(key, &format!("{reason:?}"))),
    };
    match effigy_kernel::run::summary(&reader, key.id()) {
        Observed::Absent(_) => Ok(()),
        Observed::Verified(existing, _) => {
            let same = existing
                .invocation
                .as_ref()
                .is_some_and(|invocation| invocation.spec().task.resource() == task.resource());
            if same {
                Err(RunnerError::RunKeyExists {
                    run: key.id().to_string(),
                })
            } else {
                let other = existing
                    .invocation
                    .as_ref()
                    .map(|invocation| format!("`{}`", invocation.spec().task.task()))
                    .unwrap_or_else(|| "another task".to_owned());
                Err(RunnerError::task_invocation(format!(
                    "`--run-id {}` already names run {} of {other}; a key names exactly one run. \
                     Choose a new key, or inspect that run with `effigy run status {}`",
                    key.key(),
                    key.id(),
                    key.key()
                )))
            }
        }
        Observed::Unknown(reason) => Err(unverifiable_key(key, &format!("{reason:?}"))),
    }
}

fn unverifiable_key(key: &RunKey, reason: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "`--run-id {}` cannot be checked against the run journal ({reason}); nothing was \
         started, so a repeat can never run twice",
        key.key()
    ))
}

/// Whether the key's run appeared: a racing invocation may hold the gate
/// before its row commits, so the check waits briefly for the row.
fn key_run_exists(key: &RunKey) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let cx = kernel::observe();
        if let Observed::Verified(root, _) = kernel::observe_root(&cx) {
            if let Observed::Verified(reader, _) = Store::read(&cx, &root) {
                if matches!(
                    effigy_kernel::run::summary(&reader, key.id()),
                    Observed::Verified(..)
                ) {
                    return true;
                }
            }
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A fresh random run id, for requests and runs no key names.
pub(super) fn fresh_run_id() -> Result<RunId, RunnerError> {
    new_run_id()
}

fn new_run_id() -> Result<RunId, RunnerError> {
    let mut random = [0u8; 10];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|error| RunnerError::task_invocation(format!("run id entropy: {error}")))?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    RunId::new(millis, random)
        .map_err(|error| RunnerError::task_invocation(format!("run id: {error}")))
}

fn run_error(error: &RunError) -> String {
    match error {
        RunError::Store(error) => format!("state store: {error}"),
        RunError::Gate(error) => format!("gate: {error}"),
        other => format!("{other:?}"),
    }
}

fn begin_error(
    error: RunError,
    target: &TaskTarget,
    owner: &ResourceId,
    lock_wait_ms: u64,
    started: Instant,
) -> RunnerError {
    match error {
        RunError::Gate(GateError::Busy { holder, .. }) => {
            conflict(target, owner, *holder, None, started)
        }
        RunError::Gate(GateError::TimedOut { holder, waited, .. }) => conflict(
            target,
            owner,
            *holder,
            Some((lock_wait_ms, waited.as_millis() as u64)),
            started,
        ),
        RunError::PredecessorUnsettled { run, effects } => RunnerError::task_invocation(format!(
            "`{}` cannot start: its previous run {run} left {} unsettled effect(s) that could not \
             be proven stopped. Inspect with `effigy tasks status {}`; nothing was started",
            target.label,
            effects.len(),
            target.label
        )),
        RunError::HostAuthorityRequired | RunError::Gate(GateError::HostAuthorityRequired) => {
            RunnerError::task_invocation(format!(
                "`{}` needs host authority; run it on the host",
                target.label
            ))
        }
        other => RunnerError::task_invocation(format!(
            "`{}` could not be admitted: {}",
            target.label,
            run_error(&other)
        )),
    }
}

/// A busy gate names its live holder from the gate probe and the holder's
/// own run row. It never suggests breaking the lock: holding is liveness.
fn conflict(
    target: &TaskTarget,
    owner: &ResourceId,
    holder: Observed<HolderDiag>,
    wait: Option<(u64, u64)>,
    started: Instant,
) -> RunnerError {
    let diag = holder.verified().cloned();
    let run_started = holders::holder_run_started_ms(owner);
    let scope = match &target.shared_lock {
        Some(name) if owner.kind() == effigy_kernel::id::ResourceKind::Store => {
            shared_scope_label(name)
        }
        _ => format!("task:{}", target.label),
    };
    // A shared or built-in lock's holder may be another task or command:
    // the inventory names every live holder. A built-in section has no
    // selector to re-run with a wait.
    let builtin = target
        .shared_lock
        .as_deref()
        .is_some_and(|lock| lock.starts_with(BUILTIN_LOCK_PREFIX));
    let status_command = match target.shared_lock {
        Some(_) => "effigy tasks status --all".to_owned(),
        None => format!("effigy tasks status {}", target.label),
    };
    let holder_text = match &diag {
        Some(diag) => {
            let mut parts = vec![format!("pid {} on this host", diag.pid)];
            if let Some(run) = &diag.run {
                parts.push(format!("run {run}"));
            }
            if let Some(age) = run_started.and_then(age_text) {
                parts.push(format!("started {age} ago"));
            }
            if matches!(diag.state, Observed::Verified(ProcessState::Stopped, _)) {
                parts.push("stopped (SIGSTOP); it keeps its lock until resumed".to_owned());
            }
            parts.join(", ")
        }
        None => "a live process on this host; its diagnostics are unreadable".to_owned(),
    };
    let what =
        if target.shared_lock.is_some() && owner.kind() == effigy_kernel::id::ResourceKind::Store {
            format!("lock `{scope}` is held")
        } else {
            format!("`{}` is already running", target.label)
        };
    let wait_flag = (!builtin).then(|| format!("effigy --lock-wait-ms 600000 {}", target.label));
    let remediation = match (wait, &wait_flag) {
        (Some((timeout, waited)), Some(wait_flag)) => format!(
            "{what} ({holder_text}); waited {waited}ms of {timeout}ms. Inspect it with \
             `{status_command}`, wait longer with `{wait_flag}`, or stop that run"
        ),
        (Some((timeout, waited)), None) => format!(
            "{what} ({holder_text}); waited {waited}ms of {timeout}ms. Inspect it with \
             `{status_command}`, let it finish, or stop that run"
        ),
        (None, Some(wait_flag)) => format!(
            "{what} ({holder_text}). Inspect it with `{status_command}`, wait for it with \
             `{wait_flag}`, or stop that run"
        ),
        (None, None) => format!(
            "{what} ({holder_text}). Inspect it with `{status_command}`, let it finish, or \
             stop that run"
        ),
    };
    let details_json = serde_json::json!({
        "schema": "effigy.lock-wait.v2",
        "schema_version": 2,
        "scope": scope,
        "task": target.label,
        "holder": diag.as_ref().map(|diag| serde_json::json!({
            "pid": diag.pid,
            "run": diag.run,
            "start": diag.start,
            "state": match &diag.state {
                Observed::Verified(ProcessState::Stopped, _) => "stopped",
                Observed::Verified(_, _) => "running",
                _ => "unknown",
            },
        })),
        "holder_started_at_epoch_ms": run_started,
        "wait_timeout_ms": wait.map(|(timeout, _)| timeout),
        "waited_ms": wait.map(|(_, waited)| waited).or_else(|| {
            Some(started.elapsed().as_millis() as u64)
        }),
        "status_command": status_command,
        "next": std::iter::once(format!("inspect the live run with `{status_command}`"))
            .chain(wait_flag.iter().map(|flag| format!("wait for it with `{flag}`")))
            .collect::<Vec<_>>(),
    })
    .to_string();
    RunnerError::TaskLockConflict(Box::new(effigy_core::task_lock::TaskLockConflict {
        scope,
        task: target.label.clone(),
        holder_pid: diag.as_ref().map(|diag| diag.pid),
        holder_run: diag.as_ref().and_then(|diag| diag.run.clone()),
        holder_started_at_epoch_ms: run_started,
        wait_timeout_ms: wait.map(|(timeout, _)| timeout),
        waited_ms: wait.map(|(_, waited)| waited),
        status_command: Some(status_command),
        details_json: Some(details_json),
        remediation,
    }))
}

fn age_text(started_ms: i64) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    let seconds = now.saturating_sub(started_ms).max(0) / 1000;
    Some(match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m{}s", seconds / 60, seconds % 60),
        _ => format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60),
    })
}
