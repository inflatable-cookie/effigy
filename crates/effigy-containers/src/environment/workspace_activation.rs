//! Reserved workspace activation. The owning session is admitted and retained
//! before Env admission; no publication callback can reverse Task/Env order.

use effigy_kernel::gate::Wait;
use effigy_kernel::id::ResolvedInvocation;
use effigy_kernel::run::{BeginOpts, Run};
use effigy_kernel::workspace_session::{
    ActivationDecision, PublishInputs, SessionRef, SessionSupervisor,
    WorkspaceActivationReservation,
};

use super::{EnvContext, EnvError, EnvRef, EnvRun, EnvSettlement, Operation};

#[derive(Debug)]
pub enum WorkspaceBegin<'r> {
    /// Exactly the current selection: no new run, effect or generation.
    Attach(SessionRef),
    Activate(Box<WorkspaceActivation<'r>>),
}

/// Admission failure and any exact owners that could not yet be settled.
#[derive(Debug)]
pub struct WorkspaceBeginFailure<'r> {
    pub error: EnvError,
    pub custody: Option<Box<GuardedAbortCustody<'r>>>,
}

impl From<EnvError> for WorkspaceBeginFailure<'_> {
    fn from(error: EnvError) -> Self {
        Self {
            error,
            custody: None,
        }
    }
}

/// Ownership retained when finalising an activation fails.
#[derive(Debug)]
pub enum WorkspaceFinishCustody<'r> {
    Activation(Box<WorkspaceActivation<'r>>),
    Session {
        supervisor: Box<SessionSupervisor<'r>>,
        session: SessionRef,
        environment: Box<Run<'r>>,
        /// The commit receipt identifies the selection; re-read its durable
        /// state because this retained copy predates selector readback.
        obligation: Box<Option<effigy_kernel::workspace_session::SelectionObligation>>,
    },
}

/// Exact owners retained while an uncommitted guarded activation is aborted.
#[derive(Debug)]
pub struct GuardedAbortCustody<'r> {
    state: GuardedAbortState<'r>,
}

#[derive(Debug)]
enum GuardedAbortState<'r> {
    Activation(Box<WorkspaceActivation<'r>>),
    RawSession {
        root: &'r effigy_kernel::root::StateRoot,
        run: Box<Run<'r>>,
        reservation: WorkspaceActivationReservation,
        attempt: effigy_kernel::run::RequestId,
    },
    AdmissionUnknown {
        root: &'r effigy_kernel::root::StateRoot,
        supervisor: Box<SessionSupervisor<'r>>,
        reservation: WorkspaceActivationReservation,
        attempt: effigy_kernel::run::RequestId,
    },
    Environment {
        root: &'r effigy_kernel::root::StateRoot,
        environment: Box<Run<'r>>,
        supervisor: Box<SessionSupervisor<'r>>,
        reservation: WorkspaceActivationReservation,
        attempt: effigy_kernel::run::RequestId,
    },
    Session {
        root: &'r effigy_kernel::root::StateRoot,
        supervisor: Box<SessionSupervisor<'r>>,
        reservation: WorkspaceActivationReservation,
        attempt: effigy_kernel::run::RequestId,
    },
    Reservation {
        root: &'r effigy_kernel::root::StateRoot,
        reservation: WorkspaceActivationReservation,
        attempt: effigy_kernel::run::RequestId,
    },
}

impl<'r> GuardedAbortCustody<'r> {
    /// Recover the untouched activation when abort was refused before settlement.
    pub fn into_activation(self) -> Result<Box<WorkspaceActivation<'r>>, Box<Self>> {
        match self.state {
            GuardedAbortState::Activation(activation) => Ok(activation),
            state => Err(Box::new(Self { state })),
        }
    }

    /// Retry from the retained stage with fresh cleanup authority.
    pub fn retry(self, ctx: &EnvContext<'r>, wait: Wait) -> Result<(), (EnvError, Box<Self>)> {
        self.state
            .retry(ctx, wait)
            .map_err(|(error, state)| (error, Box::new(Self { state: *state })))
    }
}

impl<'r> GuardedAbortState<'r> {
    fn retry(self, ctx: &EnvContext<'r>, wait: Wait) -> Result<(), (EnvError, Box<Self>)> {
        let self_ = match self {
            Self::Activation(activation) => return (*activation).abort_guarded_inner(ctx, wait),
            other => other,
        };
        let root = match &self_ {
            Self::Activation(_) => unreachable!(),
            Self::Environment { root, .. }
            | Self::AdmissionUnknown { root, .. }
            | Self::RawSession { root, .. }
            | Self::Session { root, .. }
            | Self::Reservation { root, .. } => *root,
        };
        if !std::ptr::eq(root, ctx.root) {
            return Err((
                EnvError::Invalid("workspace cleanup uses a different state root".into()),
                Box::new(self_),
            ));
        }
        match self_ {
            Self::Activation(_) => unreachable!(),
            Self::RawSession {
                run,
                reservation,
                attempt,
                ..
            } => {
                let cleanup = match ctx.cleanup() {
                    Ok(cleanup) => cleanup,
                    Err(error) => {
                        return Err((
                            error,
                            Box::new(Self::RawSession {
                                root,
                                run,
                                reservation,
                                attempt,
                            }),
                        ))
                    }
                };
                match run.settle_workspace_recoverable(&cleanup, &mut EnvSettlement) {
                    Ok(_) => Self::Reservation {
                        root,
                        reservation,
                        attempt,
                    }
                    .retry(ctx, wait),
                    Err((error, run)) => Err((
                        EnvError::Kernel(error),
                        Box::new(Self::RawSession {
                            root,
                            run,
                            reservation,
                            attempt,
                        }),
                    )),
                }
            }
            Self::AdmissionUnknown {
                supervisor,
                reservation,
                attempt,
                ..
            } => {
                let unclaimed = match effigy_kernel::store::Store::read(&ctx.observe(), root) {
                    effigy_kernel::observe::Observed::Verified(reader, _) => matches!(
                        WorkspaceActivationReservation::recover(&reader, reservation.environment()),
                        effigy_kernel::observe::Observed::Verified(Some(current), _)
                            if current.id() == reservation.id()
                                && current.generation() == reservation.generation()
                                && current.state() == effigy_kernel::workspace_session::ActivationReservationState::Reserved
                                && current.record().environment_run.is_none()
                    ),
                    _ => false,
                };
                if unclaimed {
                    Self::Session {
                        root,
                        supervisor,
                        reservation,
                        attempt,
                    }
                    .retry(ctx, wait)
                } else {
                    Err((
                        EnvError::Invalid(
                            "environment admission uncertain; reservation remains recoverable"
                                .into(),
                        ),
                        Box::new(Self::AdmissionUnknown {
                            root,
                            supervisor,
                            reservation,
                            attempt,
                        }),
                    ))
                }
            }
            Self::Environment {
                environment,
                supervisor,
                reservation,
                attempt,
                ..
            } => {
                let cleanup = match ctx.cleanup() {
                    Ok(cleanup) => cleanup,
                    Err(error) => {
                        return Err((
                            error,
                            Box::new(Self::Environment {
                                root,
                                environment,
                                supervisor,
                                reservation,
                                attempt,
                            }),
                        ))
                    }
                };
                match environment.settle_workspace_recoverable(&cleanup, &mut EnvSettlement) {
                    Ok(_) => Self::Session {
                        root,
                        supervisor,
                        reservation,
                        attempt,
                    }
                    .retry(ctx, wait),
                    Err((error, environment)) => Err((
                        EnvError::Kernel(error),
                        Box::new(Self::Environment {
                            root,
                            environment,
                            supervisor,
                            reservation,
                            attempt,
                        }),
                    )),
                }
            }
            Self::Session {
                supervisor,
                reservation,
                attempt,
                ..
            } => {
                let cleanup = match ctx.cleanup() {
                    Ok(cleanup) => cleanup,
                    Err(error) => {
                        return Err((
                            error,
                            Box::new(Self::Session {
                                root,
                                supervisor,
                                reservation,
                                attempt,
                            }),
                        ))
                    }
                };
                match supervisor.settle_recoverable(&cleanup, &mut EnvSettlement) {
                    Ok(_) => Self::Reservation {
                        root,
                        reservation,
                        attempt,
                    }
                    .retry(ctx, wait),
                    Err((error, supervisor)) => Err((
                        EnvError::Kernel(error),
                        Box::new(Self::Session {
                            root,
                            supervisor,
                            reservation,
                            attempt,
                        }),
                    )),
                }
            }
            Self::Reservation {
                reservation,
                attempt,
                ..
            } => {
                let result = ctx.cleanup().and_then(|cleanup| {
                    let effects = cleanup.operation().effects().map_err(|error| {
                        EnvError::Invalid(format!("cleanup authority unavailable: {error}"))
                    })?;
                    reservation
                        .cancel_for_attempt(&effects, ctx.root, wait, attempt)
                        .map(|_| ())
                        .map_err(EnvError::Kernel)
                });
                result.map_err(|error| {
                    (
                        error,
                        Box::new(Self::Reservation {
                            root,
                            reservation,
                            attempt,
                        }),
                    )
                })
            }
        }
    }
}

