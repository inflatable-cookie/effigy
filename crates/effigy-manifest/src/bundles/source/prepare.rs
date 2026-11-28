//! The single source updater. Every acquisition runs under a Prepare owner;
//! one checkout lock selects a complete generation after all sources validate.
//!
//! Contract 020: `effigy bundle sync` updates every selected source; the
//! Prepare paths (`dev`, `bootstrap`, `container up`) also refresh a pin that
//! is behind its declaration, its path content or its floating Git upstream.
use super::*;
use effigy_kernel::{
    ctx::{CleanupContext, EffectContext},
    custody::{Custody, EnvValue, Lifetime, ProcessSpec, SecretValue, SpawnError, Terminal},
    gate::Wait,
    id::{
        CheckoutId, CheckoutKind, ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId,
        Route, RunId, Scope, Surface, TaskId,
    },
    observe::UnknownReason,
    outcome::{CleanupOutcome, Invocation, WorkloadOutcome},
    root::StateRoot,
    run::{BeginOpts, Effect, EffectSpec, Fence, Reconciled, Run, Settlement, SettlementAdapter},
    supervise::{Executable, LifecyclePlan},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::{
    ffi::OsStrExt,
    fs::{OpenOptionsExt, PermissionsExt},
};

const BUDGET: Duration = Duration::from_secs(120);
const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_TREE: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 65_536;
/// A floating Git source probes its remote at most once per checkout in this
/// window (contract 020).
const REMOTE_CHECK_TTL: Duration = Duration::from_secs(10 * 60);
/// The probe is bounded well below the Prepare budget: a slow network keeps
/// the current pin instead of holding `dev` or `container up`.
const REMOTE_CHECK_BUDGET: Duration = Duration::from_secs(5);
/// Per-checkout record of floating-source remote checks. It sits beside the
/// lock until the per-checkout state store owns it (#276).
const UPDATE_STATE_FILE: &str = "sources.update.json";
const UPDATE_STATE_MAX: u64 = 64 * 1024;
const UPDATE_STATE_KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// A snapshot tree is bounded to depth 32 (contract 020), so pruning a
/// generation at a deeper bound never stops inside a removable tree.
const GENERATION_TREE_DEPTH: usize = 40;
/// Maximum number of candidate tags inspected when a floating Git bundle branch
/// tip declares an Effigy version bound that the running binary does not satisfy.
const MAX_COMPATIBILITY_TAG_CANDIDATES: usize = 12;

#[derive(Default)]
struct SnapshotUsage {
    bytes: u64,
    entries: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceUpdateState {
    version: u8,
    #[serde(default)]
    checks: BTreeMap<String, u64>,
}

impl SourceUpdateState {
    fn empty() -> Self {
        Self {
            version: 1,
            checks: BTreeMap::new(),
        }
    }
}

impl Record for SourceUpdateState {
    const MAX_BYTES: u64 = UPDATE_STATE_MAX;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let state: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if state.version != 1 {
            return Err("unsupported source update state version".into());
        }
        if state.checks.len() > 4096 {
            return Err("source update state exceeds limit".into());
        }
        Ok(state)
    }
}

/// The update lines this process selected on a Prepare path, for a JSON
/// envelope's `notes`. Reading is idempotent: several envelopes in one process
/// can name the same lines.
static UPDATE_NOTES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

#[derive(Debug)]
pub struct SourcePreparation {
    pub home: PathBuf,
    pub executable: PathBuf,
    pub cancel: CancelToken,
    pub next: Invocation,
    pub budget: Duration,
    /// How acquisition says what it is doing while it does it. Silent by
    /// default; the command plane attaches `effigy_ui::steps` here.
    pub progress: SourceProgress,
}

/// Only custody effects need recovery calls. Published source generations are
/// retained; an unknown future engine effect is never interpreted as absent.
#[derive(Debug)]
pub struct SourceSettlement;
impl SettlementAdapter for SourceSettlement {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unrecognized source effect".into(),
        ))
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unrecognized source release".into(),
        ))
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unrecognized source reconciliation".into(),
        ))
    }
}

/// Explicit sync updates selected sources, after importing any v0.14.1 bytes.
pub fn sync_bundle_source(
    manifest: &Path,
    preparation: &SourcePreparation,
) -> Result<Option<BundleSyncReport>, ManifestError> {
    prepare(manifest, preparation, Mode::Sync)
}
/// Task execution and Prepare paths acquire missing sources and refresh stale
/// pins before resolving their selected bytes.
pub fn prepare_bundle_sources(
    manifest: &Path,
    preparation: &SourcePreparation,
) -> Result<Option<BundleSyncReport>, ManifestError> {
    prepare(manifest, preparation, Mode::Refresh)
}

/// Read-only discovery fills missing pins without moving usable ones
/// (contract 020 §Behaviour rule 1).
pub fn prepare_missing_bundle_sources(
    manifest: &Path,
    preparation: &SourcePreparation,
) -> Result<Option<BundleSyncReport>, ManifestError> {
    prepare(manifest, preparation, Mode::Missing)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// `effigy bundle sync`: re-acquire every selected source.
    Sync,
    /// Task execution and Prepare paths: acquire what is missing and refresh
    /// what is behind.
    Refresh,
    /// Read-only discovery: acquire only a source without a usable pin.
    Missing,
}

/// Why a pin is about to move (contract 020 §Behaviour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshReason {
    Missing,
    Declaration,
    Content,
    Upstream,
}

impl RefreshReason {
    fn describe(self) -> &'static str {
        match self {
            Self::Missing => "first acquisition",
            Self::Declaration => "declaration changed",
            Self::Content => "content changed",
            Self::Upstream => "upstream moved",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourcePlan {
    /// Missing-pin reads can snapshot legacy bytes without launching a child.
    Import,
    Acquire(RefreshReason),
    /// A floating Git source whose remote check is due.
    RemoteCheck,
    Keep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreparationAction {
    Keep,
    Acquire,
    Refuse(String),
}

fn preparation_action(
    mode: Mode,
    plans: &BTreeMap<String, SourcePlan>,
    in_workspace_container: bool,
) -> PreparationAction {
    if !in_workspace_container {
        return if plans.values().all(|plan| *plan == SourcePlan::Keep) {
            PreparationAction::Keep
        } else {
            PreparationAction::Acquire
        };
    }

    if mode != Mode::Sync
        && plans
            .values()
            .all(|plan| matches!(plan, SourcePlan::Keep | SourcePlan::RemoteCheck))
    {
        return PreparationAction::Keep;
    }

    const HOST_SYNC_NEXT: &str = "Exit the container and run `effigy bundle sync` on the host.";
    let (headline, explanation) = if mode == Mode::Sync {
        (
            "Bundles cannot be synced from inside a workspace container",
            None,
        )
    } else if plans
        .values()
        .any(|plan| *plan == SourcePlan::Acquire(RefreshReason::Declaration))
    {
        (
            "This project's bundle needs to be synced on the host",
            Some("Its declaration changed since it was last synced on the host."),
        )
    } else if plans
        .values()
        .any(|plan| *plan == SourcePlan::Acquire(RefreshReason::Content))
    {
        (
            "This project's bundle needs to be synced on the host",
            Some("The local path bundle changed since it was last snapshotted on the host."),
        )
    } else {
        (
            "This project's bundle needs to be synced on the host",
            Some("It has not been synced yet."),
        )
    };
    PreparationAction::Refuse(crate::retired_notice::format_error_detail(
        headline,
        explanation,
        Some(HOST_SYNC_NEXT),
        &[],
    ))
}

fn prepare(
    manifest: &Path,
    preparation: &SourcePreparation,
    mode: Mode,
) -> Result<Option<BundleSyncReport>, ManifestError> {
    let active_version = effigy_core::build_info::active_version();
    prepare_with_version(manifest, preparation, mode, &active_version)
}

fn prepare_with_version(
    manifest: &Path,
    preparation: &SourcePreparation,
    mode: Mode,
    active_version: &str,
) -> Result<Option<BundleSyncReport>, ManifestError> {
    let manifest = std::fs::canonicalize(manifest).map_err(|err| io_error(manifest, err))?;
    let (bundle, catalogs) = crate::composition::load_manifest_source_declarations_for_version(
        &manifest,
        Some(active_version),
    )?;
    let mut selections = BTreeMap::new();
    let mut policies: BTreeMap<String, crate::ManifestSourceUpdate> = BTreeMap::new();
    if let Some(bundle) = &bundle {
        selections.insert(
            "bundle".to_owned(),
            resolve_bundle_selection(&manifest, bundle)?,
        );
        policies.insert("bundle".to_owned(), bundle.update);
    }
    for (name, spec) in catalogs {
        if name.trim().is_empty() || name.contains('/') || name.contains(':') {
            return Err(error(
                &manifest,
                "catalog source names must be non-empty handles without '/' or ':'",
            ));
        }
        selections.insert(
            format!("catalog:{name}"),
            BundleSelection::from_spec(&manifest, &spec.base)?,
        );
        policies.insert(format!("catalog:{name}"), spec.update);
    }
    if selections.is_empty() {
        return Ok(None);
    }
    let old = read_lock(&manifest)?.unwrap_or(SourceLock {
        version: 1,
        sources: BTreeMap::new(),
    });
    let checked = read_update_state(&manifest);
    let now = unix_seconds();
    let plans = selections
        .iter()
        .map(|(name, selection)| {
            let plan = match mode {
                Mode::Sync => SourcePlan::Acquire(RefreshReason::Missing),
                Mode::Missing => match old.sources.get(name) {
                    // A lock entry whose recorded object is gone is not a usable
                    // pin: a copied checkout brings the lock without its objects,
                    // and the read path would refuse `pinned source` instead of
                    // running the task (contract 020 §Behaviour rule 1).
                    Some(pin) if parent(&manifest).join(".effigy").join(&pin.object).is_dir() => {
                        SourcePlan::Keep
                    }
                    None if legacy_root(&manifest, selection).is_some_and(|root| root.is_dir()) => {
                        SourcePlan::Import
                    }
                    _ => SourcePlan::Acquire(RefreshReason::Missing),
                },
                Mode::Refresh => plan_source(
                    &manifest,
                    name,
                    selection,
                    old.sources.get(name),
                    policies.get(name).copied().unwrap_or_default(),
                    checked.checks.get(name).copied(),
                    now,
                    active_version,
                    &preparation.progress,
                )?,
            };
            Ok((name.clone(), plan))
        })
        .collect::<Result<BTreeMap<_, _>, ManifestError>>()?;
    // Guests keep materialized pins when only a floating Git remote check is due.
    match preparation_action(
        mode,
        &plans,
        effigy_core::container_detection::process_is_inside_effigy_workspace_container(),
    ) {
        PreparationAction::Keep => return Ok(None),
        PreparationAction::Acquire => {}
        PreparationAction::Refuse(message) => return Err(error(&manifest, message)),
    }
    let deadline = Instant::now()
        .checked_add(preparation.budget)
        .ok_or_else(|| error(&manifest, "source preparation deadline is out of range"))?;
    let op = OperationContext::new(
        Deadline::At(deadline),
        preparation.cancel.clone(),
        Limits {
            capture_tail: 1024 * 1024,
            log_rotate: 1024 * 1024,
            log_keep: 2,
            line_max: 64 * 1024,
            queue_bytes: 1024 * 1024,
            probe_concurrency: 1,
        },
        EffectClass::Prepare,
        Placement::Host,
    );
    let cx = op.effects().map_err(|_| {
        error(
            &manifest,
            "source preparation could not start: this operation is not allowed to write",
        )
    })?;
    let location = state_location(&manifest)?;
    let root = StateRoot::checkout_effects(&cx, &preparation.home, &location)
        .map_err(|err| error(&manifest, err.to_string()))?;
    let token = effigy_core::worktree_scope::prepare_checkout_token(&location.root)
        .map_err(|err| error(&manifest, err.to_string()))?;
    // Core validates a token as exactly 32 ASCII hexadecimal characters.
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&token[index * 2..index * 2 + 2], 16)
            .map_err(|err| error(&manifest, err.to_string()))?;
    }
    let kind = match effigy_core::worktree_scope::scope_kind(&location.root)
        .map_err(|err| error(&manifest, err.to_string()))?
    {
        Some(effigy_core::worktree_scope::ScopeKind::LinkedWorktree) => {
            CheckoutKind::LinkedWorktree
        }
        Some(effigy_core::worktree_scope::ScopeKind::EphemeralClone) => {
            CheckoutKind::EphemeralClone
        }
        None => CheckoutKind::Primary,
    };
    let checkout = CheckoutId::new(kind, bytes);
    let owner = ResourceId::store(Scope::Checkout(checkout), "bundles")
        .map_err(|err| error(&manifest, err.to_string()))?;
    let invocation = ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(checkout, "", Surface::Published, "bundle:sync", None)
            .map_err(|err| error(&manifest, err.to_string()))?,
        owner,
        route: Route::Builtin("bundle sync".into()),
        argv: Vec::new(),
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        gates: Vec::new(),
        attachments: Vec::new(),
        class: EffectClass::Prepare,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .map_err(|err| error(&manifest, err.to_string()))?;
    let mut random = [0; 10];
    getrandom::fill(&mut random).map_err(|err| error(&manifest, err.to_string()))?;
    let run_id = RunId::new(0, random).map_err(|err| error(&manifest, err.to_string()))?;
    // A legacy import is local snapshot/publication work. Keep its real run
    // and gates, but do not wait for a sentinel when no child can be launched.
    let remote = selections.iter().any(|(name, selection)| {
        !matches!(selection, BundleSelection::Path { .. })
            && matches!(
                plans.get(name),
                Some(SourcePlan::Acquire(_) | SourcePlan::RemoteCheck)
            )
    });
    let custody = if remote {
        Some(LifecyclePlan {
            executable: Executable::capture(&preparation.executable)
                .map_err(|err| io_error(&preparation.executable, err))?,
            placement: effigy_kernel::supervise::Placement::Inline,
        })
    } else {
        None
    };
    let cleanup = op.cleanup(Duration::from_secs(15)).map_err(|_| {
        error(
            &manifest,
            "source preparation could not reserve cleanup time: the deadline is out of range",
        )
    })?;
    let mut run = Run::begin(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id: run_id,
            wait: Wait::Until(deadline),
            acknowledge: Vec::new(),
            next: preparation.next.clone(),
            custody,
        },
        &cleanup,
        &mut SourceSettlement,
    )
    .map_err(|err| error(&manifest, err.to_string()))?;
    let result = acquire_and_publish(
        &manifest,
        &selections,
        mode,
        &plans,
        &old,
        &checked,
        now,
        active_version,
        &cx,
        &mut run,
        &preparation.home,
        &preparation.progress,
    );
    // Settlement gets its own fresh budget after acquisition, independent of
    // both the workload deadline and begin's predecessor-recovery budget.
    let cleanup = op.cleanup(Duration::from_secs(15)).map_err(|_| {
        error(
            &manifest,
            "source preparation could not reserve settlement time: the deadline is out of range",
        )
    })?;
    let completion = match &result {
        // A required source command that failed cannot leave the run Succeeded
        // (K5 obligations); the kept pins are still what the caller uses.
        Ok(report) if !report.warnings.is_empty() => {
            run.finish(WorkloadOutcome::Failed { code: 1 })
        }
        Ok(_) => run.finish(WorkloadOutcome::Succeeded),
        Err(_) => match op.check("source preparation") {
            Ok(()) => run.finish(WorkloadOutcome::Failed { code: 1 }),
            Err(effigy_kernel::ctx::Interrupt::TimedOut { phase, .. }) => {
                run.interrupt(&cleanup, WorkloadOutcome::TimedOut { phase })
            }
            Err(effigy_kernel::ctx::Interrupt::Cancelled { cause }) => run.interrupt(
                &cleanup,
                WorkloadOutcome::Cancelled {
                    cause,
                    signal: None,
                },
            ),
        },
    };
    let settled = run
        .settle(&cleanup, &mut SourceSettlement)
        .map_err(|err| error(&manifest, err.to_string()))?;
    if settled.cleanup() != CleanupOutcome::Complete {
        return Err(error(
            &manifest,
            format!(
                "source preparation cleanup is incomplete: {}; retry `effigy bundle sync` to settle the prior run",
                settled.unresolved().join("; ")
            ),
        ));
    }
    if let Err(record_error) = completion {
        return Err(error(
            &manifest,
            format!(
                "{}; recording source outcome failed: {record_error}",
                result
                    .as_ref()
                    .err()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "source acquisition completed".into())
            ),
        ));
    }
    result.map(Some)
}

