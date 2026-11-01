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
use crate::runner::container_command::{
    reserved_input::ContainerBackendObservation, WorkspacePreparedEffects, WorkspaceUpEffects,
};
use crate::runner::environment::workspace::{
    integrated::{self, IntegratedFailure},
    phase_a::PhaseARequest,
};
use crate::runner::workspace_client::serving_host::{self, ContainerStartupReport};
use crate::runner::workspace_client::supervisor::{
    ServiceAdoptFailure, ServiceOwner, ServingHandoff,
};
use crate::runner::workspace_command::provisioning::reserved_guest::BoundReservedGuest;

type ResumeResult =
    Result<WorkspaceActivation<'static>, (EnvError, Box<ParkedWorkspaceActivation<'static>>)>;

pub(super) struct ActivatedContainer {
    pub(super) service: ServiceOwner<'static>,
    pub(super) effects: WorkspaceUpEffects,
    pub(super) report: ContainerStartupReport,
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
    let mut predecessor = ContainerBackendObservation::prepare(planned.plan.policy())
        .map_err(|_| RunError::unavailable("workspace predecessor observation unavailable"))?;
    let mut publication_observation =
        ContainerBackendObservation::prepare(planned.plan.policy())
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
        &mut predecessor,
        DEFAULT_INTEGRATION_BUDGET,
        |owner| {
            plan::publication_input(
                owner,
                &planned.plan,
                &planned.context,
                &mut publication_observation,
            )
        },
        |target| Ok(prepared_effects.bind(target.clone())),
        resume,
    );
    let (owner, obligation, effects) = match selected {
        Ok(selected) => selected,
        Err(failure) => {
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
    let handoff = match ServingHandoff::committed(owner, &obligation) {
        Ok(handoff) => handoff,
        Err((_error, mut owner)) => {
            owner.channel.launch.stop();
            return retain_unqualified(
                session,
                host_run,
                (owner, effects),
                "workspace selected handoff refused; exact owner retained",
            );
        }
    };
    let service = match ServiceOwner::adopt(handoff) {
        Ok(service) => service,
        Err(ServiceAdoptFailure::Before {
            error: _,
            mut owner,
        }) => {
            owner.channel.launch.stop();
            return retain_unqualified(
                session,
                host_run,
                (owner, effects),
                "workspace channel adoption refused; exact owner retained",
            );
        }
        Err(ServiceAdoptFailure::After {
            error: _,
            mut owner,
        }) => {
            owner.stop();
            return retain_unqualified(
                session,
                host_run,
                (owner, effects),
                "workspace service adoption refused; exact owner retained",
            );
        }
    };
    Ok(ActivatedContainer {
        service,
        effects,
        report,
    })
}

/// This is a named fail-closed fallback for unresolved adoption custody, not a
/// settlement claim. Keep the original owner and handles in this control loop.
fn retain_unqualified<T>(
    session: &mut EngineSession,
    run: &mut Run<'_>,
    custody: T,
    message: &'static str,
) -> Result<ActivatedContainer, RunError> {
    session.refused_message(effigy_kernel::human::Category::Unavailable, message);
    loop {
        let _held = &custody;
        let _ = session.control(run, &[]);
        std::thread::sleep(std::time::Duration::from_millis(20));
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
