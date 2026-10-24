//! An inventory write is one bounded checkout run. Legacy files are read-only
//! inputs for an explicitly named live checkout, never a machine-wide index.
use super::*;
use crate::environment::{new_run_id, operation};
use effigy_kernel::ctx::{CancelToken, EffectClass};
use effigy_kernel::fs::{DirFd, FileName};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{
    ExecutionTarget, InvocationSpec, ResourceId, Route, Scope, Surface, TaskId,
};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::{Invocation, WorkloadOutcome};
use effigy_kernel::record::{read_record, Record};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{
    BeginOpts, Effect, EffectSpec, Fence, Reconciled, Run, Settlement, SettlementAdapter,
};
use std::time::{Duration, Instant};

const FAMILY: &str = "runtime-scopes";
const LIMIT: usize = 1000;
fn error(value: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("checkout scope inventory: {value}"))
}
fn name(token: &str) -> io::Result<FileName> {
    if token.len() != 32
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error("invalid scope token"));
    }
    FileName::new(&format!("{token}.json")).map_err(error)
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
struct Entry(Option<ScopeRecord>);
impl Record for Entry {
    const MAX_BYTES: u64 = 64 << 10;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let value: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if let Some(record) = &value.0 {
            if record.schema != RECORD_SCHEMA
                || record.schema_version != 1
                || name(&record.token).is_err()
            {
                return Err("unsupported scope record".into());
            }
        }
        Ok(value)
    }
}
fn read(
    cx: &effigy_kernel::ctx::ObserveContext,
    dir: &DirFd,
    token: &str,
) -> io::Result<Option<Entry>> {
    match read_record(cx, dir, &name(token)?) {
        Observed::Verified(entry, _) => {
            let entry: Entry = entry;
            if entry.0.as_ref().is_some_and(|record| record.token != token) {
                return Err(error("record token disagrees with its filename"));
            }
            Ok(Some(entry))
        }
        Observed::Absent(_) => Ok(None),
        other => Err(error(format!(
            "the scope record for {token} is unreadable ({}); run `effigy doctor` to inspect the state root",
            other.unproven()
        ))),
    }
}
fn local(
    cx: &effigy_kernel::ctx::ObserveContext,
    checkout: &Path,
) -> io::Result<Option<StateRoot>> {
    let home = crate::policy::render::state_home().map_err(error)?;
    let location = crate::environment::location(checkout).map_err(error)?;
    match StateRoot::checkout_observe_in(cx, &home, &location) {
        Observed::Verified(root, _) => Ok(Some(root)),
        Observed::Absent(_) => Ok(None),
        other => Err(error(other.unproven())),
    }
}
fn legacy(
    cx: &effigy_kernel::ctx::ObserveContext,
    checkout: &Path,
    token: &str,
) -> io::Result<Option<ScopeRecord>> {
    let home = crate::effigy_home().ok_or_else(|| error("HOME is required"))?;
    let path = home.join(FAMILY);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
        Ok(_) => {}
    }
    let entry = read(cx, &DirFd::open(&path).map_err(error)?, token)?.and_then(|entry| entry.0);
    match entry {
        Some(record)
            if Path::new(&record.checkout) == checkout
                || Path::new(&record.checkout)
                    .canonicalize()
                    .ok()
                    .zip(checkout.canonicalize().ok())
                    .is_some_and(|(a, b)| a == b) =>
        {
            Ok(Some(record))
        }
        Some(_) => Err(error("legacy scope belongs to another checkout")),
        None => Ok(None),
    }
}
/// Read only this checkout. A local tombstone suppresses the legacy input.
pub fn load(checkout: &Path, token: &str) -> io::Result<Option<ScopeRecord>> {
    name(token)?;
    if !checkout.is_dir() {
        return Ok(None);
    }
    let op = operation(
        EffectClass::Observe,
        CancelToken::default(),
        Duration::from_secs(30),
    );
    let cx = op.observe().map_err(|_| error("observe context"))?;
    if let Some(root) = local(&cx, checkout)? {
        if let Some(dir) = root
            .state_directory()
            .observe_child(&cx, &FileName::new(FAMILY).map_err(error)?)
            .map_err(error)?
        {
            if let Some(entry) = read(&cx, &dir, token)? {
                return Ok(entry.0);
            }
        }
    }
    legacy(&cx, checkout, token)
}
/// Bounded local inventory, plus the legacy input for this checkout's own token.
pub fn load_local_records(checkout: &Path) -> io::Result<Vec<ScopeRecord>> {
    if !checkout.is_dir() {
        return Ok(Vec::new());
    }
    let op = operation(
        EffectClass::Observe,
        CancelToken::default(),
        Duration::from_secs(30),
    );
    let cx = op.observe().map_err(|_| error("observe context"))?;
    let mut records = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    if let Some(root) = local(&cx, checkout)? {
        if let Some(dir) = root
            .state_directory()
            .observe_child(&cx, &FileName::new(FAMILY).map_err(error)?)
            .map_err(error)?
        {
            for entry in dir.observe_entries(&cx, LIMIT).map_err(error)? {
                cx.check("scope inventory")
                    .map_err(|interrupt| error(format!("{interrupt}")))?;
                let Some(token) = entry.to_str().and_then(|name| name.strip_suffix(".json")) else {
                    continue;
                };
                if let Some(entry) = read(&cx, &dir, token)? {
                    if let Some(record) = entry.0 {
                        records.insert(token.to_owned(), record);
                    }
                }
                seen.insert(token.to_owned());
            }
        }
    }
    if let Some(token) = effigy_core::worktree_scope::load(checkout)? {
        if !seen.contains(&token) {
            if let Some(record) = legacy(&cx, checkout, &token)? {
                records.insert(token, record);
            }
        }
    }
    Ok(records.into_values().collect())
}
/// The ordinary retirement surface selects only this checkout's own scope.
pub fn load_for_checkout(checkout: &Path) -> io::Result<Vec<ScopeRecord>> {
    if !checkout.is_dir() {
        return Ok(Vec::new());
    }
    let wanted = checkout.canonicalize().map_err(error)?;
    Ok(load_local_records(checkout)?
        .into_iter()
        .filter(|record| {
            Path::new(&record.checkout)
                .canonicalize()
                .is_ok_and(|root| root == wanted)
        })
        .collect())
}
#[derive(Debug)]
struct InventoryOnly;
impl SettlementAdapter for InventoryOnly {
    fn observe(&mut self, _: &Effect, _: &effigy_kernel::ctx::CleanupContext) -> Observed<Fence> {
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "inventory has no reversible external effect".into(),
        ))
    }
    fn release(&mut self, _: &Effect, _: &effigy_kernel::ctx::CleanupContext) -> Observed<()> {
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "inventory has no reversible external effect".into(),
        ))
    }
    fn reconcile(
        &mut self,
        _: &Effect,
        _: &effigy_kernel::ctx::CleanupContext,
    ) -> Observed<Reconciled> {
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "inventory has no reversible external effect".into(),
        ))
    }
}
fn with_run<T, E>(
    checkout: &Path,
    token: &str,
    role: &str,
    custody: Option<effigy_kernel::supervise::LifecyclePlan>,
    action: impl FnOnce(&mut Run<'_>, &StateRoot, &effigy_kernel::ctx::EffectContext) -> Result<T, E>,
) -> Result<T, ScopeJournalError<E>> {
    let (root, cx, cleanup, invocation) = (|| -> io::Result<_> {
        name(token)?;
        let op = operation(
            EffectClass::Mutate,
            CancelToken::default(),
            Duration::from_secs(if role == "retire-scope" { 900 } else { 60 }),
        );
        let cx = op.effects().map_err(|_| error("Mutate context"))?;
        let cleanup = op
            .cleanup(Duration::from_secs(30))
            .map_err(|_| error("cleanup context"))?;
        let home = crate::policy::render::state_home().map_err(error)?;
        let root = StateRoot::checkout_effects(
            &cx,
            &home,
            &crate::environment::location(checkout).map_err(error)?,
        )
        .map_err(error)?;
        effigy_core::worktree_scope::prepare_checkout_token(checkout)?;
        effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root(checkout)?;
        let identity = crate::policy::render::checkout_for_root(checkout).map_err(error)?;
        let owner = ResourceId::store(Scope::Checkout(identity), &format!("{role}:{token}"))
            .map_err(error)?;
        let invocation = effigy_kernel::id::ResolvedInvocation::new(InvocationSpec {
            task: TaskId::new(identity, "", Surface::Published, "runtime-scope", None)
                .map_err(error)?,
            route: Route::Builtin("container".into()),
            argv: vec!["container".into()],
            cwd: String::new(),
            target: ExecutionTarget::Host,
            env: BTreeMap::new(),
            owner,
            gates: vec![],
            attachments: vec![],
            class: EffectClass::Mutate,
            placement: op.placement(),
            provenance: BTreeMap::new(),
        })
        .map_err(error)?;
        Ok((root, cx, cleanup, invocation))
    })()
    .map_err(ScopeJournalError::Storage)?;
    let mut adapter = InventoryOnly;
    let mut run = Run::begin(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id: new_run_id().map_err(error)?,
            wait: Wait::Until(Instant::now() + Duration::from_secs(30)),
            acknowledge: vec![],
            next: Invocation::validated(
                vec!["effigy".into(), "container".into(), "retire".into()],
                |a| a == ["effigy", "container", "retire"],
            )
            .map_err(|e| error(format!("{e:?}")))?,
            custody,
        },
        &cleanup,
        &mut adapter,
    )
    .map_err(error)
    .map_err(ScopeJournalError::Storage)?;
    let result = action(&mut run, &root, &cx);
    let finish = run
        .finish(if result.is_ok() {
            WorkloadOutcome::Succeeded
        } else {
            WorkloadOutcome::Failed { code: 1 }
        })
        .map_err(error);
    let settled = run.settle(&cleanup, &mut adapter).map_err(error);
    let value = result.map_err(ScopeJournalError::Operation)?;
    finish.map_err(ScopeJournalError::Storage)?;
    let outcome = settled.map_err(ScopeJournalError::Storage)?;
    if outcome.cleanup() != effigy_kernel::outcome::CleanupOutcome::Complete {
        return Err(ScopeJournalError::Storage(error(
            "scope run cleanup is unresolved",
        )));
    }
    Ok(value)
}
/// Storage and caller failures remain distinct across the kernel close protocol.
#[derive(Debug)]
pub enum ScopeJournalError<E> {
    Storage(io::Error),
    Operation(E),
}
impl<E> From<io::Error> for ScopeJournalError<E> {
    fn from(error: io::Error) -> Self {
        Self::Storage(error)
    }
}
/// Hold one retirement owner through every external effect and positive close.
pub fn with_retirement<T, E>(
    checkout: &Path,
    token: &str,
    plan: effigy_kernel::supervise::LifecyclePlan,
    action: impl FnOnce(&mut Run<'_>) -> Result<T, E>,
) -> Result<T, ScopeJournalError<E>> {
    with_run(checkout, token, "retire-scope", Some(plan), |run, _, _| {
        action(run)
    })
}
fn edit(
    checkout: &Path,
    token: &str,
    change: impl FnOnce(Option<ScopeRecord>) -> io::Result<Option<ScopeRecord>>,
) -> io::Result<PathBuf> {
    with_run(checkout, token, "runtime-scope", None, |run, root, cx| {
        let file = name(token)?;
        let current = load(checkout, token)?;
        let mut record = change(current)?;
        if let Some(record) = &mut record {
            if Path::new(&record.checkout).canonicalize().ok() == checkout.canonicalize().ok() {
                record.checkout_generation = Some(root.owning_directory().identity().map_err(error)?);
                let observe = cx.operation().observe().map_err(|_| error("observe context"))?;
                if let Observed::Verified(identity, _) = effigy_kernel::gate::Gate::checkout_fence_identity(&observe, root, token) {
                    record.checkout_fence = Some(identity);
                }
            }
        }
        let bytes = serde_json::to_vec_pretty(&Entry(record)).map_err(error)?;
        if bytes.len() as u64 > Entry::MAX_BYTES {
            return Err(error("scope record exceeds 64 KiB"));
        }
        root.verify_binding().map_err(error)?;
        let directory = root.state_directory().prepare_child(cx, &FileName::new(FAMILY).map_err(error)?).map_err(error)?;
        let ticket = run.intend(EffectSpec {
            target: run.fence().clone(),
            kind: "container:scope-inventory".into(),
            payload: serde_json::json!({"token": token, "path": directory.path().join(file.as_str())}),
            settlement: Settlement::Retain,
            required: false,
        }).map_err(error)?;
        run.replace_record(&directory, &file, &bytes, Default::default()).map_err(error)?;
        run.applied(&ticket, serde_json::Value::Null).map_err(error)?;
        Ok(directory.path().join(file.as_str()))
    }).map_err(|e| match e {
        ScopeJournalError::Storage(e) | ScopeJournalError::Operation(e) => e,
    })
}

/// Record a live checkout's own scope immediately before activation.
pub fn upsert(record: &ScopeRecord) -> io::Result<PathBuf> {
    upsert_at(Path::new(&record.checkout), record)
}
/// Retain dead-checkout cleanup progress in the invoking live checkout.
pub fn upsert_at(checkout: &Path, record: &ScopeRecord) -> io::Result<PathBuf> {
    let mut record = record.clone();
    if Path::new(&record.checkout).canonicalize().ok() == checkout.canonicalize().ok() {
        record.checkout_generation = Some(
            DirFd::open(checkout)
                .map_err(error)?
                .identity()
                .map_err(error)?,
        );
    }
    edit(checkout, &record.token, |existing| {
        Ok(Some(match existing {
            Some(existing) if existing.checkout == record.checkout => existing.merged_with(&record),
            _ => record.clone(),
        }))
    })
}
/// A journalled tombstone prevents a legacy inventory from being re-imported.
pub fn remove(checkout: &Path, token: &str) -> io::Result<()> {
    edit(checkout, token, |_| Ok(None)).map(|_| ())
}