/// One source's place in this Prepare (contract 020 §Behaviour): a missing pin
/// is acquired, a changed declaration is re-acquired with no TTL, a path source
/// is re-snapshotted when its content digest moved, and a floating Git source
/// gets a bounded remote check at most once per checkout per TTL. A full-SHA
/// `ref`, an OCI pin and a source declared `update = "manual"` never move here.
#[allow(clippy::too_many_arguments)]
fn plan_source(
    manifest: &Path,
    name: &str,
    selection: &BundleSelection,
    pin: Option<&SourcePin>,
    policy: crate::ManifestSourceUpdate,
    checked_at: Option<u64>,
    now: u64,
    active_version: &str,
    progress: &SourceProgress,
) -> Result<SourcePlan, ManifestError> {
    let Some(pin) = pin else {
        return Ok(SourcePlan::Acquire(RefreshReason::Missing));
    };
    if !parent(manifest).join(".effigy").join(&pin.object).is_dir() {
        return Ok(SourcePlan::Acquire(RefreshReason::Missing));
    }
    if policy.is_manual() {
        return Ok(SourcePlan::Keep);
    }
    if !same_declaration(&pin.declaration, selection) {
        return Ok(SourcePlan::Acquire(RefreshReason::Declaration));
    }
    match selection {
        BundleSelection::Path { path } => match source_digest(path, manifest) {
            Ok(digest) if digest == pin.version => Ok(SourcePlan::Keep),
            Ok(_) => Ok(SourcePlan::Acquire(RefreshReason::Content)),
            // A vanished or unreadable path keeps the pin, like any other
            // failed update attempt.
            Err(failure) => {
                warn(progress, &kept_warning(name, pin, &failure));
                Ok(SourcePlan::Keep)
            }
        },
        BundleSelection::Git { reference, rev, .. } => {
            if rev.is_some() || reference.as_deref().is_some_and(is_full_revision) {
                return Ok(SourcePlan::Keep);
            }
            if pin.fallback.as_ref().is_some_and(|fallback| {
                !super::super::compatibility::same_running_effigy_version(
                    active_version,
                    &fallback.running_effigy,
                )
            }) {
                return Ok(SourcePlan::Acquire(RefreshReason::Upstream));
            }
            if checked_at.is_some_and(|at| now.saturating_sub(at) < REMOTE_CHECK_TTL.as_secs()) {
                return Ok(SourcePlan::Keep);
            }
            Ok(SourcePlan::RemoteCheck)
        }
        // An OCI pin is its resolved digest; a moving tag still needs sync.
        BundleSelection::Oci { .. } => Ok(SourcePlan::Keep),
    }
}

/// A full commit id is a pinned revision, not a floating one.
fn is_full_revision(reference: &str) -> bool {
    matches!(reference.len(), 40 | 64) && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// The last floating-source remote check this checkout recorded per source.
/// An unreadable or absent record grants no TTL: the check runs again.
fn read_update_state(manifest: &Path) -> SourceUpdateState {
    let Ok(dir) = DirFd::open(&parent(manifest).join(".effigy")) else {
        return SourceUpdateState::empty();
    };
    let op = observe_context();
    let Ok(cx) = op.observe() else {
        return SourceUpdateState::empty();
    };
    let Ok(name) = FileName::new(UPDATE_STATE_FILE) else {
        return SourceUpdateState::empty();
    };
    match read_record::<SourceUpdateState>(&cx, &dir, &name) {
        Observed::Verified(state, _) => state,
        Observed::Absent(_) | Observed::Unknown(_) => SourceUpdateState::empty(),
    }
}

/// The one line a Prepare path prints when it selects new bytes.
fn update_note(
    name: &str,
    previous: &SourcePin,
    selected: &SourcePin,
    reason: RefreshReason,
) -> String {
    format!(
        "{name} updated {} → {} ({})",
        short_pin(previous),
        short_pin(selected),
        reason.describe()
    )
}

fn short_pin(pin: &SourcePin) -> String {
    let value = pin.revision.as_deref().unwrap_or(&pin.version);
    let value = value.strip_prefix("sha256:").unwrap_or(value);
    value.chars().take(8).collect()
}

/// The short name a person reads for a source while it is fetched: the
/// bundle's own base name for `bundle`, the catalog handle for a catalog
/// source. Labels stay short enough for a spinner line.
fn source_subject(name: &str, selection: &BundleSelection) -> String {
    if name != "bundle" {
        return name.trim_start_matches("catalog:").to_owned();
    }
    match selection {
        BundleSelection::Path { path } => path
            .file_name()
            .map(|base| base.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
        BundleSelection::Git { url, .. } => url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .map(|base| base.trim_end_matches(".git").to_owned())
            .filter(|base| !base.is_empty())
            .unwrap_or_else(|| url.clone()),
        BundleSelection::Oci { url } => url.clone(),
    }
}

/// The revision a fetch names: the declared ref, the floating default, or the
/// content digest a path and OCI source re-derives.
fn source_revision(selection: &BundleSelection) -> String {
    match selection {
        BundleSelection::Path { .. } => "path".to_owned(),
        BundleSelection::Git { reference, rev, .. } => rev
            .clone()
            .or_else(|| reference.clone())
            .unwrap_or_else(|| "HEAD".to_owned()),
        BundleSelection::Oci { .. } => "oci".to_owned(),
    }
}

/// Why a source acquisition process did not start, in words: the kernel keeps
/// its own variant names for `--json` and diagnostics.
fn spawn_refusal_text(err: &SpawnError) -> String {
    match err {
        SpawnError::Refused(detail) => {
            format!("source acquisition was refused: {detail}; retry `effigy bundle sync`")
        }
        SpawnError::Io(error) => format!(
            "source acquisition could not start the clone: {error}; check the toolchain and retry"
        ),
        SpawnError::Run(error) | SpawnError::Unrecorded(error) => format!(
            "source acquisition could not be recorded in this checkout's run journal: {error}; run `effigy doctor`"
        ),
    }
}

fn remember_update_notes(notes: &[String]) {
    if let Ok(mut stored) = UPDATE_NOTES.lock() {
        stored.extend(notes.iter().cloned());
    }
}

/// The update lines this process selected on a Prepare path, for a JSON
/// envelope's `notes`.
pub fn bundle_update_notes() -> Vec<String> {
    UPDATE_NOTES
        .lock()
        .map(|stored| stored.clone())
        .unwrap_or_default()
}

/// A failed refresh keeps the current pin and says so once (contract 020
/// §Failure). Only the first acquisition of a declaration that has no usable
/// pin is a hard error.
fn kept_warning(name: &str, pin: &SourcePin, failure: &ManifestError) -> String {
    format!(
        "{name} update failed: {failure}; keeping {}",
        short_pin(pin)
    )
}

/// A warning goes through the progress hook, which draws it above any running
/// spinner and keeps it off stderr under `--json`.
fn warn(progress: &SourceProgress, message: &str) {
    progress.emit(SourceProgressEvent::Warning {
        message: message.to_owned(),
    });
}

/// One source's Started event and the single Finished that ends it. A step
/// dropped before [`AcquisitionStep::finish`] reports a failure, so an early
/// return cannot leave a spinner running.
struct AcquisitionStep<'a> {
    progress: &'a SourceProgress,
    source: String,
    subject: String,
    done: bool,
}

impl<'a> AcquisitionStep<'a> {
    fn start(
        progress: &'a SourceProgress,
        source: &str,
        subject: String,
        revision: String,
        local: bool,
    ) -> Self {
        progress.emit(SourceProgressEvent::Started {
            source: source.to_owned(),
            subject: subject.clone(),
            revision,
            local,
        });
        Self {
            progress,
            source: source.to_owned(),
            subject,
            done: false,
        }
    }

    fn finish(mut self, kept: Option<String>, failure: Option<String>) {
        self.done = true;
        self.progress.emit(SourceProgressEvent::Finished {
            source: self.source.clone(),
            subject: self.subject.clone(),
            kept,
            failure,
        });
    }
}

impl Drop for AcquisitionStep<'_> {
    fn drop(&mut self) {
        if !self.done {
            self.progress.emit(SourceProgressEvent::Finished {
                source: self.source.clone(),
                subject: self.subject.clone(),
                kept: None,
                failure: Some("acquisition stopped before it finished".to_owned()),
            });
        }
    }
}

/// The acquisition error with the one command that retries it appended, unless
/// the message already names it.
fn with_sync_hint(manifest: &Path, failure: ManifestError) -> ManifestError {
    match failure {
        ManifestError::Compose { detail, .. } if detail.contains("effigy bundle sync") => {
            error(manifest, detail)
        }
        ManifestError::Compose { detail, .. } => {
            error(manifest, format!("{detail}; run `effigy bundle sync`"))
        }
        other => error(manifest, format!("{other}; run `effigy bundle sync`")),
    }
}

