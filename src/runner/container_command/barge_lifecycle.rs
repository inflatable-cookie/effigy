//! Where Effigy's container lifecycle takes, holds and returns Barge leases.
//!
//! A lease is reserved before Compose applies a stack, its ID is recorded so
//! every render labels the stack with `barge.lease`, and it is returned in the
//! same step as any teardown Effigy performs. A held lease lives in this
//! process, so a crash releases it for the sweep. Without a Barge account or a
//! declared profile every entry point here does nothing.

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use barge::container::{ContainerLease, ContainerLeaseOutcome, ContainerLeaseRequest};
use barge::Barge;
use effigy_containers::environment::Retention;
use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
use effigy_core::worktree_scope::ScopeKind;

use super::barge_leases::{self, ContainerClassContext, LeaseAcquireOutcome};
use super::RunnerError;

const INVENTORY_BUDGET: Duration = Duration::from_secs(15);
const HANDOFF_BUDGET: Duration = Duration::from_secs(5);
/// Names the file an idle reaper writes once it holds its parent's lease.
pub(in crate::runner) const HANDOFF_ENV: &str = "EFFIGY_INTERNAL_CONTAINER_LEASE_HANDOFF";

/// `(profile, project)`, Barge's key for one stack.
type StackKey = (String, String);

/// The leases this process holds. Dropping one releases its hold.
static HELD: Mutex<BTreeMap<StackKey, ContainerLease>> = Mutex::new(BTreeMap::new());

fn held() -> MutexGuard<'static, BTreeMap<StackKey, ContainerLease>> {
    HELD.lock().unwrap_or_else(PoisonError::into_inner)
}

fn stack_key(policy: &EffectiveContainerPolicy) -> StackKey {
    (policy.profile.clone(), policy.project_name.clone())
}

/// How one bring-up uses its stack, which decides whether its lease is held.
#[derive(Debug, Clone, Copy)]
pub(in crate::runner) struct LeaseIntent {
    /// The retention the bring-up asks for.
    pub(in crate::runner) retention: Retention,
    /// This command lives as long as it uses the stack: an attached `up` or a
    /// release build.
    pub(in crate::runner) session: bool,
    /// An explicit detached `container up`, which is a worker's when no
    /// terminal drives it.
    pub(in crate::runner) explicit_detached: bool,
}

/// A lease reserved, or already held, for the stack being brought up.
#[derive(Debug)]
pub(in crate::runner) struct Reservation {
    request: ContainerLeaseRequest,
    id: String,
    /// The stack had no lease before this reservation.
    fresh: bool,
    /// An outer holder in this process owns the lease and settles it.
    borrowed: bool,
}

fn recorded_retention(policy: &EffectiveContainerPolicy) -> Option<Retention> {
    crate::runner::environment::record(policy)
        .and_then(|record| record.engine().map(|engine| engine.retention))
}

/// Leases are the host's: a guest run neither takes one nor touches the label
/// record it can see through the checkout mount.
fn inside_guest() -> bool {
    effigy_core::container_detection::process_is_inside_effigy_workspace_container()
}

/// The ID of the lease this process still holds for the stack. A guard whose
/// lease a teardown elsewhere returned is dropped, so a long-lived process
/// reserves again.
fn held_lease_id(
    barge: &Barge,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<String>, RunnerError> {
    let key = stack_key(policy);
    let Some(id) = held().get(&key).map(|lease| lease.id().to_owned()) else {
        return Ok(None);
    };
    let current = barge_leases::find_lease(barge, &policy.profile, &policy.project_name)?;
    if current.is_some_and(|lease| lease.id == id) {
        return Ok(Some(id));
    }
    held().remove(&key);
    Ok(None)
}

/// The Barge account the lifecycle uses, or `None` when this machine has no
/// container leases. It never creates an account on a machine without one.
fn open_wired_barge() -> Result<Option<Barge>, RunnerError> {
    if inside_guest() {
        return Ok(None);
    }
    // Unit tests reach only an account they name themselves.
    if cfg!(test) && std::env::var_os("BARGE_STATE_DIR").is_none() {
        return Ok(None);
    }
    let Some(probe) = barge_leases::classify_open_barge_result(Barge::open_read_only())? else {
        return Ok(None);
    };
    for name in ["container-profiles.json", "container-leases"] {
        let path = probe.state_dir().join(name);
        if path.try_exists().map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot inspect Barge state `{}`: {error}",
                path.display()
            ))
        })? {
            return barge_leases::open_host_barge();
        }
    }
    Ok(None)
}

