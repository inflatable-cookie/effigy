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
    Route, RunId, Scope, Surface, TaskId,
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
    fn observe(&mut self, _: &effigy_kernel::run::Effect, _: &CleanupContext) -> Observed<Fence> {
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

/// The machine home used for migration; release state belongs to its checkout.
pub fn release_home() -> Result<PathBuf, ReleaseError> {
    StateRoot::home_from_env().ok_or_else(|| {
        ReleaseError::TaskInvocation(
            "a release Run needs an absolute HOME for runtime migration checks; its state and receipts live in the checkout's `.effigy/state`"
                .into(),
        )
    })
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

fn invocation(checkout: CheckoutId, owner: ResourceId) -> ResolvedInvocation {
    ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(checkout, "release", Surface::Published, "release run", None)
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

/// Release authority is local to the checkout's retained identity.
pub fn release_owner(checkout: CheckoutId) -> ResourceId {
    ResourceId::store(Scope::Checkout(checkout), "release").expect("fixed release owner")
}
fn location(root: &Path) -> Result<effigy_kernel::migration::CheckoutLocation, ReleaseError> {
    Ok(effigy_kernel::migration::CheckoutLocation {
        root: root.into(),
        git_directory: effigy_core::worktree_scope::git_directory(root).map_err(storage_error)?,
    })
}
fn storage_error(error: impl std::fmt::Display) -> ReleaseError {
    ReleaseError::TaskInvocation(format!("release checkout state: {error}"))
}
fn checkout(root: &Path, mint: bool) -> Result<Option<CheckoutId>, ReleaseError> {
    use effigy_core::worktree_scope::{self, ScopeKind};
    let kind = match worktree_scope::scope_kind(root).map_err(storage_error)? {
        Some(ScopeKind::LinkedWorktree) => CheckoutKind::LinkedWorktree,
        Some(ScopeKind::EphemeralClone) => CheckoutKind::EphemeralClone,
        None => CheckoutKind::Primary,
    };
    let token = if mint {
        Some(worktree_scope::prepare_checkout_token(root).map_err(storage_error)?)
    } else {
        worktree_scope::checkout_token(root).map_err(storage_error)?
    };
    token
        .map(|token| {
            if token.len() != 32 || !token.is_ascii() {
                return Err(storage_error("invalid checkout token"));
            }
            let mut bytes = [0; 16];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&token[index * 2..index * 2 + 2], 16)
                    .map_err(storage_error)?;
            }
            Ok(CheckoutId::new(kind, bytes))
        })
        .transpose()
}

/// A release's Effigy-owned state directory: the record and the candidate.
#[derive(Debug, Clone)]
pub struct ReleaseState {
    pub record_path: PathBuf,
    pub candidate_dir: PathBuf,
}

impl ReleaseState {
    pub fn for_owner(state_root: &Path, owner: &ResourceId) -> Self {
        let directory = state_root
            .join(RELEASE_STATE_DIR)
            .join(owner.name().to_string());
        Self {
            record_path: directory.join("record.json"),
            candidate_dir: directory.join("candidate"),
        }
    }
    /// The run owns this Store resource; the intent precedes publication and
    /// the kernel commits the file and its directory before acknowledging it.
    pub fn write_record(
        &self,
        record: &ReleaseRecord,
        run: &mut Run<'_>,
    ) -> Result<(), ReleaseError> {
        let bytes = serde_json::to_vec_pretty(record).map_err(storage_error)?;
        if bytes.len() as u64 > <ReleaseRecord as effigy_kernel::record::Record>::MAX_BYTES {
            return Err(storage_error("release record exceeds 4 MiB"));
        }
        let ticket = run.intend(effigy_kernel::run::EffectSpec {
            target: run.fence().clone(), kind: "release:checkpoint-file".into(),
            payload: serde_json::json!({"record":self.record_path,"sha256":super::artifacts::sha256_bytes(&bytes)}),
            settlement: effigy_kernel::run::Settlement::Retain, required: false,
        }).map_err(storage_error)?;
        let directory = effigy_kernel::fs::DirFd::open(
            self.record_path
                .parent()
                .ok_or_else(|| storage_error("record parent missing"))?,
        )
        .map_err(storage_error)?;
        let name = effigy_kernel::fs::FileName::new("record.json").map_err(storage_error)?;
        run.replace_record(&directory, &name, &bytes, Default::default())
            .map_err(storage_error)?;
        run.applied(&ticket, serde_json::Value::Null)
            .map_err(storage_error)
    }

    pub fn read_record(&self) -> Result<Option<ReleaseRecord>, ReleaseError> {
        let parent = self
            .record_path
            .parent()
            .ok_or_else(|| storage_error("record parent missing"))?;
        match std::fs::symlink_metadata(parent) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(storage_error(error)),
            Ok(_) => {}
        }
        let directory = effigy_kernel::fs::DirFd::open(parent).map_err(storage_error)?;
        let observe = operation()
            .child(Duration::from_secs(30))
            .observe()
            .map_err(|_| storage_error("record observation context"))?;
        let name = effigy_kernel::fs::FileName::new("record.json").map_err(storage_error)?;
        match effigy_kernel::record::read_record::<ReleaseRecord>(&observe, &directory, &name) {
            Observed::Verified(record, _) => Ok(Some(record)),
            Observed::Absent(_) => Ok(None),
            Observed::Unknown(reason) => {
                Err(storage_error(format!("record is unreadable: {reason:?}")))
            }
        }
    }
}