/// Field order releases Env before Task if a crash/unwind interrupts the owner.
/// Such a drop is interrupted evidence, never successful settlement.
#[derive(Debug)]
pub struct WorkspaceActivation<'r> {
    environment: EnvRun<'r>,
    supervisor: SessionSupervisor<'r>,
    session: SessionRef,
    reservation: WorkspaceActivationReservation,
    selected: bool,
    committed: Option<effigy_kernel::workspace_session::SelectionObligation>,
    supervisor_id: String,
    attempt: effigy_kernel::run::RequestId,
}

/// The same session and environment run, with only Env released. This state
/// cannot publish or mutate the environment until evidence is revalidated.
#[derive(Debug)]
pub struct ParkedWorkspaceActivation<'r> {
    _environment: super::reconcile::ParkedEnvRun<'r>,
    _supervisor: SessionSupervisor<'r>,
    session: SessionRef,
    _reservation: WorkspaceActivationReservation,
    _supervisor_id: String,
    _attempt: effigy_kernel::run::RequestId,
}

/// Custody retained when a parked activation cannot be safely cancelled.
#[derive(Debug)]
pub enum ParkedAbortCustody<'r> {
    Activation(Box<ParkedWorkspaceActivation<'r>>),
    Environment {
        environment: effigy_kernel::run::WorkspaceCleanupCustody<'r>,
        supervisor: Box<SessionSupervisor<'r>>,
        reservation: WorkspaceActivationReservation,
        attempt: effigy_kernel::run::RequestId,
    },
    Session {
        supervisor: Box<SessionSupervisor<'r>>,
        reservation: WorkspaceActivationReservation,
        attempt: effigy_kernel::run::RequestId,
    },
    Reservation {
        reservation: WorkspaceActivationReservation,
        attempt: effigy_kernel::run::RequestId,
    },
}

impl<'r> ParkedWorkspaceActivation<'r> {
    pub fn session(&self) -> &SessionRef {
        &self.session
    }

    /// Release a reacquired Env gate before running integration cleanup.
    pub fn repark_for_cleanup(mut self) -> Result<Self, (EnvError, Box<Self>)> {
        match self._environment.repark() {
            Ok(environment) => {
                self._environment = environment;
                Ok(self)
            }
            Err((error, environment)) => {
                self._environment = *environment;
                Err((error, Box::new(self)))
            }
        }
    }

    pub fn integration_snapshot(
        &self,
    ) -> Result<
        std::collections::BTreeMap<
            effigy_kernel::run::WorkspaceIntegrationStep,
            effigy_kernel::run::WorkspaceIntegrationState,
        >,
        EnvError,
    > {
        self._environment.integration_snapshot(&self._supervisor)
    }

    /// Record only step names and status; never store secret values.
    pub fn record_integration_step(
        &mut self,
        ctx: &EnvContext<'_>,
        step: effigy_kernel::run::WorkspaceIntegrationStep,
        state: effigy_kernel::run::WorkspaceIntegrationState,
    ) -> Result<(), EnvError> {
        if !self._environment.owns_root(ctx.root) {
            return Err(EnvError::Invalid(
                "workspace integration uses a different state root".into(),
            ));
        }
        if state == effigy_kernel::run::WorkspaceIntegrationState::Cleared {
            let cleanup = ctx.cleanup()?;
            let cx = cleanup.operation().effects().map_err(|error| {
                EnvError::Invalid(format!("cleanup authority unavailable: {error}"))
            })?;
            self._environment
                .record_integration_step(&mut self._supervisor, &cx, step, state)
        } else {
            self._environment
                .record_integration_step(&mut self._supervisor, &ctx.cx, step, state)
        }
    }

