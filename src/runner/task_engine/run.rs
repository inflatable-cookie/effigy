//! One task invocation as one kernel `Run` (contract 052 §5.6, §10.1).
//!
//! Admission takes `Gate(Task)` for the resolved identity, settles any
//! interrupted predecessor (K5) and commits `Admitted` before the first
//! effect (K3). The gate is the run's liveness (K2): there is no heartbeat,
//! no staleness and no unlock. Closing records the workload outcome, settles
//! every effect and releases the gate; journal failures become warnings and
//! never change the task's own result (K10).

pub(super) mod foreground;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read;
use std::sync::{Arc, Mutex, MutexGuard};
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
use effigy_kernel::store::{Store, StoreError};
use effigy_kernel::supervise::{LifecyclePlan, Placement as LifecyclePlacement};

use super::identity::TaskTarget;
use super::kernel::{self, TaskSettlement};
use super::{checkout, holders};
use crate::runner::error::RunnerError;

pub(in crate::runner) const LOCK_WAIT_ENV: &str = "EFFIGY_LOCK_WAIT_MS";
/// How long a key's check waits for a racing invocation's row to commit. The
/// writer holds the store's own gate and commits in a moment; a key that never
/// appears is absent, not refused. One cheap read per interval, never a tight
/// spin (effigy#339).
const KEY_JOURNAL_WAIT: Duration = Duration::from_secs(5);
const KEY_JOURNAL_POLL: Duration = Duration::from_millis(250);
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

/// `--lock-wait-ms`, else `EFFIGY_LOCK_WAIT_MS`, else fail fast.
pub(in crate::runner) fn resolve_lock_wait_ms(explicit: Option<u64>) -> Result<u64, RunnerError> {
    resolve_lock_wait_ms_or(explicit, 0)
}

/// `--lock-wait-ms`, else `EFFIGY_LOCK_WAIT_MS`, else `default_ms`.
///
/// `default_ms` is only for a target that opts into a wait (Q-015); an
/// explicit `0` from either source still fails at once, because the operator
/// named a wait and the default is what applies when nobody did.
pub(in crate::runner) fn resolve_lock_wait_ms_or(
    explicit: Option<u64>,
    default_ms: u64,
) -> Result<u64, RunnerError> {
    if let Some(value) = explicit {
        return Ok(value);
    }
    match std::env::var(LOCK_WAIT_ENV) {
        Ok(raw) if raw.trim().is_empty() => Ok(default_ms),
        Ok(raw) => raw.trim().parse::<u64>().map_err(|_| {
            RunnerError::task_invocation(format!(
                "`{LOCK_WAIT_ENV}` must be a non-negative integer millisecond count"
            ))
        }),
        Err(_) => Ok(default_ms),
    }
}

pub(in crate::runner) struct TaskRunRequest<'a> {
    pub(in crate::runner) target: &'a TaskTarget,
    pub(in crate::runner) lock_wait_ms: u64,
    /// The run will spawn under kernel custody, so it gets a sentinel.
    pub(in crate::runner) custody: bool,
    /// This run may get the configured per-run scratch directory: it is
    /// host-placed and its task did not opt out.
    pub(in crate::runner) scratch: bool,
    pub(in crate::runner) managed_run_token: Option<&'a str>,
    pub(in crate::runner) args: &'a [String],
    /// A caller's `--run-id` key: it names this run and no other.
    pub(in crate::runner) key: Option<&'a RunKey>,
    /// The task's declared admission class (Q-006).
    pub(in crate::runner) admission: Option<effigy_manifest::ManifestTaskAdmission>,
    /// A keyed workspace request's reservation, when a host dispatcher holds
    /// one for this exact run (052 §8.11, `EngineLaunch::admission`).
    ///
    /// Only a top-level task run may carry it: the run then begins with
    /// [`Run::begin_admitted`] under the ticket's own id and never mints one
    /// itself. A nested run, a caller key and a shared-lock run each own
    /// another run or another id, so they refuse a ticket before any effect.
    pub(in crate::runner) workspace_admission: Option<WorkspaceAdmission>,
    /// The resolved invocation directory, reported to the admission hook.
    pub(in crate::runner) cwd: Option<&'a std::path::Path>,
}

#[derive(Clone)]
pub(in crate::runner) struct WorkspaceAdmission {
    pub(in crate::runner) ticket: effigy_kernel::run::AdmissionTicket,
    pub(in crate::runner) catalog_snapshot: Option<Digest>,
}

/// A validated caller key. Admission binds it to the checkout before using
/// its id. The unbound digest is retained solely to locate imported old runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct RunKey {
    key: String,
    id: RunId,
}

#[derive(Debug)]
pub(super) struct KeyNamespace {
    pub ids: Vec<RunId>,
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

    pub(in crate::runner) fn scoped(
        &self,
        checkout: effigy_kernel::id::CheckoutId,
    ) -> Result<Self, RunnerError> {
        let mut bytes = b"effigy-checkout-run-key-v1\0".to_vec();
        bytes.push(checkout.kind() as u8);
        bytes.extend_from_slice(checkout.token());
        bytes.extend_from_slice(self.key.as_bytes());
        let digest = Digest::of(&bytes);
        let mut random = [0; 10];
        random.copy_from_slice(&digest.0[..10]);
        let id = RunId::new(0, random)
            .map_err(|error| RunnerError::task_invocation(format!("run id: {error}")))?;
        Ok(Self {
            key: self.key.clone(),
            id,
        })
    }

