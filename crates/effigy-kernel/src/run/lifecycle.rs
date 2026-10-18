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
impl Run<'_> {
    /// The lease is transferred only after the one fenced transaction commits.
    /// On refusal its sole write end closes, so an unregistered helper cannot
    /// outlive the failed registration. No second journal writer is exposed.
    pub(crate) fn register_lifecycle(
        &mut self,
        lease: SentinelLease,
        plan: &LifecyclePlan,
    ) -> Result<(), RunError> {
        self.can_effect()?;
        if self.sentinel.is_some() {
            return Err(RunError::Invalid("sentinel already registered".into()));
        }
        if !self.guard.covers(&self.owner.resource) {
            return Err(RunError::Invalid("registration needs owner gate".into()));
        }
        let identity = ident::process_identity(std::process::id())
            .verified()
            .cloned()
            .ok_or_else(|| RunError::Invalid("owner identity unavailable".into()))?;
        let result = (|| {
            crate::fault_point!(LifecycleBefore)?;
            self.store.transaction(&self.cx,|txn| {
                journal::check_fence(txn.conn(),&self.owner)?;
                txn.conn().execute("UPDATE runs SET kind='custody',placement=?2,owner_identity=?3,supervisor_identity=?4,sentinel_identity=?5 WHERE run_id=?1",
                    params![self.id.to_string(),plan.placement.name(),serde_json::to_string(&identity)?,
                    if plan.placement == crate::supervise::Placement::Detached {Some(serde_json::to_string(&identity)?)}else{None},serde_json::to_string(&lease.identity)?])?;
                crate::fault_point!(LifecycleCommit)?;
                Ok::<_,RunError>(())
            })?;
            // Assignment is infallible. A kill in this window closes the local
            // writer as well; the already-recorded helper recovers the run.
            self.sentinel = Some(lease);
            crate::fault_point!(LifecycleAfter)?;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
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
