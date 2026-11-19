//! Memory-only task admission for a foreground tree.
use super::*;
use effigy_kernel::custody::{Custody, Exit, OwnedProcess, ProcessSpec, SpawnError};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::ForegroundRun;

/// This foreground run could not start; the evidence is the cause.
fn start_error(reason: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "this task run could not start ({reason}); nothing was started"
    ))
}

impl TaskRun {
    /// The caller classified the declared tree before the first effect.
    /// Children of a durable run remain on its checkout journal.
    pub(in crate::runner) fn begin_foreground(
        request: TaskRunRequest<'_>,
    ) -> Result<Self, RunnerError> {
        Self::begin_foreground_with(request, true)
    }

    pub(super) fn begin_foreground_with(
        request: TaskRunRequest<'_>,
        task_gate: bool,
    ) -> Result<Self, RunnerError> {
        let phase_scope = effigy_core::run_deadline::begin_run();
        let parent = PARENTS.with(|parents| parents.borrow().last().cloned());
        if parent.as_ref().is_some_and(|parent| parent.run.is_some()) {
            return Self::begin_with(request, task_gate, LifecyclePlacement::Inline);
        }
        if request.key.is_some() || request.managed_run_token.is_some() {
            return Err(start_error(
                "a named or managed run needs a journal, and this tree keeps its record in memory",
            ));
        }
        let plan = super::super::admission::plan(request.admission)?;
        // Hook custody is connected to the same memory record below; until
        // admitted it cannot grant a child permission to bypass admission.
        let location = kernel::checkout_location(&request.target.root);
        kernel::prepare_state_owner(&location).map_err(|error| start_error(&error.to_string()))?;
        let cx = kernel::effects();
        let checkout = checkout::read(&request.target.root)
            .map_err(|error| start_error(&error.to_string()))?
            .map(Ok)
            .unwrap_or_else(|| StateRoot::filesystem_checkout(&kernel::observe(), &location))
            .map_err(|error| start_error(&error.to_string()))?;
        let root = StateRoot::foreground_effects(&cx, &location)
            .map_err(|error| start_error(&error.to_string()))?;
        let owner = if task_gate {
            request.target.owner(checkout)?
        } else {
            request
                .target
                .shared_lock_owner(checkout)?
                .ok_or_else(|| start_error("standalone foreground owner needs its private gate"))?
        };
        let task = request.target.task_id(checkout)?;
        let ancestors = parent
            .as_ref()
            .map(|parent| parent.ancestors.clone())
            .unwrap_or_default();
        if ancestors.contains(&owner) {
            return Err(start_error("recursive foreground task reference"));
        }
        let mut shared_owners = parent
            .as_ref()
            .map(|parent| parent.shared_owners.clone())
            .unwrap_or_default();
        let argv = std::iter::once(request.target.label.clone())
            .chain(request.args.iter().cloned())
            .map(OsString::from)
            .collect();
        let mut invocation = invocation(&task, owner.clone(), argv, &BTreeMap::new())?;
        if let Some(shared) = request.target.shared_lock_owner(checkout)? {
            if !shared_owners.contains(&shared) {
                shared_owners.push(shared.clone());
                let mut spec = invocation.spec().clone();
                spec.gates.push(shared);
                invocation = ResolvedInvocation::new(spec)
                    .map_err(|error| start_error(&error.to_string()))?;
            }
        }
        kernel::install_router()?;
        let custody_scope = kernel::CustodyWait::enter();
        let started = Instant::now();
        // The absolute run deadline also bounds the one setup wait a caller
        // can ask for: the task gate. An explicit `--lock-wait` may shorten it,
        // never extend it.
        let lock_wait_until = started + Duration::from_millis(request.lock_wait_ms);
        let lock_wait_until = match effigy_core::run_deadline::workload_ceiling() {
            Some(at) => lock_wait_until.min(at),
            None => lock_wait_until,
        };
        let opts = BeginOpts {
            id: new_run_id()?,
            wait: if parent.is_some() || request.lock_wait_ms == 0 {
                Wait::FailFast
            } else {
                Wait::Until(lock_wait_until)
            },
            acknowledge: Vec::new(),
            next: kernel::next_invocation(&request.target.label)?,
            custody: Some(LifecyclePlan {
                executable: kernel::executable()?,
                placement: LifecyclePlacement::Inline,
            }),
        };
        let memory = match parent.and_then(|parent| parent.memory) {
            Some(parent) => parent
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .begin_child(&root, &invocation, opts),
            None => ForegroundRun::begin(&cx, &root, &invocation, opts),
        }
        .map_err(|error| begin_error(error, request.target, request.lock_wait_ms, started))?;
        let mut run = Self {
            run: None,
            memory: Some(Arc::new(Mutex::new(memory))),
            shared: None,
            target: request.target.clone(),
            ancestors,
            shared_owners,
            _custody_scope: Some(custody_scope),
            admission: None,
            scratch: None,
            programs: None,
            keyed: false,
            _phase_scope: Some(phase_scope),
        };
        effigy_core::output_cap::foreground_started();
        // The deadline's own evidence: its clock, origin and phase budgets.
        // Off by default, so an invocation without the opt-in writes nothing.
        effigy_core::run_deadline::note_phase(effigy_core::run_deadline::Phase::Admission);
        if let Some(deadline) = effigy_core::run_deadline::current() {
            // Name the guard too: a guard that cannot prove closure parks as
            // the verified exclusion holder, and an owner-side caller needs
            // its recorded pid to stop it after the assertions.
            let mut line = deadline.describe();
            if let Some(guard) = run.guard_identity() {
                if line.ends_with('\n') {
                    line.pop();
                }
                line.push_str(&format!(", guard pid {}\n", guard.pid));
            }
            let _ = effigy_core::output_cap::write(
                effigy_core::output_cap::Stream::Stderr,
                line.as_bytes(),
            );
        }
        let admitted = run.admit(request.admission, plan, request.cwd);
        effigy_core::run_deadline::note_phase(effigy_core::run_deadline::Phase::Admission);
        admitted?;
        Ok(run)
    }

