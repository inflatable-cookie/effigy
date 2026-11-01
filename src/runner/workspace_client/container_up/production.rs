//! The actual detached caller. All borrowed guest/plan state stays on this
//! supervisor stack until either selected adoption or retained failure custody.
use std::sync::Arc;

use effigy_containers::environment::{
    EnvError, EnvironmentBackend, ParkedWorkspaceActivation, WorkspaceActivation,
};
use effigy_kernel::id::{
    ExecutionTarget, InvocationSpec, ResolvedInvocation, Route, Surface, TaskId,
};
use effigy_kernel::run::{BeginOpts, Run, RunError};
use effigy_kernel::supervise::EngineSession;

use super::integrations::DEFAULT_INTEGRATION_BUDGET;
use super::supervisor_preflight::PrevalidatedContainer;
use super::{plan, ContainerUpRequest, ServingHostRequest};
use crate::runner::container_command::{WorkspacePreparedEffects, WorkspaceUpEffects};
use crate::runner::environment::workspace::{
    integrated::{self, IntegratedFailure},
    phase_a::PhaseARequest,
};
use crate::runner::workspace_client::serving::OwnedWorkspaceChannel;
use crate::runner::workspace_client::serving_host::{self, ContainerStartupReport};
use crate::runner::workspace_client::supervisor::{
    ChannelOwner, RetirementCause, ServiceAdoptFailure, ServiceOwner, ServingHandoff,
};
use crate::runner::workspace_command::provisioning::reserved_guest::BoundReservedGuest;

type ResumeResult =
    Result<WorkspaceActivation<'static>, (EnvError, Box<ParkedWorkspaceActivation<'static>>)>;

pub(crate) struct ActivatedContainer {
    pub(crate) service: ServiceOwner<'static>,
    pub(crate) effects: WorkspaceUpEffects,
    pub(crate) report: ContainerStartupReport,
}

/// The typed facade and retaining cleanup are injected only to keep this
/// assembly compilable before A's reviewed APIs are promoted into the base.
/// The installed caller must supply those real methods, never expected facts.
pub(super) fn activate_with(
    host: &ServingHostRequest,
    request: ContainerUpRequest,
    validation: PrevalidatedContainer,
    session: &mut EngineSession,
    host_run: &mut Run<'_>,
    resume: impl for<'a> FnOnce(
        ParkedWorkspaceActivation<'static>,
        &mut dyn EnvironmentBackend,
        &mut BoundReservedGuest<'a>,
    ) -> ResumeResult,
    mut recover: impl for<'a> FnMut(
        IntegratedFailure<'a, WorkspaceUpEffects>,
    ) -> Result<(), IntegratedFailure<'a, WorkspaceUpEffects>>,
) -> Result<ActivatedContainer, RunError> {
    let declaration = request
        .post_prep
        .as_ref()
        .ok_or_else(|| RunError::refused("workspace post-prep declaration is absent"))?;
    let planned = plan::build(validation, declaration)?;
    planned.validation.recheck_directory()?;
    let invocation = activation_invocation(host, planned.environment.checkout)?;
    let options = BeginOpts {
        id: serving_host::fresh_run_id()?,
        wait: effigy_kernel::gate::Wait::FailFast,
        acknowledge: Vec::new(),
        next: serving_host::recovery_invocation()?,
        custody: None,
    };
    let prepared_effects = WorkspacePreparedEffects::prepare(planned.plan.policy().clone())
        .map_err(|_| {
            RunError::unavailable("workspace secret integration preparation unavailable")
        })?;
    let compose_env = prepared_effects.compose_env();
    let mut predecessor = plan::prepare_observation(planned.plan.policy())
        .map_err(|_| RunError::unavailable("workspace predecessor observation unavailable"))?;
    let mut publication_observation = plan::prepare_observation(planned.plan.policy())
        .map_err(|_| RunError::unavailable("workspace publication observation unavailable"))?;
    let supervisor_id = host_run.id().to_string();
    let request = PhaseARequest {
        plan: Arc::clone(&planned.plan),
        retention: request.retention,
        invocation: &invocation,
        options,
        supervisor_id: &supervisor_id,
        decide: planned.decision()?,
    };
    let selected = integrated::run(
        request,
        &compose_env,
        predecessor.as_mut(),
        DEFAULT_INTEGRATION_BUDGET,
        |owner| {
            plan::publication_input(
                owner,
                &planned.plan,
                &planned.context,
                publication_observation.as_mut(),
            )
        },
        |target| Ok(prepared_effects.bind(target.clone())),
        resume,
    );
    let (owner, obligation, effects) = match selected {
        Ok(selected) => selected,
        Err(failure) => {
            #[cfg(feature = "workspace-serving-fixture")]
            if super::fake_io::configured() {
                super::fake_io::activation_failure(&failure);
            }
            let mut failure = failure;
            let mut owner_loss_handled = false;
            loop {
                match recover(failure) {
                    Ok(()) => {
                        return Err(RunError::unavailable(
                            "workspace activation failed; exact owner cleanup settled",
                        ))
                    }
                    Err(retained) => failure = retained,
                }
                session.refused_message(effigy_kernel::human::Category::Unavailable,
                    format!("workspace activation failed; integration effects unsettled; re-run up; exact owner retained (supervisor_run={}); request stop to retry cleanup", host_run.id()));
                serving_host::await_cleanup_recovery(session, host_run, &mut owner_loss_handled);
            }
        }
    };
    let report = effects.startup_report();
    #[cfg(feature = "workspace-serving-fixture")]
    let mut owner = owner;
    #[cfg(feature = "workspace-serving-fixture")]
    if super::fake_io::fail_handoff() {
        owner.channel.launch.stop();
    }
    let handoff = match ServingHandoff::committed(owner, &obligation) {
        Ok(handoff) => handoff,
        Err((_error, mut owner)) => {
            owner.channel.launch.stop();
            return Err(retire_selected(
                session,
                host_run,
                SelectedCustody::Unadopted(Box::new((*owner).into()), Box::new(effects)),
                "workspace selected handoff refused",
            ));
        }
    };
    let service = match ServiceOwner::adopt(handoff) {
        Ok(service) => service,
        Err(ServiceAdoptFailure::Before {
            error: _,
            mut owner,
        }) => {
            owner.channel.launch.stop();
            return Err(retire_selected(
                session,
                host_run,
                SelectedCustody::Unadopted(owner, Box::new(effects)),
                "workspace channel adoption refused",
            ));
        }
        Err(ServiceAdoptFailure::After {
            error: _,
            mut owner,
        }) => {
            owner.stop();
            return Err(retire_selected(
                session,
                host_run,
                SelectedCustody::Adopted(owner, Box::new(effects)),
                "workspace service adoption refused",
            ));
        }
    };
    Ok(ActivatedContainer {
        service,
        effects,
        report,
    })
}

