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
            return Err(RunError::broken("sentinel already registered"));
        }
        if !self.guard.covers(&self.owner.resource) {
            return Err(RunError::broken("registration needs owner gate"));
        }
        let identity = ident::process_identity(std::process::id())
            .verified()
            .cloned()
            .ok_or_else(|| RunError::unavailable("owner identity unavailable"))?;
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
                let cleanup = self
                    .cx
                    .operation()
                    .cleanup(std::time::Duration::from_secs(30))
                    .map_err(|_| RunError::broken("cleanup overflow"))?;
                lease.close(&cleanup)?;
            }
        }
        result?;
        self.start_checkout_backup()
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
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    #[test]
    fn descriptor_closes_once_after_closed_commit_before_gate_release() {
        let home = super::super::tests::home();
        let cx = super::super::tests::context();
        let root = Arc::new(StateRoot::effects(&cx, home.path()).unwrap());
        let mut options = super::super::tests::opts(1);
        options.custody = Some(LifecyclePlan {
            executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
                .unwrap(),
            placement: crate::supervise::Placement::Inline,
        });
        let mut fake = super::super::tests::Fake::new(home.path());
        let run = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(super::super::tests::owner()),
            options,
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let fd = run.sentinel.as_ref().unwrap().writer_fd();
        let count = Arc::new(AtomicUsize::new(0));
        let observed_count = count.clone();
        let observed_root = root.clone();
        let observed_cx = cx.clone();
        crate::fault::hook(Some(Box::new(move |point| {
            if point == crate::fault::Point::SentinelClose {
                // Check before opening the reader, which could reuse this fd number.
                assert_eq!(unsafe { libc::fcntl(fd, libc::F_GETFD) }, -1);
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::EBADF)
                );
                let observe = observed_cx.operation().observe().unwrap();
                assert!(matches!(
                    metadata(&observe, &observed_root, super::super::tests::id(1))
                        .verified()
                        .unwrap()
                        .sentinel_status,
                    SentinelStatus::Closed
                ));
                assert!(matches!(
                    Gate::probe(&observe, &observed_root, &super::super::tests::owner()),
                    Observed::Verified(crate::gate::GateState::Held(_), _)
                ));
                observed_count.fetch_add(1, Ordering::SeqCst);
            }
        })));
        run.settle(&super::super::tests::cleanup(&cx), &mut fake)
            .unwrap();
        settle_interrupted(
            &cx,
            &root,
            &super::super::tests::owner(),
            super::super::tests::id(1),
            super::super::tests::next(),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        crate::fault::hook(None);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