#[cfg(not(test))]
fn lease_runtime() -> impl barge_leases::LeaseRuntime {
    barge_leases::RealLeaseRuntime
}

/// A unit test's sweep never reads or tears down a host runtime.
#[cfg(test)]
fn lease_runtime() -> impl barge_leases::LeaseRuntime {
    tests::NoProfiles
}

/// Record the ID the next render labels this stack with.
fn label_renders(policy: &EffectiveContainerPolicy, id: Option<&str>) -> Result<(), RunnerError> {
    let repo = &policy.repo_root;
    let current =
        effigy_containers::recorded_barge_lease(repo, &policy.profile, &policy.project_name);
    if current.as_deref() == id {
        return Ok(());
    }
    effigy_containers::record_barge_lease(repo, &policy.profile, &policy.project_name, id).map_err(
        |error| {
            RunnerError::task_invocation(format!(
                "cannot record the Barge lease of project `{}`: {error}",
                policy.project_name
            ))
        },
    )
}

/// Reserve this stack's slot before Compose applies it, sweeping and waiting
/// while the profile is full. `None` when the stack is not lease-managed.
///
/// Only a generated stack is leased: its render carries the `barge.lease`
/// label a sweep finds it by. An inline workspace stack has neither the label
/// nor a teardown that returns a lease.
pub(in crate::runner) fn reserve(
    policy: &EffectiveContainerPolicy,
    intent: LeaseIntent,
) -> Result<Option<Reservation>, RunnerError> {
    if policy.compose_source != EffectiveComposeSource::Generated || inside_guest() {
        return Ok(None);
    }
    let Some(barge) = open_wired_barge()? else {
        label_renders(policy, None)?;
        return Ok(None);
    };
    let checkout = effigy_core::worktree_scope::checkout_root(&policy.repo_root);
    let owner = barge_leases::resolve_container_owner(&barge, &checkout)?;
    let scope = effigy_core::worktree_scope::scope_kind(&checkout)
        .ok()
        .flatten();
    // A session and a task's own stack hold their lease, so a crash releases
    // it. A persistent stack outlives its command, and a task's ensure never
    // demotes it.
    let hold = intent.session
        || (intent.retention == Retention::Held
            && recorded_retention(policy) != Some(Retention::Persistent));
    let class = barge_leases::resolve_container_class(ContainerClassContext {
        profile: &policy.profile,
        is_worktree: scope == Some(ScopeKind::LinkedWorktree)
            || matches!(owner, barge::container::ContainerOwner::Worktree(_)),
        is_ephemeral: scope == Some(ScopeKind::EphemeralClone),
        is_ci: barge_leases::is_ci_environment(),
        is_detached_worker: intent.explicit_detached && !std::io::stdin().is_terminal(),
        is_release_builder: false,
    });
    let request = ContainerLeaseRequest {
        profile: policy.profile.clone(),
        project: policy.project_name.clone(),
        owner,
        class,
        hold,
        wait: None,
    };
    if let Some(id) = held_lease_id(&barge, policy)? {
        label_renders(policy, Some(&id))?;
        return Ok(Some(Reservation {
            request,
            id,
            fresh: false,
            borrowed: true,
        }));
    }
    let fresh = barge_leases::find_lease(&barge, &policy.profile, &policy.project_name)?.is_none();
    let outcome = barge_leases::acquire_container_lease_with(
        Some(&barge),
        &lease_runtime(),
        &request,
        barge_leases::lease_wait_budget_for(None)?,
        barge_leases::DEFAULT_LEASE_WAIT_SLICE,
    )?;
    let LeaseAcquireOutcome::Leased(lease) = outcome else {
        label_renders(policy, None)?;
        return Ok(None);
    };
    let reservation = Reservation {
        id: lease.id().to_owned(),
        request,
        fresh,
        borrowed: false,
    };
    keep(policy, lease);
    if let Err(error) = label_renders(policy, Some(&reservation.id)) {
        reservation.settle_failed(policy, false);
        return Err(error);
    }
    Ok(Some(reservation))
}