    /// Reacquire Env and require an actual fresh observation against the
    /// retained journal, session and creator facts before exposing effects.
    /// Failure returns this exact owner, parked or guarded, for retry/cleanup.
    pub fn reacquire(
        self,
        backend: &mut dyn super::EnvironmentBackend,
        observe: impl FnOnce(
            &effigy_kernel::run::WorkspaceResumeView<'_>,
            std::time::Instant,
            &mut dyn super::EnvironmentBackend,
        ) -> effigy_kernel::observe::Observed<
            effigy_kernel::run::WorkspaceResumeFacts,
        >,
    ) -> Result<WorkspaceActivation<'r>, (EnvError, Box<Self>)> {
        let parked = self.repark_for_cleanup()?;
        match parked.integration_snapshot() {
            Ok(steps)
                if steps.values().all(|state| {
                    *state == effigy_kernel::run::WorkspaceIntegrationState::Completed
                }) => {}
            Ok(_) => {
                return Err((
                    EnvError::Invalid("integration effects unsettled; re-run up".into()),
                    Box::new(parked),
                ));
            }
            Err(error) => return Err((error, Box::new(parked))),
        }
        let Self {
            _environment,
            _supervisor,
            session,
            _reservation,
            _supervisor_id,
            _attempt,
        } = parked;
        match _environment.reacquire(&_supervisor, backend, observe) {
            Ok(environment) => Ok(WorkspaceActivation {
                environment,
                supervisor: _supervisor,
                session,
                reservation: _reservation,
                selected: false,
                committed: None,
                supervisor_id: _supervisor_id,
                attempt: _attempt,
            }),
            Err((error, environment)) => Err((
                error,
                Box::new(Self {
                    _environment: *environment,
                    _supervisor,
                    session,
                    _reservation,
                    _supervisor_id,
                    _attempt,
                }),
            )),
        }
    }

    /// Settle both exact owners before attempting tagged reservation cleanup.
    /// Unknown or incomplete cleanup leaves the reservation recoverable.
    pub fn abort(
        self,
        ctx: &EnvContext<'_>,
        wait: Wait,
    ) -> Result<(), (EnvError, Box<ParkedAbortCustody<'r>>)> {
        use effigy_kernel::outcome::CleanupOutcome;
        if !self._environment.owns_root(ctx.root) {
            return Err((
                EnvError::Invalid("parked activation cleanup uses a different state root".into()),
                Box::new(ParkedAbortCustody::Activation(Box::new(self))),
            ));
        }
        let self_ = match self.repark_for_cleanup() {
            Ok(parked) => parked,
            Err((error, parked)) => {
                return Err((error, Box::new(ParkedAbortCustody::Activation(parked))))
            }
        };
        let self_ = match self_.integration_snapshot() {
            Ok(steps) if steps.is_empty() => self_,
            Ok(_) => {
                return Err((
                    EnvError::Invalid("integration effects unsettled; re-run up".into()),
                    Box::new(ParkedAbortCustody::Activation(Box::new(self_))),
                ))
            }
            Err(error) => {
                return Err((
                    error,
                    Box::new(ParkedAbortCustody::Activation(Box::new(self_))),
                ))
            }
        };
        let cleanup = match ctx.cleanup() {
            Ok(cleanup) => cleanup,
            Err(error) => {
                return Err((
                    error,
                    Box::new(ParkedAbortCustody::Activation(Box::new(self_))),
                ))
            }
        };
        let Self {
            _environment,
            _supervisor,
            session: _,
            _reservation,
            _supervisor_id: _,
            _attempt,
        } = self_;
        match _environment.settle(&cleanup) {
            Err((error, environment)) => {
                return Err((
                    error,
                    Box::new(ParkedAbortCustody::Environment {
                        environment,
                        supervisor: Box::new(_supervisor),
                        reservation: _reservation,
                        attempt: _attempt,
                    }),
                ))
            }
            Ok(CleanupOutcome::Incomplete) => {
                return Err((
                    EnvError::Invalid("integration effects unsettled; re-run up".into()),
                    Box::new(ParkedAbortCustody::Session {
                        supervisor: Box::new(_supervisor),
                        reservation: _reservation,
                        attempt: _attempt,
                    }),
                ))
            }
            Ok(_) => {}
        }
        let cleanup = match ctx.cleanup() {
            Ok(cleanup) => cleanup,
            Err(error) => {
                return Err((
                    error,
                    Box::new(ParkedAbortCustody::Session {
                        supervisor: Box::new(_supervisor),
                        reservation: _reservation,
                        attempt: _attempt,
                    }),
                ))
            }
        };
        match _supervisor.settle_recoverable(&cleanup, &mut EnvSettlement) {
            Err((error, supervisor)) => {
                return Err((
                    EnvError::Kernel(error),
                    Box::new(ParkedAbortCustody::Session {
                        supervisor,
                        reservation: _reservation,
                        attempt: _attempt,
                    }),
                ))
            }
            Ok(outcome) if outcome.cleanup() == CleanupOutcome::Incomplete => {
                return Err((
                    EnvError::Invalid("integration effects unsettled; re-run up".into()),
                    Box::new(ParkedAbortCustody::Reservation {
                        reservation: _reservation,
                        attempt: _attempt,
                    }),
                ))
            }
            Ok(_) => {}
        }
        let result = ctx.cleanup().and_then(|cleanup| {
            let effects = cleanup.operation().effects().map_err(|error| {
                EnvError::Invalid(format!("cleanup authority unavailable: {error}"))
            })?;
            _reservation
                .cancel_for_attempt(&effects, ctx.root, wait, _attempt)
                .map(|_| ())
                .map_err(EnvError::Kernel)
        });
        result.map_err(|error| {
            (
                error,
                Box::new(ParkedAbortCustody::Reservation {
                    reservation: _reservation,
                    attempt: _attempt,
                }),
            )
        })
    }
}

