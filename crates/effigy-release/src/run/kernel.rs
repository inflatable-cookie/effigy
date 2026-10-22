//! Kernel custody of one release Run (contracts 052 §5.6 and 054 §3).
//!
//! Every invocation of `release run`, `release resume` and `release approve`
//! is one journalled kernel Run for one release owner. The Run holds the owner
//! gate, journals each irreversible publish before it happens and settles
//! whatever an interrupted invocation left behind. The release's own durable
//! facts — the candidate commit, versions, artifact digests and destinations —
//! live in a record file under the Effigy state root, and the kernel engine
//! record carries the checkpoint pointer to it (054 §10 ruling 2).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use effigy_kernel::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, EffectContext, Limits, OperationContext,
    Placement,
};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{
    CheckoutId, CheckoutKind, ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId,
    Route, RunId, Surface, TaskId,
};
use effigy_kernel::observe::{AbsenceProof, Observed, UnknownReason};
use effigy_kernel::outcome::Invocation;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{BeginOpts, Fence, Reconciled, Run, SettlementAdapter};
use effigy_kernel::store::{Store, StoreReader};

use super::record::{ReleaseCheckpoint, ReleaseRecord, RELEASE_CHECKPOINT_SCHEMA};
use crate::ReleaseError;

/// The state-root directory a release's record and candidate live in.
const RELEASE_STATE_DIR: &str = "release";

/// A release invocation can outlive many gates and builds; the budget bounds
/// the kernel's own waits, not the release's work.
const RUN_BUDGET: Duration = Duration::from_secs(12 * 60 * 60);
const OWNER_GATE_WAIT: Duration = Duration::from_secs(30);
const CLEANUP_BUDGET: Duration = Duration::from_secs(60);

/// The tag-push effect kind. Its settlement is `Irreversible`.
pub const TAG_PUSH_EFFECT: &str = "release:tag-push";
/// The release-commit push effect kind.
pub const COMMIT_PUSH_EFFECT: &str = "release:commit-push";

/// An engine adapter that reports nothing it does not know.
///
/// Every release effect is `Irreversible`, and settle records those without
/// asking the adapter, so this adapter has no destructive work to guess at.
/// A release destination is reconciled by the publish stage, which looks the
/// tag up before it ever pushes again (contract 054 §7).
#[derive(Debug)]
pub struct ReportsOnly;

impl SettlementAdapter for ReportsOnly {
    fn observe(
        &mut self,
        _: &effigy_kernel::run::Effect,
        _: &CleanupContext,
    ) -> Observed<Fence> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "release effects are irreversible and reconciled by the release run".into(),
        ))
    }

    fn release(&mut self, _: &effigy_kernel::run::Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "release effects are irreversible".into(),
        ))
    }

    fn reconcile(
        &mut self,
        _: &effigy_kernel::run::Effect,
        _: &CleanupContext,
    ) -> Observed<Reconciled> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "release effects are irreversible".into(),
        ))
    }
}

fn limits() -> Limits {
    Limits {
        capture_tail: 65536,
        log_rotate: 8 << 20,
        log_keep: 2,
        line_max: 65536,
        queue_bytes: 1 << 20,
        probe_concurrency: 2,
    }
}

fn operation() -> OperationContext {
    OperationContext::new(
        Deadline::At(std::time::Instant::now() + RUN_BUDGET),
        CancelToken::default(),
        limits(),
        EffectClass::Mutate,
        Placement::Host,
    )
}

/// A fresh run id from supplied entropy.
pub fn new_run_id() -> Result<RunId, ReleaseError> {
    let mut random = [0u8; 10];
    getrandom::fill(&mut random).map_err(|error| {
        ReleaseError::TaskInvocation(format!("failed to read release run entropy: {error}"))
    })?;
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("system clock precedes the epoch: {error}"))
        })?
        .as_millis();
    let millis = u64::try_from(millis).map_err(|_| {
        ReleaseError::TaskInvocation("system clock is out of range for a run id".to_owned())
    })?;
    RunId::new(millis, random).map_err(|error| {
        ReleaseError::TaskInvocation(format!("failed to mint a release run id: {error}"))
    })
}