/// Keep a held lease for as long as this process lives; an unheld one needs no
/// guard.
fn keep(policy: &EffectiveContainerPolicy, lease: ContainerLease) {
    if lease.is_held() {
        held().insert(stack_key(policy), lease);
    }
}

impl Reservation {
    /// After a failed bring-up, free the slot at once unless the stack still
    /// runs. `applied` is whether Compose may have changed anything.
    pub(in crate::runner) fn settle_failed(
        &self,
        policy: &EffectiveContainerPolicy,
        applied: bool,
    ) {
        if self.borrowed || ((applied || !self.fresh) && stack_may_be_running(policy)) {
            return;
        }
        held().remove(&stack_key(policy));
        let returned = open_wired_barge()
            .and_then(|barge| barge_leases::return_lease_by_id_with(barge.as_ref(), &self.id));
        if let Err(error) = returned {
            eprintln!(
                "[warn] Barge lease `{}` of project `{}` was not returned: {error}",
                self.id, policy.project_name
            );
        }
    }

    /// A teardown that ran while this bring-up waited for the environment
    /// returned the lease; reserve the stack that is now running again, so no
    /// sweep mistakes it for an orphan.
    pub(in crate::runner) fn confirm(&self, policy: &EffectiveContainerPolicy) {
        if self.borrowed {
            return;
        }
        let renewed = open_wired_barge().and_then(|barge| {
            let Some(barge) = barge else {
                return Ok(());
            };
            if barge_leases::find_lease(&barge, &policy.profile, &policy.project_name)?.is_some() {
                return Ok(());
            }
            match barge
                .container_lease(&self.request)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?
            {
                ContainerLeaseOutcome::Leased(lease) => {
                    label_renders(policy, Some(lease.id()))?;
                    keep(policy, lease);
                    Ok(())
                }
                ContainerLeaseOutcome::NotManaged => Ok(()),
                ContainerLeaseOutcome::Full { max, active, .. } => {
                    Err(RunnerError::task_invocation(format!(
                        "profile `{}` is full ({active}/{max} in use)",
                        policy.profile
                    )))
                }
            }
        });
        if let Err(error) = renewed {
            eprintln!(
                "[warn] project `{}` is running without a Barge lease: {error}",
                policy.project_name
            );
        }
    }

    /// An attached session that ends with its stack left running turns the
    /// held lease into one that lasts until the stack is torn down.
    pub(in crate::runner) fn keep_after_session(&self, policy: &EffectiveContainerPolicy) {
        if self.borrowed || !self.request.hold {
            return;
        }
        let mut request = self.request.clone();
        request.hold = false;
        let kept = open_wired_barge().and_then(|barge| {
            let Some(barge) = barge else {
                return Ok(());
            };
            // The outcome carries no hold; dropping it leaves the reservation.
            barge
                .container_lease(&request)
                .map(drop)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))
        });
        held().remove(&stack_key(policy));
        if let Err(error) = kept {
            eprintln!(
                "[warn] Barge lease `{}` of project `{}` could not be kept for the running stack: {error}",
                self.id, policy.project_name
            );
        }
    }
}

/// Whether any container of the project may still run. An unreadable inventory
/// answers yes, so a running stack never loses its lease.
fn stack_may_be_running(policy: &EffectiveContainerPolicy) -> bool {
    let rows =
        effigy_containers::exec::list_compose_containers_for_policy_including_stopped_with_deadline(
            &policy.repo_root,
            policy,
            Some(Instant::now() + INVENTORY_BUDGET),
        );
    rows.map_or(true, |rows| {
        rows.iter().any(|row| {
            row.project_name.as_deref() == Some(policy.project_name.as_str())
                && !row.oneoff
                && row.status.is_running()
        })
    })
}

/// Return the lease of a stack Effigy just tore down. The teardown already
/// succeeded, so a failed return only warns.
pub(in crate::runner) fn return_torn_down_stack(profile: &str, project: &str) {
    held().remove(&(profile.to_owned(), project.to_owned()));
    let returned = open_wired_barge().and_then(|barge| {
        barge_leases::return_lease_for_project_with(barge.as_ref(), profile, project)
    });
    if let Err(error) = returned {
        eprintln!(
            "[warn] the Barge lease of project `{project}` on profile `{profile}` was not returned: {error}"
        );
    }
}

