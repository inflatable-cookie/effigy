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
    /// The journal or state root could not be read: an unknown holder,
    /// which grants nothing destructive (R3).
    Unreadable(String),
}

/// One live (or unknown) holder of a task gate or shared lock in this
/// checkout. `pid` is `None` when the gate is held but its diagnostics or
/// the probe itself are unreadable.
#[derive(Debug, Clone)]
pub(in crate::runner) struct LiveHolder {
    pub(in crate::runner) kind: HolderKind,
    pub(in crate::runner) checkout_root: Option<std::path::PathBuf>,
    pub(in crate::runner) run: Option<RunId>,
    pub(in crate::runner) catalog: String,
    pub(in crate::runner) task: String,
    pub(in crate::runner) profile: Option<String>,
    pub(in crate::runner) pid: Option<u32>,
    pub(in crate::runner) managed_run: Option<Digest>,
}

impl LiveHolder {
    /// `task:<task>`, `profile:<task>/<profile>` or `shared:<name>`.
    pub(in crate::runner) fn scope_label(&self) -> String {
        match (&self.kind, &self.profile) {
            (HolderKind::Unreadable(reason), _) => format!("unreadable run journal: {reason}"),
            (HolderKind::SharedLock(name), _) => super::run::shared_scope_label(name),
            (HolderKind::Task, Some(profile)) => format!("profile:{}/{profile}", self.task),
            (HolderKind::Task, None) => format!("task:{}", self.task),
        }
    }
}

/// Every live holder recorded for `repo_root`'s checkout, or for the root
/// that declares it when `repo_root` is a member catalog. Pure read: no
/// token is minted and no gate is acquired.
pub(in crate::runner) fn live_holders(repo_root: &Path) -> Vec<LiveHolder> {
    let mut roots = vec![repo_root.to_path_buf()];
    if let Ok(resolved) = effigy_core::resolver::resolve_target_root(repo_root.to_path_buf(), None)
    {
        roots.push(resolved.resolved_root);
    }
    roots.sort();
    roots.dedup();
    let mut live = Vec::new();
    let mut checkouts = Vec::new();
    for root in roots {
        let cx = kernel::observe();
        if let Observed::Verified(state, _) = kernel::observe_root(&cx, &root) {
            match Gate::foreground_holders(&cx, &state) {
                Observed::Verified(rows, _) => {
                    for row in rows {
                        let coordinates = row.coordinates;
                        if coordinates.owner.kind() != ResourceKind::Task {
                            continue;
                        }
                        let id = row.holder.run.as_deref().and_then(|run| run.parse().ok());
                        let kind = holder_kind(&coordinates.gate);
                        if live
                            .iter()
                            .any(|held: &LiveHolder| held.run == id && held.kind == kind)
                        {
                            continue;
                        }
                        live.push(LiveHolder {
                            kind,
                            checkout_root: Some(root.clone()),
                            run: id,
                            catalog: coordinates.task.catalog().into(),
                            task: coordinates.task.task().into(),
                            profile: None,
                            pid: matches!(
                                row.holder.state,
                                Observed::Verified(
                                    effigy_kernel::ident::ProcessState::Active
                                        | effigy_kernel::ident::ProcessState::Stopped,
                                    _
                                )
                            )
                            .then_some(row.holder.pid),
                            managed_run: None,
                        });
                    }
                }
                Observed::Unknown(reason) => {
                    live.push(unreadable(format!("foreground gates: {reason:?}")))
                }
                Observed::Absent(_) => {}
            }
        }
        if let Ok(Some(checkout)) = super::checkout::read(&root) {
            if !checkouts.iter().any(|(_, id)| *id == checkout) {
                checkouts.push((root, checkout));
            }
        }
    }
    live.extend(
        checkouts
            .into_iter()
            .flat_map(|(root, checkout)| checkout_holders(&root, checkout)),
    );
    live
}

fn checkout_holders(path: &Path, checkout: effigy_kernel::id::CheckoutId) -> Vec<LiveHolder> {
    let cx = kernel::observe();
    let root = match kernel::observe_root(&cx, path) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Vec::new(),
        Observed::Unknown(reason) => return vec![unreadable(format!("{reason:?}"))],
    };
    let reader = match Store::read(&cx, &root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Vec::new(),
        Observed::Unknown(reason) => return vec![unreadable(format!("{reason:?}"))],
    };
    let runs = match unclosed(&reader, Scope::Checkout(checkout)) {
        Observed::Verified(runs, _) => runs,
        Observed::Absent(_) => return Vec::new(),
        Observed::Unknown(reason) => return vec![unreadable(format!("{reason:?}"))],
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
                checkout_root: Some(path.into()),
                kind: holder_kind(&spec.owner),
                run: Some(summary.id),
                catalog: spec.task.catalog().to_owned(),
                task: spec.task.task().to_owned(),
                profile: spec.task.profile().map(str::to_owned),
                pid,
                managed_run: spec.provenance.get(MANAGED_RUN_PROVENANCE).cloned(),
            })
        })
        .collect()
}

fn unreadable(reason: String) -> LiveHolder {
    LiveHolder {
        checkout_root: None,
        kind: HolderKind::Unreadable(reason),
        run: None,
        catalog: String::new(),
        task: String::new(),
        profile: None,
        pid: None,
        managed_run: None,
    }
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
pub(in crate::runner) fn holder_run_started_ms(path: &Path, owner: &ResourceId) -> Option<i64> {
    let cx = kernel::observe();
    let Observed::Verified(root, _) = kernel::observe_root(&cx, path) else {
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
