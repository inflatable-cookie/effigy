//! Bounded gateway startup: one admitted intent set, one receipt publication.
//! Bootstrap helpers and blocked children never run the gateway before both
//! commits. No transaction spans a spawn, socket bind or identity observation.
use super::*;
use crate::ident::ProcessIdentity;
use crate::supervise::Placement;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use super::provenance::{self, OperatorAction};

/// Log one operator event to the existing gateway log. Logging is best effort
/// so an unavailable log never prevents settlement, and the record is the same
/// shape for `up`, `down` and `setup`: the invoking pid, redacted argv, cwd,
/// cause and the bounded caller provenance behind them.
pub fn record_gateway_operator(directory: &std::path::Path, action: OperatorAction, cause: &str) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    // The event appends to the log the gateway already keeps. Recording never
    // creates the directory, so an `up` or `setup` before any gateway state
    // exists stays read-only and a diagnostic adds no state tree of its own.
    if !directory.is_dir() {
        return;
    }
    let pid = std::process::id();
    let caller = provenance::caller(
        &provenance::Platform,
        pid,
        crate::ident::controlling_terminal(),
        |name: &str| std::env::var_os(name),
    );
    let argv = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let cwd = std::env::current_dir().ok();
    let line = provenance::event(action, cause, pid, &argv, cwd.as_deref(), &caller);
    if let Ok(mut log) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        // An operator log is private to its owner, in the shared gateway state
        // directory and in a private root the fixture policy validates.
        .mode(0o600)
        .open(directory.join("gateway.stderr.log"))
    {
        if let Err(error) = writeln!(log, "{line}") {
            eprintln!("[gateway] cannot log operator event: {error}");
        }
    } else {
        eprintln!("[gateway] operator event log unavailable: {line}");
    }
}

/// The stop event keeps its name and fields; it additionally names its caller.
pub fn record_gateway_stop(directory: &std::path::Path, cause: &str) {
    record_gateway_operator(directory, OperatorAction::Down, cause)
}