/// Return every lease of a profile whose VM Effigy just deleted, with all the
/// stacks on it.
pub(in crate::runner) fn return_destroyed_profile(profile: &str) {
    held().retain(|(held_profile, _), _| held_profile != profile);
    let returned = open_wired_barge().and_then(|barge| {
        let Some(barge) = barge else {
            return Ok(());
        };
        let leases = barge
            .container_status()
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        for lease in leases.iter().filter(|lease| lease.profile == profile) {
            barge_leases::return_lease_by_id_with(Some(&barge), &lease.id)?;
        }
        Ok(())
    });
    if let Err(error) = returned {
        eprintln!(
            "[warn] the Barge leases of deleted profile `{profile}` were not all returned: {error}"
        );
    }
}

/// A session that uses a held stack holds its lease, so a sweep leaves the
/// stack alone while it is in use. A stack without a lease gains none here.
pub(in crate::runner) fn hold_for_session(policy: &EffectiveContainerPolicy) {
    let adopted = open_wired_barge().and_then(|barge| {
        let Some(barge) = barge else {
            return Ok(());
        };
        if held_lease_id(&barge, policy)?.is_some() {
            return Ok(());
        }
        if barge_leases::find_lease(&barge, &policy.profile, &policy.project_name)?.is_none()
            || recorded_retention(policy) != Some(Retention::Held)
        {
            return Ok(());
        }
        let checkout = effigy_core::worktree_scope::checkout_root(&policy.repo_root);
        let owner = barge_leases::resolve_container_owner(&barge, &checkout)?;
        if let Some(lease) = barge_leases::hold_existing_lease_with(
            &barge,
            &policy.profile,
            &policy.project_name,
            &owner,
        )? {
            label_renders(policy, Some(lease.id()))?;
            keep(policy, lease);
        }
        Ok(())
    });
    if let Err(error) = adopted {
        eprintln!(
            "[warn] the Barge lease of project `{}` could not be held: {error}",
            policy.project_name
        );
    }
}

/// Stop holding this stack's lease in this process. The reservation stays, and
/// a sweep may reclaim it.
pub(in crate::runner) fn release_hold(policy: &EffectiveContainerPolicy) {
    held().remove(&stack_key(policy));
}

/// Whether the lease `id` was returned: its stack was torn down by Effigy.
pub(in crate::runner) fn lease_was_returned(id: &str) -> Result<bool, RunnerError> {
    let Some(barge) = open_wired_barge()? else {
        return Ok(false);
    };
    let leases = barge
        .container_status()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(!leases.iter().any(|lease| lease.id == id))
}

/// The file an idle reaper signals on, when this process holds the lease it
/// hands over.
pub(in crate::runner) fn handoff_file(
    policy: &EffectiveContainerPolicy,
    token: &str,
) -> Option<PathBuf> {
    held().contains_key(&stack_key(policy)).then(|| {
        effigy_core::worktree_scope::checkout_root(&policy.repo_root)
            .join(".effigy/runtime/barge-lease")
            .join(format!("handoff-{token}"))
    })
}

/// Keep this process's hold until the reaper has taken its own, so the lease
/// never reads as ended between them. A reaper that exits or stalls only
/// leaves the lease to the sweep.
pub(in crate::runner) fn await_handoff(path: &Path, reaper: &mut Child) {
    let deadline = Instant::now() + HANDOFF_BUDGET;
    while !path.exists() && Instant::now() < deadline && matches!(reaper.try_wait(), Ok(None)) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::super::barge_leases::LeaseRuntime;
    use super::super::profile_inventory::ContainerRow;
    use super::*;
    use effigy_containers::RetireOutcome;

    /// A runtime with no profile, for unit tests that reach the lifecycle.
    pub(super) struct NoProfiles;

    impl LeaseRuntime for NoProfiles {
        fn running_profiles(&self) -> Result<Vec<String>, RunnerError> {
            Ok(Vec::new())
        }

        fn list_containers(&self, _profile: &str) -> Result<Vec<ContainerRow>, RunnerError> {
            Ok(Vec::new())
        }

        fn teardown_project(
            &self,
            _profile: &str,
            _project: &str,
            _remove_volumes: bool,
        ) -> Result<RetireOutcome, RunnerError> {
            Ok(RetireOutcome::default())
        }
    }
}
