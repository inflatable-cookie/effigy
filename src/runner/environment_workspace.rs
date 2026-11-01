//! Container-mode Up owner sequence. The publication provider is a typed
//! injection boundary: only the locator owner's qualified implementation may
//! supply a SelectionReceipt and a live PreparedHostChannel. There is no
//! string/default-locator or generic-publication fallback here.
#![allow(dead_code)] // The workspace-client owner installs the serialized consuming caller.

use effigy_containers::environment::{
    BackendError, CreationOutcome, EnvError, EnvRun, Retention, Up, WorkspaceActivation,
    WorkspaceBegin,
};
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::id::ResolvedInvocation;
use effigy_kernel::run::BeginOpts;
use effigy_kernel::workspace_session::{
    PublishInputs, SelectionObligation, SelectionReceipt, SelectorReader, SessionRef,
    SessionSupervisor,
};
use std::sync::Arc;

use crate::runner::workspace_transport::qualification::PreparedHostChannel;

/// The guest adapter performs CAS only; the owner records kernel readback
/// through the same reader under the same Env gate immediately afterward.
pub(in crate::runner) trait WorkspaceSelectionIo: SelectorReader {
    fn target_liveness(&mut self) -> effigy_kernel::observe::Observed<()>;
    fn switch_recovery(
        &mut self,
        guard: &effigy_kernel::gate::GateGuard,
        obligation: &SelectionObligation,
    ) -> Result<(), EnvError>;
    fn switch(
        &mut self,
        run: &mut EnvRun<'_>,
        obligation: &SelectionObligation,
    ) -> Result<(), EnvError>;
}

pub(in crate::runner) struct WorkspacePublication<'a> {
    pub(in crate::runner) receipt: SelectionReceipt,
    pub(in crate::runner) channel: PreparedHostChannel,
    /// The validated facts this session serves with, frozen before the Env
    /// commit and the selector switch (contract 052 §8.11, owner ruling
    /// 2026-10-30). The serving owner constructs the channel server from it.
    pub(in crate::runner) serving: crate::runner::workspace_client::ServingFacts,
    pub(in crate::runner) guest: Box<dyn WorkspaceSelectionIo + 'a>,
}

impl std::fmt::Debug for WorkspacePublication<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspacePublication")
            .field("receipt", &self.receipt)
            .finish_non_exhaustive()
    }
}

/// Exact custody is handed to the #452 supervisor; dropping this owner is an
/// interrupted session, never detached success. A front end keeps SessionRef.
#[derive(Debug)]
pub(in crate::runner) struct WorkspaceOwner {
    pub(in crate::runner) supervisor: SessionSupervisor<'static>,
    pub(in crate::runner) session: SessionRef,
    pub(in crate::runner) channel: PreparedHostChannel,
    /// What this activation's create recorded: a receipt sealed to its own
    /// environment run, or the explicit absence of attribution (effigy#510).
    pub(in crate::runner) creation: Option<CreationOutcome>,
    /// The serving facts the publication validated; carried unchanged into the
    /// serving owner with the adopted supervisor.
    pub(in crate::runner) serving: crate::runner::workspace_client::ServingFacts,
    pub(in crate::runner) effect: Arc<effigy_kernel::ctx::EffectContext>,
    pub(in crate::runner) observe: Arc<effigy_kernel::ctx::ObserveContext>,
    pub(in crate::runner) root: &'static effigy_kernel::root::StateRoot,
}

#[derive(Debug)]
pub(in crate::runner) enum WorkspaceUp {
    Hybrid,
    Attached(SessionRef),
    Selected(Box<WorkspaceOwner>),
}

#[derive(Debug)]
pub(in crate::runner) struct WorkspaceUpRequest<'a> {
    pub(in crate::runner) policy: &'a EffectiveContainerPolicy,
    pub(in crate::runner) retention: Retention,
    pub(in crate::runner) inputs: &'a PublishInputs,
    pub(in crate::runner) invocation: &'a ResolvedInvocation,
    pub(in crate::runner) options: BeginOpts,
    pub(in crate::runner) supervisor_id: &'a str,
}

#[derive(Debug)]
pub(in crate::runner) enum WorkspaceUpError {
    Preparation(crate::runner::error::RunnerError),
    Owner {
        cause: Box<EnvError>,
        cleanup: Option<Box<EnvError>>,
    },
    Finish {
        cause: Box<EnvError>,
        custody: Box<effigy_containers::environment::WorkspaceFinishCustody<'static>>,
        channel: Box<PreparedHostChannel>,
        serving: Box<crate::runner::workspace_client::ServingFacts>,
    },
}

