//! Memory-only task admission and explicit parent-class inheritance.
use super::*;
use effigy_kernel::custody::{Custody, Exit, OwnedProcess, ProcessSpec, SpawnError};
use effigy_kernel::inheritance::{MemoryLease, RunClass, RuntimeParent};
use effigy_kernel::migration::CheckoutLocation;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{ForegroundRun, LegacyForeground};

pub(in crate::runner::task_engine) fn inherited_class() -> Result<Option<RuntimeParent>, RunnerError>
{
    RuntimeParent::inherited(kernel::effects().operation())
        .map_err(|error| class_error(&error.to_string()))
}
fn class_error(reason: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "parent run capability refused: {reason}; nothing was started"
    ))
}

pub(super) fn require_durable_parent() -> Result<(), RunnerError> {
    if PARENTS.with(|parents| parents.borrow().last().is_some_and(|p| p.memory.is_some()))
        || inherited_class()?.is_some_and(|grant| grant.class == RunClass::Foreground)
    {
        return Err(class_error(
            "a foreground parent cannot acquire late durable capabilities",
        ));
    }
    Ok(())
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
        let inherited = inherited_class()?;
        if parent.as_ref().is_some_and(|parent| parent.run.is_some())
            || inherited.is_some_and(|grant| grant.class != RunClass::Foreground)
        {
            return Self::begin_with(request, task_gate, LifecyclePlacement::Inline);
        }
        if request.key.is_some() || request.managed_run_token.is_some() {
            return Err(class_error(
                "named and managed runs require durable admission",
            ));
        }
        let plan = super::super::admission::plan(request.admission)?;
        // Hook custody is connected to the same memory record below; until
        // admitted it cannot grant a child permission to bypass admission.
        let location = CheckoutLocation {
            root: request.target.root.clone(),
            git_directory: effigy_core::worktree_scope::git_directory(&request.target.root)
                .map_err(|error| class_error(&error.to_string()))?,
        };
        let cx = kernel::effects();
        let checkout = checkout::read(&request.target.root)
            .map_err(|error| class_error(&error.to_string()))?
            .map(Ok)
            .unwrap_or_else(|| StateRoot::filesystem_checkout(&kernel::observe(), &location))
            .map_err(|error| class_error(&error.to_string()))?;
        let root = StateRoot::foreground_effects(&cx, &location)
            .map_err(|error| class_error(&error.to_string()))?;
        let owner = if task_gate {
            request.target.owner(checkout)?
        } else {
            request
                .target
                .shared_lock_owner(checkout)?
                .ok_or_else(|| class_error("standalone foreground owner needs its private gate"))?
        };
        let task = request.target.task_id(checkout)?;
        let ancestors = parent
            .as_ref()
            .map(|parent| parent.ancestors.clone())
            .unwrap_or_default();
        if ancestors.contains(&owner) {
            return Err(class_error("recursive foreground task reference"));
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
                    .map_err(|error| class_error(&error.to_string()))?;
            }
        }
        kernel::install_router()?;
        let custody_scope = kernel::CustodyWait::enter();
        let started = Instant::now();
        let opts = BeginOpts {
            id: new_run_id()?,
            wait: if parent.is_some() || request.lock_wait_ms == 0 {
                Wait::FailFast
            } else {
                Wait::Until(started + Duration::from_millis(request.lock_wait_ms))
            },
            acknowledge: Vec::new(),
            next: kernel::next_invocation(&request.target.label)?,
            custody: Some(LifecyclePlan {
                executable: kernel::executable()?,
                placement: LifecyclePlacement::Inline,
            }),
        };
        let aliases = if task_gate {
            checkout::identities(&kernel::observe(), &request.target.root)
                .map_err(|error| class_error(&error.to_string()))?
        } else {
            // Fresh private builtin owners cannot have an old incarnation.
            Vec::new()
        };
        let home = kernel::state_home();
        let legacy = LegacyForeground::for_checkout(
            &kernel::observe(),
            &root,
            home.as_deref(),
            &invocation,
            &aliases,
        )
        .map_err(|error| begin_error(error, request.target, request.lock_wait_ms, started))?;
        let memory = match parent.and_then(|parent| parent.memory) {
            Some(parent) => parent
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .begin_child_with_legacy(&root, &invocation, opts, legacy.as_ref()),
            None => {
                ForegroundRun::begin_with_legacy(&cx, &root, &invocation, opts, legacy.as_ref())
            }
        }
        .map_err(|error| begin_error(error, request.target, request.lock_wait_ms, started))?;
        let mut run = Self {
            run: None,
            memory: Some(Arc::new(Mutex::new(memory))),
            lease: None,
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
        run.admit(request.admission, plan, request.cwd)?;
        Ok(run)
    }

    pub(in crate::runner) fn spawn(
        &mut self,
        spec: ProcessSpec,
        auxiliary: bool,
    ) -> Result<OwnedProcess, RunnerError> {
        self.verify_admission()?;
        let cx = kernel::effects();
        if self.lease.is_none() {
            let closure = self
                .memory
                .as_ref()
                .map(|memory| {
                    memory
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .closure_identity()
                })
                .transpose()
                .map_err(|error| class_error(&error.to_string()))?;
            self.lease = Some(
                MemoryLease::new(
                    &cx,
                    &RuntimeParent::new(
                        self.id(),
                        if self.memory.is_some() {
                            RunClass::Foreground
                        } else {
                            RunClass::CheckoutDurable
                        },
                        self.admission
                            .as_ref()
                            .map(|admission| admission.identity())
                            .or(super::super::admission::current_hook()?),
                    )
                    .with_closure(closure),
                )
                .map_err(|error| class_error(&error.to_string()))?,
            );
        }
        let capability = self
            .lease
            .as_ref()
            .expect("lease constructed above")
            .capability();
        let process: Result<_, SpawnError> = match &self.memory {
            Some(memory) => Custody::spawn_foreground_with_lease(
                &mut memory
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                spec,
                &cx,
                &capability,
                !auxiliary,
            ),
            None => {
                Custody::spawn_with_lease(&mut *self.run_mut()?, spec, &cx, &capability, !auxiliary)
            }
        };
        process.map_err(|error| {
            RunnerError::task_invocation(format!("process custody refused: {error:?}"))
        })
    }

    pub(in crate::runner) fn verify_admission(&mut self) -> Result<(), RunnerError> {
        let binding = match &self.memory {
            Some(memory) => memory
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .verify_custody(),
            None => self.run_mut()?.verify_custody(),
        };
        binding.map_err(|error| {
            RunnerError::task_invocation(format!("run filesystem authority changed: {error}"))
        })?;
        super::super::admission::verify_held()?;
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
            return Err(class_error("memory parent is not checkout-local"));
        };
        let location = CheckoutLocation {
            root: self.target.root.clone(),
            git_directory: effigy_core::worktree_scope::git_directory(&self.target.root)
                .map_err(|error| class_error(&error.to_string()))?,
        };
        let root = StateRoot::foreground_effects(&kernel::effects(), &location)
            .map_err(|error| class_error(&error.to_string()))?;
        let step_owner = ResourceId::store(
            effigy_kernel::id::Scope::Checkout(checkout),
            &format!("task-step:{}:{index}", owner.name()),
        )
        .map_err(|error| class_error(&error.to_string()))?;
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
            .map_err(|error| class_error(&error.to_string()))?;
        let mut target = self.target.clone();
        target.label = label.into();
        let mut ancestors = self.ancestors.clone();
        ancestors.push(owner);
        Ok(Self {
            run: None,
            memory: Some(Arc::new(Mutex::new(memory))),
            lease: None,
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
    let mut warnings = recorded
        .err()
        .map(|error| format!("foreground outcome: {error}"))
        .into_iter()
        .collect::<Vec<_>>();
    match run.settle(cleanup) {
        Ok(outcome) => {
            if outcome.cleanup() == CleanupOutcome::Incomplete {
                warnings
                    .push("foreground cleanup is incomplete; remaining evidence is residue".into());
            }
            (Some(outcome), warnings)
        }
        Err(error) => {
            warnings.push(format!("foreground settle: {error}"));
            (None, warnings)
        }
    }
}