impl effigy_kernel::record::Record for ReleaseRecord {
    const MAX_BYTES: u64 = 4 << 20;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let record: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if record.schema != super::record::RELEASE_RECORD_SCHEMA {
            return Err("unsupported release record schema".into());
        }
        Ok(record)
    }
}

/// One kernel session: the capability, the state root and the cleanup budget.
pub struct ReleaseSession {
    cx: EffectContext,
    cleanup: CleanupContext,
    root: StateRoot,
    checkout: CheckoutId,
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
        effigy_kernel::inheritance::require_durable(&op).map_err(storage_error)?;
        let root =
            StateRoot::checkout_effects(&cx, home, &location(repo_root)?).map_err(storage_error)?;
        effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root(repo_root)
            .map_err(storage_error)?;
        let checkout =
            checkout(repo_root, true)?.ok_or_else(|| storage_error("checkout identity missing"))?;
        Ok(Self {
            cx,
            cleanup,
            root,
            checkout,
            owner: release_owner(checkout),
            repo_root: repo_root.to_path_buf(),
        })
    }

    pub fn state(&self) -> Result<ReleaseState, ReleaseError> {
        self.root.verify_binding().map_err(storage_error)?;
        use effigy_kernel::fs::FileName;
        let release = self
            .root
            .state_directory()
            .prepare_child(
                &self.cx,
                &FileName::new(RELEASE_STATE_DIR).map_err(storage_error)?,
            )
            .map_err(storage_error)?;
        let owner = release
            .prepare_child(
                &self.cx,
                &FileName::new(&self.owner.name().to_string()).map_err(storage_error)?,
            )
            .map_err(storage_error)?;
        owner
            .prepare_child(
                &self.cx,
                &FileName::new("candidate").map_err(storage_error)?,
            )
            .map_err(storage_error)?;
        Ok(ReleaseState::for_owner(self.root.path(), &self.owner))
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
        let root = match StateRoot::checkout_observe(&observe, &location(&self.repo_root)?) {
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

    pub fn project_record(&self, record: &mut ReleaseRecord) -> Result<(), ReleaseError> {
        let observe = self
            .cx
            .operation()
            .observe()
            .map_err(|_| storage_error("record observation context"))?;
        match Store::read(&observe, &self.root) {
            Observed::Verified(reader, _) => project_cleanup(&reader, record),
            Observed::Absent(_) => Ok(()),
            Observed::Unknown(reason) => {
                Err(storage_error(format!("journal is unreadable: {reason:?}")))
            }
        }
    }

    pub fn begin(&self, id: RunId, adapter: &mut ReportsOnly) -> Result<Run<'_>, ReleaseError> {
        let deadline = std::time::Instant::now() + OWNER_GATE_WAIT;
        Run::begin(
            &self.cx,
            &self.root,
            &invocation(self.checkout, self.owner.clone()),
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
    let root = match StateRoot::checkout_observe_in(&observe, home, &location(repo_root)?) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return legacy_release_refusal(&observe, home, repo_root),
        Observed::Unknown(reason) => {
            return Err(ReleaseError::TaskInvocation(format!(
                "release state is unreadable: {reason:?}"
            )))
        }
    };
    let reader = match Store::read(&observe, &root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return legacy_release_refusal(&observe, home, repo_root),
        Observed::Unknown(reason) => {
            return Err(ReleaseError::TaskInvocation(format!(
                "release state store is unreadable: {reason:?}"
            )))
        }
    };
    let checkout = checkout(repo_root, false)?
        .ok_or_else(|| storage_error("journal exists without its checkout identity"))?;
    let owner = release_owner(checkout);
    match checkpoint_from_reader(&reader, &owner)? {
        Some(checkpoint) => {
            let state = ReleaseState::for_owner(root.path(), &owner);
            if !record_path_is_owned(&state, Path::new(&checkpoint.record)) {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release Run {} points its state outside this release's state directory",
                    checkpoint.release_id
                )));
            }
            let mut record = state.read_record()?.ok_or_else(|| storage_error(format!(
                "release {} has a checkpoint but its record is missing; preserving the journal for recovery", checkpoint.release_id)))?;
            project_cleanup(&reader, &mut record)?;
            Ok(Some(record))
        }
        None => Ok(None),
    }
}

