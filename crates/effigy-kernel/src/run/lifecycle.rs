//! The Run owner alone records custody identities and owns its death pipe.
use super::*;
use crate::ctx::ObserveContext;
use crate::ident::{self, ProcessIdentity, ProcessState};
use crate::observe::{Observed, UnknownReason};
use crate::supervise::{LifecyclePlan, SentinelLease};

#[derive(Debug, Clone)]
pub struct RunMetadata {
    pub owner: Option<ProcessIdentity>,
    pub supervisor: Option<ProcessIdentity>,
    pub sentinel: Option<ProcessIdentity>,
    pub sentinel_status: SentinelStatus,
    pub placement: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SentinelStatus {
    NotPlanned,
    Closed,
    Live,
    Gap(UnknownReason),
}
impl<'a> Run<'a> {
    pub(crate) fn anchor_image(&self) -> Result<crate::supervise::anchor::Image, RunError> {
        let lease = self
            .sentinel
            .as_ref()
            .ok_or_else(|| RunError::broken("custody needs its sentinel"))?;
        crate::supervise::anchor::Image::prepare(&lease.executable)
    }
}
impl Run<'_> {
    /// Inspect the same retained Env sentinel after an ambiguous registration.
    /// This does not clear poison, start a backup, qualify a helper birth or
    /// grant permission for any new effect.
    #[allow(dead_code)]
    pub(crate) fn inspect_workspace_lifecycle(&self) -> Result<(), RunError> {
        if self.root.is_machine() || self.owner.resource.kind() != crate::id::ResourceKind::Env {
            return Err(RunError::refused(
                "workspace lifecycle needs a checkout environment",
            ));
        }
        self.require_environment_gate()?;
        journal::check_run_fence(self.store.conn(), self.id, &self.owner)?;
        let row = journal::run(self.store.conn(), self.id)?;
        if row.owner != self.owner.resource.name().to_string()
            || row.generation != self.owner.generation
            || matches!(row.phase, Phase::Stopping | Phase::Closed)
        {
            return Err(RunError::unavailable("workspace lifecycle run differs"));
        }
        let lease = self
            .sentinel
            .as_ref()
            .ok_or_else(|| RunError::unavailable("retained sentinel unavailable"))?;
        lease.verify_retained()?;
        let owner = ident::process_identity(std::process::id())
            .verified()
            .cloned()
            .ok_or_else(|| RunError::unavailable("workspace owner identity unavailable"))?;
        let (kind, placement, recorded_owner, recorded_supervisor, recorded_sentinel): (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = self.store.conn().query_row(
            "SELECT kind, placement, owner_identity, supervisor_identity, sentinel_identity FROM runs WHERE run_id=?1",
            [self.id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )?;
        let parse = |raw: Option<String>| -> Result<Option<ProcessIdentity>, RunError> {
            Ok(raw.map(|value| serde_json::from_str(&value)).transpose()?)
        };
        if kind != "custody"
            || placement != "detached"
            || parse(recorded_owner)? != Some(owner.clone())
            || parse(recorded_supervisor)? != Some(owner)
            || parse(recorded_sentinel)? != Some(lease.identity.clone())
        {
            return Err(RunError::unavailable(
                "workspace lifecycle evidence differs",
            ));
        }
        Ok(())
    }

    /// The lease is transferred only after the one fenced transaction commits.
    /// On refusal its sole write end closes, so an unregistered helper cannot
    /// outlive the failed registration. No second journal writer is exposed.
    pub(crate) fn register_lifecycle(
        &mut self,
        lease: SentinelLease,
        plan: &LifecyclePlan,
    ) -> Result<(), RunError> {
        if self.sentinel.is_some() {
            return Err(RunError::broken("sentinel already registered"));
        }
        let identity = (|| {
            self.can_effect()?;
            if !self.holds_owner_gate()? {
                return Err(RunError::broken("registration needs owner gate"));
            }
            ident::process_identity(std::process::id())
                .verified()
                .cloned()
                .ok_or_else(|| RunError::unavailable("owner identity unavailable"))
        })();
        let identity = match identity {
            Ok(identity) => identity,
            Err(error) => {
                // The helper already started. Retain it for cleanup even when
                // the original operation can no longer register it.
                self.poisoned = true;
                self.sentinel = Some(lease);
                return Err(error);
            }
        };
        if let Some(startup) = self.machine_startup.as_mut() {
            startup.lifecycle = Some((identity, lease.identity.clone(), plan.placement));
            self.sentinel = Some(lease);
            return Ok(());
        }
        let mut lease = Some(lease);
        let result: Result<(), RunError> = (|| {
            crate::fault_point!(LifecycleBefore)?;
            self.store.transaction(&self.cx.child(std::time::Duration::from_secs(30)),|txn| {
                journal::check_run_fence(txn.conn(), self.id, &self.owner)?;
                txn.conn().execute("UPDATE runs SET kind='custody',placement=?2,owner_identity=?3,supervisor_identity=?4,sentinel_identity=?5 WHERE run_id=?1",
                    params![self.id.to_string(),plan.placement.name(),serde_json::to_string(&identity)?,
                    if plan.placement == crate::supervise::Placement::Detached {Some(serde_json::to_string(&identity)?)}else{None},serde_json::to_string(&lease.as_ref().expect("registration owns lease").identity)?])?;
                crate::fault_point!(LifecycleCommit)?;
                Ok::<_,RunError>(())
            })?;
            // Assignment is infallible. A kill in this window closes the local
            // writer as well; the already-recorded helper recovers the run.
            self.sentinel = lease.take();
            crate::fault_point!(LifecycleAfter)?;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
            if let Some(mut lease) = lease {
                let cleanup = match self
                    .cx
                    .operation()
                    .cleanup(std::time::Duration::from_secs(30))
                {
                    Ok(cleanup) => cleanup,
                    Err(_) => {
                        self.sentinel = Some(lease);
                        return Err(RunError::broken("cleanup overflow"));
                    }
                };
                if let Err(error) = lease.close(&cleanup) {
                    // A failed close is not proof that the exact helper has
                    // exited. Keep its lease in the original gated Run so a
                    // recoverable settlement can continue owning it.
                    self.sentinel = Some(lease);
                    return Err(error);
                }
            }
        }
        result?;
        let backup = self.start_checkout_backup();
        if backup.is_err() {
            self.poisoned = true;
        }
        backup
    }
}
/// Pure reads of persisted identities plus fresh process facts. A dead,
/// replaced, unreadable or unrecorded planned sentinel is always a visible gap.
pub fn metadata(cx: &ObserveContext, root: &StateRoot, id: RunId) -> Observed<RunMetadata> {
    let reader = match Store::read(cx, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    let read = (|| -> Result<RunMetadata, RunError> {
        let (owner,supervisor,sentinel,kind,phase,placement)=reader.conn().query_row(
            "SELECT owner_identity,supervisor_identity,sentinel_identity,kind,phase,placement FROM runs WHERE run_id=?1",[id.to_string()],
            |row|Ok((row.get::<_,Option<String>>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?)))?;
        let parse = |s: Option<String>| -> Result<Option<ProcessIdentity>, RunError> {
            Ok(s.map(|s| serde_json::from_str(&s)).transpose()?)
        };
        let sentinel = parse(sentinel)?;
        let sentinel_status = if phase == "closed" {
            SentinelStatus::Closed
        } else if kind != "custody" {
            SentinelStatus::NotPlanned
        } else {
            match sentinel.as_ref() {
                Some(identity)
                    if ident::process_identity(identity.pid).verified() == Some(identity)
                        && matches!(
                            ident::process_state(identity.pid),
                            Observed::Verified(ProcessState::Active | ProcessState::Stopped, _)
                        ) =>
                {
                    SentinelStatus::Live
                }
                _ => SentinelStatus::Gap(UnknownReason::GuardUnavailable),
            }
        };
        Ok(RunMetadata {
            owner: parse(owner)?,
            supervisor: parse(supervisor)?,
            sentinel,
            sentinel_status,
            placement,
        })
    })();
    match read {
        Ok(value) => Observed::Verified(value, super::settle::evidence()),
        Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
    }
}

#[cfg(all(test, feature = "fault-injection"))]
#[path = "lifecycle/tests.rs"]
mod tests;
