//! Reserved workspace activation. The owning session is admitted and retained
//! before Env admission; no publication callback can reverse Task/Env order.

use effigy_kernel::gate::Wait;
use effigy_kernel::id::ResolvedInvocation;
use effigy_kernel::run::{BeginOpts, Run};
use effigy_kernel::workspace_session::{
    ActivationDecision, PublishInputs, SessionRef, SessionSupervisor,
    WorkspaceActivationReservation,
};

use super::{EnvContext, EnvError, EnvRef, EnvRun, EnvSettlement, Operation};

#[derive(Debug)]
pub enum WorkspaceBegin<'r> {
    /// Exactly the current selection: no new run, effect or generation.
    Attach(SessionRef),
    Activate(Box<WorkspaceActivation<'r>>),
}

/// Field order releases Env before Task if a crash/unwind interrupts the owner.
/// Such a drop is interrupted evidence, never successful settlement.
#[derive(Debug)]
pub struct WorkspaceActivation<'r> {
    environment: EnvRun<'r>,
    supervisor: SessionSupervisor<'r>,
    session: SessionRef,
    reservation: WorkspaceActivationReservation,
    selected: bool,
    committed: Option<effigy_kernel::workspace_session::SelectionObligation>,
    supervisor_id: String,
}

impl<'r> WorkspaceActivation<'r> {
    /// Decide and freeze one exact session binding with Env free. The kernel's
    /// exclusive staging and private Task slot serialize concurrent successors;
    /// a loser cannot replace the staged run or publish another session's S.
    pub fn begin(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        inputs: &PublishInputs,
        invocation: &ResolvedInvocation,
        options: BeginOpts,
        supervisor_id: &str,
    ) -> Result<WorkspaceBegin<'r>, EnvError> {
        if supervisor_id.trim().is_empty() || supervisor_id.contains('\0') {
            return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "workspace supervisor identity is empty or malformed",
            )));
        }
        let decision = WorkspaceActivationReservation::decide(
            &ctx.cx,
            ctx.root,
            &env.resource,
            inputs,
            options.wait,
        )
        .map_err(EnvError::Kernel)?;
        Self::begin_decided(ctx, env, decision, invocation, options, supervisor_id)
    }

    /// Observe decision inputs under the kernel's environment gate before
    /// admitting the session. The same absolute deadline bounds the observation.
    pub fn begin_observing(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        invocation: &ResolvedInvocation,
        options: BeginOpts,
        supervisor_id: &str,
        deadline: std::time::Instant,
        observe: impl FnOnce(std::time::Instant) -> Result<PublishInputs, effigy_kernel::run::RunError>
            + Send
            + 'static,
    ) -> Result<WorkspaceBegin<'r>, EnvError> {
        if supervisor_id.trim().is_empty() || supervisor_id.contains('\0') {
            return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "workspace supervisor identity is empty or malformed",
            )));
        }
        let bounded = ctx.bounded_until(deadline)?;
        let decision = WorkspaceActivationReservation::decide_observing(
            &bounded.cx,
            bounded.root,
            &env.resource,
            options.wait,
            deadline,
            observe,
        )
        .map_err(EnvError::Kernel)?;
        Self::begin_decided(&bounded, env, decision, invocation, options, supervisor_id)
    }

    fn begin_decided(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        decision: ActivationDecision,
        invocation: &ResolvedInvocation,
        options: BeginOpts,
        supervisor_id: &str,
    ) -> Result<WorkspaceBegin<'r>, EnvError> {
        let wait = options.wait;
        let reservation = match decision {
            ActivationDecision::Attach(session) => return Ok(WorkspaceBegin::Attach(session)),
            ActivationDecision::Activate(reservation) => *reservation,
        };
        reservation
            .stage_session_binding(&ctx.cx, ctx.root, &invocation.spec().owner, options.id)
            .map_err(EnvError::Kernel)?;
        let cleanup = ctx.cleanup()?;
        let (session_run, session) = SessionRef::admit_reserved(
            &ctx.cx,
            ctx.root,
            invocation,
            options,
            &cleanup,
            &mut EnvSettlement,
            &reservation,
            effigy_kernel::supervise::Placement::Detached,
        )
        .map_err(EnvError::Kernel)?;
        // Custody is registered before listener construction. The consumer
        // receives this already-adopted supervisor and never re-adopts it.
        let supervisor =
            SessionSupervisor::adopt(session_run, supervisor_id).map_err(EnvError::Kernel)?;
        // Retain this exact owning Task Run while acquiring the claimed Env.
        let environment = match EnvRun::begin_claimed(ctx, env, Operation::Up, wait, &reservation) {
            Ok(environment) => environment,
            Err(error) => {
                let mut failures = Vec::new();
                let settled = ctx.cleanup().and_then(|cleanup| {
                    supervisor
                        .settle(&cleanup, &mut EnvSettlement)
                        .map_err(EnvError::Kernel)
                });
                if let Err(error) = settled {
                    failures.push(error);
                }
                if let Err(error) = reservation.cancel(&ctx.cx, ctx.root, wait) {
                    failures.push(EnvError::Kernel(error));
                }
                return Err(if failures.is_empty() {
                    error
                } else {
                    EnvError::Cleanup {
                        cause: Box::new(error),
                        failures,
                    }
                });
            }
        };
        Ok(WorkspaceBegin::Activate(Box::new(Self {
            environment,
            supervisor,
            session,
            reservation,
            selected: false,
            committed: None,
            supervisor_id: supervisor_id.to_owned(),
        })))
    }

    /// Apply under the same claimed Env owner used for target binding and
    /// publication. Existing live holders preserve the backend and selection.
    /// The returned receipt names exactly the containers this activation
    /// created, and nothing it attached (effigy#510).
    pub fn apply_with(
        &mut self,
        request: super::Up<'_>,
        apply: impl FnOnce() -> Result<(), super::BackendError>,
    ) -> Result<super::BringUp, EnvError> {
        super::ops::reconcile_in(&mut self.environment, request, None, apply, |run| {
            run.holders()
        })
    }

    pub fn apply_sealed_with(
        &mut self,
        request: super::Up<'_>,
        sealed: &effigy_kernel::objects::SealedPreflight,
        preflight: &super::CreationPreflight,
        apply: impl FnOnce() -> Result<(), super::BackendError>,
    ) -> Result<super::BringUp, EnvError> {
        super::ops::reconcile_in(
            &mut self.environment,
            request,
            Some((sealed, preflight)),
            apply,
            |run| run.holders(),
        )
    }

    pub fn supervisor_id(&self) -> &str {
        &self.supervisor_id
    }

    pub fn session(&self) -> &SessionRef {
        &self.session
    }
    pub fn reservation(&self) -> &WorkspaceActivationReservation {
        &self.reservation
    }
    pub fn environment_mut(&mut self) -> &mut EnvRun<'r> {
        &mut self.environment
    }

    /// A listener factory must borrow this exact run, never recreate an owner
    /// from SessionRef. It registers custody before launching its bound key.
    pub fn session_run_mut(&mut self) -> &mut Run<'r> {
        &mut self.supervisor
    }

    pub fn current_plan(
        &self,
    ) -> Result<effigy_kernel::workspace_session::ActivationPlan, EnvError> {
        self.environment.workspace_plan(&self.reservation)
    }

    /// Atomic commit retains both exact owning runs and the qualified launch.
    /// Returning its durable obligation grants no claim that selectors moved.
    pub fn commit(
        &mut self,
        receipt: &effigy_kernel::workspace_session::SelectionReceipt,
        launch: &effigy_kernel::workspace_session::SessionLaunch,
    ) -> Result<effigy_kernel::workspace_session::CommittedActivation, EnvError> {
        let committed = self.environment.commit_workspace_selection(
            &self.reservation,
            &self.supervisor,
            receipt,
            launch,
        )?;
        self.committed = committed.selection_obligation().cloned();
        Ok(committed)
    }

    /// Only actual target readback completes the committed obligation. The
    /// guest adapter must compare-and-swap selectors before calling this.
    pub fn selected(
        &mut self,
        obligation: &effigy_kernel::workspace_session::SelectionObligation,
        launch: &effigy_kernel::workspace_session::SessionLaunch,
        reader: &mut dyn effigy_kernel::workspace_session::SelectorReader,
    ) -> Result<(), EnvError> {
        self.environment
            .workspace_selection_switched(obligation, launch, reader)?;
        self.selected = true;
        Ok(())
    }

    /// Ends Env first, then hands the still-owning session to the supervisor.
    /// A pending obligation cannot be advertised as a completed activation.
    pub fn finish_selected(self) -> Result<(SessionSupervisor<'r>, SessionRef), EnvError> {
        if !self.selected {
            return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "workspace selection has not been verified",
            )));
        }
        self.environment.finish(true)?;
        Ok((self.supervisor, self.session))
    }

    /// An interrupted activation can hand its still-owned supervisor to the
    /// recovery driver without claiming successful selection. Env settles first;
    /// the durable Pending obligation continues to block successors.
    pub fn interrupt_for_recovery(self) -> Result<(SessionSupervisor<'r>, SessionRef), EnvError> {
        if self.committed.is_none() || self.selected {
            return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "only an unfinished committed activation enters selection recovery",
            )));
        }
        self.environment.finish(false)?;
        Ok((self.supervisor, self.session))
    }

    /// Pre-commit failure: settle both bound runs before cancelling. A committed
    /// generation instead remains fenced by its durable selection obligation;
    /// its owner must call selection recovery/abandon, never restore a predecessor.
    pub fn abort(self, ctx: &EnvContext<'_>, wait: Wait) -> Result<(), EnvError> {
        self.abort_with_target(
            ctx,
            wait,
            effigy_kernel::observe::Observed::Unknown(
                effigy_kernel::observe::UnknownReason::GuardUnavailable,
            ),
        )
    }

    /// Stop/settle the exact successor before revoking a committed failed
    /// selection. Unknown target evidence keeps it Pending for recovery; no
    /// selector is restored and no predecessor authority is recreated.
    pub fn abort_with_target(
        self,
        ctx: &EnvContext<'_>,
        wait: Wait,
        target: effigy_kernel::observe::Observed<()>,
    ) -> Result<(), EnvError> {
        let environment = self.environment.finish(false);
        let session = ctx.cleanup().and_then(|cleanup| {
            self.supervisor
                .settle(&cleanup, &mut EnvSettlement)
                .map_err(EnvError::Kernel)
        });
        environment?;
        session?;
        if let Some(obligation) = self.committed {
            use effigy_kernel::observe::{AbsenceProof, Observed};
            let closed = || Observed::Absent(AbsenceProof::GateFreeRunClosed);
            effigy_kernel::workspace_session::selection_abandoned(
                &ctx.cx,
                ctx.root,
                &obligation,
                &effigy_kernel::workspace_session::SelectionLiveness {
                    session: closed(),
                    target,
                    listener: closed(),
                },
                "owning publication failed and its exact supervisor settled",
                wait,
            )
            .map_err(EnvError::Kernel)
        } else {
            self.reservation
                .cancel(&ctx.cx, ctx.root, wait)
                .map(|_| ())
                .map_err(EnvError::Kernel)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{checkout, env, home, root};
    use super::*;
    use effigy_kernel::ctx::{CancelToken, EffectClass, Placement};
    use effigy_kernel::id::{
        Digest, ExecutionTarget, InvocationSpec, Route, RunId, Surface, TaskId,
    };
    use effigy_kernel::workspace_session::{
        ArtifactEvidence, GuestContextInputs, GuestPaths, TargetEvidence,
    };
    use std::collections::BTreeMap;

    fn invocation() -> ResolvedInvocation {
        let task =
            TaskId::new(checkout(), "", Surface::Published, "workspace", None).expect("task");
        ResolvedInvocation::new(InvocationSpec {
            owner: task.resource(),
            task,
            route: Route::Task,
            argv: vec![],
            cwd: String::new(),
            target: ExecutionTarget::Host,
            env: BTreeMap::new(),
            gates: vec![],
            attachments: vec![],
            class: EffectClass::Mutate,
            placement: Placement::Host,
            provenance: BTreeMap::new(),
        })
        .expect("invocation")
    }
    fn inputs() -> PublishInputs {
        PublishInputs {
            artifact: ArtifactEvidence::Unselected {
                image_reference: "fixture".into(),
                backend: "docker".into(),
            },
            selection: Digest::of(b"selection"),
            inputs_digest: Digest::of(b"inputs"),
            target: TargetEvidence::Absent {
                project: "fixture".into(),
                backend: "docker".into(),
            },
            platform: None,
            guest: Some(GuestContextInputs {
                version: "0.15.0".into(),
                protocol: 1,
                host_user: "1000".into(),
                checkout_id: checkout().text(),
                environment_resource: env("workspace").resource.name().to_string(),
                profile: "fixture".into(),
                runtime: None,
                vm_type: None,
                routes: vec![],
                paths: GuestPaths {
                    host_checkout: "/host".into(),
                    guest_checkout: "/workspace".into(),
                    workdir: "/workspace".into(),
                    user: "1000".into(),
                    home: "/home/worker".into(),
                },
                sources: vec![],
            }),
        }
    }
    fn options(id: u8) -> BeginOpts {
        BeginOpts {
            id: RunId::new(1_760_000_000_000, [id; 10]).expect("run"),
            wait: Wait::FailFast,
            acknowledge: vec![],
            custody: None,
            next: effigy_kernel::outcome::Invocation::validated(
                vec!["effigy".into(), "container".into(), "status".into()],
                |_| true,
            )
            .expect("next"),
        }
    }

    #[test]
    fn gated_decision_observation_precedes_session_admission_and_refuses_failure() {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
        let env = env("workspace");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let observe_cx = ctx.cx.clone();
        let observe_state = state.clone();
        let observe_resource = env.resource.clone();
        let refused = WorkspaceActivation::begin_observing(
            &ctx,
            &env,
            &invocation(),
            options(41),
            "fixture-supervisor",
            deadline,
            move |until| {
                assert!(until <= deadline);
                assert!(effigy_kernel::gate::Gate::acquire(
                    &observe_cx,
                    &observe_state,
                    std::slice::from_ref(&observe_resource),
                    Wait::FailFast
                )
                .is_err());
                Err(effigy_kernel::run::RunError::unavailable("unknown target"))
            },
        );
        assert!(refused.is_err());
        let WorkspaceBegin::Activate(activation) = WorkspaceActivation::begin_observing(
            &ctx,
            &env,
            &invocation(),
            options(41),
            "fixture-supervisor",
            deadline,
            |_| Ok(inputs()),
        )
        .expect("fresh decision after refusal") else {
            panic!("fresh activation");
        };
        activation
            .abort(&ctx, Wait::FailFast)
            .expect("settle activation");
    }

    #[test]
    fn reserved_owner_admits_task_before_env_and_refuses_concurrent_or_unselected_success() {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
        let env = env("workspace");
        let WorkspaceBegin::Activate(mut activation) = WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(1),
            "fixture-supervisor",
        )
        .expect("begin") else {
            panic!("fresh activation")
        };
        assert_eq!(activation.session().run(), options(1).id);
        let session = activation.session().clone();
        assert_eq!(
            activation
                .session_run_mut()
                .session_owner()
                .expect("adopted before channel")
                .run(),
            session.run()
        );
        let listener = effigy_kernel::workspace_session::HostChannelListener::bind(
            "127.0.0.1:0".parse().expect("socket"),
        )
        .expect("bound fixture listener");
        let key = effigy_kernel::workspace_session::ServingKey::new(
            vec![7; 32],
            Digest::of(b"fixture pin"),
        )
        .expect("key");
        let mut launch = effigy_kernel::workspace_session::SessionLaunch::launch(
            activation.session_run_mut(),
            &session,
            "fixture-supervisor",
            key,
            Box::new(listener),
        )
        .expect("already adopted session launches its own channel");
        assert!(
            launch.publish(activation.session_run_mut()).is_err(),
            "adoption alone never qualifies a locator"
        );
        launch.stop();
        assert_eq!(
            activation
                .current_plan()
                .expect("plan")
                .binding()
                .generation,
            activation.session().generation()
        );
        assert!(WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(2),
            "fixture-supervisor"
        )
        .is_err());
        activation
            .abort(&ctx, Wait::FailFast)
            .expect("settle before cancel");
        let WorkspaceBegin::Activate(retry) = WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(3),
            "fixture-supervisor",
        )
        .expect("retry") else {
            panic!("unpublished retry")
        };
        assert!(
            retry.finish_selected().is_err(),
            "no successful activation without selector readback"
        );
    }
}