    pub(in crate::runner) fn spawn(
        &mut self,
        spec: ProcessSpec,
        auxiliary: bool,
    ) -> Result<OwnedProcess, RunnerError> {
        self.spawn_with_handoff(spec, auxiliary, None)
    }

    /// A spawn that hands its child one short-lived reader at the kernel's
    /// [`effigy_kernel::custody::HANDOFF_FD`] slot. Effigy uses it for its own
    /// child processes only: a workload never receives one.
    pub(in crate::runner) fn spawn_with_handoff(
        &mut self,
        spec: ProcessSpec,
        auxiliary: bool,
        handoff: Option<std::os::fd::OwnedFd>,
    ) -> Result<OwnedProcess, RunnerError> {
        self.verify_admission()?;
        let cx = kernel::effects();
        let process: Result<_, SpawnError> = match &self.memory {
            Some(memory) => {
                let mut memory = memory
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                Custody::spawn_foreground_with_handoff(&mut memory, spec, &cx, !auxiliary, handoff)
            }
            None => {
                Custody::spawn_with_handoff(&mut *self.run_mut()?, spec, &cx, !auxiliary, handoff)
            }
        };
        process.map_err(|error| {
            RunnerError::task_invocation(format!("process custody refused: {error}"))
        })
    }

    pub(in crate::runner::task_engine) fn admission_monitor(
        &self,
    ) -> Option<super::super::admission::AdmissionMonitor> {
        self.admission.as_ref().map(|admission| admission.monitor())
    }

    pub(in crate::runner) fn verify_admission(&mut self) -> Result<(), RunnerError> {
        if let Some(admission) = &mut self.admission {
            admission.verify()?;
        }
        Ok(())
    }

    pub(in crate::runner) fn complete(
        &mut self,
        process: &OwnedProcess,
        exit: Exit,
    ) -> Result<(), RunnerError> {
        let result = match &self.memory {
            Some(memory) => process.complete_foreground(
                &mut memory
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                exit,
            ),
            None => process.complete(&mut *self.run_mut()?, exit),
        };
        result.map_err(|error| RunnerError::task_invocation(format!("process completion: {error}")))
    }

