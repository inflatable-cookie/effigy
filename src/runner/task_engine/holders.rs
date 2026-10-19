//! Live task runs of one checkout, from the run journal plus a fresh gate
//! probe of each owner (R1, R3). A free gate makes a non-closed run
//! interrupted, never live; an unreadable probe counts as live, because an
//! unknown holder grants nothing destructive.

use std::path::Path;

use effigy_kernel::gate::{Gate, GateState};
use effigy_kernel::id::{Digest, ResourceId, ResourceKind, RunId, Scope};
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{history, unclosed, Phase};
use effigy_kernel::store::Store;

use super::identity::shared_lock_store;
use super::kernel;
use super::run::MANAGED_RUN_PROVENANCE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum HolderKind {
    Task,
    SharedLock(String),
}

/// One live (or unknown) holder of a task gate or shared lock in this
/// checkout. `pid` is `None` when the gate is held but its diagnostics or
/// the probe itself are unreadable.
#[derive(Debug, Clone)]
pub(in crate::runner) struct LiveHolder {
    pub(in crate::runner) kind: HolderKind,
    pub(in crate::runner) run: RunId,
    pub(in crate::runner) catalog: String,
    pub(in crate::runner) task: String,
    pub(in crate::runner) profile: Option<String>,
    pub(in crate::runner) pid: Option<u32>,
    pub(in crate::runner) managed_run: Option<Digest>,
    pub(in crate::runner) started_at_ms: i64,
}

impl LiveHolder {
    /// `task:<task>`, `profile:<task>/<profile>` or `shared:<name>`.
    pub(in crate::runner) fn scope_label(&self) -> String {
        match (&self.kind, &self.profile) {
            (HolderKind::SharedLock(name), _) => super::run::shared_scope_label(name),
            (HolderKind::Task, Some(profile)) => format!("profile:{}/{profile}", self.task),
            (HolderKind::Task, None) => format!("task:{}", self.task),
        }
    }

    pub(in crate::runner) fn carries_managed_run(&self, token: &str) -> bool {
        self.managed_run == Some(Digest::of(token.as_bytes()))
    }
}

/// Every live holder recorded for `repo_root`'s checkout. Pure read: no
/// token is minted and no gate is acquired.
pub(in crate::runner) fn live_holders(repo_root: &Path) -> Vec<LiveHolder> {
    let Ok(Some(checkout)) = super::checkout::read(repo_root) else {
        return Vec::new();
    };
    let cx = kernel::observe();
    let root = match kernel::observe_root(&cx) {
        Observed::Verified(root, _) => root,
        _ => return Vec::new(),
    };
    let reader = match Store::read(&cx, &root) {
        Observed::Verified(reader, _) => reader,
        _ => return Vec::new(),
    };
    let Observed::Verified(runs, _) = unclosed(&reader, Scope::Checkout(checkout)) else {
        return Vec::new();
    };
    runs.into_iter()
        .filter_map(|summary| {
            let invocation = summary.invocation.as_ref()?;
            let spec = invocation.spec();
            let pid = match Gate::probe(&cx, &root, &spec.owner) {
                Observed::Verified(GateState::Held(diag), _) => {
                    diag.verified().map(|diag| diag.pid)
                }
                // Free: the run was interrupted; its successor settles it.
                Observed::Verified(GateState::Free, _) | Observed::Absent(_) => return None,
                Observed::Unknown(_) => None,
            };
            Some(LiveHolder {
                kind: holder_kind(&spec.owner),
                run: summary.id,
                catalog: spec.task.catalog().to_owned(),
                task: spec.task.task().to_owned(),
                profile: spec.task.profile().map(str::to_owned),
                pid,
                managed_run: spec.provenance.get(MANAGED_RUN_PROVENANCE).cloned(),
                started_at_ms: summary.created_at_ms,
            })
        })
        .collect()
}

fn holder_kind(owner: &ResourceId) -> HolderKind {
    if owner.kind() == ResourceKind::Store {
        let name = owner
            .key()
            .first()
            .and_then(Option::as_deref)
            .unwrap_or_default();
        let prefix = shared_lock_store("");
        return HolderKind::SharedLock(name.strip_prefix(&prefix).unwrap_or(name).to_owned());
    }
    HolderKind::Task
}

/// When the current holder of `owner` was admitted, for conflict messages.
pub(in crate::runner) fn holder_run_started_ms(owner: &ResourceId) -> Option<i64> {
    let cx = kernel::observe();
    let Observed::Verified(root, _) = kernel::observe_root(&cx) else {
        return None;
    };
    let Observed::Verified(reader, _) = Store::read(&cx, &root) else {
        return None;
    };
    let Observed::Verified(runs, _) = history(&reader, owner, 1) else {
        return None;
    };
    runs.into_iter()
        .find(|run| run.phase != Phase::Closed)
        .map(|run| run.created_at_ms)
}
