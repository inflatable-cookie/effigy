//! Container-mode Up owner sequence. The publication provider is a typed
//! injection boundary: only the locator owner's qualified implementation may
//! supply a SelectionReceipt and a live PreparedHostChannel. There is no
//! string/default-locator or generic-publication fallback here.
#![allow(dead_code)] // Installed at the Up call site with #504's merged provider.

use effigy_containers::environment::{
    BackendError, EnvError, EnvRun, Retention, Up, WorkspaceActivation, WorkspaceBegin,
};
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::id::ResolvedInvocation;
use effigy_kernel::run::{BeginOpts, Run};
use effigy_kernel::workspace_session::{
    PublishInputs, SelectionObligation, SelectionReceipt, SelectorReader, SessionRef,
};

use crate::runner::workspace_transport::qualification::PreparedHostChannel;

/// The guest adapter performs CAS only; the owner records kernel readback
/// through the same reader under the same Env gate immediately afterward.
pub(in crate::runner) trait WorkspaceSelectionIo: SelectorReader {
    fn switch(
        &mut self,
        run: &mut EnvRun<'_>,
        obligation: &SelectionObligation,
    ) -> Result<(), EnvError>;
}

pub(in crate::runner) struct WorkspacePublication<'a> {
    pub(in crate::runner) receipt: SelectionReceipt,
    pub(in crate::runner) channel: PreparedHostChannel,
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
    pub(in crate::runner) run: Run<'static>,
    pub(in crate::runner) session: SessionRef,
    pub(in crate::runner) channel: PreparedHostChannel,
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
}

#[derive(Debug)]
pub(in crate::runner) enum WorkspaceUpError {
    Preparation(crate::runner::error::RunnerError),
    Owner {
        cause: EnvError,
        cleanup: Option<EnvError>,
    },
}

/// The real owner orchestration, with backend/qualified publication I/O
/// injected. Fakes may stop at this boundary; none can fabricate the private
/// kernel receipt/lease or claim production channel qualification.
pub(in crate::runner) fn up<'a>(
    request: WorkspaceUpRequest<'_>,
    apply: impl FnOnce() -> Result<(), BackendError>,
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
    let owner_error = |cause| WorkspaceUpError::Owner {
        cause,
        cleanup: None,
    };
    let mut activation = match WorkspaceActivation::begin(
        &ctx,
        &env,
        request.inputs,
        request.invocation,
        request.options,
    )
    .map_err(owner_error)?
    {
        WorkspaceBegin::Attach(session) => return Ok(WorkspaceUp::Attached(session)),
        WorkspaceBegin::Activate(activation) => activation,
    };
    let result = (|| {
        let backend = super::backend(&policy.repo_root, policy);
        activation.apply_with(
            Up {
                render: policy.render.as_ref().map(|render| &render.closure),
                project: &policy.project_name,
                backend: &backend,
                retention: request.retention,
            },
            apply,
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
        publication
            .guest
            .switch(activation.environment_mut(), obligation)?;
        activation.selected(
            obligation,
            &publication.channel.launch,
            publication.guest.as_mut(),
        )?;
        Ok(publication.channel)
    })();
    let channel = match result {
        Ok(channel) => channel,
        Err(cause) => {
            let cleanup = activation
                .abort(&ctx, effigy_kernel::gate::Wait::FailFast)
                .err();
            return Err(WorkspaceUpError::Owner { cause, cleanup });
        }
    };
    let (run, session) = activation.finish_selected().map_err(owner_error)?;
    Ok(WorkspaceUp::Selected(Box::new(WorkspaceOwner {
        run,
        session,
        channel,
    })))
}
