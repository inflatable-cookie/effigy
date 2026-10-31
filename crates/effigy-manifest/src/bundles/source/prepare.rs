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
/// Prepare paths acquire a missing source and refresh a pin that is behind, so
/// `dev`, `bootstrap` and `container up` never need a manual `bundle sync`.
pub fn prepare_bundle_sources(
    manifest: &Path,
    preparation: &SourcePreparation,
) -> Result<Option<BundleSyncReport>, ManifestError> {
    prepare(manifest, preparation, Mode::Refresh)
}

/// A plain task run fills a gap: it acquires a declared source that has no pin
/// at all and never moves an existing one, so a fresh checkout runs a task
/// instead of refusing (contract 020 §Behaviour rule 1).
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
    /// `dev`, `bootstrap`, `container up`: acquire what is missing and refresh
    /// what is behind.
    Refresh,
    /// Any other task run: acquire only a source with no pin at all.
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
    Acquire(RefreshReason),
    /// A floating Git source whose remote check is due.
    RemoteCheck,
    Keep,
}

fn prepare(
    manifest: &Path,
    preparation: &SourcePreparation,
    mode: Mode,
) -> Result<Option<BundleSyncReport>, ManifestError> {
    static TTL_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if std::env::var_os("EFFIGY_GIT_BUNDLE_REMOTE_CHECK_TTL_SECS").is_some()
        && !TTL_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        warn(&preparation.progress, "EFFIGY_GIT_BUNDLE_REMOTE_CHECK_TTL_SECS is retired; a floating source is checked at most once per checkout every 10 minutes");
    }
    let manifest = std::fs::canonicalize(manifest).map_err(|err| io_error(manifest, err))?;
    let (bundle, catalogs) = crate::composition::load_manifest_source_declarations(&manifest)?;
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
                    &preparation.progress,
                )?,
            };
            Ok((name.clone(), plan))
        })
        .collect::<Result<BTreeMap<_, _>, ManifestError>>()?;
    // No gate, token or file creation when every selected source is current.
    if plans.values().all(|plan| *plan == SourcePlan::Keep) {
        return Ok(None);
    }
    if effigy_core::container_detection::process_is_inside_effigy_workspace_container() {
        return Err(error(&manifest, "source acquisition requires host authority; run `effigy bundle sync` on the host before entering the workspace"));
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
    let remote = selections
        .values()
        .any(|selection| !matches!(selection, BundleSelection::Path { .. }));
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
        return Err(error(&manifest, "source preparation cleanup is incomplete; retry `effigy bundle sync` to settle the prior run"));
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
    progress: &SourceProgress,
) -> Result<SourcePlan, ManifestError> {
    let Some(pin) = pin else {
        return Ok(SourcePlan::Acquire(RefreshReason::Missing));
    };
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
        BundleSelection::Git { reference, .. } => {
            if reference.as_deref().is_some_and(is_full_revision) {
                return Ok(SourcePlan::Keep);
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
        BundleSelection::Git { reference, .. } => {
            reference.clone().unwrap_or_else(|| "HEAD".to_owned())
        }
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
    cx: &EffectContext,
    run: &mut Run<'_>,
    home: &Path,
    progress: &SourceProgress,
) -> Result<BundleSyncReport, ManifestError> {
    let ticket = retained_intent(run, "sources:generation", parent(manifest), manifest)?;
    let project = DirFd::open(parent(manifest)).map_err(|err| error(manifest, err.to_string()))?;
    let directory = source_dir(&project, ".effigy", cx, manifest)?;
    let cache = source_dir(&directory, "cache", cx, manifest)?;
    let objects = source_dir(&cache, "sources", cx, manifest)?;
    let generation = source_dir(&objects, &run.id().to_string(), cx, manifest)?;
    run.applied(
        &ticket,
        serde_json::json!({"path":generation.path().to_string_lossy()}),
    )
    .map_err(|err| error(manifest, err.to_string()))?;
    let mut imported = old.sources.clone();
    let mut imported_now = Vec::new();
    // Preserve the exact pre-upgrade bytes even when the first explicit sync
    // subsequently fails. No legacy cache is changed or removed.
    for (name, selection) in selections {
        if imported.contains_key(name) {
            continue;
        }
        if let Some(legacy) = legacy_root(manifest, selection).filter(|root| root.is_dir()) {
            let object = source_dir(
                &generation,
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
            let BundleSelection::Git { url, reference } = selection else {
                return Err(error(manifest, "only a Git source has a remote check"));
            };
            checks.insert(name.clone(), now);
            checked_any = true;
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
        let Some(reason) = reason else {
            if let Some(pin) = &pinned {
                reports.insert(name.clone(), pin_report(manifest, selection, pin));
            }
            continue;
        };
        let object = source_dir(&generation, &sha256_hex(name.as_bytes()), cx, manifest)?;
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
            run,
            cx,
            home,
            &generation,
            &object,
        )
        .and_then(|(version, revision)| {
            if name == "bundle" {
                crate::composition::validate_source_snapshot(manifest, object.path())?;
            }
            Ok((version, revision))
        });
        let (version, revision) = match acquired {
            Ok(acquired) => acquired,
            Err(failure) => {
                // Contract 020 §Failure: a pin whose declaration is unchanged
                // survives a failed refresh. A changed declaration with no
                // usable pin is the acquisition error.
                let keepable = pinned
                    .as_ref()
                    .is_some_and(|pin| same_declaration(&pin.declaration, selection));
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
                    Mode::Missing => with_sync_hint(manifest, failure),
                    Mode::Sync | Mode::Refresh => failure,
                });
            }
        };
        step.finish(None, None);
        let selected = SourcePin {
            declaration: selection.spec(),
            object: relative_object(manifest, object.path())?,
            version,
            revision,
        };
        changed |= pinned.as_ref().is_none_or(|pin| {
            pin.version != selected.version || !same_declaration(&pin.declaration, selection)
        });
        if mode == Mode::Refresh {
            if let Some(previous) = &pinned {
                if previous.version != selected.version || previous.revision != selected.revision {
                    notes.push(update_note(name, previous, &selected, reason));
                }
            }
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
    // Other checkouts can report a known newer source without remote probes.
    // This bounded hint is never used to select or acquire source bytes.
    // This is a derived shared hint cache, never a runtime database or run
    // index. A short cache lock serializes publishers from different checkouts.
    // This handle supplies only a shared cache lock. It never opens a
    // machine database or makes source acquisition depend on its health.
    let machine =
        StateRoot::machine_effects(cx, home).map_err(|err| error(manifest, err.to_string()))?;
    let cache_gate = effigy_kernel::gate::Gate::acquire(
        cx,
        &machine,
        &[ResourceId::store(Scope::Machine, "source-hints")
            .map_err(|err| error(manifest, err.to_string()))?],
        Wait::Until(Instant::now() + Duration::from_secs(10)),
    )
    .map_err(|err| error(manifest, err.to_string()))?;
    let home_dir = DirFd::open(home).map_err(|err| error(manifest, err.to_string()))?;
    let effigy = home_dir
        .prepare_child(
            cx,
            &FileName::new(".effigy").map_err(|err| error(manifest, err.to_string()))?,
        )
        .map_err(|err| error(manifest, err.to_string()))?;
    let store_dir = effigy
        .prepare_child(
            cx,
            &FileName::new("cache").map_err(|err| error(manifest, err.to_string()))?,
        )
        .map_err(|err| error(manifest, err.to_string()))?;
    let known_name =
        FileName::new("sources-known.json").map_err(|err| error(manifest, err.to_string()))?;
    let observe = cx.operation().observe().map_err(|_| {
        error(
            manifest,
            "source preparation could not read its source cache: the observation window is not finite",
        )
    })?;
    let mut known =
        effigy_kernel::record::read_cache::<KnownVersions>(&observe, &store_dir, &known_name)
            .unwrap_or_default();
    for (name, selection) in selections {
        if let Some(pin) = pins.get(name) {
            known.0.insert(
                source_key(selection).map_err(|err| error(manifest, err.to_string()))?,
                pin.version.clone(),
            );
        }
    }
    while known.0.len() > 4096 {
        known.0.pop_first();
    }
    let known_bytes = serde_json::to_vec(&known).map_err(|err| error(manifest, err.to_string()))?;
    if known_bytes.len() as u64 > LOCK_MAX {
        return Err(error(manifest, "source hint inventory exceeds size limit"));
    }
    let known_ticket = retained_intent(
        run,
        "sources:known-versions",
        &store_dir.path().join("sources-known.json"),
        manifest,
    )?;
    effigy_kernel::record::atomic_replace(
        cx,
        &cache_gate,
        &store_dir,
        &known_name,
        &known_bytes,
        effigy_kernel::record::ReplaceOpts::default(),
    )
    .map_err(|err| error(manifest, err.to_string()))?;
    run.applied(&known_ticket, serde_json::Value::Null)
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
    prune_unpinned_generations(manifest, &objects, &pins, cx, run, progress)?;
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

/// Remove the source generations the published lock does not pin. Every
/// Prepare writes a new generation, so without this the predecessor of each
/// sync stays on disk for good. Runs after the lock is published, so the
/// generations it selects are never removed.
///
/// Cleanup is maintenance: an unlistable or undeletable generation warns and
/// the successful Prepare stands.
fn prune_unpinned_generations(
    manifest: &Path,
    objects: &DirFd,
    pins: &BTreeMap<String, SourcePin>,
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
    let mut pruned = 0usize;
    for entry in entries {
        let Some(name) = entry.to_str() else {
            continue;
        };
        let prefix = Path::new("cache/sources").join(name);
        if pins.values().any(|pin| pin.object.starts_with(&prefix)) {
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
                pruned += 1;
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
/// version and revision hint.
#[allow(clippy::too_many_arguments)]
fn acquire_source(
    manifest: &Path,
    name: &str,
    selection: &BundleSelection,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    generation: &DirFd,
    object: &DirFd,
) -> Result<(String, Option<String>), ManifestError> {
    match selection {
        BundleSelection::Path { path } => Ok((snapshot(path, object, cx, manifest)?, None)),
        BundleSelection::Git { url, reference } => {
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
            )?;
            command(
                manifest,
                run,
                cx,
                home,
                checkout.path(),
                vec![
                    "git".into(),
                    "fetch".into(),
                    "--depth".into(),
                    "1".into(),
                    "--".into(),
                    "origin".into(),
                    reference.as_deref().unwrap_or("main").into(),
                ],
                BUDGET,
            )?;
            command(
                manifest,
                run,
                cx,
                home,
                checkout.path(),
                vec![
                    "git".into(),
                    "-c".into(),
                    "core.hooksPath=/dev/null".into(),
                    "checkout".into(),
                    "--detach".into(),
                    "FETCH_HEAD".into(),
                ],
                BUDGET,
            )?;
            Ok((
                snapshot(checkout.path(), object, cx, manifest)?,
                legacy_version(checkout.path(), selection),
            ))
        }
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
            )?;
            Ok((
                snapshot(pull.path(), object, cx, manifest)?,
                Some(digest.into()),
            ))
        }
    }
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
    let reference = reference.unwrap_or("main");
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
    )?;
    Ok(parse_ls_remote(&raw, reference))
}

/// One `ls-remote` line: the object id, then the ref name. Captured output
/// passes the terminal sanitizer, which drops the TAB between them, so the id
/// is taken by its length and the name starts at `refs/`.
fn split_ls_remote_line(line: &str) -> Option<(&str, &str)> {
    [64, 40].into_iter().find_map(|len| {
        let sha = line.get(..len)?;
        let name = line.get(len..)?.trim();
        (is_full_revision(sha) && name.starts_with("refs/")).then_some((sha, name))
    })
}

fn parse_ls_remote(raw: &[u8], reference: &str) -> Option<String> {
    let output = String::from_utf8_lossy(raw);
    let branch = format!("refs/heads/{reference}");
    let tag = format!("refs/tags/{reference}");
    let peeled = format!("{tag}^{{}}");
    let (mut head, mut tag_object, mut commit, mut first) = (None, None, None, None);
    for line in output.lines() {
        let Some((sha, name)) = split_ls_remote_line(line) else {
            continue;
        };
        first.get_or_insert_with(|| sha.to_owned());
        if name == branch {
            head = Some(sha.to_owned());
        } else if name == peeled {
            commit = Some(sha.to_owned());
        } else if name == tag {
            tag_object = Some(sha.to_owned());
        }
    }
    head.or(commit).or(tag_object).or(first)
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
    }
}
fn command(
    manifest: &Path,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    cwd: &Path,
    mut argv: Vec<OsString>,
    limit: Duration,
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
    match child.wait(cx.operation()) {
        Observed::Verified(exit, _) => {
            let bytes = child.take_output();
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
                next: Invocation::validated(vec!["recover".into()], |args| args == ["recover"])
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
}
