//! The reserved standard engine's owning channel. One supervisor thread owns
//! one root admission; nested tasks poll that root without a second writer.
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use effigy_kernel::ctx::StopCause;
use effigy_kernel::human::Category;
use effigy_kernel::id::RunId;
use effigy_kernel::outcome::WorkloadOutcome;
use effigy_kernel::run::Run;
use effigy_kernel::supervise::EngineSession;

use super::TaskRun;
use crate::runner::error::RunnerError;

struct Owner {
    session: EngineSession,
    run: Arc<Mutex<Run<'static>>>,
    id: RunId,
    stop: Option<StopCause>,
    escalate: bool,
    secrets: Vec<Vec<u8>>,
}

thread_local! {
    static OWNER: RefCell<Option<Owner>> = const { RefCell::new(None) };
}

pub(in crate::runner) fn attach(
    run: &mut TaskRun,
    mut session: EngineSession,
    secrets: Vec<Vec<u8>>,
) -> Result<(), RunnerError> {
    let refuse =
        || RunnerError::task_invocation("reserved standard owner does not match the admitted run");
    if OWNER.with(|owner| owner.borrow().is_some())
        || session.admission().map(|ticket| ticket.run()) != Some(run.id())
    {
        return Err(refuse());
    }
    let result = (|| {
        session
            .admitted(&mut *run.run_mut()?)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let capture = session
            .capture_output(&mut *run.run_mut()?, secrets.clone())
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        if !matches!(capture, effigy_kernel::custody::CaptureState::Complete) {
            return Err(RunnerError::task_invocation(
                "reserved standard output capture is unavailable; no command started",
            ));
        }
        Ok(())
    })();
    if let Err(error) = result {
        session.refused_message(error.category(), error.to_string());
        return Err(error);
    }
    let owner = Owner {
        session,
        run: run.durable_handle()?,
        id: run.id(),
        stop: None,
        escalate: false,
        secrets,
    };
    OWNER.with(|slot| *slot.borrow_mut() = Some(owner));
    Ok(())
}

/// A later task/alias cannot introduce unclassified bytes into this owner's
/// inherited output descriptors. Refuse before spawn rather than raw fallback.
pub(super) fn verify_environment(env: &[(String, super::EnvOverride)]) -> Result<(), RunnerError> {
    OWNER.with(|slot| match slot.borrow().as_ref() {
        Some(owner) => verify_known_secrets(&owner.secrets, env),
        None => Ok(()),
    })
}

fn verify_known_secrets(
    known: &[Vec<u8>],
    env: &[(String, super::EnvOverride)],
) -> Result<(), RunnerError> {
    if env.iter().any(|(_, value)| {
        matches!(value, super::EnvOverride::Secret(value)
        if !known.iter().any(|secret| secret.as_slice() == value.as_bytes()))
    }) {
        return Err(RunnerError::task_invocation(
            "workspace task introduced secret provenance outside its prepared output boundary",
        ));
    }
    Ok(())
}

/// Poll liveness and fenced control on the same run the dispatcher reserved.
/// In-process nested tasks inherit this thread's owner; external children stay
/// under their parent's kernel custody and are stopped by its settlement.
pub(in crate::runner) fn poll_stop() -> Result<Option<StopCause>, RunnerError> {
    OWNER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(owner) = slot.as_mut() else {
            return Ok(None);
        };
        let mut run = owner
            .run
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let control = owner
            .session
            .control(&mut run, &[])
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        owner.escalate |= control.escalate;
        if owner.stop.is_none() {
            owner.stop = control.stop;
        }
        Ok(owner.stop.clone())
    })
}

pub(super) fn escalating() -> bool {
    OWNER.with(|slot| slot.borrow().as_ref().is_some_and(|owner| owner.escalate))
}

/// Flush canonical output and announce closure only after real settlement.
pub(in crate::runner) fn close(run: TaskRun, mut workload: WorkloadOutcome) -> Vec<String> {
    let owns = OWNER.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|owner| owner.id == run.id())
    });
    if !owns {
        return run.close(workload);
    }
    match poll_stop() {
        Ok(Some(cause)) => {
            workload = WorkloadOutcome::Cancelled {
                cause,
                signal: None,
            }
        }
        Err(error) => {
            eprintln!("workspace owner control unavailable at settlement: {error}");
            workload = WorkloadOutcome::Interrupted;
        }
        Ok(None) => {}
    }
    let Owner {
        mut session,
        run: handle,
        ..
    } = OWNER
        .with(|slot| slot.borrow_mut().take())
        .expect("owning standard engine");
    drop(handle);
    let (outcome, mut warnings) = run.close_settled(workload, super::kernel::CLEANUP_BUDGET);
    match outcome {
        Some(outcome) => session.closed(&outcome),
        None => {
            session.refused(
                Category::Unavailable,
                "reserved standard run did not settle",
            );
            warnings.push("reserved standard run did not settle".into());
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_secret_provenance_refuses_without_exposing_its_value() {
        let known = vec![b"known-private-value".to_vec()];
        assert!(verify_known_secrets(
            &known,
            &[(
                "KEY".into(),
                super::super::EnvOverride::Secret("known-private-value".into())
            )]
        )
        .is_ok());
        let error = verify_known_secrets(
            &known,
            &[(
                "KEY".into(),
                super::super::EnvOverride::Secret("unclassified-private-value".into()),
            )],
        )
        .unwrap_err();
        assert!(!error.to_string().contains("unclassified-private-value"));
    }
}
