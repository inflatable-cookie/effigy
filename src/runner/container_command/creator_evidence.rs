//! Exact-ID attribution from the creation owner's journal, never from inventory
//! or labels in the input builder. Binding still rechecks the receipt under the
//! live activation fence through EnvRun::bind_target.
use super::{bounded_publication_read, BoundedReadRefusal, PublicationDeadline};
use effigy_kernel::ctx::ObserveContext;
use effigy_kernel::id::{ResourceId, RunId};
use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::root::StateRoot;

#[derive(Debug)]
pub(super) struct WorkspaceCreatorEvidence {
    observe: ObserveContext,
    root: &'static StateRoot,
    environment: ResourceId,
}

impl WorkspaceCreatorEvidence {
    pub(super) fn new(
        ctx: &effigy_containers::environment::EnvContext<'static>,
        environment: ResourceId,
    ) -> Self {
        Self {
            observe: ctx.observe(),
            root: ctx.root,
            environment,
        }
    }

    pub(super) fn created_by(
        &self,
        project: &str,
        full_id: &str,
        deadline: PublicationDeadline,
    ) -> Observed<RunId> {
        if deadline.remaining().is_err() {
            return Observed::Unknown(UnknownReason::TimedOut {
                phase: "creator journal".into(),
            });
        }
        let observe = self.observe.clone();
        let root = self.root;
        let environment = self.environment.clone();
        let project = project.to_owned();
        let full_id = full_id.to_owned();
        // A stalled journal reader may finish reading after timeout; it owns no
        // mutation capability and cannot publish or attribute a returned target.
        match bounded_publication_read("creator journal", deadline.at(), move || {
            Ok(effigy_containers::environment::created_by(
                &observe,
                root,
                &environment,
                &project,
                &full_id,
            ))
        }) {
            Ok(observed) if deadline.remaining().is_ok() => observed,
            Ok(_) | Err(BoundedReadRefusal::TimedOut) => {
                Observed::Unknown(UnknownReason::TimedOut {
                    phase: "creator journal".into(),
                })
            }
            Err(BoundedReadRefusal::Unavailable(reason)) => {
                Observed::Unknown(UnknownReason::Inconsistent(reason))
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/creator_evidence.rs"]
mod tests;