fn source_dir(
    parent: &DirFd,
    name: &str,
    cx: &EffectContext,
    manifest: &Path,
) -> Result<DirFd, ManifestError> {
    let name = FileName::new(name).map_err(|err| error(manifest, err.to_string()))?;
    parent
        .prepare_child(cx, &name)
        .map_err(|err| error(manifest, err.to_string()))
}
fn ensure_generation<'a>(
    manifest: &Path,
    objects: &DirFd,
    generation_id: &str,
    generation: &'a mut Option<DirFd>,
    cx: &EffectContext,
    run: &mut Run<'_>,
) -> Result<&'a DirFd, ManifestError> {
    if generation.is_none() {
        let path = objects.path().join(generation_id);
        let ticket = retained_intent(run, "sources:generation", &path, manifest)?;
        let created = source_dir(objects, generation_id, cx, manifest)?;
        run.applied(
            &ticket,
            serde_json::json!({"path":created.path().to_string_lossy()}),
        )
        .map_err(|err| error(manifest, err.to_string()))?;
        *generation = Some(created);
    }
    Ok(generation.as_ref().expect("generation was initialized"))
}
fn retained_intent(
    run: &mut Run<'_>,
    kind: &str,
    path: &Path,
    manifest: &Path,
) -> Result<effigy_kernel::run::EffectTicket, ManifestError> {
    run.intend(EffectSpec {
        target: run.fence().clone(),
        kind: kind.into(),
        payload: serde_json::json!({"path":path.to_string_lossy()}),
        settlement: Settlement::Retain,
        required: false,
    })
    .map_err(|err| error(manifest, err.to_string()))
}
fn publish_lock(
    manifest: &Path,
    lock: &SourceLock,
    directory: &DirFd,
    run: &mut Run<'_>,
) -> Result<(), ManifestError> {
    let bytes = serde_json::to_vec_pretty(lock).map_err(|err| error(manifest, err.to_string()))?;
    if bytes.len() as u64 > LOCK_MAX {
        return Err(error(manifest, "source lock exceeds size limit"));
    }
    let ticket = retained_intent(
        run,
        "sources:publish",
        &directory.path().join(LOCK_FILE),
        manifest,
    )?;
    let name = FileName::new(LOCK_FILE).map_err(|err| error(manifest, err.to_string()))?;
    run.replace_record(
        directory,
        &name,
        &bytes,
        effigy_kernel::record::ReplaceOpts {
            mode: 0o644,
            keep_previous: false,
        },
    )
    .map_err(|err| error(manifest, err.to_string()))?;
    run.applied(&ticket, serde_json::json!({"sha256":sha256_hex(&bytes)}))
        .map_err(|err| error(manifest, err.to_string()))
}
#[allow(clippy::too_many_arguments)]
fn acquire_and_publish(
    manifest: &Path,
    selections: &BTreeMap<String, BundleSelection>,
    mode: Mode,
    plans: &BTreeMap<String, SourcePlan>,
    old: &SourceLock,
    checked: &SourceUpdateState,
    now: u64,
    active_version: &str,
    cx: &EffectContext,
    run: &mut Run<'_>,
    home: &Path,
    progress: &SourceProgress,
) -> Result<BundleSyncReport, ManifestError> {
    let project = DirFd::open(parent(manifest)).map_err(|err| error(manifest, err.to_string()))?;
    let directory = source_dir(&project, ".effigy", cx, manifest)?;
    let cache = source_dir(&directory, "cache", cx, manifest)?;
    let objects = source_dir(&cache, "sources", cx, manifest)?;
    let generation_id = run.id().to_string();
    let mut generation = None;
    let mut imported = old.sources.clone();
    let mut imported_now = Vec::new();
    // Preserve the exact pre-upgrade bytes even when the first explicit sync
    // subsequently fails. No legacy cache is changed or removed.
    for (name, selection) in selections {
        if imported.contains_key(name) {
            continue;
        }
        if let Some(legacy) = legacy_root(manifest, selection).filter(|root| root.is_dir()) {
            let generation =
                ensure_generation(manifest, &objects, &generation_id, &mut generation, cx, run)?;
            let object = source_dir(
                generation,
                &format!("import-{}", sha256_hex(name.as_bytes())),
                cx,
                manifest,
            )?;
            let version = snapshot(&legacy, &object, cx, manifest)?;
            imported.insert(
                name.clone(),
                SourcePin {
                    declaration: selection.spec(),
                    object: relative_object(manifest, object.path())?,
                    version,
                    revision: legacy_version(&legacy, selection),
                    fallback: None,
                },
            );
            imported_now.push(name.clone());
        }
    }
    if imported.len() != old.sources.len() {
        publish_lock(
            manifest,
            &SourceLock {
                version: 1,
                sources: imported.clone(),
            },
            &directory,
            run,
        )?;
    }
    // If a legacy directory disappeared after planning, refuse honestly rather
    // than silently falling through to a fetch without planned custody.
    for (name, plan) in plans {
        if *plan == SourcePlan::Import && !imported.contains_key(name) {
            return Err(error(
                manifest,
                format!(
                    "legacy source `{name}` disappeared before import; run `effigy bundle sync`"
                ),
            ));
        }
    }
    let mut pins = imported.clone();
    let mut reports = BTreeMap::new();
    let mut changed = false;
    let mut notes = Vec::new();
    let mut warnings = Vec::new();
    let mut checks = checked.checks.clone();
    let mut checked_any = false;
    for (name, selection) in selections {
        let pinned = pins.get(name).cloned();
        let mut reason = match plans.get(name) {
            // A task run that just imported this source's v0.14.1 bytes already
            // has a usable pin, so it is neither acquired nor refused. A
            // Prepare path still refreshes what is behind: the import is its
            // safety net if the update fails.
            Some(SourcePlan::Acquire(_))
                if mode == Mode::Missing && imported_now.contains(name) =>
            {
                None
            }
            Some(SourcePlan::Acquire(reason)) => Some(*reason),
            _ => None,
        };
        if matches!(plans.get(name), Some(SourcePlan::RemoteCheck)) {
            let BundleSelection::Git {
                url,
                reference,
                rev: _,
            } = selection
            else {
                return Err(error(manifest, "only a Git source has a remote check"));
            };
            checks.insert(name.clone(), now);
            checked_any = true;
            if pinned
                .as_ref()
                .and_then(|pin| pin.fallback.as_ref())
                .is_some()
            {
                // Re-probe the branch and bounded tag candidates after the
                // REMOTE_CHECK_TTL expires so a newer compatible tag published
                // without moving the skipped branch tip is picked up.
                reason = Some(RefreshReason::Upstream);
            } else {
                match git_remote_revision(
                    manifest,
                    run,
                    cx,
                    home,
                    parent(manifest),
                    url,
                    reference.as_deref(),
                ) {
                    Ok(Some(revision)) => {
                        if pinned.as_ref().and_then(|pin| pin.revision.as_deref())
                            != Some(revision.as_str())
                        {
                            reason = Some(RefreshReason::Upstream);
                        }
                    }
                    Ok(None) => {}
                    Err(failure) => {
                        if let Some(pin) = &pinned {
                            let message = kept_warning(name, pin, &failure);
                            warn(progress, &message);
                            warnings.push(message);
                        }
                    }
                }
            }
        }
        let Some(reason) = reason else {
            if let Some(pin) = &pinned {
                reports.insert(name.clone(), pin_report(manifest, selection, pin));
            }
            continue;
        };
        let generation =
            ensure_generation(manifest, &objects, &generation_id, &mut generation, cx, run)?;
        let object = source_dir(generation, &sha256_hex(name.as_bytes()), cx, manifest)?;
        let step = AcquisitionStep::start(
            progress,
            name,
            source_subject(name, selection),
            source_revision(selection),
            matches!(selection, BundleSelection::Path { .. }),
        );
        let acquired = acquire_source(
            manifest,
            name,
            selection,
            active_version,
            run,
            cx,
            home,
            generation,
            &object,
            progress,
        )
        .and_then(|(version, revision, fallback)| {
            if name == "bundle" {
                crate::composition::validate_source_snapshot(
                    manifest,
                    object.path(),
                    Some(active_version),
                )?;
            }
            Ok((version, revision, fallback))
        });
        let (version, revision, fallback) = match acquired {
            Ok(acquired) => acquired,
            Err(failure) => {
                // Contract 020 §Failure: a pin whose declaration is unchanged
                // survives a failed refresh. A changed declaration with no
                // usable pin is the acquisition error.
                let keepable = pinned.as_ref().is_some_and(|pin| {
                    same_declaration(&pin.declaration, selection)
                        && parent(manifest).join(".effigy").join(&pin.object).is_dir()
                });
                if let Some(pin) = pinned
                    .as_ref()
                    .filter(|_| mode == Mode::Refresh && keepable)
                {
                    let message = kept_warning(name, pin, &failure);
                    warn(progress, &message);
                    warnings.push(message.clone());
                    step.finish(Some(short_pin(pin)), Some(message));
                    reports.insert(name.clone(), pin_report(manifest, selection, pin));
                    continue;
                }
                step.finish(None, Some(failure.to_string()));
                return Err(match mode {
                    // A task run keeps the strict parse's guidance when its own
                    // first acquisition cannot reach the source.
                    Mode::Missing | Mode::Refresh => with_sync_hint(manifest, failure),
                    Mode::Sync => failure,
                });
            }
        };
        step.finish(None, None);
        if mode != Mode::Missing && fallback.is_some() {
            // Cache the bounded fallback probe under REMOTE_CHECK_TTL without
            // turning normal initial acquisitions or read-only discovery into a
            // completed remote refresh.
            checks.insert(name.clone(), now);
            checked_any = true;
        }
        let selected = SourcePin {
            declaration: selection.spec(),
            object: relative_object(manifest, object.path())?,
            version,
            revision,
            fallback,
        };
        let pin_changed = pinned.as_ref().is_none_or(|pin| {
            pin.version != selected.version
                || pin.revision != selected.revision
                || pin.fallback != selected.fallback
                || !same_declaration(&pin.declaration, selection)
        });
        if mode == Mode::Refresh && !pin_changed {
            if let Some(existing) = &pinned {
                reports.insert(name.clone(), pin_report(manifest, selection, existing));
                continue;
            }
        }
        changed |= pin_changed;
        if mode == Mode::Refresh {
            if let Some(previous) = &pinned {
                if previous.version != selected.version || previous.revision != selected.revision {
                    notes.push(update_note(name, previous, &selected, reason));
                }
            }
        }
        if let Some(fallback) = &selected.fallback {
            notes.push(fallback.notice(name));
        }
        reports.insert(name.clone(), pin_report(manifest, selection, &selected));
        pins.insert(name.clone(), selected);
    }
    // Git exclusions are explicit Prepare effects, never a manifest read effect.
    let exclude = retained_intent(run, "sources:git-exclude", parent(manifest), manifest)?;
    effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root(parent(manifest))
        .map_err(|err| io_error(manifest, err))?;
    effigy_core::runtime_dir::ensure_local_overlay_ignored_in_git_root(parent(manifest))
        .map_err(|err| io_error(manifest, err))?;
    run.applied(&exclude, serde_json::Value::Null)
        .map_err(|err| error(manifest, err.to_string()))?;
    // The TTL record of this run's remote checks. It is an explicit Prepare
    // effect beside the lock until the per-checkout state store owns it (#276).
    if checked_any {
        checks.retain(|_, at| now.saturating_sub(*at) < UPDATE_STATE_KEEP.as_secs());
        while checks.len() > 4096 {
            let Some(oldest) = checks
                .iter()
                .min_by_key(|(_, at)| **at)
                .map(|(name, _)| name.clone())
            else {
                break;
            };
            checks.remove(&oldest);
        }
        let bytes = serde_json::to_vec(&SourceUpdateState { version: 1, checks })
            .map_err(|err| error(manifest, err.to_string()))?;
        let state_ticket = retained_intent(
            run,
            "sources:update-state",
            &directory.path().join(UPDATE_STATE_FILE),
            manifest,
        )?;
        let state_name =
            FileName::new(UPDATE_STATE_FILE).map_err(|err| error(manifest, err.to_string()))?;
        run.replace_record(
            &directory,
            &state_name,
            &bytes,
            effigy_kernel::record::ReplaceOpts {
                mode: 0o644,
                keep_previous: false,
            },
        )
        .map_err(|err| error(manifest, err.to_string()))?;
        run.applied(&state_ticket, serde_json::Value::Null)
            .map_err(|err| error(manifest, err.to_string()))?;
    }
    if mode == Mode::Sync {
        pins.retain(|name, _| selections.contains_key(name));
    }
    // One pointer publication commits bundle and catalog updates together. A
    // Prepare that found nothing stale selects the bytes it already had, so it
    // does not rewrite the lock.
    if pins != old.sources {
        publish_lock(
            manifest,
            &SourceLock {
                version: 1,
                sources: pins.clone(),
            },
            &directory,
            run,
        )?;
    }
    prune_unpinned_generations(
        manifest,
        &objects,
        &pins,
        generation.as_ref().map(|_| generation_id.as_str()),
        cx,
        run,
        progress,
    )?;
    for note in &notes {
        progress.emit(SourceProgressEvent::Note {
            message: note.clone(),
        });
    }
    remember_update_notes(&notes);
    let primary = reports
        .get("bundle")
        .or_else(|| reports.values().next())
        .ok_or_else(|| error(manifest, "no source reports"))?;
    Ok(BundleSyncReport {
        source_type: primary.source_type,
        source_path: primary.source_path.clone(),
        local_path: Some(primary.local_path.clone()),
        version_hint: primary.version_hint.clone(),
        changed,
        applicable: true,
        sources: reports,
        notes,
        warnings,
    })
}

