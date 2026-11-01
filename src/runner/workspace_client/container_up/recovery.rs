//! Existing-owner cleanup only. Unknown integrations and committed ambiguity
//! retain custody and their journal; they never create a new activation.
use effigy_containers::environment::WorkspaceBeginFailure;
use effigy_kernel::gate::Wait;

use super::cleanup_integrations::CleanupAdapter;
use super::integrations::{IntegrationAdapter, IntegrationEffects};
use crate::runner::environment::workspace::integrated::{
    CleanupPhase, GuardedCleanup, IntegratedFailure, ParkedCleanupSupport,
};
use crate::runner::environment::workspace::phase_a::{
    AdmissionFailure, GuardedPhaseA, PhaseAFailure,
};
use crate::runner::environment::workspace::phase_b::PublicationFailure;

pub(super) fn cleanup<'a, E: IntegrationEffects>(
    failure: IntegratedFailure<'a, E>,
) -> Result<(), IntegratedFailure<'a, E>> {
    match failure {
        IntegratedFailure::PhaseA(PhaseAFailure::BeforeAdmission(_)) => Ok(()),
        IntegratedFailure::PhaseA(PhaseAFailure::Guarded { cause, custody }) => {
            cleanup(IntegratedFailure::Guarded {
                cause,
                owner: custody,
                bound: None,
                effects: None,
            })
        }
        IntegratedFailure::PhaseA(PhaseAFailure::Admission(admission)) => {
            let AdmissionFailure {
                failure,
                context,
                backend,
                plan,
            } = *admission;
            let WorkspaceBeginFailure { error, custody } = failure;
            let Some(custody) = custody else {
                return Ok(());
            };
            match custody.retry(&context, Wait::FailFast) {
                Ok(()) => Ok(()),
                Err((cleanup_error, custody)) => {
                    Err(IntegratedFailure::Cleanup(Box::new(GuardedCleanup {
                        primary: error,
                        cleanup: cleanup_error,
                        custody,
                        context,
                        backend,
                        plan,
                        bound: None,
                        effects: None,
                    })))
                }
            }
        }
        IntegratedFailure::Guarded {
            cause,
            owner,
            bound,
            effects,
        } => {
            if let Some(effects) = effects {
                let GuardedPhaseA {
                    activation,
                    context,
                    backend,
                    plan,
                    creation,
                } = *owner;
                let owner = match activation.park_for_cleanup(&context) {
                    Ok(owner) => owner,
                    Err((_, activation)) => {
                        return Err(IntegratedFailure::Guarded {
                            cause,
                            owner: Box::new(GuardedPhaseA {
                                activation,
                                context,
                                backend,
                                plan,
                                creation,
                            }),
                            bound,
                            effects: Some(effects),
                        })
                    }
                };
                return cleanup(IntegratedFailure::CleanupOnly {
                    cause,
                    custody: Box::new(CleanupPhase {
                        owner,
                        support: Box::new(ParkedCleanupSupport {
                            context,
                            backend,
                            plan,
                            bound,
                            effects,
                        }),
                    }),
                });
            }
            let GuardedPhaseA {
                activation,
                context,
                backend,
                plan,
                creation: _,
            } = *owner;
            match activation.abort_guarded_recoverable(&context, Wait::FailFast) {
                Ok(()) => Ok(()),
                Err((cleanup_error, custody)) => {
                    Err(IntegratedFailure::Cleanup(Box::new(GuardedCleanup {
                        primary: cause,
                        cleanup: cleanup_error,
                        custody,
                        context,
                        backend,
                        plan,
                        bound,
                        effects,
                    })))
                }
            }
        }
        IntegratedFailure::Cleanup(custody) => {
            let GuardedCleanup {
                primary,
                cleanup: _,
                custody,
                context,
                backend,
                plan,
                bound,
                effects,
            } = *custody;
            match custody.retry(&context, Wait::FailFast) {
                Ok(()) => Ok(()),
                Err((cleanup_error, custody)) => {
                    Err(IntegratedFailure::Cleanup(Box::new(GuardedCleanup {
                        primary,
                        cleanup: cleanup_error,
                        custody,
                        context,
                        backend,
                        plan,
                        bound,
                        effects,
                    })))
                }
            }
        }
        IntegratedFailure::Parked { cause, mut custody } => {
            // A failed reacquire can still hold Env. Release it under its exact
            // owner before any integration cleanup runs.
            custody.owner = match custody.owner.repark_for_cleanup() {
                Ok(owner) => owner,
                Err((_, owner)) => {
                    custody.owner = *owner;
                    return Err(IntegratedFailure::Parked { cause, custody });
                }
            };
            let cleanup_until =
                std::time::Instant::now() + effigy_containers::environment::CLEANUP_BUDGET;
            let mut adapter = IntegrationAdapter {
                owner: &mut custody.owner,
                context: &custody.context,
                effects: &mut custody.effects,
                integration_until: custody.integration_until,
                reservation_until: custody.plan.deadline().at(),
            };
            if adapter.unwind(&mut custody.outcome, cleanup_until).is_err() {
                return Err(IntegratedFailure::Parked { cause, custody });
            }
            let crate::runner::environment::workspace::integrated::ParkedPhase {
                owner,
                context,
                backend,
                plan,
                bound,
                effects,
                creation: _,
                outcome: _,
                integration_until: _,
            } = *custody;
            match owner.abort(&context, Wait::FailFast) {
                Ok(()) => Ok(()),
                Err((cleanup_error, custody)) => Err(IntegratedFailure::ParkedCleanup {
                    cause: cleanup_error,
                    custody,
                    support: Box::new(ParkedCleanupSupport {
                        context,
                        backend,
                        plan,
                        bound: Some(Box::new(bound)),
                        effects: Box::new(effects),
                    }),
                }),
            }
        }
        IntegratedFailure::CleanupOnly { cause, mut custody } => {
            let until = std::time::Instant::now() + effigy_containers::environment::CLEANUP_BUDGET;
            let mut adapter = CleanupAdapter {
                owner: &mut custody.owner,
                context: &custody.support.context,
                effects: custody.support.effects.as_mut(),
                until,
            };
            if adapter.unwind().is_err() {
                return Err(IntegratedFailure::CleanupOnly { cause, custody });
            }
            let CleanupPhase { owner, support } = *custody;
            match owner.abort(&support.context, Wait::FailFast) {
                Ok(()) => Ok(()),
                Err((cleanup_error, custody)) => Err(IntegratedFailure::ParkedCleanup {
                    cause: cleanup_error,
                    custody,
                    support,
                }),
            }
        }
        IntegratedFailure::Publication {
            cause: PublicationFailure::Guarded { cause, custody },
            effects,
        } => cleanup(IntegratedFailure::Guarded {
            cause,
            owner: custody,
            bound: None,
            effects: Some(effects),
        }),
        // A committed/partly-finalized generation requires its existing
        // selection recovery. Stop exposure and retain its exact custody.
        IntegratedFailure::Publication { mut cause, effects } => {
            if let PublicationFailure::Finishing { channel, .. } = &mut cause {
                channel.launch.stop();
            }
            Err(IntegratedFailure::Publication { cause, effects })
        }
        retained @ IntegratedFailure::ParkedCleanup { .. } => Err(retained),
    }
}