/// The real owner orchestration, with backend/qualified publication I/O
/// injected. Fakes may stop at this boundary; none can fabricate the private
/// kernel receipt/lease or claim production channel qualification.
pub(in crate::runner) fn up<'a>(
    request: WorkspaceUpRequest<'_>,
    compose_env: &[(String, std::ffi::OsString)],
    publish: impl FnOnce(
        &mut WorkspaceActivation<'static>,
    ) -> Result<WorkspacePublication<'a>, EnvError>,
) -> Result<WorkspaceUp, WorkspaceUpError> {
    let policy = request.policy;
    if crate::runner::workspace_command::operator_workspace_for_policy(&policy.repo_root, policy)
        .map_err(WorkspaceUpError::Preparation)?
        .is_none()
    {
        return Ok(WorkspaceUp::Hybrid);
    }
    let ctx = super::context(&policy.repo_root).map_err(WorkspaceUpError::Preparation)?;
    let env = super::env_ref(policy).map_err(WorkspaceUpError::Preparation)?;
    let effect = Arc::new(ctx.cx.clone());
    let observe = Arc::new(
        ctx.cx
            .operation()
            .observe()
            .map_err(|_| WorkspaceUpError::Owner {
                cause: Box::new(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                    "workspace activation cannot observe its owner state",
                ))),
                cleanup: None,
            })?,
    );
    let owner_error = |cause| WorkspaceUpError::Owner {
        cause: Box::new(cause),
        cleanup: None,
    };
    let closure = policy.render.as_ref().ok_or_else(|| {
        owner_error(EnvError::Invalid(
            "workspace activation has no frozen render".into(),
        ))
    })?;
    let backend = super::backend(&policy.repo_root, policy);
    let until = match ctx.cx.operation().deadline() {
        effigy_kernel::ctx::Deadline::At(until) => until,
        effigy_kernel::ctx::Deadline::Open(_) => {
            std::time::Instant::now() + effigy_containers::environment::RUN_BUDGET
        }
    };
    let mut preflight_request = Up {
        render: Some(&closure.closure),
        project: &policy.project_name,
        backend: &backend,
        retention: request.retention,
        workdir: &policy.repo_root,
        primary_service: &policy.primary_service,
        compose_files: &policy.compose_files,
    }
    .creation_request();
    preflight_request.deadline = until;
    let preflight = effigy_containers::environment::CreationPreflight::observe(
        backend.adapter().as_mut(),
        preflight_request,
    )
    .map_err(|error| {
        owner_error(EnvError::Backend(BackendError::NotStarted(
            error.to_string(),
        )))
    })?;
    let sealed = effigy_kernel::objects::Objects::seal_for_preflight(
        &ctx.cx,
        ctx.root,
        &closure.closure,
        until,
    )
    .map_err(|error| owner_error(EnvError::Kernel(error)))?;
    let mut activation = match WorkspaceActivation::begin(
        &ctx,
        &env,
        request.inputs,
        request.invocation,
        request.options,
        request.supervisor_id,
    )
    .map_err(owner_error)?
    {
        WorkspaceBegin::Attach(session) => return Ok(WorkspaceUp::Attached(session)),
        WorkspaceBegin::Activate(activation) => activation,
    };
    let mut failed_target = effigy_kernel::observe::Observed::Unknown(
        effigy_kernel::observe::UnknownReason::GuardUnavailable,
    );
    let mut creation = None;
    let result = (|| {
        let backend = super::backend(&policy.repo_root, policy);
        let (program, args) = backend
            .compose_invocation(&effigy_containers::compose::compose_up_args(policy))
            .map_err(EnvError::Backend)?;
        let bringing_up = activation.apply_sealed_with(
            Up {
                render: policy.render.as_ref().map(|render| &render.closure),
                project: &policy.project_name,
                backend: &backend,
                retention: request.retention,
                workdir: &policy.repo_root,
                primary_service: &policy.primary_service,
                compose_files: &policy.compose_files,
            },
            &sealed,
            &preflight,
            || {
                effigy_containers::exec::run_backend_invocation_until(
                    &policy.repo_root,
                    &program,
                    &args,
                    "apply frozen workspace render",
                    compose_env,
                    until,
                )
                .map(|_| ())
                .map_err(|error| BackendError::Uncertain(error.to_string()))
            },
        )?;
        creation = bringing_up.creation;
        // A publication provider cannot turn an incomplete apply observation
        // into a fresh target by supplying its own candidate.
        let reservation = activation.reservation().clone();
        activation.environment_mut().recheck_creation(
            &reservation,
            backend.adapter().as_mut(),
            until,
        )?;

        // The provider binds the apply's exact target/platform and stages the
        // required guest binary ticket before independently probing the guest.
        // It returns only a real qualified launch and constructor-validated
        // receipt of the very document that the guest parsed.
        let mut publication = publish(&mut activation)?;
        let committed = activation.commit(&publication.receipt, &publication.channel.launch)?;
        let obligation = committed.selection_obligation().ok_or_else(|| {
            EnvError::Kernel(effigy_kernel::run::RunError::broken(
                "workspace commit has no durable selection obligation",
            ))
        })?;
        let selection = (|| {
            publication
                .guest
                .switch(activation.environment_mut(), obligation)?;
            activation.selected(
                obligation,
                &publication.channel.launch,
                publication.guest.as_mut(),
            )
        })();
        if let Err(error) = selection {
            failed_target = publication.guest.target_liveness();
            publication.channel.launch.stop();
            return Err(error);
        }
        Ok((publication.channel, publication.serving))
    })();
    let (channel, serving) = match result {
        Ok(channel) => channel,
        Err(cause) => {
            let cleanup = activation
                .abort_with_target(&ctx, effigy_kernel::gate::Wait::FailFast, failed_target)
                .err()
                .map(Box::new);
            return Err(WorkspaceUpError::Owner {
                cause: Box::new(cause),
                cleanup,
            });
        }
    };
    let (supervisor, session) = match activation.finish_selected() {
        Ok(value) => value,
        Err((cause, custody)) => {
            return Err(WorkspaceUpError::Finish {
                cause: Box::new(cause),
                custody,
                channel: Box::new(channel),
                serving: Box::new(serving),
            })
        }
    };
    Ok(WorkspaceUp::Selected(Box::new(WorkspaceOwner {
        supervisor,
        session,
        channel,
        creation,
        serving,
        effect,
        observe,
        root: ctx.root,
    })))
}

