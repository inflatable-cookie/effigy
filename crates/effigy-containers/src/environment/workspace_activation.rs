//! Reserved workspace activation. The owning session is admitted and retained
//! before Env admission; no publication callback can reverse Task/Env order.

use effigy_kernel::gate::Wait;
use effigy_kernel::id::ResolvedInvocation;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{BeginOpts, Run};
use effigy_kernel::workspace_session::{
    ActivationDecision, PublishInputs, SessionRef, WorkspaceActivationReservation,
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
    session_run: Run<'r>,
    session: SessionRef,
    reservation: WorkspaceActivationReservation,
    selected: bool,
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
    ) -> Result<WorkspaceBegin<'r>, EnvError> {
        let wait = options.wait;
        let reservation = match WorkspaceActivationReservation::decide(
            &ctx.cx,
            ctx.root,
            &env.resource,
            inputs,
            wait,
        )
        .map_err(EnvError::Kernel)?
        {
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
        // Retain this exact owning Task Run while acquiring the claimed Env.
        let environment = match EnvRun::begin_claimed(ctx, env, Operation::Up, wait, &reservation) {
            Ok(environment) => environment,
            Err(error) => {
                session_run
                    .settle(&ctx.cleanup()?, &mut EnvSettlement)
                    .map_err(EnvError::Kernel)?;
                reservation
                    .cancel(&ctx.cx, ctx.root, wait)
                    .map_err(EnvError::Kernel)?;
                return Err(error);
            }
        };
        Ok(WorkspaceBegin::Activate(Box::new(Self {
            environment,
            session_run,
            session,
            reservation,
            selected: false,
        })))
    }

    /// Apply under the same claimed Env owner used for target binding and
    /// publication. Existing live holders preserve the backend and selection.
    pub fn apply_with(
        &mut self,
        request: super::Up<'_>,
        apply: impl FnOnce() -> Result<(), super::BackendError>,
    ) -> Result<super::Applied, EnvError> {
        super::ops::reconcile_in(&mut self.environment, request, apply, |run| run.holders())
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
        &mut self.session_run
    }

    pub fn plan(
        &self,
        ctx: &EnvContext<'_>,
    ) -> Result<effigy_kernel::workspace_session::ActivationPlan, EnvError> {
        let reader = match effigy_kernel::store::Store::read(&ctx.observe(), ctx.root) {
            Observed::Verified(reader, _) => reader,
            _ => {
                return Err(EnvError::Kernel(effigy_kernel::run::RunError::unavailable(
                    "workspace activation journal is unreadable",
                )))
            }
        };
        match self.reservation.plan(&reader) {
            Observed::Verified(Some(plan), _) => Ok(plan),
            _ => Err(EnvError::Kernel(effigy_kernel::run::RunError::unavailable(
                "workspace activation plan is not admitted",
            ))),
        }
    }

    /// Atomic commit retains both exact owning runs and the qualified launch.
    /// Returning its durable obligation grants no claim that selectors moved.
    pub fn commit(
        &mut self,
        receipt: &effigy_kernel::workspace_session::SelectionReceipt,
        launch: &effigy_kernel::workspace_session::SessionLaunch,
    ) -> Result<effigy_kernel::workspace_session::CommittedActivation, EnvError> {
        self.environment.commit_workspace_selection(
            &self.reservation,
            &self.session_run,
            receipt,
            launch,
        )
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
    pub fn finish_selected(self) -> Result<(Run<'r>, SessionRef), EnvError> {
        if !self.selected {
            return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "workspace selection has not been verified",
            )));
        }
        self.environment.finish(true)?;
        Ok((self.session_run, self.session))
    }

    /// Pre-commit failure: settle both bound runs before cancelling. A committed
    /// generation instead remains fenced by its durable selection obligation;
    /// its owner must call selection recovery/abandon, never restore a predecessor.
    pub fn abort(self, ctx: &EnvContext<'_>, wait: Wait) -> Result<(), EnvError> {
        let environment = self.environment.finish(false);
        let session = self
            .session_run
            .settle(&ctx.cleanup()?, &mut EnvSettlement)
            .map_err(EnvError::Kernel);
        let reservation = self
            .reservation
            .cancel(&ctx.cx, ctx.root, wait)
            .map_err(EnvError::Kernel);
        environment?;
        session?;
        reservation?;
        Ok(())
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
    fn reserved_owner_admits_task_before_env_and_refuses_concurrent_or_unselected_success() {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
        let env = env("workspace");
        let WorkspaceBegin::Activate(activation) =
            WorkspaceActivation::begin(&ctx, &env, &inputs(), &invocation(), options(1))
                .expect("begin")
        else {
            panic!("fresh activation")
        };
        assert_eq!(activation.session().run(), options(1).id);
        assert_eq!(
            activation.plan(&ctx).expect("plan").binding().generation,
            activation.session().generation()
        );
        assert!(
            WorkspaceActivation::begin(&ctx, &env, &inputs(), &invocation(), options(2)).is_err()
        );
        activation
            .abort(&ctx, Wait::FailFast)
            .expect("settle before cancel");
        let WorkspaceBegin::Activate(retry) =
            WorkspaceActivation::begin(&ctx, &env, &inputs(), &invocation(), options(3))
                .expect("retry")
        else {
            panic!("unpublished retry")
        };
        assert!(
            retry.finish_selected().is_err(),
            "no successful activation without selector readback"
        );
    }
}