    pub(super) fn owner(&self) -> ResourceId {
        match &self.memory {
            Some(memory) => memory
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record()
                .incarnation()
                .owner
                .clone(),
            None => self
                .run
                .as_ref()
                .expect("open run")
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .fence()
                .resource
                .clone(),
        }
    }

    pub(super) fn memory_step(&self, index: usize, label: &str) -> Result<Self, RunnerError> {
        let parent = self.memory.as_ref().expect("memory step selected");
        let owner = self.owner();
        let effigy_kernel::id::Scope::Checkout(checkout) = owner.scope() else {
            return Err(start_error("memory parent is not checkout-local"));
        };
        let location = kernel::checkout_location(&self.target.root);
        kernel::prepare_state_owner(&location).map_err(|error| start_error(&error.to_string()))?;
        let root = StateRoot::foreground_effects(&kernel::effects(), &location)
            .map_err(|error| start_error(&error.to_string()))?;
        let step_owner = ResourceId::store(
            effigy_kernel::id::Scope::Checkout(checkout),
            &format!("task-step:{}:{index}", owner.name()),
        )
        .map_err(|error| start_error(&error.to_string()))?;
        let inv = invocation(
            &self.target.task_id(checkout)?,
            step_owner,
            vec![OsString::from(label)],
            &BTreeMap::new(),
        )?;
        let memory = parent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .begin_child(
                &root,
                &inv,
                BeginOpts {
                    id: new_run_id()?,
                    wait: Wait::FailFast,
                    acknowledge: Vec::new(),
                    next: kernel::next_invocation(label)?,
                    custody: Some(LifecyclePlan {
                        executable: kernel::executable()?,
                        placement: LifecyclePlacement::Inline,
                    }),
                },
            )
            .map_err(|error| start_error(&error.to_string()))?;
        let mut target = self.target.clone();
        target.label = label.into();
        let mut ancestors = self.ancestors.clone();
        ancestors.push(owner);
        Ok(Self {
            run: None,
            memory: Some(Arc::new(Mutex::new(memory))),
            shared: None,
            target,
            ancestors,
            shared_owners: self.shared_owners.clone(),
            _custody_scope: Some(kernel::CustodyWait::enter()),
            admission: None,
            scratch: None,
            programs: None,
            keyed: false,
            _phase_scope: None,
        })
    }
}

