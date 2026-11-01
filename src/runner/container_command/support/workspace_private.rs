//! Fail-closed interface scaffold for private workspace preparation and apply.
//!
//! These types do not attest identity or transfer custody. The implementation
//! refuses before preparation effects, helper negotiation, or a process fork.
use effigy_containers::environment::BackendSelection;
use effigy_containers::policy::render::ComposeInvocation;
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::ctx::EffectContext;
use effigy_kernel::fs::{DirFd, DirectoryIdentity};
use effigy_kernel::id::CheckoutId;
use effigy_kernel::run::Run;
use std::ffi::OsString;
use std::path::PathBuf;
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

/// Exact caller-owned Compose inputs, including its live interpolation owner.
pub(in crate::runner) struct WorkspaceComposeInputOwner {
    workdir: PathBuf,
    program: OsString,
    env: Vec<(String, OsString)>,
    invocation: ComposeInvocation,
}

impl WorkspaceComposeInputOwner {
    /// Move the actual invocation; never reopen or recreate its file.
    pub(in crate::runner) fn from_invocation(
        workdir: PathBuf,
        program: OsString,
        env: Vec<(String, OsString)>,
        invocation: ComposeInvocation,
    ) -> Self {
        Self {
            workdir,
            program,
            env,
            invocation,
        }
    }
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

/// Reserved for a future reviewed completion and cleanup proof.
pub(in crate::runner) struct PrivateApplyComplete {
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

/// Refusal returns the exact borrowed Run and context to the caller.
pub(in crate::runner) struct PrivateNegotiationFailure<'a, 'r> {
    pub failure: PrivateApplyFailure,
    pub run: &'a mut Run<'r>,
    pub effect: &'a EffectContext,
}

/// Apply failure also returns the borrow of the caller's exact input owner.
pub(in crate::runner) struct PrivateExecutionFailure<'a, 'r, 'i> {
    pub failure: PrivateApplyFailure,
    pub run: &'a mut Run<'r>,
    pub effect: &'a EffectContext,
    pub inputs: &'i mut WorkspaceComposeInputOwner,
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
    run: &'a mut Run<'r>,
    effect: &'a EffectContext,
    _deadline: Instant,
) -> Result<PrivateBounded<'a, 'r>, PrivateNegotiationFailure<'a, 'r>> {
    Err(PrivateNegotiationFailure {
        failure: unavailable_apply(PrivateApplyStage::Negotiation),
        run,
        effect,
    })
}

impl<'a, 'r> PrivateBounded<'a, 'r> {
    pub(in crate::runner) fn run_compose_discard<'i>(
        self,
        inputs: &'i mut WorkspaceComposeInputOwner,
    ) -> Result<PrivateApplyComplete, PrivateExecutionFailure<'a, 'r, 'i>> {
        Err(PrivateExecutionFailure {
            failure: unavailable_apply(PrivateApplyStage::Apply),
            run: self._run,
            effect: self._effect,
            inputs,
        })
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

    #[test]
    fn quiet_preparation_refuses_without_writing() {
        use effigy_containers::environment::{BackendKind, BackendReason};

        let home = tempfile::tempdir().expect("isolated checkout");
        let root = DirFd::open(home.path()).expect("checkout descriptor");
        let identity = root.identity().expect("checkout identity");
        let backend = BackendSelection {
            kind: BackendKind::DockerEngine,
            engine: effigy_containers::BackendId::docker_compose(),
            context: "fixture".to_owned(),
            reason: BackendReason::UserPreference,
        };
        let request = || QuietPreparationRequest {
            root: &root,
            root_identity: identity,
            checkout: None,
            environment: "fixture",
            backend: &backend,
        };
        for failure in [
            prepare_workspace_quiet(request())
                .err()
                .expect("CLI refusal"),
            prepare_supervisor_quiet(request())
                .err()
                .expect("supervisor refusal"),
        ] {
            assert_eq!(failure.class, PrivateFailureClass::Unavailable);
            assert!(failure.custody.is_none());
        }
        assert!(std::fs::read_dir(home.path()).unwrap().next().is_none());
    }

