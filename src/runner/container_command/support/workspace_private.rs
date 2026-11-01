//! Fail-closed interface scaffold for private workspace preparation and apply.
//!
//! These types do not attest identity or transfer custody. The implementation
//! refuses before preparation effects, helper negotiation, or a process fork.
use effigy_containers::environment::BackendSelection;
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::ctx::EffectContext;
use effigy_kernel::fs::{DirFd, DirectoryIdentity};
use effigy_kernel::id::CheckoutId;
use effigy_kernel::run::Run;
use std::time::Instant;

/// Declarative expectations only; none of these fields prove current identity.
pub(in crate::runner) struct QuietPreparationRequest<'a> {
    pub root: &'a DirFd,
    pub root_identity: DirectoryIdentity,
    pub checkout: Option<CheckoutId>,
    pub environment: &'a str,
    pub backend: &'a BackendSelection,
}

/// In-process preparation owner. No caller can construct one from a policy.
pub(in crate::runner) struct QuietPreparedWorkspace {
    policy: EffectiveContainerPolicy,
}

impl QuietPreparedWorkspace {
    pub(in crate::runner) fn policy(&self) -> &EffectiveContainerPolicy {
        &self.policy
    }
}

/// A separate supervisor-local owner; it is never a serialized CLI lease.
pub(in crate::runner) struct QuietSupervisorPreparedEffects {
    _private: (),
}

/// Placeholder for retained preparation custody; no constructor is exposed.
pub(in crate::runner) struct QuietPreparationCustody {
    _private: (),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) enum QuietPreparationStage {
    Cli,
    Supervisor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) enum PrivateFailureClass {
    Unavailable,
}

pub(in crate::runner) struct QuietPreparationFailure {
    pub stage: QuietPreparationStage,
    pub class: PrivateFailureClass,
    pub custody: Option<Box<QuietPreparationCustody>>,
}

pub(in crate::runner) fn prepare_workspace_quiet(
    _request: QuietPreparationRequest<'_>,
) -> Result<QuietPreparedWorkspace, QuietPreparationFailure> {
    Err(QuietPreparationFailure {
        stage: QuietPreparationStage::Cli,
        class: PrivateFailureClass::Unavailable,
        custody: None,
    })
}

pub(in crate::runner) fn prepare_supervisor_quiet(
    _request: QuietPreparationRequest<'_>,
) -> Result<QuietSupervisorPreparedEffects, QuietPreparationFailure> {
    Err(QuietPreparationFailure {
        stage: QuietPreparationStage::Supervisor,
        class: PrivateFailureClass::Unavailable,
        custody: None,
    })
}

/// Placeholder for exact caller-owned Compose inputs, including interpolation.
/// No constructor is exposed until the resource-owning path is reviewed.
pub(in crate::runner) struct WorkspaceComposeInputOwner {
    _private: (),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) enum PrivateApplyDisposition {
    RefusedBeforeFork,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) enum PrivateApplyStage {
    Negotiation,
    Apply,
}

/// Reserved for any separately retained unresolved helper or process owner.
pub(in crate::runner) struct PrivateApplyCustody {
    _private: (),
}

pub(in crate::runner) struct PrivateApplyFailure {
    pub stage: PrivateApplyStage,
    pub class: PrivateFailureClass,
    pub disposition: PrivateApplyDisposition,
    pub exit_status: Option<i32>,
    pub cleanup: Option<PrivateFailureClass>,
    pub custody: Option<Box<PrivateApplyCustody>>,
}

/// Non-cloneable borrow of the actual Env Run. No constructor is exposed.
pub(in crate::runner) struct PrivateBounded<'a, 'r> {
    _run: &'a mut Run<'r>,
    _effect: &'a EffectContext,
    _deadline: Instant,
}

fn unavailable_apply(stage: PrivateApplyStage) -> PrivateApplyFailure {
    PrivateApplyFailure {
        stage,
        class: PrivateFailureClass::Unavailable,
        disposition: PrivateApplyDisposition::RefusedBeforeFork,
        exit_status: None,
        cleanup: None,
        custody: None,
    }
}

pub(in crate::runner) fn negotiate_private_bound<'a, 'r>(
    _run: &'a mut Run<'r>,
    _effect: &'a EffectContext,
    _deadline: Instant,
) -> Result<PrivateBounded<'a, 'r>, PrivateApplyFailure> {
    Err(unavailable_apply(PrivateApplyStage::Negotiation))
}

impl PrivateBounded<'_, '_> {
    pub(in crate::runner) fn run_compose_discard(
        &mut self,
        _inputs: &mut WorkspaceComposeInputOwner,
    ) -> Result<(), PrivateApplyFailure> {
        Err(unavailable_apply(PrivateApplyStage::Apply))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_failure_reports_refusal_before_fork() {
        let failure = unavailable_apply(PrivateApplyStage::Negotiation);
        assert_eq!(failure.stage, PrivateApplyStage::Negotiation);
        assert_eq!(failure.class, PrivateFailureClass::Unavailable);
        assert_eq!(
            failure.disposition,
            PrivateApplyDisposition::RefusedBeforeFork
        );
        assert!(failure.exit_status.is_none());
        assert!(failure.cleanup.is_none());
        assert!(failure.custody.is_none());
    }
}