pub(super) fn close_memory(
    mut run: ForegroundRun,
    workload: WorkloadOutcome,
    cleanup: &effigy_kernel::ctx::CleanupContext,
) -> (
    Option<effigy_kernel::outcome::Outcome<effigy_kernel::run::Settled>>,
    Vec<String>,
) {
    let retained_guard = run.guard_identity().cloned();
    let retained_effects = run.retained_effects();
    let recorded = match workload {
        WorkloadOutcome::Cancelled { .. }
        | WorkloadOutcome::TimedOut { .. }
        | WorkloadOutcome::NotStarted => run.interrupt(workload),
        other => run.finish(other),
    };
    let recorded_ok = recorded.is_ok();
    let mut warnings = recorded
        .err()
        .map(|error| format!("foreground outcome: {error}"))
        .into_iter()
        .collect::<Vec<_>>();
    match run.settle(cleanup) {
        Ok(outcome) => {
            let positive = recorded_ok
                && outcome.cleanup() == effigy_kernel::outcome::CleanupOutcome::Complete;
            effigy_core::output_cap::foreground_settled(positive);
            effigy_core::run_deadline::note_phase(effigy_core::run_deadline::Phase::Settlement);
            if !positive {
                // Under the opt-in an incomplete settlement is never success;
                // the marker makes the caller fail closed. The retained
                // resources are named by the outcome's own residue.
                if effigy_core::run_deadline::ceiling().is_some() {
                    effigy_core::run_deadline::note_settlement_incomplete();
                }
                let unresolved = outcome.unresolved();
                if unresolved.is_empty() {
                    warnings.push(
                        "foreground cleanup is incomplete; remaining evidence is residue".into(),
                    );
                } else {
                    warnings.push(format!(
                        "foreground cleanup is incomplete; retained: {}",
                        unresolved.join("; ")
                    ));
                }
            }
            (Some(outcome), warnings)
        }
        Err(error) => {
            effigy_core::output_cap::foreground_settled(false);
            effigy_core::run_deadline::note_phase(effigy_core::run_deadline::Phase::Settlement);
            if effigy_core::run_deadline::ceiling().is_some() {
                effigy_core::run_deadline::note_settlement_incomplete();
            }
            // The owner kept its handles and attempted positive settlement to
            // the ceiling; this is what remains, named truthfully. The guard is
            // only retired once its close succeeded; an error here leaves it to
            // its own owner-death settlement.
            let retained = retained_guard
                .map(|guard| format!("guard pid {}", guard.pid))
                .into_iter()
                .chain(retained_effects.iter().cloned())
                .collect::<Vec<_>>();
            if retained.is_empty() {
                warnings.push(format!("foreground settle: {error}"));
            } else {
                warnings.push(format!(
                    "foreground settle: {error}; retained: {}",
                    retained.join(", ")
                ));
            }
            (None, warnings)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::task_engine::{
        process_environment, run_plain, PlainCommand, TaskRunRequest,
    };
    use effigy_kernel::custody::{Lifetime, ProcessSpec, Terminal};
    use effigy_kernel::observe::Observed;
    use std::path::Path;
    use std::time::{Duration, Instant};

    /// Installs the deterministic deadline and clock, and clears both even
    /// when a test fails.
    struct InjectedDeadline;
    impl InjectedDeadline {
        fn at(origin: Instant, ms: u64) -> Self {
            let deadline = effigy_core::run_deadline::RunDeadline::from_origin(origin, ms)
                .expect("injected deadline");
            effigy_core::run_deadline::clear_clock_for_tests();
            effigy_core::run_deadline::clear_phases();
            effigy_core::run_deadline::inject_for_tests(Some(deadline));
            Self
        }
        fn advance(by: Duration) {
            effigy_core::run_deadline::advance_clock_for_tests(by);
        }
    }
    impl Drop for InjectedDeadline {
        fn drop(&mut self) {
            effigy_core::run_deadline::clear_injected_for_tests();
            effigy_core::run_deadline::clear_clock_for_tests();
            effigy_core::run_deadline::clear_phases();
        }
    }

    fn target(root: &Path, name: &str) -> TaskTarget {
        let catalog = effigy_manifest::LoadedCatalog::for_test(
            "root",
            root.to_path_buf(),
            toml::from_str("").expect("empty manifest"),
            0,
        );
        TaskTarget::for_catalog(root, name, &catalog)
    }

    fn alive(pid: u32) -> bool {
        matches!(
            effigy_kernel::ident::process_identity(pid),
            Observed::Verified(..)
        )
    }

    fn stop_group(pid: i32) {
        // SAFETY: kill the group this test started, by its recorded pid.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
            libc::kill(pid, libc::SIGKILL);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive(pid as u32) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn request(target: &TaskTarget) -> TaskRunRequest<'_> {
        TaskRunRequest {
            target,
            lock_wait_ms: 0,
            custody: false,
            scratch: false,
            managed_run_token: None,
            args: &[],
            key: None,
            admission: None,
            workspace_admission: None,
            cwd: None,
        }
    }

    fn begin(target: &TaskTarget) -> TaskRun {
        TaskRun::begin_foreground(request(target)).expect("foreground admitted")
    }

    #[test]
    fn the_clock_expires_the_workload_and_settlement_still_finishes() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path(), "sleepy");
        let mut run = begin(&target);
        let origin = Instant::now();
        let _deadline = InjectedDeadline::at(origin, 12_000);
        let budget = effigy_core::run_deadline::workload_remaining().expect("configured");
        assert!(
            budget > Duration::from_millis(500) && budget <= Duration::from_secs(2),
            "workload budget is the ceiling less the allowance: {budget:?}"
        );
        assert!(crate::runner::task_engine::remaining_lifetime().is_some());
        // The clock, not a wall-clock sleep, ends the workload: a helper
        // advances it past the workload boundary while the owner polls.
        let advance = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(80));
            InjectedDeadline::advance(Duration::from_secs(3));
        });
        let _ = effigy_core::run_deadline::take_settlement_incomplete();
        let started = Instant::now();
        let exit = run_plain(
            &mut run,
            PlainCommand {
                command: "sleep 60",
                cwd: root.path(),
                env: Vec::new(),
                capture: true,
                timeout_ms: None,
            },
        )
        .expect("the workload ran");
        advance.join().expect("clock helper");
        let elapsed = started.elapsed();
        assert_eq!(
            exit.workload,
            WorkloadOutcome::TimedOut {
                phase: "sleepy".into()
            }
        );
        assert_eq!(exit.code, 124);
        assert!(
            elapsed < Duration::from_secs(5),
            "the injected clock, not a real wait, ended the workload: {elapsed:?}"
        );
        let warnings = run.close(exit.workload);
        assert!(
            !effigy_core::run_deadline::take_settlement_incomplete(),
            "settlement fit inside the reserved allowance: {warnings:?}"
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(effigy_core::output_cap::foreground_complete());
    }

    #[test]
    fn a_settlement_after_the_ceiling_is_not_success() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path(), "cut");
        let run = begin(&target);
        // The clock advances past the whole ceiling: settlement has no
        // allowance and cannot claim closure.
        let _deadline = InjectedDeadline::at(Instant::now(), 12_000);
        InjectedDeadline::advance(Duration::from_secs(13));
        let _ = effigy_core::run_deadline::take_settlement_incomplete();
        let warnings = run.close(WorkloadOutcome::Succeeded);
        assert!(
            effigy_core::run_deadline::take_settlement_incomplete(),
            "an expired settlement must be recorded: {warnings:?}"
        );
        assert!(
            !warnings.is_empty(),
            "an incomplete settlement reports its residue"
        );
        assert!(
            !effigy_core::output_cap::foreground_complete(),
            "an incomplete settlement suppresses the completeness footer"
        );
    }

    #[test]
    fn expiry_before_any_effect_starts_no_workload() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path(), "spent");
        let mut run = begin(&target);
        let _deadline = InjectedDeadline::at(Instant::now(), 12_000);
        InjectedDeadline::advance(Duration::from_secs(3));
        assert_eq!(
            crate::runner::task_engine::remaining_lifetime(),
            Some(Duration::ZERO)
        );
        let exit = run_plain(
            &mut run,
            PlainCommand {
                command: "touch must-not-exist",
                cwd: root.path(),
                env: Vec::new(),
                capture: true,
                timeout_ms: None,
            },
        )
        .expect("no workload ran");
        assert_eq!(exit.code, 124);
        assert!(!root.path().join("must-not-exist").exists());
        let _ = effigy_core::run_deadline::take_settlement_incomplete();
        let _ = run.close(exit.workload);
    }

    /// An effect that settlement can classify only as residue: its receipt
    /// names an invalid group. The settle then returns a normal
    /// `Ok(incomplete)` outcome, which the opt-in must still fail closed.
    #[test]
    fn an_incomplete_settlement_fails_closed_and_names_retained_resources() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path(), "residue");
        let run = begin(&target);
        let identity = match effigy_kernel::ident::process_identity(std::process::id()) {
            Observed::Verified(identity, _) => identity,
            _ => panic!("the test process identity is not verified"),
        };
        let memory = run.memory.as_ref().expect("foreground record").clone();
        {
            let mut record = memory
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let fence = record.record().fence().clone();
            let ticket = record
                .record_mut()
                .intend_process(effigy_kernel::run::EffectSpec {
                    target: fence,
                    // The kernel's local process effect kind, checked by
                    // `intend_process`.
                    kind: "kernel:process".into(),
                    payload: serde_json::Value::Null,
                    settlement: effigy_kernel::run::Settlement::Release(serde_json::Value::Null),
                    required: true,
                })
                .expect("intent");
            record
                .record_mut()
                .applied(
                    &ticket,
                    serde_json::json!({
                        "identity": identity,
                        "pgid": 0u32,
                        "nonce": "residue-proof",
                        "grace_millis": 1u64,
                    }),
                )
                .expect("receipt");
        }
        drop(memory);

        let _deadline = InjectedDeadline::at(Instant::now(), 12_000);
        let _ = effigy_core::run_deadline::take_settlement_incomplete();
        let warnings = run.close(WorkloadOutcome::Succeeded);
        assert!(
            effigy_core::run_deadline::take_settlement_incomplete(),
            "an Ok(incomplete) settle must fail closed under the deadline: {warnings:?}"
        );
        let rendered = warnings.join(" ");
        assert!(
            rendered.contains("invalid group identity"),
            "the unresolved effect is named: {rendered}"
        );
        assert!(
            !effigy_core::output_cap::foreground_complete(),
            "an incomplete settlement suppresses the completeness footer"
        );
    }

    #[test]
    fn a_ceiling_already_spent_refuses_the_run_before_any_effect() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path(), "spent");
        let _deadline = InjectedDeadline::at(Instant::now(), 12_000);
        InjectedDeadline::advance(Duration::from_secs(13));
        let error = TaskRun::begin_foreground(TaskRunRequest {
            target: &target,
            lock_wait_ms: 0,
            custody: false,
            scratch: false,
            managed_run_token: None,
            args: &[],
            key: None,
            admission: None,
            workspace_admission: None,
            cwd: None,
        })
        .expect_err("a spent ceiling starts nothing");
        assert!(
            error.to_string().contains("deadline") || error.to_string().contains("timed out"),
            "{error}"
        );
        assert!(!root.path().join("must-not-exist").exists());
    }

    #[test]
    fn an_expired_settlement_leaves_the_live_group_and_guard_to_the_custodian() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path(), "live");
        let mut run = begin(&target);
        let guard = run
            .memory
            .as_ref()
            .expect("foreground record")
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .guard_identity()
            .cloned()
            .expect("guard identity");
        let _deadline = InjectedDeadline::at(Instant::now(), 12_000);
        let spec = ProcessSpec {
            argv: vec!["/bin/sh".into(), "-c".into(), "sleep 60".into()],
            cwd: root.path().to_path_buf(),
            env: process_environment(root.path(), Vec::new()).expect("environment"),
            grace: Duration::from_secs(3),
            lifetime: Lifetime::Open(effigy_kernel::ctx::OpenReason::Shell),
            terminal: Terminal::None,
            label: "live".into(),
        };
        let process = run.spawn(spec, false).expect("workload spawned");
        let leader = process.identity().identity.pid as i32;
        drop(process);
        // The clock, not the workload, crosses the ceiling: settlement has no
        // budget and must leave the live group and the guard intact.
        InjectedDeadline::advance(Duration::from_secs(13));
        let _ = effigy_core::run_deadline::take_settlement_incomplete();
        let warnings = run.close(WorkloadOutcome::Succeeded);
        assert!(
            effigy_core::run_deadline::take_settlement_incomplete(),
            "an expired settle must be recorded: {warnings:?}"
        );
        assert!(
            alive(leader as u32),
            "the owner must not kill a group it left to the custodian"
        );
        assert!(
            alive(guard.pid),
            "the guard must survive the owner's incomplete settle"
        );
        let rendered = warnings.join(" ");
        assert!(
            rendered.contains(&format!("guard pid {}", guard.pid)),
            "the retained guard is named: {rendered}"
        );
        assert!(
            rendered.contains("kernel:process"),
            "the retained group is named: {rendered}"
        );
        stop_group(leader);
    }

    #[test]
    fn the_no_option_path_keeps_the_previous_unresolved_child_behaviour() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let parent_target = target(root.path(), "parent");
        let child_target = target(root.path(), "child");
        let run = |deadline: bool| {
            let parent = begin(&parent_target);
            let parent_scope = parent.enter();
            let child = begin(&child_target);
            drop(parent_scope);
            if deadline {
                let _deadline = InjectedDeadline::at(Instant::now(), 30_000);
                let _ = effigy_core::run_deadline::take_settlement_incomplete();
                let (outcome, warnings) =
                    parent.close_settled(WorkloadOutcome::Succeeded, Duration::from_secs(60));
                let incomplete = effigy_core::run_deadline::take_settlement_incomplete();
                drop(child);
                (outcome, warnings, incomplete)
            } else {
                let _ = effigy_core::run_deadline::take_settlement_incomplete();
                let (outcome, warnings) =
                    parent.close_settled(WorkloadOutcome::Succeeded, Duration::from_secs(60));
                let incomplete = effigy_core::run_deadline::take_settlement_incomplete();
                drop(child);
                (outcome, warnings, incomplete)
            }
        };
        let (off_outcome, off_warnings, off_incomplete) = run(false);
        assert!(!off_incomplete, "the no-option path has no marker");
        assert_eq!(
            off_outcome.as_ref().map(|outcome| outcome.cleanup()),
            Some(CleanupOutcome::Complete),
            "the no-option path keeps the previous unresolved-child behaviour: {off_warnings:?}"
        );
        let (on_outcome, on_warnings, on_incomplete) = run(true);
        assert!(
            on_incomplete,
            "the opt-in turns an unresolved child into fail-closed residue: {on_warnings:?}"
        );
        assert_eq!(
            on_outcome.as_ref().map(|outcome| outcome.cleanup()),
            Some(CleanupOutcome::Incomplete)
        );
        let rendered = on_warnings.join(" ");
        assert!(
            rendered.contains("in-process child"),
            "the unresolved child is named: {rendered}"
        );
    }

    #[test]
    fn a_nested_runs_completed_frame_is_its_own() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let parent_target = target(root.path(), "parent");
        let child_target = target(root.path(), "child");
        let _deadline = InjectedDeadline::at(Instant::now(), 30_000);
        let parent = begin(&parent_target);
        let parent_handle = parent.deadline_handle().expect("parent frame");
        let child = {
            let _parent_scope = parent.enter();
            begin(&child_target)
        };
        let child_handle = child.deadline_handle().expect("child frame");
        let (_, _) = child.close_settled(WorkloadOutcome::Succeeded, Duration::from_secs(60));
        child_handle.note(effigy_core::run_deadline::Phase::Terminal);
        let child_evidence = child_handle.evidence();
        assert!(
            child_evidence["settlement"].is_object(),
            "the child's own settlement is in its frame: {child_evidence}"
        );
        assert!(
            child_evidence["terminal"].is_object(),
            "the child's terminal evidence is in its frame: {child_evidence}"
        );
        let parent_evidence = parent_handle.evidence();
        assert!(
            !parent_evidence["terminal"].is_object(),
            "the child's terminal evidence never lands in the parent: {parent_evidence}"
        );
        let (_, _) = parent.close_settled(WorkloadOutcome::Succeeded, Duration::from_secs(60));
    }

    #[test]
    fn the_feature_off_path_is_byte_identical_and_writes_no_evidence() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path(), "same");
        let run_once = || {
            let mut run = begin(&target);
            let exit = run_plain(
                &mut run,
                PlainCommand {
                    command: "printf same",
                    cwd: root.path(),
                    env: Vec::new(),
                    capture: true,
                    timeout_ms: None,
                },
            )
            .expect("the workload ran");
            assert_eq!(exit.code, 0);
            let _ = run.close(exit.workload);
            exit.stdout
        };
        assert!(effigy_core::run_deadline::current().is_none());
        assert!(effigy_core::run_deadline::evidence().is_none());
        let off = run_once();
        assert_eq!(off, b"same");
        {
            let _deadline = InjectedDeadline::at(Instant::now(), 12_000);
            let on = run_once();
            assert_eq!(
                on, off,
                "the opt-in changes budgets, never the workload's own bytes"
            );
        }
        assert!(effigy_core::run_deadline::current().is_none());
        assert!(effigy_core::run_deadline::evidence().is_none());
    }
}