fn invocation(owner: ResourceId) -> ResolvedInvocation {
    ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(
            CheckoutId::new(CheckoutKind::Machine, [0; 16]),
            "release",
            Surface::Published,
            "release run",
            None,
        )
        .expect("fixed release task identity"),
        route: Route::Builtin("release".into()),
        argv: vec![OsString::from("release"), OsString::from("run")],
        cwd: String::new(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner,
        gates: Vec::new(),
        attachments: Vec::new(),
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .expect("fixed release invocation")
}

fn next_invocation() -> Invocation {
    Invocation::validated(
        vec![OsString::from("release"), OsString::from("resume")],
        |args| args == ["release", "resume"],
    )
    .expect("fixed recovery invocation")
}

/// The release owner: one per release root, machine-scoped, so two checkouts of
/// the same project never share a run.
pub fn release_owner(repo_root: &Path) -> ResourceId {
    let canonical = repo_root.canonicalize().unwrap_or_else(|_| repo_root.into());
    ResourceId::store(
        effigy_kernel::id::Scope::Machine,
        &format!("release {}", canonical.display()),
    )
    .expect("release owner identity")
}

/// A release's Effigy-owned state directory: the record and the candidate.
#[derive(Debug, Clone)]
pub struct ReleaseState {
    pub record_path: PathBuf,
    pub candidate_dir: PathBuf,
}

impl ReleaseState {
    pub fn for_owner(state_root: &Path, owner: &ResourceId) -> Result<Self, ReleaseError> {
        let directory = state_root
            .join(RELEASE_STATE_DIR)
            .join(owner.name().to_string());
        for path in [directory.clone(), directory.join("candidate")] {
            std::fs::create_dir_all(&path).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to create release state directory {}: {error}",
                    path.display()
                ))
            })?;
        }
        Ok(Self {
            record_path: directory.join("record.json"),
            candidate_dir: directory.join("candidate"),
        })
    }

    /// Write the record atomically: a crash leaves the previous generation.
    pub fn write_record(&self, record: &ReleaseRecord) -> Result<(), ReleaseError> {
        let bytes = serde_json::to_vec_pretty(record).map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to encode the release record: {error}"))
        })?;
        let temporary = self.record_path.with_extension("record-tmp");
        std::fs::write(&temporary, &bytes).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to write the release record {}: {error}",
                temporary.display()
            ))
        })?;
        std::fs::rename(&temporary, &self.record_path).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to publish the release record {}: {error}",
                self.record_path.display()
            ))
        })?;
        if let Some(parent) = self.record_path.parent() {
            if let Ok(directory) = std::fs::File::open(parent) {
                let _ = directory.sync_all();
            }
        }
        Ok(())
    }

    pub fn read_record(&self) -> Result<Option<ReleaseRecord>, ReleaseError> {
        let raw = match std::fs::read(&self.record_path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "failed to read the release record {}: {error}",
                    self.record_path.display()
                )))
            }
        };
        let record: ReleaseRecord = serde_json::from_slice(&raw).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "release record {} is unreadable: {error}",
                self.record_path.display()
            ))
        })?;
        Ok(Some(record))
    }
}

/// One kernel session: the capability, the state root and the cleanup budget.
pub struct ReleaseSession {
    cx: EffectContext,
    cleanup: CleanupContext,
    root: StateRoot,
    home: PathBuf,
    pub owner: ResourceId,
    pub repo_root: PathBuf,
}

impl std::fmt::Debug for ReleaseSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReleaseSession")
            .field("owner", &self.owner)
            .field("root", &self.root.path())
            .finish()
    }
}

impl ReleaseSession {
    pub fn open(home: &Path, repo_root: &Path) -> Result<Self, ReleaseError> {
        let op = operation();
        let cx = op.effects().map_err(|_| {
            ReleaseError::TaskInvocation("release runs need write capability".into())
        })?;
        let cleanup = op.cleanup(CLEANUP_BUDGET).map_err(|_| {
            ReleaseError::TaskInvocation("release cleanup budget is unusable".into())
        })?;
        let root = StateRoot::effects(&cx, home).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "release state root is unusable at {}: {error:?}",
                home.join(".effigy/state").display()
            ))
        })?;
        Ok(Self {
            cx,
            cleanup,
            root,
            home: home.to_path_buf(),
            owner: release_owner(repo_root),
            repo_root: repo_root.to_path_buf(),
        })
    }

    pub fn state(&self) -> Result<ReleaseState, ReleaseError> {
        ReleaseState::for_owner(self.root.path(), &self.owner)
    }

    /// The checkpoint pointer the kernel engine record carries.
    pub fn read_checkpoint(&self) -> Result<Option<ReleaseCheckpoint>, ReleaseError> {
        let observe = self
            .cx
            .child(Duration::from_secs(30))
            .operation()
            .observe()
            .map_err(|_| {
                ReleaseError::TaskInvocation("release state observation needs a deadline".into())
            })?;
        let root = match StateRoot::observe(&observe, &self.home) {
            Observed::Verified(root, _) => root,
            Observed::Absent(_) => return Ok(None),
            Observed::Unknown(reason) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release state is unreadable: {reason:?}"
                )))
            }
        };
        let reader = match Store::read(&observe, &root) {
            Observed::Verified(reader, _) => reader,
            Observed::Absent(_) => return Ok(None),
            Observed::Unknown(reason) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release state store is unreadable: {reason:?}"
                )))
            }
        };
        checkpoint_from_reader(&reader, &self.owner)
    }

    pub fn begin(
        &self,
        id: RunId,
        adapter: &mut ReportsOnly,
    ) -> Result<Run<'_>, ReleaseError> {
        let deadline = std::time::Instant::now() + OWNER_GATE_WAIT;
        Run::begin(
            &self.cx,
            &self.root,
            &invocation(self.owner.clone()),
            BeginOpts {
                id,
                wait: Wait::Until(deadline),
                acknowledge: Vec::new(),
                next: next_invocation(),
                custody: None,
            },
            &self.cleanup,
            adapter,
        )
        .map_err(|error| begin_error(&self.repo_root, error))
    }

    pub fn cleanup(&self) -> &CleanupContext {
        &self.cleanup
    }

    /// The kernel run id of the last invocation, when the journal knows it.
    pub fn known_run(&self, run: &str) -> Result<Option<RunId>, ReleaseError> {
        run.parse::<RunId>().map(Some).map_err(|error| {
            ReleaseError::TaskInvocation(format!("`{run}` is not a release Run id: {error}"))
        })
    }
}

