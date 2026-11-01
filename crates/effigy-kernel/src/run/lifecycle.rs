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
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};

    fn reap_test_child(identity: &ProcessIdentity) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let mut status = 0;
            // SAFETY: this is the exact helper child spawned by the test.
            let waited = unsafe { libc::waitpid(identity.pid as i32, &mut status, libc::WNOHANG) };
            if waited != 0 {
                break;
            }
            if Instant::now() >= deadline {
                // SAFETY: only the exact unreaped child spawned by this test.
                unsafe {
                    libc::kill(identity.pid as i32, libc::SIGKILL);
                    libc::waitpid(identity.pid as i32, &mut status, 0);
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn check_workspace_sentinel_start_fault(point: crate::fault::Point) {
        let home = effigy_test_support::short_tempdir("ls-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
        let owner = ResourceId::environment(checkout, "sentinel-start-proof").unwrap();
        let mut fake = super::super::tests::Fake::new(home.path());
        let mut run = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner.clone()),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let plan = LifecyclePlan {
            executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
                .unwrap(),
            placement: crate::supervise::Placement::Detached,
        };
        let helper_log = home.path().join("sentinel.jsonl");
        crate::fault::set_helper_log(Some(helper_log.clone()));
        crate::fault::arm_point(point, crate::fault::Fault::Enospc);
        let start = run.enable_workspace_custody(&plan);
        let run_id = run.id();
        let fired = crate::fault::fired();
        crate::fault::arm(None);
        let before_retry = run
            .partial_sentinel
            .as_ref()
            .map(|partial| partial.child_id());
        let retry = run.enable_workspace_custody(&plan);
        let after_retry = run
            .partial_sentinel
            .as_ref()
            .map(|partial| partial.child_id());
        crate::fault::set_helper_log(None);
        let settlement =
            run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
        let unsettled = settlement.is_err();
        let retained = settlement.as_ref().err().and_then(|(_, owner)| {
            owner
                .partial_sentinel
                .as_ref()
                .map(|partial| (owner.id(), partial.child_id()))
        });
        let gate = Gate::probe(&cx.operation().observe().unwrap(), &root, &owner);
        let log = std::fs::read_to_string(helper_log).unwrap();
        let mut recorded = Vec::new();
        for line in log.lines() {
            if let Ok(child) = serde_json::from_str::<ProcessIdentity>(line) {
                recorded.push(child.pid);
                reap_test_child(&child);
            }
        }
        drop(settlement); // Fixture teardown, not a cleanup receipt.
        assert!(fired && start.is_err());
        assert_eq!(recorded.len(), 1);
        assert!(retry.is_err(), "unsettled startup must refuse retry");
        assert_eq!(before_retry, after_retry);
        assert_eq!(retained, Some((run_id, recorded[0])));
        assert!(matches!(
            gate,
            Observed::Verified(crate::gate::GateState::Held(_), _)
        ));
        assert!(
            unsettled,
            "a started sentinel with unknown custody must not settle cleanly"
        );
    }

    #[test]
    fn sentinel_start_failure_keeps_workspace_custody_unsettled() {
        for point in [
            crate::fault::Point::WorkspaceSentinelAfterSpawn,
            crate::fault::Point::WorkspaceSentinelPartialConfig,
            crate::fault::Point::WorkspaceSentinelAfterConfig,
            crate::fault::Point::WorkspaceSentinelAfterAck,
        ] {
            check_workspace_sentinel_start_fault(point);
        }
    }

    #[test]
    fn workspace_sentinel_cancellation_after_spawn_retains_partial() {
        let home = effigy_test_support::short_tempdir("sc-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
        let owner = ResourceId::environment(checkout, "sentinel-cancel-proof").unwrap();
        let mut fake = super::super::tests::Fake::new(home.path());
        let mut run = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let plan = LifecyclePlan {
            executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
                .unwrap(),
            placement: crate::supervise::Placement::Detached,
        };
        let helper_log = home.path().join("sentinel.jsonl");
        crate::fault::set_helper_log(Some(helper_log.clone()));
        let cancel = cx.operation().cancel_token().clone();
        crate::fault::hook(Some(Box::new(move |point| {
            if point == crate::fault::Point::WorkspaceSentinelAfterSpawn {
                cancel.cancel(crate::ctx::StopCause::Requested);
            }
        })));
        let result = run.enable_workspace_custody(&plan);
        crate::fault::hook(None);
        crate::fault::set_helper_log(None);
        let retained = run
            .partial_sentinel
            .as_ref()
            .map(|partial| partial.child_id());
        let cleanup =
            run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
        let logged: Vec<ProcessIdentity> = std::fs::read_to_string(helper_log)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        for child in &logged {
            reap_test_child(child);
        }
        drop(cleanup); // Exact-child fixture teardown only.
        assert!(result.is_err());
        assert_eq!(logged.len(), 1);
        assert_eq!(
            retained,
            Some(logged[0].pid),
            "cancellation must retain the started child"
        );
    }

    #[test]
    fn enabling_existing_custody_does_not_spawn_a_second_sentinel() {
        let home = effigy_test_support::short_tempdir("ld-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
        let owner = ResourceId::environment(checkout, "duplicate-sentinel-proof").unwrap();
        let mut fake = super::super::tests::Fake::new(home.path());
        let mut run = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let plan = LifecyclePlan {
            executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
                .unwrap(),
            placement: crate::supervise::Placement::Detached,
        };
        run.enable_custody(&plan).unwrap();
        let identity = run.sentinel.as_ref().unwrap().identity.clone();
        let helper_log = home.path().join("new-helpers.jsonl");
        crate::fault::set_helper_log(Some(helper_log.clone()));
        let retry = run.enable_custody(&plan);
        crate::fault::set_helper_log(None);
        let logged = match std::fs::read_to_string(helper_log) {
            Ok(contents) => Ok(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(error),
        };
        let same = run.sentinel.as_ref().map(|lease| lease.identity.clone());
        let cleanup =
            run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
        if let Ok(contents) = &logged {
            for line in contents.lines() {
                if let Ok(child) = serde_json::from_str::<ProcessIdentity>(line) {
                    reap_test_child(&child);
                }
            }
        }
        let logged = logged.expect("helper log must be absent or readable");
        assert!(retry
            .err()
            .is_some_and(|error| error.to_string().contains("sentinel already registered")));
        assert_eq!(same, Some(identity));
        assert!(
            logged.is_empty(),
            "duplicate enablement spawned a helper: {logged}"
        );
        assert!(cleanup.is_ok());
    }

    #[test]
    fn cancelled_registration_keeps_the_exact_sentinel_for_cleanup() {
        let home = effigy_test_support::short_tempdir("lc-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
        let owner = ResourceId::environment(checkout, "cancelled-sentinel-proof").unwrap();
        let mut fake = super::super::tests::Fake::new(home.path());
        let mut run = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let plan = LifecyclePlan {
            executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
                .unwrap(),
            placement: crate::supervise::Placement::Detached,
        };
        let lease =
            crate::supervise::spawn_sentinel(&run, &root, &plan, &super::super::tests::next())
                .unwrap();
        let identity = lease.identity.clone();
        cx.operation()
            .cancel_token()
            .cancel(crate::ctx::StopCause::Requested);
        let result = run.register_lifecycle(lease, &plan);
        let retained = run.sentinel.as_ref().map(|lease| lease.identity.clone());
        let cleanup =
            run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
        reap_test_child(&identity);
        assert!(result.is_err());
        assert_eq!(
            retained,
            Some(identity),
            "cancelled registration must retain its exact lease"
        );
        assert!(cleanup.is_ok(), "fresh cleanup must settle the exact owner");
    }

    #[test]
    fn failed_registration_and_close_keep_the_exact_sentinel_lease() {
        let home = effigy_test_support::short_tempdir("ls-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
        let owner = ResourceId::environment(checkout, "workspace-sentinel-proof").unwrap();
        let mut fake = super::super::tests::Fake::new(home.path());
        let mut run = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let plan = LifecyclePlan {
            executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
                .unwrap(),
            placement: crate::supervise::Placement::Detached,
        };
        let lease =
            crate::supervise::spawn_sentinel(&run, &root, &plan, &super::super::tests::next())
                .unwrap();
        let identity = lease.identity.clone();
        let before_count = Arc::new(AtomicUsize::new(0));
        let close_count = Arc::new(AtomicUsize::new(0));
        let before_seen = Arc::clone(&before_count);
        let close_seen = Arc::clone(&close_count);
        crate::fault::hook(Some(Box::new(move |point| {
            if point == crate::fault::Point::LifecycleBefore {
                before_seen.fetch_add(1, Ordering::SeqCst);
            }
            if point == crate::fault::Point::SentinelClose {
                close_seen.fetch_add(1, Ordering::SeqCst);
                crate::fault::arm_point(
                    crate::fault::Point::SentinelClose,
                    crate::fault::Fault::Eperm,
                );
            }
        })));
        crate::fault::arm_point(
            crate::fault::Point::LifecycleBefore,
            crate::fault::Fault::Enospc,
        );
        let result = run.register_lifecycle(lease, &plan);
        let close_fired = crate::fault::fired();
        crate::fault::hook(None);
        crate::fault::arm(None);
        let retained = run.sentinel.as_ref().map(|lease| lease.identity.clone());
        let cleanup =
            run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
        // The failing-first implementation can lose the Child handle. Reap
        // only this test's exact recorded child before making assertions.
        reap_test_child(&identity);
        assert!(result.is_err());
        assert_eq!(before_count.load(Ordering::SeqCst), 1);
        assert_eq!(close_count.load(Ordering::SeqCst), 1);
        assert!(close_fired, "sentinel close fault must fire");
        assert_eq!(
            retained,
            Some(identity),
            "the failed close must retain its exact lease"
        );
        assert!(
            cleanup.is_ok(),
            "fixture cleanup must settle the exact owner"
        );
    }

    fn check_backup_start_fault(point: crate::fault::Point) {
        let home = effigy_test_support::short_tempdir("lb-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
        let owner = ResourceId::environment(checkout, "workspace-backup-proof").unwrap();
        let mut fake = super::super::tests::Fake::new(home.path());
        let mut run = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner.clone()),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let plan = LifecyclePlan {
            executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
                .unwrap(),
            placement: crate::supervise::Placement::Detached,
        };
        let starts = Arc::new(AtomicUsize::new(0));
        let observed_starts = Arc::clone(&starts);
        crate::fault::hook(Some(Box::new(move |observed| {
            if observed == crate::fault::Point::WorkspaceBackupAfterSpawn {
                observed_starts.fetch_add(1, Ordering::SeqCst);
            }
        })));
        crate::fault::arm_point(point, crate::fault::Fault::Enospc);
        let enabled = run.enable_custody(&plan);
        let fired = crate::fault::fired();
        crate::fault::arm(None);
        let sentinel = run.sentinel.as_ref().map(|lease| lease.identity.clone());
        let partial = run.has_partial_checkout_backup();
        let backup_identity = run.partial_checkout_backup_identity();
        let effect_refused = run.can_effect().is_err();
        let custody_refusal = run.verify_custody().err().map(|error| error.to_string());
        let retry = run.enable_custody(&plan);
        let same_sentinel = run.sentinel.as_ref().map(|lease| lease.identity.clone());
        let same_backup = run.partial_checkout_backup_identity();
        let process_effects = run
            .effects()
            .unwrap()
            .iter()
            .filter(|effect| effect.spec.kind == crate::custody::KIND)
            .count();
        let gate = Gate::probe(&cx.operation().observe().unwrap(), &root, &owner);
        let start_count = starts.load(Ordering::SeqCst);
        crate::fault::hook(None);
        let cleanup =
            run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
        let cleanup_failed = cleanup.is_err();
        assert!(enabled.is_err());
        assert!(fired, "the backup startup fault must fire: {point:?}");
        assert!(sentinel.is_some());
        assert!(partial, "the exact partial backup must remain reachable");
        assert!(
            backup_identity.is_some(),
            "the partial helper identity must be recorded"
        );
        assert!(effect_refused, "partial startup must not allow effects");
        assert!(
            custody_refusal
                .as_deref()
                .is_some_and(|error| error.contains("checkout backup startup remains unsettled")),
            "partial backup must independently refuse custody verification: {custody_refusal:?}"
        );
        assert!(
            retry.is_err(),
            "partial startup must not launch a second helper"
        );
        assert_eq!(same_sentinel, sentinel);
        assert_eq!(same_backup, backup_identity);
        assert_eq!(start_count, 1, "retry must not cross the backup spawn hook");
        assert_eq!(process_effects, 0, "no process effect is journalled");
        assert!(matches!(
            gate,
            Observed::Verified(crate::gate::GateState::Held(_), _)
        ));
        if let Err((error, owner)) = cleanup {
            assert!(
                error
                    .to_string()
                    .contains("checkout backup cleanup remains unsettled"),
                "partial cleanup must report a named unresolved outcome: {error}"
            );
            assert!(
                owner.has_partial_checkout_backup(),
                "unknown cleanup must retain the same partial backup"
            );
            assert_eq!(owner.partial_checkout_backup_identity(), backup_identity);
            assert_eq!(
                owner.sentinel.as_ref().map(|lease| lease.identity.clone()),
                sentinel,
                "unknown cleanup must retain the same sentinel lease"
            );
            // Fixture teardown only: this is not a production settlement proof.
            drop(owner);
        }
        if point == crate::fault::Point::WorkspaceBackupAfterSpawn {
            assert!(
                cleanup_failed,
                "the pre-config child must exercise unresolved cleanup"
            );
        }
    }

    #[test]
    fn backup_start_fault_retains_partial_owner_with_same_sentinel() {
        for point in [
            crate::fault::Point::WorkspaceBackupAfterSpawn,
            crate::fault::Point::WorkspaceBackupAfterConfig,
            crate::fault::Point::WorkspaceBackupAfterAck,
        ] {
            check_backup_start_fault(point);
        }
    }

    #[test]
    fn after_commit_fault_preserves_same_env_sentinel_for_readback() {
        let home = effigy_test_support::short_tempdir("lf-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
        let owner = ResourceId::environment(checkout, "workspace-proof").unwrap();
        let mut fake = super::super::tests::Fake::new(home.path());
        let mut run = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let plan = LifecyclePlan {
            executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
                .unwrap(),
            placement: crate::supervise::Placement::Detached,
        };
        crate::fault::arm_point(
            crate::fault::Point::LifecycleAfter,
            crate::fault::Fault::Enospc,
        );
        let enabled = run.enable_custody(&plan);
        let fired = crate::fault::fired();
        crate::fault::arm(None);
        let exact = run.sentinel.as_ref().map(|lease| lease.identity.clone());
        let readback = run.inspect_workspace_lifecycle();
        let saved_sentinel: Option<String> = run
            .store
            .conn()
            .query_row(
                "SELECT sentinel_identity FROM runs WHERE run_id=?1",
                [run.id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        run.store
            .conn()
            .execute(
                "UPDATE runs SET sentinel_identity=NULL WHERE run_id=?1",
                [run.id.to_string()],
            )
            .unwrap();
        let mismatch = run.inspect_workspace_lifecycle();
        run.store
            .conn()
            .execute(
                "UPDATE runs SET sentinel_identity=?2 WHERE run_id=?1",
                rusqlite::params![run.id.to_string(), saved_sentinel],
            )
            .unwrap();
        run.store
            .conn()
            .execute(
                "UPDATE runs SET generation=generation+1 WHERE run_id=?1",
                [run.id.to_string()],
            )
            .unwrap();
        let wrong_generation = run.inspect_workspace_lifecycle();
        run.store
            .conn()
            .execute(
                "UPDATE runs SET generation=generation-1 WHERE run_id=?1",
                [run.id.to_string()],
            )
            .unwrap();
        let retained = run.sentinel.as_ref().map(|lease| lease.identity.clone());
        let poisoned = run.can_effect().is_err();
        let cleanup =
            run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
        assert!(enabled.is_err());
        assert!(fired, "the post-commit lifecycle fault must fire");
        assert!(exact.is_some());
        assert_eq!(retained, exact);
        assert!(mismatch.is_err(), "missing sentinel identity must refuse");
        assert!(
            wrong_generation.is_err(),
            "run generation mismatch must refuse"
        );
        assert!(poisoned, "readback alone must not restore effect authority");
        assert!(cleanup.is_ok(), "the same Env Run must remain recoverable");
        assert!(
            readback.is_ok(),
            "same-lease lifecycle readback must be available"
        );
    }
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