/// One shared owner's current, positively closed operation. This replaces
/// completed machine run history; it retains one bounded diagnostic only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineCompletion {
    pub run: RunId,
    pub generation: Generation,
    pub workload: WorkloadOutcome,
    pub stderr_tail: String,
}
/// Pure projection of the gateway's current completion fact.
pub fn machine_completion(
    reader: &crate::store::StoreReader,
    owner: &ResourceId,
) -> crate::observe::Observed<MachineCompletion> {
    use crate::observe::{AbsenceProof, Observed, UnknownReason};
    if owner.kind() != crate::id::ResourceKind::Gateway {
        return Observed::Unknown(UnknownReason::Inconsistent(
            "completion is only a gateway control fact".into(),
        ));
    }
    let resource = match reader.resource(owner) {
        Observed::Verified(resource, _) => resource,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    {
        let Some(state) = resource.state else {
            return Observed::Absent(AbsenceProof::NoRow {
                table: "gateway completion".into(),
            });
        };
        match serde_json::from_str::<MachineCompletion>(&state) {
            Ok(completion)
                if completion.generation == resource.generation
                    && completion.stderr_tail.len() <= 16384 =>
            {
                Observed::Verified(completion, super::settle::evidence())
            }
            _ => Observed::Unknown(UnknownReason::Inconsistent(
                "gateway completion generation or record is invalid".into(),
            )),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Startup {
    claimed: BTreeSet<i64>,
    receipts: BTreeMap<i64, Value>,
    pub lifecycle: Option<(ProcessIdentity, ProcessIdentity, Placement)>,
    pub active: bool,
}
impl<'a> Run<'a> {
    /// Only the resolved machine gateway's single-process topology has a
    /// complete static intent set. General engines keep incremental journalling.
    pub(crate) fn begin_gateway(
        cx: &EffectContext,
        root: &'a StateRoot,
        invocation: &ResolvedInvocation,
        options: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
        process: &crate::custody::ProcessSpec,
    ) -> Result<Self, RunError> {
        if !root.is_machine()
            || invocation.spec().owner.kind() != crate::id::ResourceKind::Gateway
            || options
                .custody
                .as_ref()
                .is_none_or(|plan| plan.placement != Placement::Detached)
        {
            return Err(RunError::refused(
                "planned startup requires the detached machine gateway",
            ));
        }
        let planned = crate::custody::process_intent(
            process,
            Fence {
                resource: invocation.spec().owner.clone(),
                generation: Generation::new(0),
            },
            true,
        )
        .map_err(|error| {
            RunError::refused(format!("the gateway process plan is unusable ({error})"))
        })?;
        Self::begin_under(
            cx,
            root,
            invocation,
            options,
            || Ok(cleanup.clone()),
            adapter,
            None,
            Some(planned),
        )
    }
    pub(crate) fn machine_lifecycle(&self) -> bool {
        self.machine_startup.is_some()
    }
    pub(super) fn planned_intent(&mut self, spec: &EffectSpec) -> Result<EffectTicket, RunError> {
        let encoded = serde_json::to_value(spec)?;
        for effect in self.effects()? {
            let startup = self
                .machine_startup
                .as_mut()
                .expect("planned gateway startup");
            if serde_json::to_value(&effect.spec)? == encoded
                && startup.claimed.insert(effect.sequence)
            {
                return Ok(EffectTicket {
                    run: self.id,
                    owner: self.owner.clone(),
                    sequence: effect.sequence,
                });
            }
        }
        Err(RunError::broken(
            "effect was not in the admitted gateway plan",
        ))
    }
    pub(super) fn planned_receipt(
        &mut self,
        ticket: &EffectTicket,
        receipt: Value,
    ) -> Result<(), RunError> {
        self.can_effect()?;
        if ticket.run != self.id || ticket.owner != self.owner {
            return Err(RunError::broken("foreign gateway receipt"));
        }
        let effect = journal::effect(self.store.conn(), self.id, ticket.sequence)?;
        let startup = self
            .machine_startup
            .as_mut()
            .expect("planned gateway startup");
        if !startup.claimed.contains(&ticket.sequence)
            || startup.receipts.contains_key(&ticket.sequence)
        {
            return Err(RunError::broken("unclaimed or duplicate gateway receipt"));
        }
        startup.receipts.insert(ticket.sequence, receipt);
        if effect.spec.kind == socket::KIND {
            return Ok(());
        }
        let effects = journal::effects(self.store.conn(), self.id)?;
        if effect.spec.kind != crate::custody::KIND
            || effects.len() != 2
            || !effects
                .iter()
                .all(|effect| startup.receipts.contains_key(&effect.sequence))
        {
            self.poisoned = true;
            return Err(RunError::broken("gateway receipt set is incomplete"));
        }
        let Some((owner, sentinel, placement)) = startup.lifecycle.as_ref() else {
            return Err(RunError::broken("gateway sentinel registration missing"));
        };
        let result = self.store.transaction(&self.cx.child(Duration::from_secs(30)), |txn| {
            journal::check_fence(txn.conn(), &self.owner)?;
            crate::fault_point!(LifecycleBefore)?;
            txn.conn().execute("UPDATE runs SET kind='custody',placement=?2,owner_identity=?3,supervisor_identity=?3,sentinel_identity=?4 WHERE run_id=?1",
                params![self.id.to_string(), placement.name(), serde_json::to_string(owner)?, serde_json::to_string(sentinel)?])?;
            crate::fault_point!(LifecycleCommit)?;
            for effect in &effects {
                journal::check_target(txn.conn(), effect)?;
                crate::fault_point!(AppliedBefore)?;
                journal::write_effect(txn, self.id, effect.sequence, EffectState::Applied,
                    startup.receipts.get(&effect.sequence), None)?;
            }
            crate::fault_point!(PhaseBefore)?;
            txn.conn().execute("UPDATE runs SET phase='active',updated_at=?2 WHERE run_id=?1", params![self.id.to_string(), txn.now()])?;
            Ok::<_,RunError>(())
        });
        if result.is_err() {
            self.poisoned = true;
        }
        result?;
        startup.active = true;
        crate::fault_point!(LifecycleAfter)?;
        crate::fault_point!(AppliedAfter)?;
        crate::fault_point!(PhaseAfter)?;
        Ok(())
    }
}

#[cfg(all(test, feature = "fault-injection"))]
mod tests {
    use super::*;
    use crate::custody::{Custody, EnvValue, Lifetime, ProcessSpec, Terminal};
    use crate::fault::{self, Fault, Point};
    use crate::run::tests::{cleanup, context, id, next, Fake};
    fn inv(owner: ResourceId) -> ResolvedInvocation {
        let mut spec = crate::run::tests::inv(owner).spec().clone();
        spec.task = crate::id::TaskId::new(
            crate::id::CheckoutId::new(crate::id::CheckoutKind::Machine, [0; 16]),
            "",
            crate::id::Surface::Published,
            "gateway",
            None,
        )
        .unwrap();
        ResolvedInvocation::new(spec).unwrap()
    }
    use crate::supervise::{Executable, LifecyclePlan};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn executable() -> Executable {
        Executable::capture(&crate::fixture::custody_fixture()).unwrap()
    }
    fn process(home: &std::path::Path) -> ProcessSpec {
        ProcessSpec {
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf ran > ran; exec sleep 30".into(),
            ],
            cwd: home.into(),
            env: BTreeMap::from([("PATH".into(), EnvValue::Plain("/usr/bin:/bin".into()))]),
            grace: Duration::from_millis(20),
            lifetime: Lifetime::Finite(Duration::from_secs(60)),
            terminal: Terminal::None,
            label: "gateway fixture".into(),
        }
    }
    fn options() -> BeginOpts {
        BeginOpts {
            id: id(1),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable: executable(),
                placement: Placement::Detached,
            }),
        }
    }
    #[test]
    fn gateway_startup_commits_two_intents_then_all_receipts_before_go() {
        let fixture = effigy_test_support::Fixture::new("kernel-gateway-budget-").unwrap();
        let cx = context();
        let root = StateRoot::runtime_machine_effects(&cx, fixture.path()).unwrap();
        drop(Store::open(&cx, &root).unwrap()); // Initialization has its own budget.
        let count = Arc::new(AtomicUsize::new(0));
        let go = Arc::new(AtomicUsize::new(0));
        let commits = count.clone();
        let before_go = go.clone();
        let observed_root = root.clone();
        let observe = cx.operation().observe().unwrap();
        fault::hook(Some(Box::new(move |point| {
            if point == Point::StoreCommitAfter {
                commits.fetch_add(1, Ordering::SeqCst);
            }
            if point == Point::SpawnBeforeGo {
                assert_eq!(commits.load(Ordering::SeqCst), 2);
                let reader = Store::read(&observe, &observed_root).verified().unwrap().conn().query_row(
                    "SELECT phase,owner_identity IS NOT NULL,sentinel_identity IS NOT NULL,(SELECT count(*) FROM events WHERE state='applied') FROM runs", [],
                    |row| Ok((row.get::<_,String>(0)?,row.get::<_,bool>(1)?,row.get::<_,bool>(2)?,row.get::<_,i64>(3)?))).unwrap();
                assert_eq!(reader, ("active".into(), true, true, 2));
                before_go.fetch_add(1, Ordering::SeqCst);
            }
        })));
        let mut adapter = Fake::new(fixture.path());
        let invocation = inv(ResourceId::gateway("operator").unwrap());
        let mut run = Run::begin_gateway(
            &cx,
            &root,
            &invocation,
            options(),
            &cleanup(&cx),
            &mut adapter,
            &process(fixture.path()),
        )
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(run.effects().unwrap().len(), 2);
        assert!(run
            .effects()
            .unwrap()
            .iter()
            .all(|effect| effect.state == EffectState::Intended));
        let listener = run.control_socket().unwrap();
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "socket receipt waits for the complete set"
        );
        let child = Custody::spawn(&mut run, process(fixture.path()), &cx).unwrap();
        run.advance(Phase::Active).unwrap();
        request(
            &cx,
            &root,
            &Request {
                id: id(2),
                run: run.id(),
                expected: run.fence().clone(),
                action: RequestAction::Detach,
            },
        )
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 3);
        assert_eq!(go.load(Ordering::SeqCst), 1);
        let before_stop = count.load(Ordering::SeqCst);
        request(
            &cx,
            &root,
            &Request {
                id: id(3),
                run: run.id(),
                expected: run.fence().clone(),
                action: RequestAction::Stop,
            },
        )
        .unwrap();
        drop(listener);
        run.interrupt(
            &cleanup(&cx),
            WorkloadOutcome::Cancelled {
                signal: None,
                cause: crate::ctx::StopCause::Requested,
            },
        )
        .unwrap();
        assert!(run
            .settle(&cleanup(&cx), &mut adapter)
            .unwrap()
            .result()
            .is_ok());
        assert_eq!(
            count.load(Ordering::SeqCst) - before_stop,
            3,
            "stop request, fenced interruption and positive close"
        );
        fault::hook(None);
        let reader = Store::read(&cx.operation().observe().unwrap(), &root).verified().unwrap().conn().query_row(
            "SELECT (SELECT count(*) FROM runs),(SELECT count(*) FROM events),(SELECT count(*) FROM requests)", [],
            |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?))).unwrap();
        assert_eq!(reader, (0, 0, 0));
        drop(child);
        assert!(
            !root.path().join("runs").join(id(1).to_string()).exists(),
            "machine process output stays bounded in memory"
        );
    }
    #[test]
    fn gateway_receipt_commit_refusal_never_releases_the_process_barrier() {
        for failure in [Fault::Enospc, Fault::Eperm] {
            let fixture = effigy_test_support::Fixture::new("kernel-gateway-refusal-").unwrap();
            let cx = context();
            let root = StateRoot::runtime_machine_effects(&cx, fixture.path()).unwrap();
            let mut adapter = Fake::new(fixture.path());
            let invocation = inv(ResourceId::gateway("operator").unwrap());
            let mut run = Run::begin_gateway(
                &cx,
                &root,
                &invocation,
                options(),
                &cleanup(&cx),
                &mut adapter,
                &process(fixture.path()),
            )
            .unwrap();
            let listener = run.control_socket().unwrap();
            fault::arm_point(Point::StoreCommitBefore, failure);
            assert!(Custody::spawn(&mut run, process(fixture.path()), &cx).is_err());
            fault::arm(None);
            assert!(!fixture.path().join("ran").exists());
            let effects = run.effects().unwrap();
            assert!(effects
                .iter()
                .all(|effect| effect.state == EffectState::Intended));
            drop(listener);
            assert!(run
                .settle(&cleanup(&cx), &mut adapter)
                .unwrap()
                .result()
                .is_ok());
        }
    }
    #[test]
    fn incomplete_machine_cleanup_keeps_its_journal_and_never_publishes_completion() {
        use crate::observe::{Observed, UnknownReason};
        #[derive(Debug)]
        struct Unknown;
        impl SettlementAdapter for Unknown {
            fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
                Observed::Unknown(UnknownReason::Injected)
            }
            fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
                panic!("unknown grants no release")
            }
            fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
                panic!("unknown grants no reconcile")
            }
        }
        let fixture = effigy_test_support::Fixture::new("kernel-machine-residue-").unwrap();
        let cx = context();
        let root = StateRoot::runtime_machine_effects(&cx, fixture.path()).unwrap();
        let owner = ResourceId::gateway("operator").unwrap();
        let mut opts = options();
        opts.custody = None;
        let mut run = Run::begin(
            &cx,
            &root,
            &inv(owner.clone()),
            opts,
            &cleanup(&cx),
            &mut Unknown,
        )
        .unwrap();
        run.intend(EffectSpec {
            target: run.fence().clone(),
            kind: "gateway:unknown".into(),
            payload: Value::Null,
            settlement: Settlement::Release(Value::Null),
            required: false,
        })
        .unwrap();
        assert!(run
            .settle(&cleanup(&cx), &mut Unknown)
            .unwrap()
            .result()
            .is_err());
        let observe = cx.operation().observe().unwrap();
        let read = Store::read(&observe, &root);
        let reader = read.verified().unwrap();
        assert!(matches!(
            machine_completion(reader, &owner),
            Observed::Absent(_)
        ));
        assert_eq!(reader.conn().query_row("SELECT count(*) FROM runs WHERE phase='closed' AND cleanup_outcome='incomplete'", [], |row| row.get::<_,i64>(0)).unwrap(), 1);
        assert_eq!(
            reader
                .conn()
                .query_row(
                    "SELECT count(*) FROM events WHERE state='residue'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }
}