fn begin_error(repo_root: &Path, error: effigy_kernel::run::RunError) -> ReleaseError {
    use effigy_kernel::run::RunError;
    let message = match &error {
        RunError::Gate(_) => format!(
            "another release Run holds the release gate for {}; retry once it finishes: {error}",
            repo_root.display()
        ),
        RunError::PredecessorUnsettled { run, effects } => format!(
            "release Run {run} left {} unresolved effect(s) and cannot be settled automatically; inspect it with `effigy release receipt {run}` before resuming: {effects:?}",
            effects.len()
        ),
        _ => format!("the kernel refused the release Run: {error}"),
    };
    ReleaseError::TaskInvocation(message)
}

fn checkpoint_from_reader(
    reader: &StoreReader,
    owner: &ResourceId,
) -> Result<Option<ReleaseCheckpoint>, ReleaseError> {
    match reader.resource(owner) {
        Observed::Verified(resource, _evidence) => {
            let Some(engine) = resource.engine else {
                return Ok(None);
            };
            let checkpoint: ReleaseCheckpoint =
                serde_json::from_value(engine).map_err(|error| {
                    ReleaseError::TaskInvocation(format!(
                        "the release checkpoint in the kernel store is unreadable: {error}"
                    ))
                })?;
            if checkpoint.schema != RELEASE_CHECKPOINT_SCHEMA {
                return Err(ReleaseError::TaskInvocation(format!(
                    "the release checkpoint schema `{}` is not `{RELEASE_CHECKPOINT_SCHEMA}`; this Effigy cannot continue that Run",
                    checkpoint.schema
                )));
            }
            Ok(Some(checkpoint))
        }
        Observed::Absent(AbsenceProof::NoRow { .. }) => Ok(None),
        Observed::Absent(_) => Ok(None),
        Observed::Unknown(reason) => Err(ReleaseError::TaskInvocation(format!(
            "release state is unknown: {reason:?}"
        ))),
    }
}

/// Read a release's record without opening the state root for effects.
///
/// Every surface that is a pure read — `release status`, `release receipt` —
/// uses this, so it never creates or changes the state root (contract 052 R2).
pub fn read_release_record(
    home: &Path,
    repo_root: &Path,
) -> Result<Option<ReleaseRecord>, ReleaseError> {
    let op = OperationContext::new(
        Deadline::At(std::time::Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        limits(),
        EffectClass::Observe,
        Placement::Host,
    );
    let observe = op.observe().map_err(|_| {
        ReleaseError::TaskInvocation("release state observation needs a deadline".into())
    })?;
    let owner = release_owner(repo_root);
    let root = match StateRoot::observe(&observe, home) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(None),
        Observed::Unknown(reason) => {
            return Err(ReleaseError::TaskInvocation(format!(
                "release state is unreadable: {reason:?}"
            )))
        }
    };
    let reader = match Store::read(&observe, &root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Ok(None),
        Observed::Unknown(reason) => {
            return Err(ReleaseError::TaskInvocation(format!(
                "release state store is unreadable: {reason:?}"
            )))
        }
    };
    match checkpoint_from_reader(&reader, &owner)? {
        Some(checkpoint) => {
            let state = ReleaseState::for_owner(root.path(), &owner)?;
            if !record_path_is_owned(&state, Path::new(&checkpoint.record)) {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release Run {} points its state outside this release's state directory",
                    checkpoint.release_id
                )));
            }
            state.read_record()
        }
        None => Ok(None),
    }
}

/// `read_release_record` for the user's `HOME`, when it is absolute.
pub fn read_release_record_for_root(
    repo_root: &Path,
) -> Result<Option<ReleaseRecord>, ReleaseError> {
    match StateRoot::home_from_env() {
        Some(home) => read_release_record(&home, repo_root),
        None => Ok(None),
    }
}

/// A record path is under this release's state directory, never anywhere else.
pub fn record_path_is_owned(state: &ReleaseState, path: &Path) -> bool {
    path.parent() == state.record_path.parent()
}