/// Historical evidence is read-only. Missing local state must not hide a
/// paused old release or silently turn it into a new release.
fn legacy_release_refusal(
    observe: &effigy_kernel::ctx::ObserveContext,
    home: &Path,
    repo_root: &Path,
) -> Result<Option<ReleaseRecord>, ReleaseError> {
    let root = match StateRoot::legacy_observe(observe, home) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(None),
        Observed::Unknown(reason) => {
            return Err(storage_error(format!("legacy release source: {reason:?}")))
        }
    };
    let reader = match Store::read(observe, &root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Ok(None),
        Observed::Unknown(reason) => {
            return Err(storage_error(format!("legacy release source: {reason:?}")))
        }
    };
    let canonical = repo_root.canonicalize().map_err(storage_error)?;
    let owner = ResourceId::store(Scope::Machine, &format!("release {}", canonical.display()))
        .map_err(storage_error)?;
    match checkpoint_from_reader(&reader, &owner)? {
        None => Ok(None),
        Some(checkpoint) => Err(storage_error(format!(
            "legacy release checkpoint {} requires `effigy run migrate`; its record and certified artifacts remain untouched", checkpoint.release_id))),
    }
}

/// Incomplete cleanup stays in the journal. Project it without a late file
/// write after the owner's gate was released by settle.
fn project_cleanup(reader: &StoreReader, record: &mut ReleaseRecord) -> Result<(), ReleaseError> {
    let id = record.run_id.parse::<RunId>().map_err(storage_error)?;
    match effigy_kernel::run::summary(reader, id) {
        Observed::Verified(summary, _) if summary.cleanup_complete == Some(false) => {
            record.status = super::record::ReleaseRunStatus::Failed;
            record.errors.push(format!(
                "release Run {} left unresolved cleanup; `effigy release receipt {}` lists it",
                record.run_id, record.release_id
            ));
        }
        Observed::Unknown(reason) => {
            return Err(storage_error(format!("cleanup is unknown: {reason:?}")))
        }
        _ => {}
    }
    Ok(())
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
