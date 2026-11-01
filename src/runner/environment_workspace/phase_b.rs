//! Publish, commit, select and finish using the SAME reacquired owner. Every
//! error carries the exact live custody; no caller may turn Drop into cleanup.
use effigy_containers::environment::{EnvError, WorkspaceFinishCustody};
use effigy_kernel::observe::Observed;
use effigy_kernel::run::RunError;
use effigy_kernel::store::Store;
use effigy_kernel::workspace_session::SelectionObligation;

use super::phase_a::GuardedPhaseA;
use super::{WorkspaceOwner, WorkspacePublication};
use crate::runner::workspace_client::ServingFacts;
use crate::runner::workspace_command::provisioning::reserved_guest::{
    publish_bound_reserved_guest, BoundReservedGuest,
};
use crate::runner::workspace_transport::qualification::PreparedHostChannel;

pub(in crate::runner) enum PublicationFailure {
    Guarded {
        cause: EnvError,
        custody: Box<GuardedPhaseA>,
    },
    Finishing {
        cause: EnvError,
        custody: Box<WorkspaceFinishCustody<'static>>,
        channel: Box<PreparedHostChannel>,
        serving: Box<ServingFacts>,
    },
}

pub(in crate::runner) fn publish_and_select(
    mut owner: GuardedPhaseA,
    bound: BoundReservedGuest<'_>,
) -> Result<(WorkspaceOwner, SelectionObligation), PublicationFailure> {
    let publication = match publish_bound_reserved_guest(&mut owner.activation, bound) {
        Ok(publication) => publication,
        Err(cause) => {
            return Err(PublicationFailure::Guarded {
                cause,
                custody: Box::new(owner),
            })
        }
    };
    let mut publication = publication;
    let selected = commit_and_select(&mut owner, &mut publication);
    let obligation = match selected {
        Ok(obligation) => obligation,
        Err(cause) => {
            // Publication has started a listener, but no acknowledgement may
            // escape. Stop it now; guarded durable cleanup decides the rest.
            publication.channel.launch.stop();
            return Err(PublicationFailure::Guarded {
                cause,
                custody: Box::new(owner),
            });
        }
    };
    let WorkspacePublication {
        channel,
        serving,
        receipt: _,
        guest: _,
    } = publication;
    let (supervisor, session) = match owner.activation.finish_selected() {
        Ok(value) => value,
        Err((cause, custody)) => {
            return Err(PublicationFailure::Finishing {
                cause,
                custody,
                channel: Box::new(channel),
                serving: Box::new(serving),
            })
        }
    };
    let effect = std::sync::Arc::new(owner.context.cx.clone());
    let observe = std::sync::Arc::new(owner.context.observe());
    Ok((
        WorkspaceOwner {
            supervisor,
            session,
            channel,
            creation: owner.creation,
            serving,
            effect,
            observe,
            root: owner.context.root,
        },
        obligation,
    ))
}

fn commit_and_select(
    owner: &mut GuardedPhaseA,
    publication: &mut WorkspacePublication<'_>,
) -> Result<SelectionObligation, EnvError> {
    owner.plan.deadline().remaining()?;
    let committed = owner
        .activation
        .commit(&publication.receipt, &publication.channel.launch)?;
    let pending = committed.selection_obligation().ok_or_else(|| {
        EnvError::Kernel(RunError::broken(
            "workspace commit has no durable selection obligation",
        ))
    })?;
    publication
        .guest
        .switch(owner.activation.environment_mut(), pending)?;
    owner.activation.selected(
        pending,
        &publication.channel.launch,
        publication.guest.as_mut(),
    )?;
    // Hand adoption the durable Switched obligation, never the in-memory
    // Pending value returned by commit before the selector CAS.
    let reader = match Store::read(&owner.context.observe(), owner.context.root) {
        Observed::Verified(reader, _) => reader,
        _ => {
            return Err(EnvError::Kernel(RunError::unavailable(
                "workspace selection readback unavailable",
            )))
        }
    };
    match effigy_kernel::workspace_session::committed_selection(
        &reader,
        owner.activation.session().environment(),
    ) {
        Observed::Verified(Some(actual), _)
            if actual.id() == pending.id()
                && actual.session_run() == owner.activation.session().run()
                && actual.state() == effigy_kernel::workspace_session::SelectionState::Switched =>
        {
            Ok(actual)
        }
        _ => Err(EnvError::Kernel(RunError::refused(
            "workspace selection was not durably switched",
        ))),
    }
}