impl<'r> WorkspaceActivation<'r> {
    /// Release Env after the one apply and target bind, retaining both exact
    /// owners and the creator evidence. Failure returns guarded custody.
    pub fn park(
        self,
        deadline: std::time::Instant,
    ) -> Result<ParkedWorkspaceActivation<'r>, (EnvError, Box<Self>)> {
        if self.committed.is_some() || self.selected {
            return Err((
                EnvError::Invalid("committed activation cannot be parked".into()),
                Box::new(self),
            ));
        }
        let Self {
            environment,
            supervisor,
            session,
            reservation,
            selected,
            committed,
            supervisor_id,
            attempt,
        } = self;
        match environment.park_workspace(&reservation, &session, deadline) {
            Ok(environment) => Ok(ParkedWorkspaceActivation {
                _environment: environment,
                _supervisor: supervisor,
                session,
                _reservation: reservation,
                _supervisor_id: supervisor_id,
                _attempt: attempt,
            }),
            Err((error, environment)) => Err((
                error,
                Box::new(Self {
                    environment: *environment,
                    supervisor,
                    session,
                    reservation,
                    selected,
                    committed,
                    supervisor_id,
                    attempt,
                }),
            )),
        }
    }
    /// Decide and freeze one exact session binding with Env free. The kernel's
    /// exclusive staging and private Task slot serialize concurrent successors;
    /// a loser cannot replace the staged run or publish another session's S.
    pub fn begin(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        inputs: &PublishInputs,
        invocation: &ResolvedInvocation,
        options: BeginOpts,
        supervisor_id: &str,
    ) -> Result<WorkspaceBegin<'r>, EnvError> {
        if supervisor_id.trim().is_empty() || supervisor_id.contains('\0') {
            return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "workspace supervisor identity is empty or malformed",
            )));
        }
        let decision = WorkspaceActivationReservation::decide(
            &ctx.cx,
            ctx.root,
            &env.resource,
            inputs,
            options.wait,
        )
        .map_err(EnvError::Kernel)?;
        Self::begin_decided(ctx, env, decision, invocation, options, supervisor_id)
    }

    /// Begin a workspace while retaining exact cleanup custody on failure.
    pub fn begin_recoverable(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        inputs: &PublishInputs,
        invocation: &ResolvedInvocation,
        options: BeginOpts,
        supervisor_id: &str,
    ) -> Result<WorkspaceBegin<'r>, WorkspaceBeginFailure<'r>> {
        if supervisor_id.trim().is_empty() || supervisor_id.contains('\0') {
            return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "workspace supervisor identity is empty or malformed",
            ))
            .into());
        }
        let decision = WorkspaceActivationReservation::decide(
            &ctx.cx,
            ctx.root,
            &env.resource,
            inputs,
            options.wait,
        )
        .map_err(EnvError::Kernel)?;
        Self::begin_decided_recoverable(ctx, env, decision, invocation, options, supervisor_id)
    }

    /// Observe decision inputs under the kernel's environment gate before
    /// admitting the session. The same absolute deadline bounds the observation.
    pub fn begin_observing(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        invocation: &ResolvedInvocation,
        options: BeginOpts,
        supervisor_id: &str,
        deadline: std::time::Instant,
        observe: impl FnOnce(std::time::Instant) -> Result<PublishInputs, effigy_kernel::run::RunError>
            + Send
            + 'static,
    ) -> Result<WorkspaceBegin<'r>, EnvError> {
        if supervisor_id.trim().is_empty() || supervisor_id.contains('\0') {
            return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "workspace supervisor identity is empty or malformed",
            )));
        }
        let bounded = ctx.bounded_until(deadline)?;
        let decision = WorkspaceActivationReservation::decide_observing(
            &bounded.cx,
            bounded.root,
            &env.resource,
            options.wait,
            deadline,
            observe,
        )
        .map_err(EnvError::Kernel)?;
        Self::begin_decided(&bounded, env, decision, invocation, options, supervisor_id)
    }

    /// Gate-scoped decision observation with recoverable admission custody.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_observing_recoverable(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        invocation: &ResolvedInvocation,
        options: BeginOpts,
        supervisor_id: &str,
        deadline: std::time::Instant,
        observe: impl FnOnce(std::time::Instant) -> Result<PublishInputs, effigy_kernel::run::RunError>
            + Send
            + 'static,
    ) -> Result<WorkspaceBegin<'r>, WorkspaceBeginFailure<'r>> {
        if supervisor_id.trim().is_empty() || supervisor_id.contains('\0') {
            return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "workspace supervisor identity is empty or malformed",
            ))
            .into());
        }
        let bounded = ctx.bounded_until(deadline)?;
        let decision = WorkspaceActivationReservation::decide_observing(
            &bounded.cx,
            bounded.root,
            &env.resource,
            options.wait,
            deadline,
            observe,
        )
        .map_err(EnvError::Kernel)?;
        Self::begin_decided_recoverable(&bounded, env, decision, invocation, options, supervisor_id)
    }

    fn begin_decided(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        decision: ActivationDecision,
        invocation: &ResolvedInvocation,
        options: BeginOpts,
        supervisor_id: &str,
    ) -> Result<WorkspaceBegin<'r>, EnvError> {
        Self::begin_decided_recoverable(ctx, env, decision, invocation, options, supervisor_id)
            .map_err(|failure| failure.error)
    }

    fn begin_decided_recoverable(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        decision: ActivationDecision,
        invocation: &ResolvedInvocation,
        options: BeginOpts,
        supervisor_id: &str,
    ) -> Result<WorkspaceBegin<'r>, WorkspaceBeginFailure<'r>> {
        let wait = options.wait;
        let reservation = match decision {
            ActivationDecision::Attach(session) => return Ok(WorkspaceBegin::Attach(session)),
            ActivationDecision::Activate(reservation) => *reservation,
        };
        let attempt = fresh_attempt().map_err(|error| EnvError::Cleanup {
            cause: Box::new(error),
            failures: vec![EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "attempt identity unavailable; reservation remains recoverable",
            ))],
        })?;
        if let Err(error) = reservation.stage_for_attempt(
            &ctx.cx,
            ctx.root,
            &invocation.spec().owner,
            options.id,
            attempt,
        ) {
            return Err(cleanup_failed_reservation(
                ctx,
                &reservation,
                attempt,
                EnvError::Kernel(error),
                None,
                None,
            )
            .into());
        }
        let cleanup = match ctx.cleanup() {
            Ok(cleanup) => cleanup,
            Err(error) => {
                return Err(cleanup_failed_reservation(
                    ctx,
                    &reservation,
                    attempt,
                    error,
                    None,
                    None,
                )
                .into())
            }
        };
        let run_id = options.id;
        let next = options.next.clone();
        let owner = invocation.spec().owner.clone();
        let (session_run, session) = match SessionRef::admit_reserved_for_attempt(
            &ctx.cx,
            ctx.root,
            invocation,
            options,
            &cleanup,
            &mut EnvSettlement,
            &reservation,
            effigy_kernel::supervise::Placement::Detached,
            attempt,
        ) {
            Ok(value) => value,
            Err(error) => {
                return Err(cleanup_failed_reservation(
                    ctx,
                    &reservation,
                    attempt,
                    EnvError::Kernel(error),
                    Some((&owner, run_id, next)),
                    None,
                )
                .into())
            }
        };
        let supervisor = match SessionSupervisor::adopt_recoverable(session_run, supervisor_id) {
            Ok(supervisor) => supervisor,
            Err((error, run)) => {
                let cause = EnvError::Kernel(error);
                let custody = GuardedAbortState::RawSession {
                    root: ctx.root,
                    run,
                    reservation,
                    attempt,
                };
                return match custody.retry(ctx, wait) {
                    Ok(()) => Err(cause.into()),
                    Err((cleanup_error, custody)) => Err(WorkspaceBeginFailure {
                        error: EnvError::Cleanup {
                            cause: Box::new(cause),
                            failures: vec![cleanup_error],
                        },
                        custody: Some(Box::new(GuardedAbortCustody { state: *custody })),
                    }),
                };
            }
        };
        let environment =
            match EnvRun::begin_claimed_recoverable(ctx, env, Operation::Up, wait, &reservation) {
                Ok(environment) => environment,
                Err(failure) => {
                    let custody = match failure.run {
                        Some(environment) => GuardedAbortState::Environment {
                            root: ctx.root,
                            environment,
                            supervisor: Box::new(supervisor),
                            reservation,
                            attempt,
                        },
                        None => GuardedAbortState::AdmissionUnknown {
                            root: ctx.root,
                            supervisor: Box::new(supervisor),
                            reservation,
                            attempt,
                        },
                    };
                    return match custody.retry(ctx, wait) {
                        Ok(()) => Err(failure.error.into()),
                        Err((cleanup_error, custody)) => Err(WorkspaceBeginFailure {
                            error: EnvError::Cleanup {
                                cause: Box::new(failure.error),
                                failures: vec![cleanup_error],
                            },
                            custody: Some(Box::new(GuardedAbortCustody { state: *custody })),
                        }),
                    };
                }
            };
        Ok(WorkspaceBegin::Activate(Box::new(Self {
            environment,
            supervisor,
            session,
            reservation,
            selected: false,
            committed: None,
            supervisor_id: supervisor_id.to_owned(),
            attempt,
        })))
    }

    /// Apply under the same claimed Env owner used for target binding and
    /// publication. Existing live holders preserve the backend and selection.
    /// The returned receipt names exactly the containers this activation
    /// created, and nothing it attached (effigy#510).
    pub fn apply_with(
        &mut self,
        request: super::Up<'_>,
        apply: impl FnOnce() -> Result<(), super::BackendError>,
    ) -> Result<super::BringUp, EnvError> {
        super::ops::reconcile_in(&mut self.environment, request, None, apply, |run| {
            run.holders()
        })
    }

    pub fn apply_sealed_with(
        &mut self,
        request: super::Up<'_>,
        sealed: &effigy_kernel::objects::SealedPreflight,
        preflight: &super::CreationPreflight,
        apply: impl FnOnce() -> Result<(), super::BackendError>,
    ) -> Result<super::BringUp, EnvError> {
        super::ops::reconcile_in(
            &mut self.environment,
            request,
            Some((sealed, preflight)),
            apply,
            |run| run.holders(),
        )
    }

    pub fn supervisor_id(&self) -> &str {
        &self.supervisor_id
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
        &mut self.supervisor
    }

    pub fn current_plan(
        &self,
    ) -> Result<effigy_kernel::workspace_session::ActivationPlan, EnvError> {
        self.environment.workspace_plan(&self.reservation)
    }

    /// Atomic commit retains both exact owning runs and the qualified launch.
    /// Returning its durable obligation grants no claim that selectors moved.
    pub fn commit(
        &mut self,
        receipt: &effigy_kernel::workspace_session::SelectionReceipt,
        launch: &effigy_kernel::workspace_session::SessionLaunch,
    ) -> Result<effigy_kernel::workspace_session::CommittedActivation, EnvError> {
        let committed = self.environment.commit_workspace_selection(
            &self.reservation,
            &self.supervisor,
            receipt,
            launch,
        )?;
        self.committed = committed.selection_obligation().cloned();
        Ok(committed)
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
    pub fn finish_selected(
        self,
    ) -> Result<(SessionSupervisor<'r>, SessionRef), (EnvError, Box<WorkspaceFinishCustody<'r>>)>
    {
        if !self.selected {
            return Err((
                EnvError::Kernel(effigy_kernel::run::RunError::refused(
                    "workspace selection has not been verified",
                )),
                Box::new(WorkspaceFinishCustody::Activation(Box::new(self))),
            ));
        }
        if let Err((error, environment)) = self.environment.finish_recoverable(true) {
            return Err((
                error,
                Box::new(WorkspaceFinishCustody::Session {
                    supervisor: Box::new(self.supervisor),
                    session: self.session,
                    environment,
                    obligation: Box::new(self.committed),
                }),
            ));
        }
        Ok((self.supervisor, self.session))
    }

    /// An interrupted activation can hand its still-owned supervisor to the
    /// recovery driver without claiming successful selection. Env settles first;
    /// the durable Pending obligation continues to block successors.
    pub fn interrupt_for_recovery(
        self,
    ) -> Result<(SessionSupervisor<'r>, SessionRef), (EnvError, Box<WorkspaceFinishCustody<'r>>)>
    {
        if self.committed.is_none() || self.selected {
            return Err((
                EnvError::Kernel(effigy_kernel::run::RunError::refused(
                    "only an unfinished committed activation enters selection recovery",
                )),
                Box::new(WorkspaceFinishCustody::Activation(Box::new(self))),
            ));
        }
        if let Err((error, environment)) = self.environment.finish_recoverable(false) {
            return Err((
                error,
                Box::new(WorkspaceFinishCustody::Session {
                    supervisor: Box::new(self.supervisor),
                    session: self.session,
                    environment,
                    obligation: Box::new(self.committed),
                }),
            ));
        }
        Ok((self.supervisor, self.session))
    }

    /// Pre-commit failure: settle both bound runs before cancelling. A committed
    /// generation instead remains fenced by its durable selection obligation;
    /// its owner must call selection recovery/abandon, never restore a predecessor.
    pub fn abort_guarded_recoverable(
        self,
        ctx: &EnvContext<'r>,
        wait: Wait,
    ) -> Result<(), (EnvError, Box<GuardedAbortCustody<'r>>)> {
        self.abort_guarded_inner(ctx, wait)
            .map_err(|(error, state)| (error, Box::new(GuardedAbortCustody { state: *state })))
    }

    fn abort_guarded_inner(
        self,
        ctx: &EnvContext<'r>,
        wait: Wait,
    ) -> Result<(), (EnvError, Box<GuardedAbortState<'r>>)> {
        if self.committed.is_some() || self.selected || !self.environment.owns_root(ctx.root) {
            return Err((
                EnvError::Invalid(
                    "guarded abort requires an uncommitted activation and its original state root"
                        .into(),
                ),
                Box::new(GuardedAbortState::Activation(Box::new(self))),
            ));
        }
        // A commit can have reached durable storage even when its caller saw
        // an error before recording `self.committed`. Check the authoritative
        // record under the retained Env gate before settling either owner.
        let durable_uncommitted = match effigy_kernel::store::Store::read(&ctx.observe(), ctx.root)
        {
            effigy_kernel::observe::Observed::Verified(reader, _) => {
                match WorkspaceActivationReservation::recover(
                    &reader,
                    self.reservation.environment(),
                ) {
                    effigy_kernel::observe::Observed::Verified(Some(current), _) => {
                        let record = current.record();
                        record.id == self.reservation.id()
                            && record.generation == self.reservation.generation()
                            && record.state == effigy_kernel::workspace_session::ActivationReservationState::Reserved
                            && record.environment_run == Some(self.environment.id())
                            && record.session.as_ref().is_some_and(|session| {
                                session.run == self.session.run()
                                    && session.owner == self.session.fence().resource
                                    && session.generation == self.session.generation()
                                    && session.run == self.supervisor.id()
                            })
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if !durable_uncommitted {
            return Err((
                EnvError::Invalid("workspace reservation committed or indeterminate; retain activation for recovery".into()),
                Box::new(GuardedAbortState::Activation(Box::new(self))),
            ));
        }
        let Self {
            environment,
            supervisor,
            reservation,
            attempt,
            ..
        } = self;
        match environment.finish_workspace_recoverable(false) {
            Ok(_) => GuardedAbortState::Session {
                root: ctx.root,
                supervisor: Box::new(supervisor),
                reservation,
                attempt,
            }
            .retry(ctx, wait),
            Err((error, environment)) => Err((
                error,
                Box::new(GuardedAbortState::Environment {
                    root: ctx.root,
                    environment,
                    supervisor: Box::new(supervisor),
                    reservation,
                    attempt,
                }),
            )),
        }
    }

    /// Legacy abort for callers that do not retain a failed owner.
    pub fn abort(self, ctx: &EnvContext<'_>, wait: Wait) -> Result<(), EnvError> {
        self.abort_with_target(
            ctx,
            wait,
            effigy_kernel::observe::Observed::Unknown(
                effigy_kernel::observe::UnknownReason::GuardUnavailable,
            ),
        )
    }

    /// Stop/settle the exact successor before revoking a committed failed
    /// selection. Unknown target evidence keeps it Pending for recovery; no
    /// selector is restored and no predecessor authority is recreated.
    pub fn abort_with_target(
        self,
        ctx: &EnvContext<'_>,
        wait: Wait,
        target: effigy_kernel::observe::Observed<()>,
    ) -> Result<(), EnvError> {
        let environment = self.environment.finish(false);
        let session = ctx.cleanup().and_then(|cleanup| {
            self.supervisor
                .settle(&cleanup, &mut EnvSettlement)
                .map_err(EnvError::Kernel)
        });
        environment?;
        session?;
        if let Some(obligation) = self.committed {
            use effigy_kernel::observe::{AbsenceProof, Observed};
            let closed = || Observed::Absent(AbsenceProof::GateFreeRunClosed);
            effigy_kernel::workspace_session::selection_abandoned(
                &ctx.cx,
                ctx.root,
                &obligation,
                &effigy_kernel::workspace_session::SelectionLiveness {
                    session: closed(),
                    target,
                    listener: closed(),
                },
                "owning publication failed and its exact supervisor settled",
                wait,
            )
            .map_err(EnvError::Kernel)
        } else {
            let cleanup = ctx.cleanup()?;
            let effects = cleanup.operation().effects().map_err(|error| {
                EnvError::Invalid(format!("cleanup authority unavailable: {error}"))
            })?;
            self.reservation
                .cancel_for_attempt(&effects, ctx.root, wait, self.attempt)
                .map(|_| ())
                .map_err(EnvError::Kernel)
        }
    }
}

fn fresh_attempt() -> Result<effigy_kernel::run::RequestId, EnvError> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| {
            EnvError::Kernel(effigy_kernel::run::RunError::unavailable(format!(
                "workspace attempt entropy: {error}"
            )))
        })?;
    Ok(effigy_kernel::run::RequestId::new(bytes))
}

/// Settle an exact admitted session before cancelling this attempt's tag.
fn cleanup_failed_reservation<'r>(
    ctx: &EnvContext<'r>,
    reservation: &WorkspaceActivationReservation,
    attempt: effigy_kernel::run::RequestId,
    cause: EnvError,
    uncertain: Option<(
        &effigy_kernel::id::ResourceId,
        effigy_kernel::id::RunId,
        effigy_kernel::outcome::Invocation,
    )>,
    owned: Option<Box<Run<'r>>>,
) -> EnvError {
    use effigy_kernel::observe::Observed;
    let mut failures = Vec::new();
    match ctx.cleanup() {
        Err(error) => failures.push(error),
        Ok(cleanup) => match cleanup.operation().effects() {
            Err(error) => failures.push(EnvError::Invalid(format!(
                "cleanup authority unavailable: {error}"
            ))),
            Ok(effects) => {
                if let Some(run) = owned {
                    if let Err(error) = (*run).settle(&cleanup, &mut EnvSettlement) {
                        failures.push(EnvError::Kernel(error));
                    }
                } else if let Some((owner, id, next)) = uncertain {
                    let observed = cleanup.operation().observe().ok().map(|observe| {
                        match effigy_kernel::store::Store::read(&observe, ctx.root) {
                            Observed::Verified(reader, _) => {
                                effigy_kernel::run::summary(&reader, id)
                            }
                            Observed::Absent(proof) => Observed::Absent(proof),
                            Observed::Unknown(reason) => Observed::Unknown(reason),
                        }
                    });
                    match observed {
                        Some(Observed::Verified(summary, _))
                            if summary.cleanup_complete != Some(true) =>
                        {
                            if let Err(error) = effigy_kernel::run::settle_interrupted(
                                &effects,
                                ctx.root,
                                owner,
                                id,
                                next,
                                &cleanup,
                                &mut EnvSettlement,
                            ) {
                                failures.push(EnvError::Kernel(error));
                            }
                        }
                        Some(Observed::Verified(_, _) | Observed::Absent(_)) => {}
                        _ => failures.push(EnvError::Invalid(
                            "cannot observe the exact session for cleanup".into(),
                        )),
                    }
                }
                if failures.is_empty() {
                    if let Err(error) =
                        reservation.cancel_for_attempt(&effects, ctx.root, Wait::FailFast, attempt)
                    {
                        failures.push(EnvError::Kernel(error));
                    }
                }
            }
        },
    }
    if failures.is_empty() {
        cause
    } else {
        EnvError::Cleanup {
            cause: Box::new(cause),
            failures,
        }
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
    fn gated_decision_observation_precedes_session_admission_and_refuses_failure() {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
        let env = env("workspace");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let observe_cx = ctx.cx.clone();
        let observe_state = state.clone();
        let observe_resource = env.resource.clone();
        let refused = WorkspaceActivation::begin_observing_recoverable(
            &ctx,
            &env,
            &invocation(),
            options(41),
            "fixture-supervisor",
            deadline,
            move |until| {
                assert!(until <= deadline);
                assert!(effigy_kernel::gate::Gate::acquire(
                    &observe_cx,
                    &observe_state,
                    std::slice::from_ref(&observe_resource),
                    Wait::FailFast
                )
                .is_err());
                Err(effigy_kernel::run::RunError::unavailable("unknown target"))
            },
        );
        assert!(refused.is_err());
        let WorkspaceBegin::Activate(activation) =
            WorkspaceActivation::begin_observing_recoverable(
                &ctx,
                &env,
                &invocation(),
                options(41),
                "fixture-supervisor",
                deadline,
                |_| Ok(inputs()),
            )
            .expect("fresh decision after refusal")
        else {
            panic!("fresh activation");
        };
        activation
            .abort(&ctx, Wait::FailFast)
            .expect("settle activation");
    }

    #[test]
    fn park_without_bound_target_retains_guarded_custody() {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
        let env = env("workspace");
        let WorkspaceBegin::Activate(activation) = WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(44),
            "fixture-supervisor",
        )
        .expect("begin") else {
            panic!("fresh activation")
        };
        let (_, activation) = activation
            .park(std::time::Instant::now() + std::time::Duration::from_secs(10))
            .expect_err("unbound target refuses parking");
        assert!(effigy_kernel::gate::Gate::acquire(
            &ctx.cx,
            &state,
            std::slice::from_ref(&env.resource),
            Wait::FailFast
        )
        .is_err());
        activation
            .abort(&ctx, Wait::FailFast)
            .expect("retained owner settles");
    }

    #[cfg(feature = "fault-injection")]
    #[test]
    fn guarded_abort_retains_env_and_session_when_settlement_fails() {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::with_cleanup_budget(
            &state,
            CancelToken::default(),
            std::time::Duration::from_millis(100),
        )
        .unwrap();
        let env = env("workspace");
        let WorkspaceBegin::Activate(activation) = WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(45),
            "fixture-supervisor",
        )
        .unwrap() else {
            panic!("fresh activation")
        };
        let result = effigy_kernel::fault_injection::with_hook(
            move |name| {
                if name == "SettleCloseBefore" {
                    return Err(std::io::Error::other("injected"));
                }
                Ok(())
            },
            || activation.abort_guarded_recoverable(&ctx, Wait::FailFast),
        );
        let (_, custody) = result.expect_err("injected cleanup must fail");
        assert!(
            effigy_kernel::gate::Gate::acquire(
                &ctx.cx,
                &state,
                std::slice::from_ref(&env.resource),
                Wait::FailFast,
            )
            .is_err(),
            "failed settlement must retain the original Env gate"
        );
        custody
            .retry(&ctx, Wait::FailFast)
            .expect("retry settles and cancels");
    }

    #[cfg(feature = "fault-injection")]
    #[test]
    fn guarded_abort_retains_session_after_env_settles() {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::with_cleanup_budget(
            &state,
            CancelToken::default(),
            std::time::Duration::from_millis(100),
        )
        .unwrap();
        let env = env("workspace");
        let WorkspaceBegin::Activate(activation) = WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(46),
            "fixture-supervisor",
        )
        .unwrap() else {
            panic!("fresh activation")
        };
        let session_id = activation.session.run();
        let mut closes = 0;
        let (_, custody) = effigy_kernel::fault_injection::with_hook(
            move |name| {
                if name == "SettleCloseBefore" {
                    closes += 1;
                    if closes >= 2 {
                        return Err(std::io::Error::other("injected"));
                    }
                }
                Ok(())
            },
            || activation.abort_guarded_recoverable(&ctx, Wait::FailFast),
        )
        .expect_err("session settlement must fail");
        let GuardedAbortState::Session { supervisor, .. } = &custody.state else {
            panic!("exact session must remain held")
        };
        assert_eq!(supervisor.id(), session_id);
        custody
            .retry(&ctx, Wait::FailFast)
            .expect("retry completes cleanup");
    }

    #[test]
    fn guarded_abort_refuses_a_durably_committed_reservation() {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
        let env = env("workspace");
        let WorkspaceBegin::Activate(activation) = WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(47),
            "fixture-supervisor",
        )
        .unwrap() else {
            panic!("fresh activation")
        };
        let reservation = activation.reservation.id().to_string();
        let connection = rusqlite::Connection::open(state.path().join("kernel.db")).unwrap();
        assert_eq!(connection.execute(
            "UPDATE workspace_activation_reservations SET state='committed' WHERE reservation_id=?1",
            [&reservation],
        ).unwrap(), 1);
        drop(connection);
        let (_, custody) = activation
            .abort_guarded_recoverable(&ctx, Wait::FailFast)
            .expect_err("committed reservation must refuse before settlement");
        let activation = custody
            .into_activation()
            .expect("original activation retained");
        assert!(effigy_kernel::gate::Gate::acquire(
            &ctx.cx,
            &state,
            std::slice::from_ref(&env.resource),
            Wait::FailFast
        )
        .is_err());
        // Restore the synthetic corruption so this fixture can settle normally.
        let connection = rusqlite::Connection::open(state.path().join("kernel.db")).unwrap();
        connection.execute("UPDATE workspace_activation_reservations SET state='reserved' WHERE reservation_id=?1", [&reservation]).unwrap();
        drop(connection);
        activation
            .abort_guarded_recoverable(&ctx, Wait::FailFast)
            .unwrap();
    }

    #[cfg(feature = "fault-injection")]
    #[test]
    fn failed_env_admission_retains_unsettled_session_owner() {
        use effigy_kernel::gate::OwnerState;
        use effigy_kernel::observe::Observed;
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::with_cleanup_budget(
            &state,
            CancelToken::default(),
            std::time::Duration::from_millis(100),
        )
        .unwrap();
        let env = env("workspace");
        let options = options(48);
        let session = options.id;
        let mut adopted = false;
        let result = effigy_kernel::fault_injection::with_hook(
            move |point| {
                if point == "SessionSupervisorAdoptAfter" {
                    adopted = true;
                }
                if adopted && (point == "BeginBefore" || point == "SettleCloseBefore") {
                    return Err(std::io::Error::other("injected"));
                }
                Ok(())
            },
            || {
                WorkspaceActivation::begin_recoverable(
                    &ctx,
                    &env,
                    &inputs(),
                    &invocation(),
                    options,
                    "fixture-supervisor",
                )
            },
        );
        let failure = result.expect_err("injected admission and cleanup fail");
        let custody = failure.custody.expect("exact owner custody retained");
        let Observed::Verified(reader, _) =
            effigy_kernel::store::Store::read(&ctx.observe(), &state)
        else {
            panic!("store unavailable")
        };
        assert!(
            matches!(
                reader.run_owner_state(&ctx.observe(), &state, &session),
                Observed::Verified(OwnerState::Held, _)
            ),
            "failed cleanup must retain the exact live session owner"
        );
        drop(reader);
        custody
            .retry(&ctx, Wait::FailFast)
            .expect("retry settles exact session");
    }

    #[cfg(feature = "fault-injection")]
    #[test]
    fn ambiguous_env_admission_keeps_session_and_refuses_cleanup() {
        use effigy_kernel::gate::OwnerState;
        use effigy_kernel::observe::Observed;
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
        let env = env("workspace");
        let options = options(49);
        let session = options.id;
        let mut adopted = false;
        let failure = effigy_kernel::fault_injection::with_hook(
            move |point| {
                if point == "SessionSupervisorAdoptAfter" {
                    adopted = true;
                }
                if adopted && point == "BeginAdmitted" {
                    return Err(std::io::Error::other("injected"));
                }
                Ok(())
            },
            || {
                WorkspaceActivation::begin_recoverable(
                    &ctx,
                    &env,
                    &inputs(),
                    &invocation(),
                    options,
                    "fixture-supervisor",
                )
            },
        )
        .expect_err("post-admission error must not settle the session");
        let custody = failure
            .custody
            .expect("retain session on ambiguous admission");
        let (error, custody) = custody
            .retry(&ctx, Wait::FailFast)
            .expect_err("bound Env run requires recovery");
        assert!(error
            .to_string()
            .contains("environment admission uncertain"));
        let Observed::Verified(reader, _) =
            effigy_kernel::store::Store::read(&ctx.observe(), &state)
        else {
            panic!("store unavailable")
        };
        assert!(matches!(
            reader.run_owner_state(&ctx.observe(), &state, &session),
            Observed::Verified(OwnerState::Held, _)
        ));
        drop(reader);
        drop(custody);
    }

    #[cfg(feature = "fault-injection")]
    #[test]
    fn supervisor_adoption_failure_retains_raw_session_on_failed_cleanup() {
        use effigy_kernel::gate::OwnerState;
        use effigy_kernel::observe::Observed;
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::with_cleanup_budget(
            &state,
            CancelToken::default(),
            std::time::Duration::from_millis(100),
        )
        .unwrap();
        let env = env("workspace");
        let options = options(50);
        let session = options.id;
        let failure = effigy_kernel::fault_injection::with_hook(
            |point| {
                if point == "SessionSupervisorAdoptAfter" || point == "SettleCloseBefore" {
                    return Err(std::io::Error::other("injected"));
                }
                Ok(())
            },
            || {
                WorkspaceActivation::begin_recoverable(
                    &ctx,
                    &env,
                    &inputs(),
                    &invocation(),
                    options,
                    "fixture-supervisor",
                )
            },
        )
        .expect_err("injected adoption and cleanup fail");
        let custody = failure.custody.expect("retain raw session");
        let Observed::Verified(reader, _) =
            effigy_kernel::store::Store::read(&ctx.observe(), &state)
        else {
            panic!("store unavailable")
        };
        assert!(matches!(
            reader.run_owner_state(&ctx.observe(), &state, &session),
            Observed::Verified(OwnerState::Held, _)
        ));
        drop(reader);
        custody
            .retry(&ctx, Wait::FailFast)
            .expect("retry settles session");
    }

    #[cfg(feature = "fault-injection")]
    #[test]
    fn unreadable_env_record_retains_admitted_env_and_session() {
        use effigy_kernel::gate::OwnerState;
        use effigy_kernel::observe::Observed;
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::with_cleanup_budget(
            &state,
            CancelToken::default(),
            std::time::Duration::from_millis(100),
        )
        .unwrap();
        let env = env("workspace");
        let options = options(51);
        let session = options.id;
        let database = state.path().join("kernel.db");
        let resource = env.resource.name().to_string();
        let hook_database = database.clone();
        let hook_resource = resource.clone();
        let mut adopted = false;
        let failure = effigy_kernel::fault_injection::with_hook(
            move |point| {
                if point == "SessionSupervisorAdoptAfter" {
                    adopted = true;
                }
                if adopted && point == "BeginAdmitted" {
                    let db = rusqlite::Connection::open(&hook_database).unwrap();
                    db.execute(
                        "UPDATE resources SET engine='{' WHERE name=?1",
                        [&hook_resource],
                    )
                    .unwrap();
                }
                if adopted && point == "SettleCloseBefore" {
                    return Err(std::io::Error::other("injected"));
                }
                Ok(())
            },
            || {
                WorkspaceActivation::begin_recoverable(
                    &ctx,
                    &env,
                    &inputs(),
                    &invocation(),
                    options,
                    "fixture-supervisor",
                )
            },
        )
        .expect_err("unreadable admitted Env record");
        let custody = failure.custody.expect("retain both owners");
        assert!(matches!(
            &custody.state,
            GuardedAbortState::Environment { .. }
        ));
        assert!(effigy_kernel::gate::Gate::acquire(
            &ctx.cx,
            &state,
            std::slice::from_ref(&env.resource),
            Wait::FailFast
        )
        .is_err());
        let db = rusqlite::Connection::open(&database).unwrap();
        db.execute(
            "UPDATE resources SET engine=NULL WHERE name=?1",
            [&resource],
        )
        .unwrap();
        drop(db);
        let Observed::Verified(reader, _) =
            effigy_kernel::store::Store::read(&ctx.observe(), &state)
        else {
            panic!("store unavailable")
        };
        assert!(matches!(
            reader.run_owner_state(&ctx.observe(), &state, &session),
            Observed::Verified(OwnerState::Held, _)
        ));
        drop(reader);
        custody
            .retry(&ctx, Wait::FailFast)
            .expect("retry settles both owners");
    }

    #[test]
    fn competing_stage_refusal_never_cancels_another_staged_session() {
        use effigy_kernel::observe::Observed;
        use effigy_kernel::workspace_session::ActivationReservationState;
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
        let env = env("workspace");
        let reservation = match WorkspaceActivationReservation::decide(
            &ctx.cx,
            &state,
            &env.resource,
            &inputs(),
            Wait::FailFast,
        )
        .expect("decision")
        {
            ActivationDecision::Activate(reservation) => reservation,
            _ => panic!("fresh activation"),
        };
        reservation
            .stage_session_binding(&ctx.cx, &state, &invocation().spec().owner, options(40).id)
            .expect("first stages");
        assert!(WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(41),
            "second"
        )
        .is_err());
        let Observed::Verified(reader, _) =
            effigy_kernel::store::Store::read(&ctx.observe(), &state)
        else {
            panic!("journal unavailable")
        };
        let Observed::Verified(Some(observed), _) =
            WorkspaceActivationReservation::recover(&reader, &env.resource)
        else {
            panic!("reservation missing")
        };
        assert_eq!(
            observed.state(),
            ActivationReservationState::Reserved,
            "other staged session retains its reservation"
        );
        reservation
            .cancel(&ctx.cx, &state, Wait::FailFast)
            .expect("fixture cleanup");
    }

    #[cfg(feature = "fault-injection")]
    #[test]
    fn tagged_owner_settles_its_early_failures_without_losing_reservations() {
        use effigy_kernel::observe::Observed;
        use effigy_kernel::workspace_session::ActivationReservationState;
        for point in [
            "SessionStageBefore",
            "SessionStageAfter",
            "BeginBefore",
            "BeginAdmitted",
            "SessionSupervisorAdoptBefore",
            "SessionSupervisorAdoptAfter",
        ] {
            let fixture = home();
            let state = root(fixture.path());
            let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
            let env = env("workspace");
            let mut fired = false;
            let outcome = effigy_kernel::fault_injection::with_hook(
                move |name| {
                    if !fired && name == point {
                        fired = true;
                        return Err(std::io::Error::other("injected"));
                    }
                    Ok(())
                },
                || {
                    WorkspaceActivation::begin(
                        &ctx,
                        &env,
                        &inputs(),
                        &invocation(),
                        options(42),
                        "fixture-supervisor",
                    )
                },
            );
            assert!(outcome.is_err(), "{point} must fail");
            let Observed::Verified(reader, _) =
                effigy_kernel::store::Store::read(&ctx.observe(), &state)
            else {
                panic!("journal unavailable: {point}");
            };
            let Observed::Verified(Some(reservation), _) =
                WorkspaceActivationReservation::recover(&reader, &env.resource)
            else {
                panic!("reservation missing: {point}");
            };
            if point == "SessionStageBefore" {
                assert_eq!(
                    reservation.state(),
                    ActivationReservationState::Reserved,
                    "pre-stage reservation remains recoverable"
                );
                reservation
                    .cancel(&ctx.cx, &state, Wait::FailFast)
                    .expect("fixture cleanup");
            } else {
                assert_eq!(
                    reservation.state(),
                    ActivationReservationState::Cancelled,
                    "{point} must cancel its tagged reservation"
                );
            }
        }
    }

    #[test]
    fn reserved_owner_admits_task_before_env_and_refuses_concurrent_or_unselected_success() {
        let fixture = home();
        let state = root(fixture.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).expect("context");
        let env = env("workspace");
        let WorkspaceBegin::Activate(mut activation) = WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(1),
            "fixture-supervisor",
        )
        .expect("begin") else {
            panic!("fresh activation")
        };
        assert_eq!(activation.session().run(), options(1).id);
        let session = activation.session().clone();
        assert_eq!(
            activation
                .session_run_mut()
                .session_owner()
                .expect("adopted before channel")
                .run(),
            session.run()
        );
        let listener = effigy_kernel::workspace_session::HostChannelListener::bind(
            "127.0.0.1:0".parse().expect("socket"),
        )
        .expect("bound fixture listener");
        let key = effigy_kernel::workspace_session::ServingKey::new(
            vec![7; 32],
            Digest::of(b"fixture pin"),
        )
        .expect("key");
        let mut launch = effigy_kernel::workspace_session::SessionLaunch::launch(
            activation.session_run_mut(),
            &session,
            "fixture-supervisor",
            key,
            Box::new(listener),
        )
        .expect("already adopted session launches its own channel");
        assert!(
            launch.publish(activation.session_run_mut()).is_err(),
            "adoption alone never qualifies a locator"
        );
        launch.stop();
        assert_eq!(
            activation
                .current_plan()
                .expect("plan")
                .binding()
                .generation,
            activation.session().generation()
        );
        assert!(WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(2),
            "fixture-supervisor"
        )
        .is_err());
        activation
            .abort(&ctx, Wait::FailFast)
            .expect("settle before cancel");
        let WorkspaceBegin::Activate(retry) = WorkspaceActivation::begin(
            &ctx,
            &env,
            &inputs(),
            &invocation(),
            options(3),
            "fixture-supervisor",
        )
        .expect("retry") else {
            panic!("unpublished retry")
        };
        let Err((_, custody)) = retry.finish_selected() else {
            panic!("no successful activation without selector readback")
        };
        let WorkspaceFinishCustody::Activation(activation) = *custody else {
            panic!("the activation must remain owned")
        };
        activation
            .abort(&ctx, Wait::FailFast)
            .expect("refused activation settles explicitly");
    }
}
