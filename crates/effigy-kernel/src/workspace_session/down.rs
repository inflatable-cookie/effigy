//! A down requested from inside the workspace session (§8.7).
//!
//! Closing a terminal does not stop the environment, and a down is not a
//! cascade: it inspects the environment's holders, refuses while any other run
//! is live or unknown, asks the session's own child runs to stop, and only then
//! takes a transition permit that re-checks the holders with the session
//! exempt. A run it never named is never settled.

use super::*;
use crate::run::{Attachments, Holder, Liveness, TransitionPermit};

/// One holder that keeps the environment up, with what the journal could prove
/// about it. Both `Live` and `Unknown` refuse a down (R3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownHolder {
    Live(RunId),
    /// The journal holds an unreleased attachment and cannot prove liveness.
    Unknown(RunId),
    /// The run is not closed but its owner gate is free: a successor settles it
    /// first, so a down refuses it too.
    Interrupted(RunId),
}
/// What a down may do now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownStart {
    /// Nothing but the session holds the environment: stop these children, wait
    /// for them to release their own attachments, then take the permit.
    Ready(DownChildren),
    /// Another run holds the environment. Nothing stopped and nothing is
    /// reported as stopped.
    Refused { holders: Vec<DownHolder> },
}

/// The session's own child runs a down asks to stop. Only these are named; the
/// session never sends a request to a run it does not own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownChildren {
    session: RunId,
    children: Vec<RunId>,
}
impl DownChildren {
    pub fn children(&self) -> &[RunId] {
        &self.children
    }

    /// Sends one ordinary stop request to a child and rings its doorbell. The
    /// child settles its own workloads and releases its own attachment.
    pub fn stop(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        reader: &StoreReader,
        request: &run::RequestId,
        child: RunId,
    ) -> Result<RunId, RunError> {
        if child != self.session && !self.children.contains(&child) {
            return Err(RunError::refused(
                "a down stops only the session and its own child runs",
            ));
        }
        request_stop(cx, root, reader, request, child)
    }

    /// The children that have not settled yet. A down waits under its own
    /// deadline; an unresolved child refuses the down and is reported.
    pub fn unresolved(&self, reader: &StoreReader) -> Observed<Vec<RunId>> {
        let mut unresolved = Vec::new();
        for child in &self.children {
            match crate::run::summary(reader, *child) {
                Observed::Verified(summary, _) if summary.phase != Phase::Closed => {
                    unresolved.push(*child)
                }
                Observed::Verified(_, _) => {}
                Observed::Absent(_) => unresolved.push(*child),
                Observed::Unknown(reason) => return Observed::Unknown(reason),
            }
        }
        Observed::Verified(unresolved, session_evidence())
    }

    /// Asks the session run itself to settle: the last step before the down
    /// applies (§8.7 step 4).
    pub fn settle_session(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        reader: &StoreReader,
        request: &run::RequestId,
    ) -> Result<(), RunError> {
        request_stop(cx, root, reader, request, self.session)?;
        Ok(())
    }

    /// The down run's transition permit. It requires the environment gate, and
    /// atomically re-checks that the only remaining holder is the session run
    /// once that run has entered its settle (R4). A run still holding the
    /// environment therefore refuses even after a `Ready` answer.
    pub fn permit<'r, 'root>(
        &self,
        down_run: &'r mut Run<'root>,
    ) -> Result<TransitionPermit<'r, 'root>, RunError> {
        if down_run.fence().resource.kind() != crate::id::ResourceKind::Env {
            return Err(RunError::broken("a down permit needs an environment run"));
        }
        Attachments::permit_exempt(down_run, self.session)
    }
}

/// Reads the environment's holders once, for a down requested from inside the
/// session.
#[derive(Debug)]
pub struct DownCoordination;
impl DownCoordination {
    pub fn begin(
        cx: &ObserveContext,
        root: &StateRoot,
        reader: &StoreReader,
        session: &SessionRef,
    ) -> Observed<DownStart> {
        let holders = Attachments::holders(cx, root, reader, session.environment());
        let holders = match holders {
            Observed::Verified(holders, evidence) => Observed::Verified(holders, evidence),
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        let observed = match holders {
            Observed::Verified(holders, _) => holders,
            _ => unreachable!("holders were read above"),
        };
        let children = match crate::run::children(reader, session.run()) {
            Observed::Verified(children, _) => children,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        let open: Vec<RunId> = children
            .iter()
            .filter(|child| child.phase != Phase::Closed)
            .map(|child| child.id)
            .collect();
        let refused = Self::foreign_holders(session.run(), &open, &observed);
        if !refused.is_empty() {
            return Observed::Verified(DownStart::Refused { holders: refused }, session_evidence());
        }
        Observed::Verified(
            DownStart::Ready(DownChildren {
                session: session.run(),
                children: open,
            }),
            session_evidence(),
        )
    }

    /// The holders a down refuses: everything but the session and its own child
    /// runs, live or not.
    fn foreign_holders(session: RunId, children: &[RunId], holders: &[Holder]) -> Vec<DownHolder> {
        holders
            .iter()
            .filter(|holder| holder.run != session && !children.contains(&holder.run))
            .map(|holder| match holder.liveness {
                Observed::Verified(Liveness::Live, _) => DownHolder::Live(holder.run),
                Observed::Verified(Liveness::Interrupted, _) => DownHolder::Interrupted(holder.run),
                Observed::Absent(_) | Observed::Unknown(_) => DownHolder::Unknown(holder.run),
            })
            .collect()
    }
}