/// The C producer with the repository's required staging/context/qualification
/// sequence. The callback supplies freshly observed backend inputs after apply;
/// it does not supply a publication receipt or perform selector writes.
pub(in crate::runner) fn up_prepared<'a>(
    request: WorkspaceUpRequest<'_>,
    compose_env: &[(String, std::ffi::OsString)],
    observe: impl FnOnce(
        &mut WorkspaceActivation<'static>,
    ) -> Result<
        crate::runner::workspace_command::provisioning::reserved_guest::ReservedGuestInput<'a>,
        EnvError,
    >,
) -> Result<WorkspaceUp, WorkspaceUpError> {
    up(request, compose_env, |activation| {
        let input = observe(activation)?;
        crate::runner::workspace_command::provisioning::reserved_guest::prepare_reserved_guest(
            activation, input,
        )
    })
}

/// Resume an exact committed Pending selection at its retained owning
/// supervisor. No listener/session is reconstructed. CAS runs under Env;
/// completion reacquires Env and revalidates all liveness and readback facts.
pub(in crate::runner) fn recover_selection(
    owner: &mut WorkspaceOwner,
    obligation: &SelectionObligation,
    guest: &mut dyn WorkspaceSelectionIo,
    target: &mut dyn effigy_kernel::workspace_session::TargetProbe,
    probe: &mut dyn effigy_kernel::workspace_session::ServingProbe,
) -> Result<(), EnvError> {
    use effigy_kernel::gate::{Gate, Wait};
    use effigy_kernel::observe::Observed;
    use effigy_kernel::run::RunError;
    use effigy_kernel::workspace_session::{selection_recovered, unfinished_selection};
    let refuse = || {
        EnvError::Kernel(RunError::refused(
            "recovery owner or durable selection differs",
        ))
    };
    if owner.session.run() != obligation.session_run()
        || owner.channel.launch.lease().as_ref() != Some(obligation.receipt().serving())
    {
        return Err(refuse());
    }
    let guard = Gate::acquire(
        &owner.effect,
        owner.root,
        std::slice::from_ref(obligation.environment()),
        Wait::FailFast,
    )
    .map_err(|error| EnvError::Kernel(error.into()))?;
    let stored = match effigy_kernel::store::Store::read(&owner.observe, owner.root) {
        Observed::Verified(journal, _) => {
            match unfinished_selection(&journal, obligation.environment()) {
                Observed::Verified(Some(stored), _) => stored,
                Observed::Unknown(reason) => {
                    return Err(EnvError::Kernel(RunError::unavailable(reason.to_string())))
                }
                _ => return Err(refuse()),
            }
        }
        Observed::Unknown(reason) => {
            return Err(EnvError::Kernel(RunError::unavailable(reason.to_string())))
        }
        _ => return Err(refuse()),
    };
    if stored.id() != obligation.id()
        || serde_json::to_vec(stored.receipt()).map_err(|_| refuse())?
            != serde_json::to_vec(obligation.receipt()).map_err(|_| refuse())?
    {
        return Err(refuse());
    }
    let publication = &stored.receipt().publication().publication;
    let budget = std::time::Duration::from_secs(5);
    let target_live =
        target.verify_target(&publication.container_id, &publication.vm_boot_id, budget);
    let listener_live = owner
        .channel
        .launch
        .liveness(&owner.supervisor, probe, budget);
    for fact in [target_live, listener_live] {
        match fact {
            Observed::Verified((), _) => {}
            Observed::Unknown(reason) => {
                return Err(EnvError::Kernel(RunError::unavailable(reason.to_string())))
            }
            Observed::Absent(_) => {
                return Err(EnvError::Kernel(RunError::refused(
                    "recovery target or listener is gone",
                )))
            }
        }
    }
    guest.switch_recovery(&guard, &stored)?;
    drop(guard);
    selection_recovered(
        &owner.effect,
        owner.root,
        &stored,
        &owner.channel.launch,
        target,
        probe,
        guest,
        Wait::FailFast,
    )
    .map_err(EnvError::Kernel)
}