/// Both states own the original SessionSupervisor; conversion creates no
/// replacement session, key, grant book or run.
enum SelectedCustody {
    Unadopted(Box<OwnedWorkspaceChannel<'static>>, Box<WorkspaceUpEffects>),
    Adopted(Box<ChannelOwner<'static>>, Box<WorkspaceUpEffects>),
}
impl SelectedCustody {
    fn session(&self) -> effigy_kernel::id::RunId {
        match self {
            Self::Unadopted(owner, _) => owner.session.run(),
            Self::Adopted(owner, _) => owner.session().run(),
        }
    }
    fn settle(self, cleanup: &effigy_kernel::ctx::CleanupContext) -> Result<(), (RunError, Self)> {
        match self {
            Self::Unadopted(mut owner, effects) => {
                owner.channel.launch.stop();
                match owner
                    .supervisor
                    .settle_recoverable(cleanup, &mut effigy_containers::environment::EnvSettlement)
                {
                    Ok(_) => Ok(()),
                    Err((error, retained)) => {
                        owner.supervisor = *retained;
                        Err((error, Self::Unadopted(owner, effects)))
                    }
                }
            }
            Self::Adopted(owner, effects) => match owner.settle_with_recoverable(
                RetirementCause::Stopped,
                cleanup,
                &mut effigy_containers::environment::EnvSettlement,
            ) {
                Ok(_) => Ok(()),
                Err((error, owner)) => Err((error, Self::Adopted(owner, effects))),
            },
        }
    }
}

/// Retire the same selected owner once. A failed cleanup keeps its exact slot
/// and handles; only an explicit stop or one owner-loss transition retries with
/// fresh cleanup authority. No control result is ignored as a recovery claim.
fn retire_selected(
    session: &mut EngineSession,
    run: &mut Run<'_>,
    mut custody: SelectedCustody,
    primary: &'static str,
) -> RunError {
    let mut cleanup = serving_host::cleanup_budget();
    #[cfg(feature = "workspace-serving-fixture")]
    if super::fake_io::fail_adoption_cleanup() {
        cleanup = crate::runner::task_engine::cleanup_context(std::time::Duration::ZERO);
    }
    let mut owner_loss_handled = false;
    let mut previously_failed = false;
    loop {
        let original = custody.session();
        match custody.settle(&cleanup) {
            Ok(()) => {
                return RunError::unavailable(if previously_failed {
                    format!("{primary}; exact selected cleanup recovered (session_run={original})")
                } else {
                    format!("{primary}; selected cleanup settled (session_run={original})")
                })
            }
            Err((_cleanup_error, retained)) => {
                custody = retained;
                previously_failed = true;
                session.refused_message(effigy_kernel::human::Category::Unavailable,
                    format!("{primary}; cleanup failed; exact selected owner retained (supervisor_run={}, session_run={original}); request stop to retry cleanup", run.id()));
                serving_host::await_cleanup_recovery(session, run, &mut owner_loss_handled);
                cleanup = serving_host::cleanup_budget();
            }
        }
    }
}

fn activation_invocation(
    host: &ServingHostRequest,
    checkout: effigy_kernel::id::CheckoutId,
) -> Result<ResolvedInvocation, RunError> {
    let task = TaskId::new(
        checkout,
        "",
        Surface::Published,
        "workspace-activation",
        Some(&host.profile),
    )
    .map_err(|_| RunError::refused("workspace activation task identity unavailable"))?;
    ResolvedInvocation::new(InvocationSpec {
        owner: task.resource(),
        task,
        route: Route::Task,
        argv: vec![
            "effigy".into(),
            "container".into(),
            "up".into(),
            "--detach".into(),
        ],
        cwd: String::new(),
        target: ExecutionTarget::Host,
        env: Default::default(),
        gates: Vec::new(),
        attachments: Vec::new(),
        class: effigy_kernel::ctx::EffectClass::Mutate,
        placement: effigy_kernel::ctx::Placement::Host,
        provenance: Default::default(),
    })
    .map_err(|_| RunError::refused("workspace activation invocation unavailable"))
}