    #[test]
    fn negotiation_and_apply_refusals_return_exact_owners_without_effects() {
        use effigy_kernel::ctx::{
            CancelToken, CleanupContext, Deadline, EffectClass, Limits, OperationContext, Placement,
        };
        use effigy_kernel::gate::Wait;
        use effigy_kernel::id::{
            CheckoutKind, ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId, Route,
            RunId, Surface, TaskId,
        };
        use effigy_kernel::observe::{Evidence, Observed};
        use effigy_kernel::outcome::Invocation;
        use effigy_kernel::root::StateRoot;
        use effigy_kernel::run::{BeginOpts, Effect, Fence, Reconciled, SettlementAdapter};
        use std::collections::BTreeMap;
        use std::time::Duration;

        struct NoEffects;
        impl SettlementAdapter for NoEffects {
            fn observe(&mut self, effect: &Effect, _: &CleanupContext) -> Observed<Fence> {
                Observed::Verified(
                    effect.spec.target.clone(),
                    Evidence::new("fixture".into(), None).unwrap(),
                )
            }
            fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
                Observed::Verified((), Evidence::new("fixture".into(), None).unwrap())
            }
            fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
                Observed::Verified(
                    Reconciled::Released,
                    Evidence::new("fixture".into(), None).unwrap(),
                )
            }
        }
        let home = tempfile::tempdir().expect("isolated home");
        let deadline = Instant::now() + Duration::from_secs(30);
        let cx = OperationContext::new(
            Deadline::At(deadline),
            CancelToken::default(),
            Limits {
                capture_tail: 1024,
                log_rotate: 4096,
                log_keep: 2,
                line_max: 512,
                queue_bytes: 2048,
                probe_concurrency: 2,
            },
            EffectClass::Mutate,
            Placement::Host,
        )
        .effects()
        .unwrap();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, [13; 16]);
        let task = TaskId::new(checkout, "", Surface::Published, "fixture", None).unwrap();
        let owner = ResourceId::environment(checkout, "fixture").unwrap();
        let invocation = ResolvedInvocation::new(InvocationSpec {
            owner,
            task,
            route: Route::Task,
            argv: vec![],
            cwd: "".into(),
            target: ExecutionTarget::Host,
            env: BTreeMap::new(),
            gates: vec![],
            attachments: vec![],
            class: EffectClass::Mutate,
            placement: Placement::Host,
            provenance: BTreeMap::new(),
        })
        .unwrap();
        let cleanup = cx.operation().cleanup(Duration::from_secs(5)).unwrap();
        let mut run = Run::begin(
            &cx,
            &root,
            &invocation,
            BeginOpts {
                id: RunId::new(1, [13; 10]).unwrap(),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: Invocation::validated(vec!["recover".into()], |_| true).unwrap(),
                custody: None,
            },
            &cleanup,
            &mut NoEffects,
        )
        .unwrap();
        let run_ptr = std::ptr::from_ref(&run);
        let before = run.effects().unwrap().len();
        let failure = match negotiate_private_bound(&mut run, &cx, deadline) {
            Ok(_) => panic!("unqualified capability was minted"),
            Err(failure) => failure,
        };
        assert!(std::ptr::eq(run_ptr, std::ptr::from_ref(&*failure.run)));
        assert!(std::ptr::eq(&cx, failure.effect));
        assert_eq!(
            failure.failure.disposition,
            PrivateApplyDisposition::RefusedBeforeFork
        );
        assert_eq!(failure.run.effects().unwrap().len(), before);
        effigy_containers::with_test_effigy_home(home.path(), || {
            let program = OsString::from("colima");
            let args = vec![OsString::from("compose"), OsString::from("up")];
            let env = vec![("FIXTURE".to_owned(), OsString::from("private-value"))];
            let invocation =
                effigy_containers::policy::render::compose_invocation_args(&program, &args, &env)
                    .expect("fixture invocation");
            let path = invocation.interpolation.as_ref().unwrap().path().to_owned();
            let mut inputs = WorkspaceComposeInputOwner::from_invocation(
                home.path().to_owned(),
                program,
                env,
                invocation,
            );
            let inputs_ptr = std::ptr::from_ref(&inputs);
            // Test-only construction exercises the consuming failure carrier;
            // the real negotiation above never mints this capability.
            let simulated = PrivateBounded {
                _run: failure.run,
                _effect: failure.effect,
                _deadline: deadline,
            };
            let error = match simulated.run_compose_discard(&mut inputs) {
                Ok(_) => panic!("private execution unexpectedly succeeded"),
                Err(error) => error,
            };
            assert!(std::ptr::eq(run_ptr, std::ptr::from_ref(&*error.run)));
            assert!(std::ptr::eq(&cx, error.effect));
            assert!(std::ptr::eq(inputs_ptr, std::ptr::from_ref(&*error.inputs)));
            assert!(
                path.exists(),
                "exact interpolation file remains with caller"
            );
            assert_eq!(
                error.failure.disposition,
                PrivateApplyDisposition::RefusedBeforeFork
            );
            assert_eq!(error.run.effects().unwrap().len(), before);
        });
        run.settle(&cleanup, &mut NoEffects).unwrap();
    }

    #[test]
    fn compose_owner_moves_the_real_interpolation_file() {
        use std::os::unix::fs::MetadataExt;

        let home = tempfile::tempdir().expect("isolated home");
        effigy_containers::with_test_effigy_home(home.path(), || {
            let program = OsString::from("colima");
            let args = vec![OsString::from("compose"), OsString::from("up")];
            let env = vec![("FIXTURE".to_owned(), OsString::from("private-value"))];
            let invocation =
                effigy_containers::policy::render::compose_invocation_args(&program, &args, &env)
                    .expect("fixture invocation");
            let path = invocation
                .interpolation
                .as_ref()
                .expect("file")
                .path()
                .to_owned();
            let before = std::fs::metadata(&path).expect("file identity");
            let owner = WorkspaceComposeInputOwner::from_invocation(
                home.path().to_owned(),
                program,
                env,
                invocation,
            );
            let retained = owner.invocation.interpolation.as_ref().expect("same owner");
            let after = std::fs::metadata(retained.path()).expect("retained file");
            assert_eq!(retained.path(), path);
            assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
            drop(owner);
            assert!(!path.exists(), "closed fixture owner removes its file");
        });
    }
}