/// Remove the source generations the published lock does not pin and no live
/// invocation holds. Every Prepare writes a new generation, so without this the
/// predecessor of each sync stays on disk for good. Runs after the lock is
/// published, so the generations it selects are never removed.
///
/// Cleanup is maintenance: an unlistable or undeletable generation warns and
/// the successful Prepare stands.
fn prune_unpinned_generations(
    manifest: &Path,
    objects: &DirFd,
    pins: &BTreeMap<String, SourcePin>,
    current_generation: Option<&str>,
    cx: &EffectContext,
    run: &mut Run<'_>,
    progress: &SourceProgress,
) -> Result<(), ManifestError> {
    if pins.is_empty() {
        return Ok(());
    }
    let listed = cx
        .operation()
        .observe()
        .ok()
        .and_then(|observe| objects.observe_entries(&observe, 4096).ok());
    let Some(entries) = listed else {
        warn(
            progress,
            "source pruning skipped: the source cache could not be listed",
        );
        return Ok(());
    };
    // Bytes a live invocation holds are not this Prepare's to remove (contract
    // 020): a run keeps the generation it started with until it exits.
    let held = super::hold::held_generations(manifest);
    let mut pruned = 0usize;
    for entry in entries {
        let Some(name) = entry.to_str() else {
            continue;
        };
        let prefix = Path::new("cache/sources").join(name);
        if pins.values().any(|pin| pin.object.starts_with(&prefix)) || held.contains(name) {
            continue;
        }
        let name = FileName::new(name).map_err(|err| error(manifest, err.to_string()))?;
        let ticket = retained_intent(
            run,
            "sources:prune",
            &objects.path().join(name.as_str()),
            manifest,
        )?;
        match objects.remove_prepared_tree(cx, &name, GENERATION_TREE_DEPTH) {
            Ok(()) => {
                run.applied(&ticket, serde_json::json!({ "pruned": name.as_str() }))
                    .map_err(|err| error(manifest, err.to_string()))?;
                if Some(name.as_str()) != current_generation {
                    pruned += 1;
                }
            }
            Err(error) => warn(
                progress,
                &format!(
                    "source generation {} was not pruned: {error}",
                    name.as_str()
                ),
            ),
        }
    }
    if pruned > 0 {
        progress.emit(SourceProgressEvent::Note {
            message: format!(
                "pruned {pruned} source generation{} nothing pins",
                if pruned == 1 { "" } else { "s" }
            ),
        });
    }
    Ok(())
}