    pub(super) fn namespace(
        &self,
        cx: &effigy_kernel::ctx::ObserveContext,
        root: &effigy_kernel::root::StateRoot,
    ) -> Result<KeyNamespace, RunnerError> {
        if root.is_machine() {
            return Ok(KeyNamespace { ids: Vec::new() });
        }
        let checkouts = checkout::identities(cx, root.owning_directory().path())
            .map_err(|error| unverifiable_key(self, &error.to_string()))?;
        let ids = checkouts
            .into_iter()
            .map(|checkout| self.scoped(checkout).map(|key| key.id()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(KeyNamespace { ids })
    }
}

#[derive(Debug)]
pub(in crate::runner) struct TaskRun {
    run: Option<Arc<Mutex<Run<'static>>>>,
    memory: Option<Arc<Mutex<effigy_kernel::run::ForegroundRun>>>,
    shared: Option<Run<'static>>,
    target: TaskTarget,
    ancestors: Vec<ResourceId>,
    shared_owners: Vec<ResourceId>,
    _custody_scope: Option<kernel::CustodyWait>,
    /// Held for the run's life; the run's settle stops the hook.
    admission: Option<super::admission::Admission>,
    /// The configured scratch directory this run holds, when it has one.
    scratch: Option<crate::runner::scratch::Handle>,
    /// The program-fallback shims first on this run's PATH, when one is active.
    programs: Option<crate::runner::program_fallback::Handle>,
    /// Named by a caller's `--run-id` key: its output is logged for
    /// `effigy run logs`.
    keyed: bool,
}

#[derive(Clone)]
struct Parent {
    run: Option<Arc<Mutex<Run<'static>>>>,
    memory: Option<Arc<Mutex<effigy_kernel::run::ForegroundRun>>>,
    ancestors: Vec<ResourceId>,
    shared_owners: Vec<ResourceId>,
}
thread_local! {
    static PARENTS: RefCell<Vec<Parent>> = const { RefCell::new(Vec::new()) };
}
pub(in crate::runner) struct ParentScope;
impl Drop for ParentScope {
    fn drop(&mut self) {
        PARENTS.with(|parents| {
            parents.borrow_mut().pop();
        });
    }
}
fn owned<T: std::fmt::Debug>(run: Arc<Mutex<T>>) -> T {
    Arc::try_unwrap(run)
        .expect("parent scope ends before run closes")
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

enum Standalone {
    Memory,
    Journal { scratch: bool, custody: bool },
}

/// Why this run shape cannot honour an exact keyed reservation, if it cannot.
///
/// A ticket names exactly one run, so a shape that owns a different run — an
/// engine-internal or standalone run, a nested task, a caller-keyed run, or a
/// run that takes a task-declared shared lock first — cannot admit it. Each of
/// those refuses before any effect instead of admitting a second run or
/// discarding the reservation (052 §8.11).
pub(super) fn workspace_admission_refusal(
    task_gate: bool,
    nested: bool,
    keyed: bool,
    shared_lock: bool,
) -> Option<RunnerError> {
    let reason = match () {
        _ if !task_gate => "be an engine-internal or standalone run",
        _ if nested => "be a nested task run",
        _ if keyed => "carry a caller run key",
        _ if shared_lock => "hold a task-declared shared lock",
        _ => return None,
    };
    Some(RunnerError::task_invocation(format!(
        "a workspace-admitted run cannot {reason}; the reserved request names another run. Retry \
         the command from the workspace session"
    )))
}

impl TaskRun {
    pub(in crate::runner) fn begin(request: TaskRunRequest<'_>) -> Result<Self, RunnerError> {
        Self::begin_with(request, true, LifecyclePlacement::Inline)
    }

    /// A run owned by its detached supervisor (§5.8): this process is the
    /// run's only writer, and its sentinel settles the run if it dies.
    pub(in crate::runner) fn begin_detached(
        request: TaskRunRequest<'_>,
    ) -> Result<Self, RunnerError> {
        Self::begin_with(
            TaskRunRequest {
                custody: true,
                ..request
            },
            true,
            LifecyclePlacement::Detached,
        )
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
            argument_scope: None,
            label: scope.to_owned(),
        };
        Self::begin_with(
            TaskRunRequest {
                target: &target,
                lock_wait_ms,
                custody: false,
                scratch: false,
                managed_run_token: None,
                args: &[],
                key: None,
                admission: None,
                workspace_admission: None,
                cwd: None,
            },
            false,
            LifecyclePlacement::Inline,
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
        Self::standalone_with(
            root,
            label,
            admission,
            Standalone::Journal {
                scratch: false,
                custody: false,
            },
        )
    }

    /// A standalone run whose host processes may use the configured per-run
    /// scratch directory: the built-in `test` route runs a project's suites
    /// exactly as a manifest test task would.
    /// Its custody is its own: the run's suites are child runs, it has a
    /// sentinel, and its settle stops the whole tree before releasing the
    /// directory — including after the owner is killed.
    pub(in crate::runner) fn standalone_scratching(
        root: &std::path::Path,
        label: &str,
    ) -> Result<Self, RunnerError> {
        Self::standalone_with(
            root,
            label,
            None,
            Standalone::Journal {
                scratch: true,
                custody: true,
            },
        )
    }

    pub(in crate::runner) fn standalone_foreground(
        root: &std::path::Path,
        label: &str,
    ) -> Result<Self, RunnerError> {
        Self::standalone_with(root, label, None, Standalone::Memory)
    }

    pub(in crate::runner) fn standalone_custody(
        root: &std::path::Path,
        label: &str,
    ) -> Result<Self, RunnerError> {
        Self::standalone_with(
            root,
            label,
            None,
            Standalone::Journal {
                scratch: false,
                custody: true,
            },
        )
    }

    fn standalone_with(
        root: &std::path::Path,
        label: &str,
        admission: Option<effigy_manifest::ManifestTaskAdmission>,
        storage: Standalone,
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
            argument_scope: None,
            label: label.to_owned(),
        };
        let (scratch, custody) = match storage {
            Standalone::Memory => (false, true),
            Standalone::Journal { scratch, custody } => (scratch, custody),
        };
        let request = TaskRunRequest {
            target: &target,
            lock_wait_ms: 0,
            custody,
            scratch,
            managed_run_token: None,
            args: &[],
            key: None,
            admission,
            workspace_admission: None,
            cwd: Some(root),
        };
        match storage {
            Standalone::Memory => Self::begin_foreground_with(request, false),
            Standalone::Journal { .. } => {
                Self::begin_with(request, false, LifecyclePlacement::Inline)
            }
        }
    }

    fn begin_with(
        request: TaskRunRequest<'_>,
        task_gate: bool,
        placement: LifecyclePlacement,
    ) -> Result<Self, RunnerError> {
        let admission = super::admission::plan(request.admission)?;
        let parent = PARENTS.with(|parents| parents.borrow().last().cloned());
        let root = kernel::effects_root(&request.target.root)?;
        let checkout = checkout::mint(&request.target.root)?;
        let key = request.key.map(|key| key.scoped(checkout)).transpose()?;
        let request = TaskRunRequest {
            key: key.as_ref(),
            ..request
        };
        let owner = request.target.owner(checkout)?;
        let task_id = request.target.task_id(checkout)?;
        let ancestors = parent
            .as_ref()
            .map(|parent| parent.ancestors.clone())
            .unwrap_or_default();
        let mut shared_owners = parent
            .as_ref()
            .map(|parent| parent.shared_owners.clone())
            .unwrap_or_default();
        if ancestors.contains(&owner) {
            return Err(RunnerError::task_invocation(format!(
                "task reference cycle at `{}`: a nested reference cannot reacquire an ancestor's Task gate; remove the recursive reference",
                request.target.label)));
        }
        // A ticketed run is admitted exactly as reserved: its id, its key and
        // its durable reservation are the dispatcher's, so nothing here may
        // mint another id or start a second run (052 §8.11). Every shape that
        // would own a different run refuses before any effect.
        let shared_lock = request.target.shared_lock_owner(checkout)?;
        if request.workspace_admission.is_some() {
            if let Some(refusal) = workspace_admission_refusal(
                task_gate,
                parent.is_some(),
                request.key.is_some(),
                shared_lock.is_some(),
            ) {
                return Err(refusal);
            }
        }
        let run_id = match (&request.workspace_admission, request.key) {
            (Some(admission), _) => admission.ticket.run(),
            (None, Some(key)) => {
                refuse_bound_key(key, &task_id, &request.target.root)?;
                key.id()
            }
            (None, None) => new_run_id()?,
        };
        let needs_custody = request.custody || admission.needs_custody();
        let custody = if needs_custody {
            kernel::install_router()?;
            Some(LifecyclePlan {
                executable: kernel::executable()?,
                placement,
            })
        } else {
            None
        };
        let custody_scope = needs_custody.then(kernel::CustodyWait::enter);
        let started = Instant::now();
        let wait = if parent.is_some() || request.lock_wait_ms == 0 {
            Wait::FailFast
        } else {
            Wait::Until(started + Duration::from_millis(request.lock_wait_ms))
        };
        let next = kernel::next_invocation(&request.target.label)?;
        let cx = kernel::effects();
        let cleanup = kernel::cleanup()?;
        let mut provenance = BTreeMap::new();
        if let Some(snapshot) = request
            .workspace_admission
            .as_ref()
            .and_then(|admission| admission.catalog_snapshot.as_ref())
        {
            provenance.insert("workspace:catalog-snapshot".to_owned(), snapshot.clone());
        }
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
        // the task's run and released after it, always in that order. For a
        // standalone run that lock run *is* the run, so it must carry the
        // custody the caller asked for: without a sentinel a killed owner's
        // run is never settled and its scratch directory can never expire.
        let lock_custody = if task_gate { None } else { custody.clone() };
        let shared = match shared_lock.clone() {
            Some(lock_owner) if !shared_owners.contains(&lock_owner) => {
                shared_owners.push(lock_owner.clone());
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
                        custody: lock_custody,
                    },
                    &cleanup,
                    &mut TaskSettlement::default(),
                )
                .map_err(|error| {
                    begin_error(error, request.target, request.lock_wait_ms, started)
                })?;
                Some(run)
            }
            _ => None,
        };
        if !task_gate {
            let mut run = Self {
                run: shared.map(|run| Arc::new(Mutex::new(run))),
                memory: None,
                shared: None,
                target: request.target.clone(),
                ancestors,
                shared_owners,
                _custody_scope: custody_scope,
                admission: None,
                scratch: None,
                programs: None,
                keyed: false,
            };
            run.admit(request.admission, admission, request.cwd)?;
            // An engine-internal run (a lock section, a QA group) has no
            // opinion on scratch; the built-in test route asks for one and
            // holds the same scratch a task run does.
            if request.scratch {
                run.scratch = crate::runner::scratch::for_run(&run, true);
                run.programs = crate::runner::program_fallback::for_run().map_err(|error| {
                    RunnerError::task_invocation(format!("program fallback: {error}"))
                })?;
            }
            return Ok(run);
        }
        let invocation = invocation(&task_id, owner.clone(), argv, &provenance)?;
        let opts = BeginOpts {
            id: run_id,
            wait,
            acknowledge: Vec::new(),
            next,
            custody,
        };
        let run = match (request.workspace_admission.clone(), parent) {
            // The dispatcher's reservation, unchanged, under the same gates
            // and the same durable transaction: this is the only admission
            // this request gets.
            (Some(admission), _) => Run::begin_admitted(
                &cx,
                root,
                &invocation,
                opts,
                &cleanup,
                &mut TaskSettlement::default(),
                admission.ticket,
            ),
            (None, Some(parent)) => parent
                .run
                .expect("durable parent checked before effects")
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .begin_child(
                    &invocation,
                    BeginOpts {
                        wait: Wait::FailFast,
                        ..opts
                    },
                    &cleanup,
                    &mut TaskSettlement::default(),
                ),
            (None, None) => Run::begin(
                &cx,
                root,
                &invocation,
                opts,
                &cleanup,
                &mut TaskSettlement::default(),
            ),
        }
        .map_err(|error| match request.key {
            // The racing first invocation with this key holds the gate, or
            // finished between our check and our insert: attach to it.
            Some(key) if key_run_exists(key, &request.target.root) => RunnerError::RunKeyExists {
                run: key.id().to_string(),
            },
            _ => begin_error(error, request.target, request.lock_wait_ms, started),
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
            run: Some(Arc::new(Mutex::new(run))),
            memory: None,
            shared,
            target: request.target.clone(),
            ancestors,
            shared_owners,
            _custody_scope: custody_scope,
            admission: None,
            scratch: None,
            programs: None,
            keyed: request.key.is_some(),
        };
        run.admit(request.admission, admission, request.cwd)?;
        // The run's own scratch directory is opened before the first workload
        // runs and removed or kept when the run ends. A nested run reuses the
        // scratch its parent already opened.
        run.scratch = crate::runner::scratch::for_run(&run, request.scratch);
        // A per-program fallback shim is created for the run that activates
        // it, inside the run's scratch when it has one, and settles with it.
        run.programs = crate::runner::program_fallback::for_run()
            .map_err(|error| RunnerError::task_invocation(format!("program fallback: {error}")))?;
        Ok(run)
    }

    /// Calls the admission hook before any workload effect. A refusal
    /// closes the run as not started, so no task command ever runs.
    fn admit(
        &mut self,
        class: Option<effigy_manifest::ManifestTaskAdmission>,
        plan: super::admission::Plan,
        cwd: Option<&std::path::Path>,
    ) -> Result<(), RunnerError> {
        let (Some(class), super::admission::Plan::Hook { command, timeout }) = (class, plan) else {
            return Ok(());
        };
        let target = self.target.clone();
        let cwd = cwd.unwrap_or(&target.root).to_path_buf();
        let admitted = super::admission::admit(self, &target, class, &command, timeout, &cwd);
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
                        memory: None,
                        shared: None,
                        target,
                        ancestors: Vec::new(),
                        shared_owners: Vec::new(),
                        _custody_scope: None,
                        admission: None,
                        scratch: None,
                        programs: None,
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

    /// Whether this run's custody is the memory class (contract 052 Q-010):
    /// the only class admission protocol v2 can report custodians for, since
    /// a durable run's sentinel is not a memory guard.
    pub(in crate::runner) fn is_memory_class(&self) -> bool {
        self.memory.is_some()
    }

    /// The run's own identity and its memory guard's, the two custodians a
    /// v2 admission reports before any workload spawns. In-process work runs
    /// inside the owner and dies with it, so the owner covers it.
    pub(in crate::runner) fn custodian_identities(
        &self,
    ) -> (
        Option<effigy_kernel::ident::ProcessIdentity>,
        Option<effigy_kernel::ident::ProcessIdentity>,
    ) {
        let owner = effigy_kernel::ident::process_identity(std::process::id())
            .verified()
            .cloned();
        let guard = self.memory.as_ref().and_then(|memory| {
            memory
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .guard_identity()
                .cloned()
        });
        (owner, guard)
    }

    /// How long the admission hook took to admit this run, when one did.
    pub(in crate::runner) fn admission_wait_ms(&self) -> Option<u64> {
        self.admission
            .as_ref()
            .map(|admission| admission.waited.as_millis().min(u128::from(u64::MAX)) as u64)
    }

    /// Whether a caller's `--run-id` key names this run.
    pub(in crate::runner) fn keyed(&self) -> bool {
        self.keyed
    }

    pub(in crate::runner) fn run_mut(
        &mut self,
    ) -> Result<MutexGuard<'_, Run<'static>>, RunnerError> {
        self.run
            .as_ref()
            .ok_or_else(|| {
                RunnerError::task_invocation(
                    "this run keeps its record in memory, so it has no journal",
                )
            })
            .map(|run| {
                run.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
            })
    }

    /// Lexical nesting carries the parent's run and memory handle on this
    /// thread, so an in-process step is a child of this run.
    pub(in crate::runner) fn enter(&self) -> ParentScope {
        let mut ancestors = self.ancestors.clone();
        ancestors.push(self.owner());
        PARENTS.with(|parents| {
            parents.borrow_mut().push(Parent {
                run: self.run.clone(),
                memory: self.memory.clone(),
                ancestors,
                shared_owners: self.shared_owners.clone(),
            })
        });
        ParentScope
    }

    pub(in crate::runner) fn step(&self, index: usize, label: &str) -> Result<Self, RunnerError> {
        if self.memory.is_some() {
            return self.memory_step(index, label);
        }
        let checkout = checkout::mint(&self.target.root)?;
        let owner = ResourceId::store(
            effigy_kernel::id::Scope::Checkout(checkout),
            &format!(
                "task-step:{}:{index}",
                self.run
                    .as_ref()
                    .expect("open run")
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .fence()
                    .resource
                    .name()
            ),
        )
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let invocation = invocation(
            &self.target.task_id(checkout)?,
            owner,
            vec![OsString::from(label)],
            &BTreeMap::new(),
        )?;
        kernel::install_router()?;
        let custody_scope = kernel::CustodyWait::enter();
        let run = self
            .run
            .as_ref()
            .expect("open run")
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .begin_child(
                &invocation,
                BeginOpts {
                    id: new_run_id()?,
                    wait: Wait::FailFast,
                    acknowledge: Vec::new(),
                    next: kernel::next_invocation(&self.target.label)?,
                    custody: Some(LifecyclePlan {
                        executable: kernel::executable()?,
                        placement: LifecyclePlacement::Inline,
                    }),
                },
                &kernel::cleanup()?,
                &mut TaskSettlement::default(),
            )
            .map_err(|error| {
                RunnerError::task_invocation(format!("cannot admit step `{label}`: {error}"))
            })?;
        let mut target = self.target.clone();
        target.label = label.to_owned();
        let mut ancestors = self.ancestors.clone();
        ancestors.push(
            self.run
                .as_ref()
                .expect("open run")
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .fence()
                .resource
                .clone(),
        );
        Ok(Self {
            run: Some(Arc::new(Mutex::new(run))),
            memory: None,
            shared: None,
            target,
            ancestors,
            shared_owners: self.shared_owners.clone(),
            _custody_scope: Some(custody_scope),
            admission: None,
            scratch: crate::runner::scratch::inherited_handle(self.scratch.as_ref()),
            // A step runs inside its parent's shim scope on this thread.
            programs: None,
            keyed: false,
        })
    }

    pub(in crate::runner) fn id(&self) -> RunId {
        if let Some(memory) = &self.memory {
            return memory
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record()
                .incarnation()
                .run;
        }
        self.run
            .as_ref()
            .expect("open run")
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .id()
    }

    pub(in crate::runner) fn target(&self) -> &TaskTarget {
        &self.target
    }

    /// `close` with its own settle budget, returning the settled outcome a
    /// supervisor announces. `None` when the run could not settle now.
    pub(in crate::runner) fn close_settled(
        mut self,
        workload: WorkloadOutcome,
        budget: Duration,
    ) -> (
        Option<effigy_kernel::outcome::Outcome<effigy_kernel::run::Settled>>,
        Vec<String>,
    ) {
        let admission = self.admission.take();
        let target = self.target.clone();
        let run_id = self.id().to_string();
        self.leave_programs();
        let scratch = self.conclude_scratch(&workload);
        let result = match self.memory.take() {
            Some(memory) => foreground::close_memory(
                owned(memory),
                workload.clone(),
                &kernel::cleanup_context(budget),
                &mut |_| {},
            ),
            None => match self.run.take() {
                Some(run) => close_run_settled(
                    owned(run),
                    workload.clone(),
                    kernel::cleanup_context(budget),
                ),
                None => (None, Vec::new()),
            },
        };
        let (outcome, mut warnings) = result;
        if let Some(admission) = admission {
            warnings.extend(super::admission::finish(
                admission,
                &target,
                &run_id,
                &workload,
                outcome.as_ref(),
            ));
        }
        warnings.extend(scratch);
        (outcome, warnings)
    }

    /// Records the workload outcome, settles every effect and releases the
    /// gates. Returns warnings only: the task's result is already decided.
    pub(in crate::runner) fn close(mut self, workload: WorkloadOutcome) -> Vec<String> {
        let mut warnings = Vec::new();
        // Settle stops the admission hook; the marker goes with it, and a v2
        // admission reports its terminal result before that happens.
        let admission = self.admission.take();
        let target = self.target.clone();
        let run_id = self.id().to_string();
        self.leave_programs();
        warnings.extend(self.conclude_scratch(&workload));
        let mut settled = None;
        let mut terminal = admission;
        let mut terminal_groups = 0u64;
        let mut terminal_reported = false;
        if let Some(memory) = self.memory.take() {
            match kernel::cleanup() {
                Ok(cleanup) => {
                    let mut report_warnings = Vec::new();
                    let (outcome, close_warnings) = foreground::close_memory(
                        owned(memory),
                        workload.clone(),
                        &cleanup,
                        &mut |preview| {
                            // A v1 admission reports nothing and keeps its
                            // marker until the settle has stopped the hook.
                            if !terminal
                                .as_ref()
                                .is_some_and(|admission| admission.reporting())
                            {
                                return;
                            }
                            let admission = terminal.take().expect("checked above");
                            terminal_groups = admission.workload_groups();
                            terminal_reported = true;
                            report_warnings.extend(super::admission::report_terminal(
                                &admission, &workload, preview,
                            ));
                        },
                    );
                    settled = outcome;
                    warnings.extend(report_warnings);
                    warnings.extend(close_warnings);
                }
                Err(error) => warnings.push(error.to_string()),
            }
        }
        if let Some(run) = self.run.take() {
            let (outcome, close_warnings) = close_run(owned(run), workload.clone());
            settled = settled.or(outcome);
            warnings.extend(close_warnings);
        }
        if let Some(shared) = self.shared.take() {
            let mirrored = match &workload {
                WorkloadOutcome::Failed { .. } | WorkloadOutcome::Unknown(_) => {
                    WorkloadOutcome::Succeeded
                }
                other => other.clone(),
            };
            warnings.extend(close_run(shared, mirrored).1);
        }
        if let Some(admission) = terminal {
            // No settle reported through it (a run with nothing to settle):
            // report and record after the fact, best effort.
            warnings.extend(super::admission::finish(
                admission,
                &target,
                &run_id,
                &workload,
                settled.as_ref(),
            ));
        } else if terminal_reported {
            if let Some(settled) = &settled {
                warnings.extend(super::admission::record_terminal(
                    &target,
                    &run_id,
                    settled,
                    terminal_groups,
                ));
            }
        }
        warnings
    }

    /// Removes this run's program-fallback shims, if it opened them. A
    /// nested run only drops its reference.
    fn leave_programs(&mut self) {
        if let Some(handle) = self.programs.take() {
            crate::runner::program_fallback::leave(handle);
        }
    }

    /// Concludes the run's scratch: a successful run removes its directory and
    /// a failed one records the 24 hours it is kept for. A kept directory is a
    /// warning here, not a failure: the task's own result is already decided.
    fn conclude_scratch(&mut self, workload: &WorkloadOutcome) -> Vec<String> {
        let Some(scratch) = self.scratch.take() else {
            return Vec::new();
        };
        let mut warnings = Vec::new();
        if let Some(warning) = crate::runner::scratch::conclude(
            &scratch,
            matches!(workload, WorkloadOutcome::Succeeded),
        ) {
            warnings.push(warning);
        }
        crate::runner::scratch::leave(&scratch);
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
            argument_scope: None,
            label: task.to_owned(),
        };
        Self::begin(TaskRunRequest {
            target: &target,
            lock_wait_ms: 0,
            custody: false,
            scratch: false,
            managed_run_token,
            args: &[],
            key: None,
            admission: None,
            workspace_admission: None,
            cwd: None,
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
    /// journal records it interrupted rather than claiming an outcome. Its
    /// scratch leaves this thread's scope but is never concluded, so the
    /// directory is retained until the sweep removes it.
    fn drop(&mut self) {
        self.leave_programs();
        if let Some(scratch) = self.scratch.take() {
            crate::runner::scratch::leave(&scratch);
        }
        drop(self.memory.take());
        let cleanup = kernel::cleanup();
        for run in [self.run.take().map(owned), self.shared.take()]
            .into_iter()
            .flatten()
        {
            if let Ok(cleanup) = cleanup.as_ref() {
                let _ = run.settle(cleanup, &mut TaskSettlement::default());
            }
        }
    }
}

/// A settle that could not write its outcome is an environment condition: the
/// run stays unsettled for its sentinel or the next holder of its gate to
/// reconcile, and the task's own result stands (K4, K10).
fn settle_warning(error: &RunError) -> String {
    match error {
        RunError::Store(busy @ StoreError::Busy { .. }) => format!(
            "run settle did not record its outcome: {busy}. The run stays unsettled for its \
             sentinel or the next run of this task to reconcile; the task's own result stands"
        ),
        other => format!("run settle failed: {other}"),
    }
}

fn record_outcome(run: &mut Run<'static>, workload: WorkloadOutcome) -> Result<(), RunError> {
    let cleanup = kernel::cleanup()
        .map_err(|error| RunError::unavailable(format!("the cleanup budget: {error}")))?;
    match workload {
        WorkloadOutcome::Cancelled { .. }
        | WorkloadOutcome::TimedOut { .. }
        | WorkloadOutcome::NotStarted => run.interrupt(&cleanup, workload),
        other => run.finish(other),
    }
}

fn close_run(
    run: Run<'static>,
    workload: WorkloadOutcome,
) -> (
    Option<effigy_kernel::outcome::Outcome<effigy_kernel::run::Settled>>,
    Vec<String>,
) {
    match kernel::cleanup() {
        Ok(cleanup) => close_run_settled(run, workload, cleanup),
        Err(error) => (None, vec![error.to_string()]),
    }
}

/// Records the outcome, then settles.
fn close_run_settled(
    mut run: Run<'static>,
    workload: WorkloadOutcome,
    cleanup: effigy_kernel::ctx::CleanupContext,
) -> (
    Option<effigy_kernel::outcome::Outcome<effigy_kernel::run::Settled>>,
    Vec<String>,
) {
    let mut warnings = Vec::new();
    if let Err(error) = record_outcome(&mut run, workload) {
        warnings.push(format!("run outcome was not recorded: {error}"));
    }
    match run.settle(&cleanup, &mut TaskSettlement::default()) {
        Ok(outcome) => {
            if outcome.cleanup() == CleanupOutcome::Incomplete {
                warnings.push(
                    "cleanup is incomplete: a process of this run could not be proven stopped; \
                     rerunning the task retries its settle"
                        .to_owned(),
                );
            }
            (Some(outcome), warnings)
        }
        Err(error) => {
            warnings.push(settle_warning(&error));
            (None, warnings)
        }
    }
}

/// Files a fenced stop request against run `id` for its live owner, as
/// `effigy run stop` does.
pub(in crate::runner) fn request_run_stop(
    checkout: &std::path::Path,
    id: RunId,
) -> Result<(), RunnerError> {
    match super::control::lookup_checkout(checkout, id) {
        super::control::Lookup::Found(record) => super::control::request_stop(&record).map(|_| ()),
        _ => Err(RunnerError::task_invocation(format!(
            "run {id} is not readable for a stop request"
        ))),
    }
}

pub(super) fn invocation(
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
fn refuse_bound_key(
    key: &RunKey,
    task: &TaskId,
    checkout: &std::path::Path,
) -> Result<(), RunnerError> {
    let cx = kernel::observe();
    let root = match kernel::observe_root(&cx, checkout) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(()),
        Observed::Unknown(reason) => return Err(unverifiable_key(key, &format!("{reason}"))),
    };
    let reader = match Store::read(&cx, &root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Ok(()),
        Observed::Unknown(reason) => return Err(unverifiable_key(key, &format!("{reason}"))),
    };
    let namespace = key.namespace(&cx, &root)?;
    for id in namespace.ids {
        let candidate = RunKey {
            key: key.key.clone(),
            id,
        };
        check_bound_key(&candidate, task, &reader)?;
    }
    Ok(())
}

fn check_bound_key(
    key: &RunKey,
    task: &TaskId,
    reader: &effigy_kernel::store::StoreReader,
) -> Result<(), RunnerError> {
    match effigy_kernel::run::summary(reader, key.id()) {
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
        Observed::Unknown(reason) => Err(unverifiable_key(key, &format!("{reason}"))),
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
fn key_run_exists(key: &RunKey, checkout: &std::path::Path) -> bool {
    let deadline = Instant::now() + KEY_JOURNAL_WAIT;
    loop {
        let cx = kernel::observe();
        if let Observed::Verified(root, _) = kernel::observe_root(&cx, checkout) {
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
        std::thread::sleep(KEY_JOURNAL_POLL);
    }
}

/// A fresh random run id, for requests and runs no key names.
/// A fresh kernel run id from the engine's own entropy. A workspace
/// dispatcher's `RunIds` asks for one exactly as an ordinary admission does;
/// it never derives an id itself.
pub(in crate::runner) fn fresh_run_id() -> Result<RunId, RunnerError> {
    new_run_id()
}

/// A fresh id for one kernel request.
pub(in crate::runner) fn fresh_request_id() -> Result<RunId, RunnerError> {
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

/// The message a person reads for a run that could not be admitted, with the
/// kernel's own category beside it: a supervisor that relays it never calls a
/// busy store a refusal (K4, effigy#360).
fn begin_error(
    error: RunError,
    target: &TaskTarget,
    lock_wait_ms: u64,
    started: Instant,
) -> RunnerError {
    // A held gate keeps its structured conflict details, which already name
    // the holder and every command that inspects or stops it.
    let category = error.category();
    let message = match error {
        RunError::Gate(GateError::Busy { id, holder }) => {
            return conflict(target, &id, *holder, None, started)
        }
        RunError::Gate(GateError::TimedOut { id, holder, waited }) => {
            return conflict(
                target,
                &id,
                *holder,
                Some((lock_wait_ms, waited.as_millis() as u64)),
                started,
            )
        }
        other => format!("`{}` could not be admitted: {other}", target.label),
    };
    RunnerError::RunAdmission { category, message }
}

/// The run-inventory command this repository actually reaches, if any.
///
/// A repository task named `tasks` owns the word (contract 043), so suggesting
/// `effigy tasks status …` there would start the repository's own task instead
/// of the inventory. A message never suggests a command this checkout routes
/// elsewhere (effigy#318).
fn run_inventory_command(target: &TaskTarget) -> Option<String> {
    if crate::runner::deferred_builtins_for_root(&target.root).contains("tasks") {
        return None;
    }
    Some(match target.shared_lock {
        // A shared or built-in lock's holder may be another task or command,
        // so the inventory of every live holder is the answer.
        Some(_) => "effigy tasks status --all".to_owned(),
        None => format!("effigy tasks status {}", target.label),
    })
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
    let run_started = holders::holder_run_started_ms(&target.root, owner);
    let scope = match &target.shared_lock {
        Some(name) if owner.kind() == effigy_kernel::id::ResourceKind::Store => {
            shared_scope_label(name)
        }
        _ => format!("task:{}", target.label),
    };
    // A shared or built-in lock's holder may be another task or command. The
    // exact commands below are the CLI's own grammar: a run is read by its run
    // id (`effigy run status <id>`), listed with `effigy runs`, and stopped
    // through its own owner (`effigy run stop <id>`). A built-in section has
    // no selector to re-run with a wait.
    let builtin = target
        .shared_lock
        .as_deref()
        .is_some_and(|lock| lock.starts_with(BUILTIN_LOCK_PREFIX));
    let holder_run = diag.as_ref().and_then(|diag| diag.run.clone());
    // A run id is the one handle a repository task cannot shadow, so it is the
    // inspect command whenever the gate names its holder. Without one, the
    // built-in inventory is suggested only when this repository has not
    // declared its own `tasks` task (contract 043).
    let status_command = match holder_run.as_deref() {
        Some(run) => Some(format!("effigy run status {run}")),
        None => run_inventory_command(target),
    };
    let stop_command = holder_run
        .as_ref()
        .map(|run| format!("effigy run stop {run}"));
    let attached_to_terminal = diag.as_ref().is_some_and(|diag| diag.foreground.is_some());
    let holder_text = match &diag {
        Some(diag) => {
            let mut parts = vec![format!("pid {} on this host", diag.pid)];
            if let Some(run) = &diag.run {
                parts.push(format!("run {run}"));
            }
            if let Some(age) = run_started.and_then(age_text) {
                parts.push(format!("started {age} ago"));
            }
            parts.push(if attached_to_terminal {
                "attached to a terminal".to_owned()
            } else {
                "not attached to a terminal".to_owned()
            });
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
    // `tasks` is a shadowable built-in (contract 043): when this repository
    // declares a task of that name, `effigy tasks status …` runs the
    // repository's task instead of the inventory, so it is not suggested.
    let inspect = match (&status_command, holder_run.is_some()) {
        (Some(status_command), true) => {
            format!("Inspect it with `{status_command}` (or list every run with `effigy runs`)")
        }
        (Some(status_command), false) => format!("Inspect it with `{status_command}`"),
        (None, _) => "this repository's own task owns `effigy tasks`, so the built-in run \
                      inventory cannot show it"
            .to_owned(),
    };
    let stop_text = match (stop_command.as_deref(), attached_to_terminal) {
        (Some(stop), true) => {
            format!("stop that run with `{stop}` (or press Ctrl-C in its terminal)")
        }
        (Some(stop), false) => format!("stop it with `{stop}`"),
        (None, _) => "stop that run through its own owner".to_owned(),
    };
    let remediation = match (wait, &wait_flag) {
        (Some((timeout, waited)), Some(wait_flag)) => format!(
            "{what} ({holder_text}); waited {waited}ms of {timeout}ms. {inspect}, wait longer \
             with `{wait_flag}`, or {stop_text}"
        ),
        (Some((timeout, waited)), None) => format!(
            "{what} ({holder_text}); waited {waited}ms of {timeout}ms. {inspect}, let it \
             finish, or {stop_text}"
        ),
        (None, Some(wait_flag)) => format!(
            "{what} ({holder_text}). {inspect}, wait for it with `{wait_flag}`, or {stop_text}"
        ),
        (None, None) => {
            format!("{what} ({holder_text}). {inspect}, let it finish, or {stop_text}")
        }
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
        "next": status_command
            .iter()
            .map(|command| format!("inspect the live run with `{command}`"))
            .chain(wait_flag.iter().map(|flag| format!("wait for it with `{flag}`")))
            .chain(stop_command.iter().map(|stop| format!("stop it with `{stop}`")))
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
        status_command,
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

/// A stop request on any lexical ancestor stops the active step. Requests stay
/// generation-fenced in the kernel; no marker file or PID grants authority.
pub(super) fn stop_requested() -> bool {
    PARENTS.with(|parents| {
        parents.borrow().iter().any(|parent| {
            parent.run.as_ref().is_some_and(|run| {
                run.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .requests()
                    .is_ok_and(|requests| {
                        requests.iter().any(|request| {
                            matches!(
                                request.action,
                                effigy_kernel::run::RequestAction::Stop
                                    | effigy_kernel::run::RequestAction::Escalate
                            )
                        })
                    })
            })
        })
    })
}
