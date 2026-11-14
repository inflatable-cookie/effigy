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
        let cx = kernel::effects();
        let checkout = checkout::read(&request.target.root)
            .map_err(|error| start_error(&error.to_string()))?
            .map(Ok)
            .unwrap_or_else(|| StateRoot::filesystem_checkout(&kernel::observe(), &location))
            .map_err(|error| start_error(&error.to_string()))?;
        let root = StateRoot::foreground_effects(&cx, &location)
            .map_err(|error| start_error(&error.to_string()))?;
        // Strict persistence (effigy#985): a v2 admission settles one slot in
        // the fixed terminal ring, so the ring must exist and have room before
        // the hook runs. Refusing here writes no gate and no byte of any file.
        super::super::admission::require_strict_terminal_ring(&plan, &root)?;
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
        };
        effigy_core::output_cap::foreground_started();
        // The deadline's own evidence: its clock, origin and phase budgets.
        // Off by default, so an invocation without the opt-in writes nothing.
        if let Some(deadline) = effigy_core::run_deadline::current() {
            let _ = effigy_core::output_cap::write(
                effigy_core::output_cap::Stream::Stderr,
                deadline.describe().as_bytes(),
            );
        }
        run.admit(request.admission, plan, request.cwd)?;
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
        // Under admission protocol v2 every workload group is reported — and
        // acknowledged — between its recorded receipt and its `go`. A refused
        // report leaves the child unexecuted and fails the run closed.
        // A helper process (the admission hook itself, a built-in's session) is
        // not a workload group; only a workload spawn is reported.
        let mut gate = if auxiliary {
            None
        } else {
            super::super::admission::WorkloadGate::for_spawn(self.admission.as_ref(), &self.target)
        };
        let process: Result<_, SpawnError> = match &self.memory {
            Some(memory) => {
                let mut memory = memory
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match gate.as_mut() {
                    Some(gate) => {
                        Custody::spawn_foreground_gated(&mut memory, spec, &cx, gate, handoff)
                    }
                    None => Custody::spawn_foreground_with_handoff(
                        &mut memory,
                        spec,
                        &cx,
                        !auxiliary,
                        handoff,
                    ),
                }
            }
            None => {
                Custody::spawn_with_handoff(&mut *self.run_mut()?, spec, &cx, !auxiliary, handoff)
            }
        };
        if let Some(error) = gate.as_mut().and_then(|gate| gate.take_failure()) {
            return Err(error);
        }
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
        })
    }
}

pub(super) fn close_memory(
    mut run: ForegroundRun,
    workload: WorkloadOutcome,
    cleanup: &effigy_kernel::ctx::CleanupContext,
    report: &mut dyn FnMut(CleanupOutcome),
) -> (
    Option<effigy_kernel::outcome::Outcome<effigy_kernel::run::Settled>>,
    Vec<String>,
) {
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
    match run.settle_reporting(cleanup, report) {
        Ok(outcome) => {
            effigy_core::output_cap::foreground_settled(
                recorded_ok
                    && outcome.cleanup() == effigy_kernel::outcome::CleanupOutcome::Complete,
            );
            if outcome.cleanup() == CleanupOutcome::Incomplete {
                warnings
                    .push("foreground cleanup is incomplete; remaining evidence is residue".into());
            }
            (Some(outcome), warnings)
        }
        Err(error) => {
            effigy_core::output_cap::foreground_settled(false);
            warnings.push(format!("foreground settle: {error}"));
            (None, warnings)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::task_engine::{run_plain, PlainCommand, TaskRunRequest};
    use std::path::Path;
    use std::time::{Duration, Instant};

    /// Installs the thread's deterministic deadline and clears it even when a
    /// test fails.
    struct InjectedDeadline;
    impl InjectedDeadline {
        fn at(origin: Instant, ms: u64) -> Self {
            let deadline = effigy_core::run_deadline::RunDeadline::from_origin(origin, ms)
                .expect("injected deadline");
            effigy_core::run_deadline::inject_for_tests(Some(deadline));
            Self
        }
    }
    impl Drop for InjectedDeadline {
        fn drop(&mut self) {
            effigy_core::run_deadline::clear_injected_for_tests();
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

    fn begin(target: &TaskTarget) -> TaskRun {
        TaskRun::begin_foreground(TaskRunRequest {
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
        })
        .expect("foreground admitted")
    }

    #[test]
    fn the_run_deadline_times_out_the_workload_and_settles_inside_the_ceiling() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path(), "sleepy");
        let mut run = begin(&target);
        let origin = Instant::now();
        let _deadline = InjectedDeadline::at(origin, 12_000);
        assert!(
            effigy_core::run_deadline::current().is_some(),
            "injection visible"
        );
        let budget = effigy_core::run_deadline::workload_remaining().expect("configured");
        assert!(
            budget > Duration::from_millis(500) && budget <= Duration::from_secs(2),
            "workload budget is the ceiling less the allowance: {budget:?}"
        );
        assert!(crate::runner::task_engine::remaining_lifetime().is_some());
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
        let elapsed = started.elapsed();
        assert_eq!(
            exit.workload,
            WorkloadOutcome::TimedOut {
                phase: "sleepy".into()
            }
        );
        assert_eq!(exit.code, 124);
        assert!(
            elapsed < Duration::from_millis(12_000),
            "the workload outlived the deadline: {elapsed:?}"
        );
        let _ = effigy_core::run_deadline::take_settlement_truncated();
        let warnings = run.close(exit.workload);
        assert!(
            !effigy_core::run_deadline::take_settlement_truncated(),
            "settlement fit inside the reserved allowance: {warnings:?}"
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(Instant::now() < origin + Duration::from_millis(12_000));
    }

    #[test]
    fn a_settlement_cut_by_the_deadline_is_not_success() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path(), "cut");
        let run = begin(&target);
        // A ceiling that has already elapsed: settlement has no allowance.
        let _deadline = InjectedDeadline::at(Instant::now() - Duration::from_secs(20), 10_000);
        let _ = effigy_core::run_deadline::take_settlement_truncated();
        let warnings = run.close(WorkloadOutcome::Succeeded);
        assert!(
            effigy_core::run_deadline::take_settlement_truncated(),
            "an expired settlement must be recorded: {warnings:?}"
        );
        assert!(
            !warnings.is_empty(),
            "an incomplete settlement reports its residue"
        );
    }

    #[test]
    fn a_spawned_workload_is_not_started_when_the_deadline_is_spent() {
        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path(), "spent");
        let mut run = begin(&target);
        let _deadline = InjectedDeadline::at(Instant::now() - Duration::from_secs(20), 10_000);
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
        let _ = effigy_core::run_deadline::take_settlement_truncated();
        let _ = run.close(exit.workload);
    }
}