/// Acquire one source into its generation directory and return the recorded
/// version, revision hint, and optional compatibility fallback metadata.
#[allow(clippy::too_many_arguments)]
fn acquire_source(
    manifest: &Path,
    name: &str,
    selection: &BundleSelection,
    active_version: &str,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    generation: &DirFd,
    object: &DirFd,
    progress: &SourceProgress,
) -> Result<(String, Option<String>, Option<SourceCompatibilityFallback>), ManifestError> {
    match selection {
        BundleSelection::Path { path } => Ok((snapshot(path, object, cx, manifest)?, None, None)),
        BundleSelection::Git {
            url,
            reference,
            rev,
        } => acquire_git_source(
            manifest,
            name,
            selection,
            url,
            reference.as_deref(),
            rev.as_deref(),
            active_version,
            run,
            cx,
            home,
            generation,
            object,
            progress,
        ),
        BundleSelection::Oci { url } => {
            let reference = url.trim_start_matches("oci://");
            let raw = command(
                manifest,
                run,
                cx,
                home,
                parent(manifest),
                vec![
                    "oras".into(),
                    "manifest".into(),
                    "fetch".into(),
                    "--descriptor".into(),
                    reference.into(),
                ],
                BUDGET,
                None,
            )?;
            let descriptor: serde_json::Value = serde_json::from_slice(&raw)
                .map_err(|err| error(manifest, format!("invalid OCI descriptor: {err}")))?;
            let digest = descriptor
                .get("digest")
                .and_then(|value| value.as_str())
                .filter(|digest| {
                    digest.starts_with("sha256:")
                        && digest.len() == 71
                        && digest[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                .ok_or_else(|| error(manifest, "OCI source has no valid sha256 digest"))?;
            let repository = reference.split('@').next().unwrap_or(reference);
            let repository = match repository.rfind(':') {
                Some(colon) if repository.rfind('/').is_some_and(|slash| colon > slash) => {
                    &repository[..colon]
                }
                _ => repository,
            };
            let pull = source_dir(
                generation,
                &format!("oci-{}", sha256_hex(name.as_bytes())),
                cx,
                manifest,
            )?;
            command(
                manifest,
                run,
                cx,
                home,
                parent(manifest),
                vec![
                    "oras".into(),
                    "pull".into(),
                    format!("{repository}@{digest}").into(),
                    "--output".into(),
                    pull.path().as_os_str().into(),
                ],
                BUDGET,
                Some(progress),
            )?;
            Ok((
                snapshot(pull.path(), object, cx, manifest)?,
                Some(digest.into()),
                None,
            ))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn acquire_git_source(
    manifest: &Path,
    name: &str,
    selection: &BundleSelection,
    url: &str,
    reference: Option<&str>,
    rev: Option<&str>,
    active_version: &str,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    generation: &DirFd,
    object: &DirFd,
    progress: &SourceProgress,
) -> Result<(String, Option<String>, Option<SourceCompatibilityFallback>), ManifestError> {
    let checkout = source_dir(
        generation,
        &format!("git-{}", sha256_hex(name.as_bytes())),
        cx,
        manifest,
    )?;
    command(
        manifest,
        run,
        cx,
        home,
        parent(manifest),
        vec![
            "git".into(),
            "clone".into(),
            "--no-checkout".into(),
            "--".into(),
            url.into(),
            checkout.path().as_os_str().into(),
        ],
        BUDGET,
        Some(progress),
    )?;
    let fetch_ref = rev.or(reference).unwrap_or("HEAD");
    git_fetch_and_checkout(
        manifest,
        run,
        cx,
        home,
        checkout.path(),
        fetch_ref,
        Some(progress),
    )?;
    let head_revision = legacy_version(checkout.path(), selection);

    if name == "bundle" {
        match super::super::compatibility::check_bundle_dir_compatibility(
            checkout.path(),
            manifest,
            active_version,
        ) {
            Ok(()) => {}
            Err(super::super::compatibility::CompatibilityCheckError::Invalid(err)) => {
                return Err(err);
            }
            Err(super::super::compatibility::CompatibilityCheckError::Incompatible(mismatch)) => {
                if let Some(pinned_rev) = rev {
                    return Err(error(
                        manifest,
                        format!(
                            "bundle rev `{pinned_rev}` requires Effigy {}, but this binary is {}; {}",
                            mismatch.required,
                            mismatch.running,
                            mismatch.guidance_for_pin()
                        ),
                    ));
                }
                let floating = is_floating_branch_ref(
                    manifest,
                    run,
                    cx,
                    home,
                    checkout.path(),
                    url,
                    reference,
                )?;
                let declared_label = match reference {
                    None | Some("HEAD") => {
                        resolve_remote_default_branch(manifest, run, cx, home, checkout.path(), url)
                            .unwrap_or_else(|| "HEAD".to_owned())
                    }
                    Some(other) => other.to_owned(),
                };
                if !floating {
                    return Err(error(
                        manifest,
                        format!(
                            "bundle ref `{declared_label}` requires Effigy {}, but this binary is {}; {}",
                            mismatch.required,
                            mismatch.running,
                            mismatch.guidance_for_pin()
                        ),
                    ));
                }
                let fallback = select_compatible_tag(
                    manifest,
                    url,
                    &declared_label,
                    head_revision,
                    &mismatch,
                    active_version,
                    run,
                    cx,
                    home,
                    checkout.path(),
                )?;
                return Ok((
                    snapshot(checkout.path(), object, cx, manifest)?,
                    legacy_version(checkout.path(), selection),
                    Some(fallback),
                ));
            }
        }
    }

    Ok((
        snapshot(checkout.path(), object, cx, manifest)?,
        head_revision,
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
fn resolve_remote_default_branch(
    manifest: &Path,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    cwd: &Path,
    url: &str,
) -> Option<String> {
    let raw = command(
        manifest,
        run,
        cx,
        home,
        cwd,
        vec![
            "git".into(),
            "ls-remote".into(),
            "--symref".into(),
            "--".into(),
            url.into(),
            "HEAD".into(),
        ],
        REMOTE_CHECK_BUDGET,
        None,
    )
    .ok()?;
    parse_symref_head_branch(&raw)
}

fn parse_symref_head_branch(raw: &[u8]) -> Option<String> {
    let output = String::from_utf8_lossy(raw);
    for line in output.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("ref: ") else {
            continue;
        };
        let target = rest
            .split_whitespace()
            .next()
            .unwrap_or(rest)
            .strip_suffix("HEAD")
            .unwrap_or_else(|| rest.split_whitespace().next().unwrap_or(rest));
        if let Some(branch) = target.strip_prefix("refs/heads/") {
            if !branch.is_empty() {
                return Some(branch.to_owned());
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn git_fetch_and_checkout(
    manifest: &Path,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    checkout_path: &Path,
    refspec: &str,
    progress: Option<&SourceProgress>,
) -> Result<(), ManifestError> {
    command(
        manifest,
        run,
        cx,
        home,
        checkout_path,
        vec![
            "git".into(),
            "fetch".into(),
            "--depth".into(),
            "1".into(),
            "--".into(),
            "origin".into(),
            refspec.into(),
        ],
        BUDGET,
        progress,
    )?;
    command(
        manifest,
        run,
        cx,
        home,
        checkout_path,
        vec![
            "git".into(),
            "-c".into(),
            "core.hooksPath=/dev/null".into(),
            "checkout".into(),
            "--detach".into(),
            "FETCH_HEAD".into(),
        ],
        BUDGET,
        progress,
    )?;
    Ok(())
}

/// Returns `true` when `reference` tracks a floating branch (`None`, `"HEAD"`,
/// `refs/heads/...`, or a remote branch name), and `false` for explicit commit
/// revisions or explicit tag pins.
#[allow(clippy::too_many_arguments)]
fn is_floating_branch_ref(
    manifest: &Path,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    cwd: &Path,
    url: &str,
    reference: Option<&str>,
) -> Result<bool, ManifestError> {
    let Some(reference) = reference else {
        return Ok(true);
    };
    if reference == "HEAD" || reference.starts_with("refs/heads/") {
        return Ok(true);
    }
    if reference.starts_with("refs/tags/") || is_full_revision(reference) {
        return Ok(false);
    }
    let raw = command(
        manifest,
        run,
        cx,
        home,
        cwd,
        vec![
            "git".into(),
            "ls-remote".into(),
            "--".into(),
            url.into(),
            format!("refs/heads/{reference}").into(),
            format!("refs/tags/{reference}").into(),
        ],
        REMOTE_CHECK_BUDGET,
        None,
    )?;
    let branch_ref = format!("refs/heads/{reference}");
    let output = String::from_utf8_lossy(&raw);
    Ok(output
        .lines()
        .filter_map(split_ls_remote_line)
        .any(|(_, name)| name == branch_ref))
}

#[allow(clippy::too_many_arguments)]
fn select_compatible_tag(
    manifest: &Path,
    url: &str,
    declared_ref: &str,
    head_revision: Option<String>,
    mismatch: &super::super::compatibility::CompatibilityMismatch,
    active_version: &str,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    checkout_path: &Path,
) -> Result<SourceCompatibilityFallback, ManifestError> {
    let raw = command(
        manifest,
        run,
        cx,
        home,
        checkout_path,
        vec![
            "git".into(),
            "ls-remote".into(),
            "--tags".into(),
            "--".into(),
            url.into(),
        ],
        REMOTE_CHECK_BUDGET,
        None,
    )?;
    let candidates = parse_ls_remote_tags(&raw);
    let mut probed = 0usize;
    for (tag_name, tag_sha) in candidates {
        if head_revision.as_deref() == Some(tag_sha.as_str()) {
            continue;
        }
        if probed >= MAX_COMPATIBILITY_TAG_CANDIDATES {
            break;
        }
        probed += 1;
        let refspec = format!("refs/tags/{tag_name}");
        if git_fetch_and_checkout(manifest, run, cx, home, checkout_path, &refspec, None).is_err() {
            continue;
        }
        if super::super::compatibility::check_bundle_dir_compatibility(
            checkout_path,
            manifest,
            active_version,
        )
        .is_ok()
        {
            return Ok(SourceCompatibilityFallback {
                selected_tag: tag_name,
                skipped_ref: declared_ref.to_owned(),
                skipped_revision: head_revision,
                required_effigy: mismatch.required.clone(),
                running_effigy: mismatch.running.clone(),
                kind: mismatch.kind,
            });
        }
    }
    if probed == 0 {
        Err(error(
            manifest,
            format!(
                "bundle ref `{declared_ref}` requires Effigy {}, but this binary is {}, and the repository has no compatible tags; {}",
                mismatch.required,
                mismatch.running,
                mismatch.guidance_for_no_tag()
            ),
        ))
    } else {
        Err(error(
            manifest,
            format!(
                "bundle ref `{declared_ref}` requires Effigy {}, but this binary is {}, and no compatible tag was found (checked {probed} candidate tag{}); {}",
                mismatch.required,
                mismatch.running,
                if probed == 1 { "" } else { "s" },
                mismatch.guidance_for_no_tag()
            ),
        ))
    }
}

/// Parse `git ls-remote --tags` output into `(tag_name, commit_sha)` sorted
/// newest-first: SemVer tags descending first, then non-SemVer tags descending.
fn parse_ls_remote_tags(raw: &[u8]) -> Vec<(String, String)> {
    let output = String::from_utf8_lossy(raw);
    let mut tags: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for line in output.lines() {
        let Some((sha, name)) = split_ls_remote_line(line) else {
            continue;
        };
        let Some(tag_part) = name.strip_prefix("refs/tags/") else {
            continue;
        };
        if let Some(tag_name) = tag_part.strip_suffix("^{}") {
            tags.insert(tag_name.to_owned(), (sha.to_owned(), true));
        } else {
            tags.entry(tag_part.to_owned())
                .or_insert_with(|| (sha.to_owned(), false));
        }
    }
    let mut list: Vec<(String, String)> = tags
        .into_iter()
        .map(|(tag_name, (sha, _))| (tag_name, sha))
        .collect();
    list.sort_by(|(a_name, _), (b_name, _)| {
        match (parse_tag_semver(a_name), parse_tag_semver(b_name)) {
            (Some(a_ver), Some(b_ver)) => b_ver.cmp(&a_ver).then_with(|| b_name.cmp(a_name)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b_name.cmp(a_name),
        }
    });
    list
}

fn parse_tag_semver(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim().trim_start_matches('v')).ok()
}

/// A bounded `git ls-remote` of the floating ref. `Ok(None)` means the remote
/// listed no ref that could be checked, so the pin stays.
#[allow(clippy::too_many_arguments)]
fn git_remote_revision(
    manifest: &Path,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    cwd: &Path,
    url: &str,
    reference: Option<&str>,
) -> Result<Option<String>, ManifestError> {
    let reference = reference.unwrap_or("HEAD");
    let raw = command(
        manifest,
        run,
        cx,
        home,
        cwd,
        vec![
            "git".into(),
            "ls-remote".into(),
            "--".into(),
            url.into(),
            reference.into(),
        ],
        REMOTE_CHECK_BUDGET,
        None,
    )?;
    Ok(parse_ls_remote(&raw, reference))
}

/// One `ls-remote` line: the object id, then the ref name (`HEAD` or `refs/...`).
/// Captured output passes the terminal sanitizer, which drops the TAB between
/// them, so the id is taken by its length.
fn split_ls_remote_line(line: &str) -> Option<(&str, &str)> {
    [64, 40].into_iter().find_map(|len| {
        let sha = line.get(..len)?;
        let name = line.get(len..)?.trim();
        (is_full_revision(sha) && (name == "HEAD" || name.starts_with("refs/")))
            .then_some((sha, name))
    })
}

fn parse_ls_remote(raw: &[u8], reference: &str) -> Option<String> {
    let output = String::from_utf8_lossy(raw);
    let branch = format!("refs/heads/{reference}");
    let tag = format!("refs/tags/{reference}");
    let peeled = format!("{tag}^{{}}");
    let (mut sym_head, mut head, mut tag_object, mut commit, mut first) =
        (None, None, None, None, None);
    for line in output.lines() {
        let Some((sha, name)) = split_ls_remote_line(line) else {
            continue;
        };
        first.get_or_insert_with(|| sha.to_owned());
        if reference == "HEAD" && name == "HEAD" {
            sym_head = Some(sha.to_owned());
        } else if name == branch {
            head = Some(sha.to_owned());
        } else if name == peeled {
            commit = Some(sha.to_owned());
        } else if name == tag {
            tag_object = Some(sha.to_owned());
        }
    }
    sym_head.or(head).or(commit).or(tag_object).or(first)
}
fn relative_object(manifest: &Path, root: &Path) -> Result<PathBuf, ManifestError> {
    root.strip_prefix(parent(manifest).join(".effigy"))
        .map(Path::to_path_buf)
        .map_err(|err| error(manifest, err.to_string()))
}
fn pin_report(
    manifest: &Path,
    selection: &BundleSelection,
    pin: &SourcePin,
) -> BundleSourceInspectReport {
    BundleSourceInspectReport {
        source_type: selection.kind(),
        source_path: selection.label(),
        local_path: parent(manifest).join(".effigy").join(&pin.object),
        version_hint: pin.revision.clone().or_else(|| Some(pin.version.clone())),
        stale: !same_declaration(&pin.declaration, selection),
        fallback: pin.fallback.clone(),
    }
}
#[allow(
    clippy::too_many_arguments,
    reason = "one owned source command and its optional output observer"
)]
fn command(
    manifest: &Path,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    cwd: &Path,
    mut argv: Vec<OsString>,
    limit: Duration,
    progress: Option<&SourceProgress>,
) -> Result<Vec<u8>, ManifestError> {
    let program = argv
        .first_mut()
        .ok_or_else(|| error(manifest, "source command has no executable"))?;
    if !Path::new(program).is_absolute() {
        let executable = std::env::var_os("PATH")
            .and_then(|path| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join(&*program))
                    .find(|candidate| {
                        candidate.is_file()
                            && std::fs::metadata(candidate)
                                .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
                    })
            })
            .ok_or_else(|| {
                error(
                    manifest,
                    format!(
                        "source acquisition requires `{}` on PATH",
                        program.to_string_lossy()
                    ),
                )
            })?;
        *program = std::fs::canonicalize(&executable)
            .map_err(|err| io_error(&executable, err))?
            .into_os_string();
    }

    let mut env = std::env::vars_os()
        .filter(|(name, _)| {
            !matches!(
                name.to_str(),
                Some(
                    "EFFIGY_CUSTODY_NONCE"
                        | "EFFIGY_PROBE_NONCE"
                        | "GIT_DIR"
                        | "GIT_WORK_TREE"
                        | "GIT_COMMON_DIR"
                        | "GIT_INDEX_FILE"
                        | "GIT_OBJECT_DIRECTORY"
                        | "GIT_ALTERNATE_OBJECT_DIRECTORIES"
                )
            )
        })
        .map(|(name, value)| {
            let upper = name.to_string_lossy().to_ascii_uppercase();
            let secret = ["TOKEN", "PASSWORD", "SECRET", "PRIVATE_KEY"]
                .iter()
                .any(|word| upper.contains(word));
            let value = if secret && !value.is_empty() {
                SecretValue::new(value.as_bytes().to_vec()).map(EnvValue::Secret)
            } else {
                Ok(EnvValue::Plain(value))
            };
            value.map(|value| (name, value))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|err| io_error(manifest, err))?;
    env.insert("HOME".into(), EnvValue::Plain(home.as_os_str().into()));
    env.insert("GIT_TERMINAL_PROMPT".into(), EnvValue::Plain("0".into()));
    let mut child = Custody::spawn_source(
        run,
        ProcessSpec {
            argv,
            cwd: cwd.into(),
            env,
            grace: Duration::from_secs(1),
            lifetime: Lifetime::Finite(limit),
            terminal: Terminal::None,
            label: "source acquisition".into(),
        },
        cx,
    )
    .map_err(|err| error(manifest, spawn_refusal_text(&err)))?;
    let streaming = progress.filter(|progress| progress.output_enabled());
    if let Some(progress) = streaming {
        let started = std::time::Instant::now();
        loop {
            let observed = child.poll();
            let bytes = child.take_output();
            if !bytes.is_empty() {
                progress.emit(SourceProgressEvent::Output { bytes });
            }
            if matches!(observed, Observed::Verified(..))
                || started.elapsed() >= limit
                || cx.operation().check("source acquisition").is_err()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    // The original wait still closes and verifies source capture, including
    // its capture cap, cancellation and process deadline.
    match child.wait(cx.operation()) {
        Observed::Verified(exit, _) => {
            let remaining = child.take_output();
            let bytes = match streaming {
                Some(progress) => {
                    if !remaining.is_empty() {
                        progress.emit(SourceProgressEvent::Output { bytes: remaining });
                    }
                    child.tail()
                }
                None => remaining,
            };
            child
                .complete(run, exit)
                .map_err(|err| error(manifest, err.to_string()))?;
            if exit.code != 0 {
                return Err(error(
                    manifest,
                    format!(
                        "source acquisition exited {}: {}",
                        exit.code,
                        String::from_utf8_lossy(&bytes)
                    ),
                ));
            }
            Ok(bytes)
        }
        other => Err(error(
            manifest,
            format!(
                "source acquisition could not report its result: {}",
                other.unproven()
            ),
        )),
    }
}
fn snapshot(
    source: &Path,
    destination: &DirFd,
    cx: &EffectContext,
    manifest: &Path,
) -> Result<String, ManifestError> {
    digest_tree(source, Some((destination, cx)), manifest)
}

/// The content digest a snapshot records, without writing one. A path source
/// is re-snapshotted when this differs from its pin (contract 020 §Behaviour),
/// so both walks must skip the same entries.
fn source_digest(source: &Path, manifest: &Path) -> Result<String, ManifestError> {
    digest_tree(source, None, manifest)
}

fn digest_tree(
    source: &Path,
    sink: Option<(&DirFd, &EffectContext)>,
    manifest: &Path,
) -> Result<String, ManifestError> {
    let mut hash = Sha256::new();
    let mut usage = SnapshotUsage::default();
    copy_tree(
        source,
        sink,
        Path::new(""),
        0,
        &mut usage,
        &mut hash,
        manifest,
    )?;
    Ok(format!(
        "sha256:{}",
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

/// Build output and VCS state are not bundle source bytes.
fn skipped_source_entry(name: &std::ffi::OsStr, is_dir: bool, depth: usize) -> bool {
    let name = name.to_string_lossy();
    if depth == 0
        && (name == ".git" || name == ".effigy" || name.starts_with(".effigy-bundle-source."))
    {
        return true;
    }
    is_dir && (name == "target" || name == "node_modules")
}

#[allow(clippy::too_many_arguments)]
fn copy_tree(
    source: &Path,
    sink: Option<(&DirFd, &EffectContext)>,
    relative: &Path,
    depth: usize,
    usage: &mut SnapshotUsage,
    hash: &mut Sha256,
    manifest: &Path,
) -> Result<(), ManifestError> {
    if depth > 32 {
        return Err(error(manifest, "source tree exceeds depth limit"));
    }
    if let Some((_, cx)) = sink {
        cx.operation()
            .check("snapshot source")
            .map_err(|err| error(manifest, format!("source snapshot stopped: {err}")))?;
    }
    let mut entries = std::fs::read_dir(source)
        .map_err(|err| io_error(source, err))?
        .take(MAX_ENTRIES + 1)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| io_error(source, err))?;
    if entries.len() > MAX_ENTRIES {
        return Err(error(manifest, "source directory exceeds entry limit"));
    }
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if let Some((_, cx)) = sink {
            cx.operation()
                .check("snapshot entry")
                .map_err(|err| error(manifest, format!("source snapshot stopped: {err}")))?;
        }
        usage.entries += 1;
        if usage.entries > MAX_ENTRIES {
            return Err(error(manifest, "source tree exceeds entry limit"));
        }
        let name = entry.file_name();
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|err| io_error(&path, err))?;
        if skipped_source_entry(&name, metadata.is_dir(), depth) {
            continue;
        }
        let relative = relative.join(&name);
        let name = name
            .to_str()
            .ok_or_else(|| error(manifest, "source filename is not UTF-8"))?;
        if metadata.is_dir() {
            hash.update([0]);
            hash.update((relative.as_os_str().as_bytes().len() as u64).to_be_bytes());
            hash.update(relative.as_os_str().as_bytes());
            match sink {
                Some((destination, cx)) => {
                    let child = source_dir(destination, name, cx, manifest)?;
                    copy_tree(
                        &path,
                        Some((&child, cx)),
                        &relative,
                        depth + 1,
                        usage,
                        hash,
                        manifest,
                    )?;
                }
                None => copy_tree(&path, None, &relative, depth + 1, usage, hash, manifest)?,
            }
        } else if metadata.is_file() {
            if metadata.len() > MAX_FILE {
                return Err(error(manifest, "source file exceeds size limit"));
            }
            usage.bytes = usage.bytes.saturating_add(metadata.len());
            if usage.bytes > MAX_TREE {
                return Err(error(manifest, "source tree exceeds size limit"));
            }
            let mode = metadata.permissions().mode() & 0o777 & !0o022;
            hash.update([1]);
            hash.update((relative.as_os_str().as_bytes().len() as u64).to_be_bytes());
            hash.update(relative.as_os_str().as_bytes());
            hash.update(mode.to_be_bytes());
            let mut bytes = Vec::new();
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)
                .map_err(|err| io_error(&path, err))?;
            file.take(MAX_FILE + 1)
                .read_to_end(&mut bytes)
                .map_err(|err| io_error(&path, err))?;
            if bytes.len() as u64 != metadata.len() {
                return Err(error(manifest, "source file changed during snapshot"));
            }
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(&bytes);
            if let Some((destination, _)) = sink {
                let target = destination.path().join(name);
                let mut output = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(mode)
                    .open(&target)
                    .map_err(|err| io_error(&target, err))?;
                output
                    .write_all(&bytes)
                    .and_then(|()| output.sync_all())
                    .map_err(|err| io_error(&target, err))?;
            }
        } else {
            return Err(error(
                manifest,
                format!(
                    "source contains a symlink or special file: {}",
                    path.display()
                ),
            ));
        }
    }
    if let Some((destination, _)) = sink {
        std::fs::File::open(destination.path())
            .and_then(|directory| directory.sync_all())
            .map_err(|err| io_error(destination.path(), err))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path) -> (PathBuf, SourcePreparation) {
        let bundle = root.join("bundle");
        std::fs::create_dir(&bundle).unwrap();
        std::fs::write(bundle.join("bundle.toml"), "[bundle]\nname = \"fixture\"\n").unwrap();
        std::fs::write(
            bundle.join("effigy.toml"),
            "[tasks.hello]\nrun = \"echo old\"\n",
        )
        .unwrap();
        let manifest = root.join("effigy.toml");
        std::fs::write(&manifest, "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n[catalog.sources.extra]\ntype = \"path\"\ndir = \"bundle\"\n").unwrap();
        let home = root.join("home");
        std::fs::create_dir(&home).unwrap();
        (
            manifest,
            SourcePreparation {
                home,
                executable: std::env::current_exe().unwrap(),
                cancel: CancelToken::default(),
                budget: Duration::from_secs(30),
                next: Invocation::validated(vec!["bundle".into(), "sync".into()], |args| {
                    args == ["bundle", "sync"]
                })
                .unwrap(),
                progress: SourceProgress::silent(),
            },
        )
    }

    fn lock_bytes(root: &Path) -> Vec<u8> {
        std::fs::read(root.join(".effigy").join(LOCK_FILE)).unwrap()
    }

    fn selected(manifest: &Path) -> String {
        crate::load_task_manifest_with_inspection(manifest)
            .unwrap()
            .effective_manifest
    }

    /// Every path under `root` with its bytes, for proving a read wrote nothing.
    fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, Vec<u8>>) {
            let metadata = std::fs::symlink_metadata(path).unwrap();
            let bytes = if metadata.is_dir() {
                Vec::new()
            } else {
                std::fs::read(path).unwrap()
            };
            entries.insert(path.strip_prefix(root).unwrap().into(), bytes);
            if metadata.is_dir() {
                for entry in std::fs::read_dir(path).unwrap() {
                    visit(root, &entry.unwrap().path(), entries);
                }
            }
        }
        let mut entries = BTreeMap::new();
        visit(root, root, &mut entries);
        entries
    }

    fn plans(plans: &[SourcePlan]) -> BTreeMap<String, SourcePlan> {
        plans
            .iter()
            .enumerate()
            .map(|(index, plan)| (format!("source:{index}"), *plan))
            .collect()
    }

    #[test]
    fn guest_keeps_materialized_pins_when_only_remote_check_is_due() {
        let plans = plans(&[SourcePlan::Keep, SourcePlan::RemoteCheck]);

        assert_eq!(
            preparation_action(Mode::Refresh, &plans, true),
            PreparationAction::Keep
        );
    }

    #[test]
    fn guest_missing_pin_requires_host_sync() {
        let plans = plans(&[SourcePlan::Acquire(RefreshReason::Missing)]);

        let PreparationAction::Refuse(message) = preparation_action(Mode::Refresh, &plans, true)
        else {
            panic!("a missing pin refuses inside a workspace container");
        };
        for expected in [
            "This project's bundle needs to be synced on the host",
            "It has not been synced yet.",
            "Next: Exit the container and run `effigy bundle sync` on the host.",
        ] {
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn guest_changed_declaration_requires_host_sync() {
        let plans = plans(&[SourcePlan::Acquire(RefreshReason::Declaration)]);

        let PreparationAction::Refuse(message) = preparation_action(Mode::Refresh, &plans, true)
        else {
            panic!("a changed declaration refuses inside a workspace container");
        };
        for expected in [
            "This project's bundle needs to be synced on the host",
            "Its declaration changed since it was last synced on the host.",
            "Next: Exit the container and run `effigy bundle sync` on the host.",
        ] {
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn guest_changed_path_content_requires_host_sync() {
        let plans = plans(&[SourcePlan::Acquire(RefreshReason::Content)]);

        let PreparationAction::Refuse(message) = preparation_action(Mode::Refresh, &plans, true)
        else {
            panic!("changed path content refuses inside a workspace container");
        };
        for expected in [
            "This project's bundle needs to be synced on the host",
            "The local path bundle changed since it was last snapshotted on the host.",
            "Next: Exit the container and run `effigy bundle sync` on the host.",
        ] {
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn guest_sync_requires_running_on_host() {
        let plans = plans(&[SourcePlan::Acquire(RefreshReason::Missing)]);

        let PreparationAction::Refuse(message) = preparation_action(Mode::Sync, &plans, true)
        else {
            panic!("an explicit sync refuses inside a workspace container");
        };
        for expected in [
            "Bundles cannot be synced from inside a workspace container",
            "Next: Exit the container and run `effigy bundle sync` on the host.",
        ] {
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn acquisition_reports_what_it_fetched_and_that_it_finished() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, mut prep) = fixture(tmp.path());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&seen);
        prep.progress = SourceProgress::to(move |event| {
            sink.lock().unwrap().push(event);
        });

        prepare_bundle_sources(&manifest, &prep).unwrap();

        let events = seen.lock().unwrap().clone();
        assert_eq!(
            events,
            [
                SourceProgressEvent::Started {
                    source: "bundle".to_owned(),
                    subject: "bundle".to_owned(),
                    revision: "path".to_owned(),
                    local: true,
                },
                SourceProgressEvent::Finished {
                    source: "bundle".to_owned(),
                    subject: "bundle".to_owned(),
                    kept: None,
                    failure: None,
                },
                SourceProgressEvent::Started {
                    source: "catalog:extra".to_owned(),
                    subject: "extra".to_owned(),
                    revision: "path".to_owned(),
                    local: true,
                },
                SourceProgressEvent::Finished {
                    source: "catalog:extra".to_owned(),
                    subject: "extra".to_owned(),
                    kept: None,
                    failure: None,
                },
            ]
        );
    }

    #[test]
    fn a_failed_acquisition_ends_its_step_once_with_the_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, mut prep) = fixture(tmp.path());
        // The local bundle is gone, so its snapshot cannot be taken.
        std::fs::remove_dir_all(tmp.path().join("bundle")).unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&seen);
        prep.progress = SourceProgress::to(move |event| {
            sink.lock().unwrap().push(event);
        });

        assert!(prepare_bundle_sources(&manifest, &prep).is_err());

        let events = seen.lock().unwrap().clone();
        let [SourceProgressEvent::Started { subject, .. }, SourceProgressEvent::Finished {
            subject: finished,
            kept,
            failure,
            ..
        }] = events.as_slice()
        else {
            panic!(
                "expected one Started and one Finished, got {} event(s)",
                events.len()
            );
        };
        assert_eq!(subject, "bundle");
        assert_eq!(finished, "bundle");
        assert_eq!(*kept, None);
        assert!(failure.is_some());
    }

    #[test]
    fn a_step_dropped_before_it_finishes_reports_one_failure() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&seen);
        let progress = SourceProgress::to(move |event| {
            sink.lock().unwrap().push(event);
        });

        // An early return between Started and the outcome must not leave the
        // step open for the operator.
        drop(AcquisitionStep::start(
            &progress,
            "catalog:extra",
            "extra".to_owned(),
            "path".to_owned(),
            true,
        ));

        let events = seen.lock().unwrap().clone();
        assert_eq!(
            events,
            [
                SourceProgressEvent::Started {
                    source: "catalog:extra".to_owned(),
                    subject: "extra".to_owned(),
                    revision: "path".to_owned(),
                    local: true,
                },
                SourceProgressEvent::Finished {
                    source: "catalog:extra".to_owned(),
                    subject: "extra".to_owned(),
                    kept: None,
                    failure: Some("acquisition stopped before it finished".to_owned()),
                },
            ]
        );
    }

    #[test]
    fn nothing_to_fetch_reports_no_steps() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, mut prep) = fixture(tmp.path());
        prepare_bundle_sources(&manifest, &prep).unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&seen);
        prep.progress = SourceProgress::to(move |event| {
            sink.lock().unwrap().push(event);
        });

        // Every selected source is current: the cheap path draws no step.
        assert!(prepare_bundle_sources(&manifest, &prep).unwrap().is_none());
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn missing_only_prepare_fills_a_gap_and_never_moves_a_pin() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        // A fresh checkout has no pin: a task run fills the gap.
        assert!(prepare_missing_bundle_sources(&manifest, &prep)
            .unwrap()
            .is_some());
        let pinned = lock_bytes(tmp.path());
        assert!(selected(&manifest).contains("echo old"));
        std::fs::write(
            tmp.path().join("bundle/effigy.toml"),
            "[tasks.hello]\nrun = \"echo new\"\n",
        )
        .unwrap();
        // An existing pin never moves on a task run, even when its content did.
        assert!(prepare_missing_bundle_sources(&manifest, &prep)
            .unwrap()
            .is_none());
        assert_eq!(lock_bytes(tmp.path()), pinned);
        assert!(selected(&manifest).contains("echo old"));
    }

    #[test]
    fn a_changed_path_bundle_is_resnapshotted_on_prepare() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        let first = prepare_bundle_sources(&manifest, &prep).unwrap().unwrap();
        assert!(!first.changed);
        assert!(first.notes.is_empty(), "{:?}", first.notes);
        assert!(selected(&manifest).contains("echo old"));
        std::fs::write(
            tmp.path().join("bundle/effigy.toml"),
            "[tasks.hello]\nrun = \"echo new\"\n",
        )
        .unwrap();
        let report = prepare_bundle_sources(&manifest, &prep).unwrap().unwrap();
        assert!(report.changed);
        assert!(selected(&manifest).contains("echo new"));
        assert_eq!(
            report.notes.len(),
            2,
            "bundle and catalog:extra: {:?}",
            report.notes
        );
        for note in &report.notes {
            assert!(note.ends_with("(content changed)"), "{note}");
            assert!(note.contains(" updated ") && note.contains(" → "), "{note}");
        }
        assert!(report
            .notes
            .iter()
            .any(|note| note.starts_with("bundle updated ")));
        assert!(report
            .notes
            .iter()
            .any(|note| note.starts_with("catalog:extra updated ")));
    }

    #[test]
    fn a_changed_declaration_reacquires_on_prepare() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        prepare_bundle_sources(&manifest, &prep).unwrap().unwrap();
        let second = tmp.path().join("second");
        std::fs::create_dir(&second).unwrap();
        std::fs::write(second.join("bundle.toml"), "[bundle]\nname = \"fixture\"\n").unwrap();
        std::fs::write(
            second.join("effigy.toml"),
            "[tasks.hello]\nrun = \"echo second\"\n",
        )
        .unwrap();
        std::fs::write(
            &manifest,
            "[bundle]\nbase = { type = \"path\", dir = \"second\" }\n[catalog.sources.extra]\ntype = \"path\"\ndir = \"bundle\"\n",
        )
        .unwrap();
        let report = prepare_bundle_sources(&manifest, &prep).unwrap().unwrap();
        assert!(report.changed);
        assert!(selected(&manifest).contains("echo second"));
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.starts_with("bundle updated ")
                    && note.ends_with("(declaration changed)")),
            "{:?}",
            report.notes
        );
    }

    #[test]
    fn a_changed_declaration_with_no_usable_pin_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        prepare_bundle_sources(&manifest, &prep).unwrap().unwrap();
        std::fs::write(
            &manifest,
            "[bundle]\nbase = { type = \"path\", dir = \"absent\" }\n[catalog.sources.extra]\ntype = \"path\"\ndir = \"bundle\"\n",
        )
        .unwrap();
        assert!(prepare_bundle_sources(&manifest, &prep).is_err());
    }

    #[test]
    fn manual_update_keeps_pins_until_sync() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        std::fs::write(
            &manifest,
            "[bundle]\nupdate = \"manual\"\nbase = { type = \"path\", dir = \"bundle\" }\n[catalog.sources.extra]\ntype = \"path\"\ndir = \"bundle\"\nupdate = \"manual\"\n",
        )
        .unwrap();
        prepare_bundle_sources(&manifest, &prep).unwrap().unwrap();
        let lock = lock_bytes(tmp.path());
        std::fs::write(
            tmp.path().join("bundle/effigy.toml"),
            "[tasks.hello]\nrun = \"echo manual\"\n",
        )
        .unwrap();
        assert!(prepare_bundle_sources(&manifest, &prep).unwrap().is_none());
        assert_eq!(lock_bytes(tmp.path()), lock);
        assert!(selected(&manifest).contains("echo old"));
        let report = sync_bundle_source(&manifest, &prep).unwrap().unwrap();
        assert!(report.changed);
        assert!(selected(&manifest).contains("echo manual"));
    }

    #[test]
    fn manual_applies_to_one_catalog_source_only() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        std::fs::write(
            &manifest,
            "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n[catalog.sources.extra]\ntype = \"path\"\ndir = \"bundle\"\nupdate = \"manual\"\n",
        )
        .unwrap();
        prepare_bundle_sources(&manifest, &prep).unwrap().unwrap();
        std::fs::write(
            tmp.path().join("bundle/effigy.toml"),
            "[tasks.hello]\nrun = \"echo mixed\"\n",
        )
        .unwrap();
        let report = prepare_bundle_sources(&manifest, &prep).unwrap().unwrap();
        assert!(report.changed);
        assert_eq!(
            report
                .notes
                .iter()
                .map(|note| note.split(' ').next().unwrap())
                .collect::<Vec<_>>(),
            vec!["bundle"],
            "the manual catalog source keeps its pin: {:?}",
            report.notes
        );
    }

    #[test]
    fn read_paths_never_acquire_or_write() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        prepare_bundle_sources(&manifest, &prep).unwrap().unwrap();
        let lock = lock_bytes(tmp.path());
        std::fs::write(
            tmp.path().join("bundle/effigy.toml"),
            "[tasks.hello]\nrun = \"echo behind\"\n",
        )
        .unwrap();
        let before = tree(&tmp.path().join(".effigy"));
        // Reads stay local: the source is behind, and only a Prepare moves it.
        let inspected = crate::inspect_bundle_source(&manifest).unwrap().unwrap();
        assert!(!inspected.stale);
        crate::catalog_source_roots(&manifest).unwrap();
        let loaded = crate::load_task_manifest_with_inspection(&manifest).unwrap();
        assert!(loaded.effective_manifest.contains("echo old"));
        assert_eq!(tree(&tmp.path().join(".effigy")), before);
        assert_eq!(lock_bytes(tmp.path()), lock);
        assert!(!tmp.path().join(".effigy").join(UPDATE_STATE_FILE).exists());
    }

    #[test]
    fn unsafe_cache_symlink_cannot_write_foreign_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        let foreign = tmp.path().join("foreign");
        std::fs::create_dir(&foreign).unwrap();
        std::fs::write(foreign.join("sentinel"), "owned by someone else").unwrap();
        std::fs::create_dir(tmp.path().join(".effigy")).unwrap();
        std::os::unix::fs::symlink(&foreign, tmp.path().join(".effigy/cache")).unwrap();
        assert!(sync_bundle_source(&manifest, &prep).is_err());
        assert_eq!(
            std::fs::read_to_string(foreign.join("sentinel")).unwrap(),
            "owned by someone else"
        );
        assert_eq!(std::fs::read_dir(&foreign).unwrap().count(), 1);
    }

    #[test]
    fn a_failed_sync_keeps_the_prior_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        prepare_bundle_sources(&manifest, &prep).unwrap().unwrap();
        let previous = lock_bytes(tmp.path());
        std::fs::write(
            tmp.path().join("bundle/effigy.toml"),
            "[tasks.hello]\nrun = 4\n",
        )
        .unwrap();
        assert!(sync_bundle_source(&manifest, &prep).is_err());
        assert_eq!(lock_bytes(tmp.path()), previous);
        assert!(selected(&manifest).contains("echo old"));
    }

    #[test]
    fn ls_remote_reads_the_head_whether_or_not_the_tab_survives_capture() {
        const HEAD: &str = "c331abd41b70aa6cc467cbc2d57e62e671a175a0";
        const TAG: &str = "4be12e4088b8712d9e279e0a7b81b6c70ec882b0";
        // The terminal sanitizer drops TAB, so captured output is `<id>refs/...`.
        let captured = format!("{HEAD}refs/heads/main\n");
        let tabbed = format!("{HEAD}\trefs/heads/main\n");
        assert_eq!(
            parse_ls_remote(captured.as_bytes(), "main").as_deref(),
            Some(HEAD)
        );
        assert_eq!(
            parse_ls_remote(tabbed.as_bytes(), "main").as_deref(),
            Some(HEAD)
        );
        // An annotated tag resolves to its peeled commit, not the tag object.
        let peeled = format!("{TAG}refs/tags/v1\n{HEAD}refs/tags/v1^{{}}\n");
        assert_eq!(
            parse_ls_remote(peeled.as_bytes(), "v1").as_deref(),
            Some(HEAD)
        );
        assert_eq!(parse_ls_remote(b"", "main"), None);
        assert_eq!(parse_ls_remote(b"not a ref line\n", "main"), None);
    }

    fn git_cmd(cwd: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "Effigy Test")
            .env("GIT_AUTHOR_EMAIL", "test@effigy.local")
            .env("GIT_COMMITTER_NAME", "Effigy Test")
            .env("GIT_COMMITTER_EMAIL", "test@effigy.local")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    struct GitBundleRepo {
        bare: PathBuf,
        work: PathBuf,
        branch: String,
    }

    impl GitBundleRepo {
        fn new(root: &Path) -> Self {
            Self::with_branch(root, "main")
        }

        fn with_branch(root: &Path, branch: &str) -> Self {
            let bare = root.join("remote.git");
            let work = root.join("work");
            std::fs::create_dir_all(&bare).unwrap();
            std::fs::create_dir_all(&work).unwrap();
            let initial_branch = format!("--initial-branch={branch}");
            git_cmd(&bare, &["init", "--bare", &initial_branch]);
            git_cmd(&work, &["init", &initial_branch]);
            git_cmd(&work, &["remote", "add", "origin", bare.to_str().unwrap()]);
            Self {
                bare,
                work,
                branch: branch.to_owned(),
            }
        }

        fn commit_bundle(
            &self,
            bundle_toml: &str,
            export_toml: &str,
            message: &str,
            tag: Option<&str>,
        ) -> String {
            std::fs::write(self.work.join("bundle.toml"), bundle_toml).unwrap();
            std::fs::write(self.work.join("effigy.toml"), export_toml).unwrap();
            git_cmd(&self.work, &["add", "bundle.toml", "effigy.toml"]);
            git_cmd(&self.work, &["commit", "--allow-empty", "-m", message]);
            let refspec = format!("HEAD:refs/heads/{}", self.branch);
            git_cmd(&self.work, &["push", "origin", &refspec]);
            let sha = git_cmd(&self.work, &["rev-parse", "HEAD"]);
            if let Some(tag) = tag {
                git_cmd(&self.work, &["tag", "-a", tag, "-m", tag]);
                git_cmd(&self.work, &["push", "origin", &format!("refs/tags/{tag}")]);
            }
            sha
        }

        /// Publish a tag on a detached commit without moving the branch tip.
        fn publish_side_tag(
            &self,
            base_sha: &str,
            bundle_toml: &str,
            export_toml: &str,
            tag: &str,
        ) -> String {
            let branch_head = git_cmd(&self.work, &["rev-parse", "HEAD"]);
            git_cmd(&self.work, &["checkout", "--detach", base_sha]);
            std::fs::write(self.work.join("bundle.toml"), bundle_toml).unwrap();
            std::fs::write(self.work.join("effigy.toml"), export_toml).unwrap();
            git_cmd(&self.work, &["add", "bundle.toml", "effigy.toml"]);
            git_cmd(&self.work, &["commit", "--allow-empty", "-m", tag]);
            let sha = git_cmd(&self.work, &["rev-parse", "HEAD"]);
            git_cmd(&self.work, &["tag", "-a", tag, "-m", tag]);
            git_cmd(&self.work, &["push", "origin", &format!("refs/tags/{tag}")]);
            git_cmd(&self.work, &["checkout", &self.branch]);
            assert_eq!(git_cmd(&self.work, &["rev-parse", "HEAD"]), branch_head);
            sha
        }
    }

    fn sentinel_executable() -> PathBuf {
        if let Some(path) = std::env::var_os("EFFIGY_EXECUTABLE")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
        {
            return path;
        }
        let workspace_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/effigy");
        if workspace_bin.is_file() {
            return std::fs::canonicalize(workspace_bin).unwrap();
        }
        std::env::current_exe().unwrap()
    }

    fn git_consumer_fixture_with_spec(
        root: &Path,
        base_spec: &str,
    ) -> (PathBuf, SourcePreparation) {
        let project = root.join("consumer");
        std::fs::create_dir_all(&project).unwrap();
        let manifest = project.join("effigy.toml");
        std::fs::write(&manifest, format!("[bundle]\nbase = {base_spec}\n")).unwrap();
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        (
            manifest,
            SourcePreparation {
                home,
                executable: sentinel_executable(),
                cancel: CancelToken::default(),
                budget: Duration::from_secs(30),
                next: Invocation::validated(vec!["bundle".into(), "sync".into()], |args| {
                    args == ["bundle", "sync"]
                })
                .unwrap(),
                progress: SourceProgress::silent(),
            },
        )
    }

    fn git_consumer_fixture(
        root: &Path,
        remote: &Path,
        reference: Option<&str>,
    ) -> (PathBuf, SourcePreparation) {
        let ref_part = reference
            .map(|value| format!(", ref = \"{value}\""))
            .unwrap_or_default();
        git_consumer_fixture_with_spec(
            root,
            &format!(
                "{{ type = \"git\", url = \"{}\"{ref_part} }}",
                remote.display()
            ),
        )
    }

    #[test]
    fn parse_ls_remote_tags_orders_semver_newest_first_and_peels_annotated_tags() {
        const A: &str = "1111111111111111111111111111111111111111";
        const B: &str = "2222222222222222222222222222222222222222";
        const C: &str = "3333333333333333333333333333333333333333";
        const TAG_OBJ: &str = "4444444444444444444444444444444444444444";
        let raw = format!(
            "{A}\trefs/tags/v1.2.0\n\
             {TAG_OBJ}\trefs/tags/v1.10.0\n\
             {B}\trefs/tags/v1.10.0^{{}}\n\
             {C}\trefs/tags/legacy-release\n"
        );
        let parsed = parse_ls_remote_tags(raw.as_bytes());
        assert_eq!(
            parsed,
            vec![
                ("v1.10.0".to_owned(), B.to_owned()),
                ("v1.2.0".to_owned(), A.to_owned()),
                ("legacy-release".to_owned(), C.to_owned()),
            ]
        );
    }

    #[test]
    fn floating_branch_falls_back_to_newest_compatible_tag_and_emits_notice() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = GitBundleRepo::new(tmp.path());
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.14.0\"\n",
            "[tasks.hello]\nrun = \"echo from-v1.0.0\"\n",
            "v1.0.0",
            Some("v1.0.0"),
        );
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.15.0\"\n",
            "[tasks.hello]\nrun = \"echo from-v1.2.0\"\n",
            "v1.2.0",
            Some("v1.2.0"),
        );
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.16.0\"\nfuture_flag = true\n",
            "[future_v016]\nenabled = true\n",
            "v2.0.0",
            Some("v2.0.0"),
        );
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.16.0\"\n",
            "[tasks.hello]\nrun = \"echo from-main\"\n[future_v016]\nenabled = true\n",
            "main tip",
            None,
        );

        let (manifest, prep) = git_consumer_fixture(tmp.path(), &repo.bare, Some("main"));
        let report = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0+local.abc1234")
            .unwrap()
            .unwrap();

        assert!(selected(&manifest).contains("echo from-v1.2.0"));
        assert!(
            report.notes.iter().any(|note| {
                note.contains("v1.2.0")
                    && note.contains("main")
                    && note.contains(">= 0.16.0")
                    && note.contains("0.15.0+local.abc1234")
                    && note.contains("upgrade Effigy")
            }),
            "expected clear fallback notice in notes: {:?}",
            report.notes
        );

        let inspected = crate::inspect_bundle_source(&manifest).unwrap().unwrap();
        let fallback = inspected.fallback.expect("fallback metadata recorded");
        assert_eq!(fallback.selected_tag, "v1.2.0");
        assert_eq!(fallback.skipped_ref, "main");
        assert_eq!(fallback.required_effigy, ">= 0.16.0");
    }

    #[test]
    fn upgrading_simulated_effigy_version_selects_main_again() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = GitBundleRepo::new(tmp.path());
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.14.0\"\n",
            "[tasks.hello]\nrun = \"echo from-v1.2.0\"\n",
            "v1.2.0",
            Some("v1.2.0"),
        );
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.15.0\"\n",
            "[tasks.hello]\nrun = \"echo from-main\"\n",
            "main tip",
            None,
        );

        let (manifest, prep) = git_consumer_fixture(tmp.path(), &repo.bare, Some("main"));
        prepare_with_version(&manifest, &prep, Mode::Refresh, "0.14.1")
            .unwrap()
            .unwrap();
        assert!(selected(&manifest).contains("echo from-v1.2.0"));
        assert!(crate::inspect_bundle_source(&manifest)
            .unwrap()
            .unwrap()
            .fallback
            .is_some());

        // Upgrading Effigy immediately re-evaluates `main` without waiting for TTL.
        let upgraded = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap()
            .unwrap();
        assert!(upgraded.changed);
        assert!(selected(&manifest).contains("echo from-main"));
        assert!(crate::inspect_bundle_source(&manifest)
            .unwrap()
            .unwrap()
            .fallback
            .is_none());
    }

    #[test]
    fn explicit_rev_or_tag_pin_refuses_with_version_mismatch_instead_of_falling_back() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = GitBundleRepo::new(tmp.path());
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.15.0\"\n",
            "[tasks.hello]\nrun = \"echo from-v1.0.0\"\n",
            "v1.0.0",
            Some("v1.0.0"),
        );
        let incompatible_sha = repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.16.0\"\n",
            "[future_v016]\nenabled = true\n",
            "v2.0.0",
            Some("v2.0.0"),
        );

        // Explicit incompatible tag pin refuses cleanly without falling back.
        let tag_root = tmp.path().join("tag-pin");
        let (manifest, prep) = git_consumer_fixture(&tag_root, &repo.bare, Some("v2.0.0"));
        let err = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("requires Effigy >= 0.16.0") && err.contains("0.15.0"),
            "{err}"
        );
        assert!(!err.contains("future_v016"), "{err}");

        // Explicit commit rev pin via `rev = "<sha>"` refuses cleanly without falling back.
        let rev_root = tmp.path().join("rev-pin");
        let (manifest, prep) = git_consumer_fixture_with_spec(
            &rev_root,
            &format!(
                "{{ type = \"git\", url = \"{}\", rev = \"{incompatible_sha}\" }}",
                repo.bare.display()
            ),
        );
        let err = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("bundle rev")
                && err.contains("requires Effigy >= 0.16.0")
                && err.contains("0.15.0"),
            "{err}"
        );
        assert!(!err.contains("future_v016"), "{err}");

        // Declaring both `ref` and `rev` is rejected cleanly.
        let both_root = tmp.path().join("both-pin");
        let (manifest, prep) = git_consumer_fixture_with_spec(
            &both_root,
            &format!(
                "{{ type = \"git\", url = \"{}\", ref = \"main\", rev = \"{incompatible_sha}\" }}",
                repo.bare.display()
            ),
        );
        let err = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap_err()
            .to_string();
        assert!(err.contains("cannot declare both `ref` and `rev`"), "{err}");
    }

    #[test]
    fn floating_branch_with_no_compatible_tag_refuses_with_version_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = GitBundleRepo::new(tmp.path());
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.16.0\"\n",
            "[future_v016]\nenabled = true\n",
            "v2.0.0",
            Some("v2.0.0"),
        );
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.17.0\"\n",
            "[future_v017]\nenabled = true\n",
            "main tip",
            None,
        );

        let (manifest, prep) = git_consumer_fixture(tmp.path(), &repo.bare, Some("main"));
        let err = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("requires Effigy >= 0.17.0")
                && err.contains("0.15.0")
                && err.contains("no compatible tag"),
            "{err}"
        );
        assert!(!err.contains("future_v017"), "{err}");
    }

    #[test]
    fn bounded_candidate_probing_respects_limit_and_caches_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = GitBundleRepo::new(tmp.path());
        // Place one compatible tag older than MAX_COMPATIBILITY_TAG_CANDIDATES
        // newer incompatible tags to prove probing is strictly bounded.
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.15.0\"\n",
            "[tasks.hello]\nrun = \"echo from-v1.0.0\"\n",
            "v1.0.0",
            Some("v1.0.0"),
        );
        for index in 0..(MAX_COMPATIBILITY_TAG_CANDIDATES + 2) {
            let tag = format!("v2.{index}.0");
            repo.commit_bundle(
                "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.16.0\"\n",
                "[tasks.hello]\nrun = \"echo incompatible\"\n",
                &tag,
                Some(&tag),
            );
        }
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.16.0\"\n",
            "[tasks.hello]\nrun = \"echo main\"\n",
            "main tip",
            None,
        );

        let bounded_root = tmp.path().join("bounded");
        let (manifest, prep) = git_consumer_fixture(&bounded_root, &repo.bare, Some("main"));
        let err = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains(&format!(
                "checked {MAX_COMPATIBILITY_TAG_CANDIDATES} candidate tags"
            )),
            "{err}"
        );

        // Now add a compatible tag within the bounded window and verify caching:
        // the first prepare selects it, and a second prepare within REMOTE_CHECK_TTL
        // keeps the pin without re-probing.
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.15.0\"\n",
            "[tasks.hello]\nrun = \"echo from-v3.0.0\"\n",
            "v3.0.0",
            Some("v3.0.0"),
        );
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.16.0\"\n",
            "[tasks.hello]\nrun = \"echo main-latest\"\n",
            "main tip 2",
            None,
        );

        let cached_root = tmp.path().join("cached");
        let (manifest, prep) = git_consumer_fixture(&cached_root, &repo.bare, Some("main"));
        let first = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap()
            .unwrap();
        assert_eq!(
            first
                .sources
                .get("bundle")
                .and_then(|s| s.fallback.as_ref())
                .map(|fb| fb.selected_tag.as_str()),
            Some("v3.0.0")
        );
        assert!(cached_root
            .join("consumer/.effigy")
            .join(UPDATE_STATE_FILE)
            .is_file());

        // Second prepare within REMOTE_CHECK_TTL returns None (kept from cache).
        assert!(
            prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn fallback_refresh_reprobes_tags_after_ttl_even_when_branch_tip_is_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = GitBundleRepo::new(tmp.path());
        let v1_sha = repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.14.0\"\n",
            "[tasks.hello]\nrun = \"echo from-v1.0.0\"\n",
            "v1.0.0",
            Some("v1.0.0"),
        );
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.16.0\"\n",
            "[tasks.hello]\nrun = \"echo from-main\"\n",
            "main tip",
            None,
        );

        let (manifest, prep) = git_consumer_fixture(tmp.path(), &repo.bare, Some("main"));
        prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap()
            .unwrap();
        assert!(selected(&manifest).contains("echo from-v1.0.0"));

        // Publish a newer compatible tag v1.1.0 without moving `main`.
        repo.publish_side_tag(
            &v1_sha,
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.15.0\"\n",
            "[tasks.hello]\nrun = \"echo from-v1.1.0\"\n",
            "v1.1.0",
        );

        // Expire REMOTE_CHECK_TTL in sources.update.json.
        let update_state_path = tmp.path().join("consumer/.effigy").join(UPDATE_STATE_FILE);
        std::fs::write(
            &update_state_path,
            serde_json::to_vec(&SourceUpdateState {
                version: 1,
                checks: BTreeMap::from([("bundle".to_owned(), 1)]),
            })
            .unwrap(),
        )
        .unwrap();

        let refreshed = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap()
            .unwrap();
        assert!(refreshed.changed);
        assert!(selected(&manifest).contains("echo from-v1.1.0"));
        assert_eq!(
            crate::inspect_bundle_source(&manifest)
                .unwrap()
                .unwrap()
                .fallback
                .as_ref()
                .map(|fb| fb.selected_tag.as_str()),
            Some("v1.1.0")
        );
    }

    #[test]
    fn omitted_ref_and_explicit_head_resolve_and_monitor_non_main_default_branch() {
        const HEAD_SHA: &str = "c331abd41b70aa6cc467cbc2d57e62e671a175a0";
        assert_eq!(
            parse_ls_remote(format!("{HEAD_SHA}\tHEAD\n").as_bytes(), "HEAD").as_deref(),
            Some(HEAD_SHA)
        );
        assert_eq!(
            parse_ls_remote(format!("{HEAD_SHA}HEAD\n").as_bytes(), "HEAD").as_deref(),
            Some(HEAD_SHA)
        );
        assert_eq!(
            parse_symref_head_branch(
                format!("ref: refs/heads/trunk\tHEAD\n{HEAD_SHA}\tHEAD\n").as_bytes()
            )
            .as_deref(),
            Some("trunk")
        );

        let tmp = tempfile::tempdir().unwrap();
        let repo = GitBundleRepo::with_branch(tmp.path(), "trunk");
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.14.0\"\n",
            "[tasks.hello]\nrun = \"echo from-v1.0.0\"\n",
            "v1.0.0",
            Some("v1.0.0"),
        );
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.16.0\"\n",
            "[tasks.hello]\nrun = \"echo from-trunk-v1\"\n",
            "trunk incompatible",
            None,
        );

        // 1. Omitted `ref` resolves default branch `trunk` and falls back to `v1.0.0`.
        let omitted_root = tmp.path().join("omitted");
        let (manifest, prep) = git_consumer_fixture(&omitted_root, &repo.bare, None);
        let report = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap()
            .unwrap();
        assert!(selected(&manifest).contains("echo from-v1.0.0"));
        let fb = report
            .sources
            .get("bundle")
            .and_then(|s| s.fallback.as_ref())
            .expect("fallback recorded");
        assert_eq!(fb.selected_tag, "v1.0.0");
        assert_eq!(fb.skipped_ref, "trunk");

        // 2. Explicit `ref = "HEAD"` resolves `trunk` and detects subsequent HEAD movement.
        let head_root = tmp.path().join("explicit-head");
        let (head_manifest, head_prep) = git_consumer_fixture(&head_root, &repo.bare, Some("HEAD"));
        prepare_with_version(&head_manifest, &head_prep, Mode::Refresh, "0.15.0")
            .unwrap()
            .unwrap();
        assert!(selected(&head_manifest).contains("echo from-v1.0.0"));

        // Move `trunk` to a compatible commit and expire TTL to prove HEAD monitoring detects it.
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.15.0\"\n",
            "[tasks.hello]\nrun = \"echo from-trunk-v2\"\n",
            "trunk compatible",
            None,
        );
        std::fs::write(
            head_root.join("consumer/.effigy").join(UPDATE_STATE_FILE),
            serde_json::to_vec(&SourceUpdateState {
                version: 1,
                checks: BTreeMap::from([("bundle".to_owned(), 1)]),
            })
            .unwrap(),
        )
        .unwrap();
        let moved = prepare_with_version(&head_manifest, &head_prep, Mode::Refresh, "0.15.0")
            .unwrap()
            .unwrap();
        assert!(moved.changed);
        assert!(selected(&head_manifest).contains("echo from-trunk-v2"));
        assert!(crate::inspect_bundle_source(&head_manifest)
            .unwrap()
            .unwrap()
            .fallback
            .is_none());
    }

    #[test]
    fn consumer_minimum_effigy_version_refuses_local_build_before_acquisition() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("consumer");
        std::fs::create_dir_all(&project).unwrap();
        let manifest = project.join("effigy.toml");
        std::fs::write(
            &manifest,
            r#"[manifest]
minimum_effigy_version = "999.0.0"

[bundle]
base = { type = "git", url = "https://invalid.invalid/should-not-fetch", ref = "main" }
"#,
        )
        .unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let prep = SourcePreparation {
            home,
            executable: sentinel_executable(),
            cancel: CancelToken::default(),
            budget: Duration::from_secs(30),
            next: Invocation::validated(vec!["bundle".into(), "sync".into()], |args| {
                args == ["bundle", "sync"]
            })
            .unwrap(),
            progress: SourceProgress::silent(),
        };

        let err = prepare_with_version(&manifest, &prep, Mode::Sync, "v0.15.0+local.abc1234")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("requires Effigy >= 999.0.0") && err.contains("v0.15.0+local.abc1234"),
            "{err}"
        );
    }

    #[test]
    fn above_maximum_fallback_notice_and_no_tag_error_do_not_recommend_upgrading() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = GitBundleRepo::new(tmp.path());
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nminimum_effigy_version = \"0.14.0\"\n",
            "[tasks.hello]\nrun = \"echo from-v1.0.0\"\n",
            "v1.0.0",
            Some("v1.0.0"),
        );
        repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nmaximum_effigy_version = \"0.14.9\"\n",
            "[tasks.hello]\nrun = \"echo from-main\"\n",
            "main capped",
            None,
        );

        let fallback_root = tmp.path().join("above-max-fallback");
        let (manifest, prep) = git_consumer_fixture(&fallback_root, &repo.bare, Some("main"));
        let report = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap()
            .unwrap();
        assert!(selected(&manifest).contains("echo from-v1.0.0"));
        assert!(
            report.notes.iter().any(|note| {
                note.contains("v1.0.0")
                    && note.contains("only supports Effigy <= 0.14.9")
                    && !note.contains("upgrade Effigy")
            }),
            "expected above-maximum notice without upgrade advice: {:?}",
            report.notes
        );

        // When no compatible tag exists and running version exceeds maximum_effigy_version:
        let no_tag_repo = GitBundleRepo::new(&tmp.path().join("no-tag-repo"));
        no_tag_repo.commit_bundle(
            "[bundle]\nname = \"acme\"\nmaximum_effigy_version = \"0.14.9\"\n",
            "[tasks.hello]\nrun = \"echo capped\"\n",
            "main capped only",
            None,
        );
        let no_tag_root = tmp.path().join("above-max-no-tag");
        let (manifest, prep) = git_consumer_fixture(&no_tag_root, &no_tag_repo.bare, Some("main"));
        let err = prepare_with_version(&manifest, &prep, Mode::Refresh, "0.15.0")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("<= 0.14.9") && err.contains("0.15.0") && !err.contains("upgrade Effigy"),
            "{err}"
        );
    }
}
