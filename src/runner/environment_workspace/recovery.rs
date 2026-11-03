//! Explicit recovery of the selected session's private liveness slot, before
//! taking Env for a successor. A live or unknown owner grants no recovery.
use std::time::Instant;

use effigy_containers::environment::EnvSettlement;
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::gate::OwnerState;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{self, RunError};
use effigy_kernel::store::Store;
use effigy_kernel::workspace_session::SessionRef;

pub(in crate::runner) fn selected_session(
    policy: &EffectiveContainerPolicy,
    until: Instant,
) -> Result<(), RunError> {
    let context = crate::runner::environment::context(&policy.repo_root)
        .map_err(|_| unavailable())?
        .bounded_until(until)
        .map_err(|_| unavailable())?;
    let env = crate::runner::environment::env_ref(policy).map_err(|_| unavailable())?;
    let observe = context
        .cx
        .operation()
        .observe()
        .map_err(|_| unavailable())?;
    let reader = match Store::read(&observe, context.root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Ok(()),
        Observed::Unknown(_) => return Err(unavailable()),
    };
    let session = match SessionRef::resolve(&reader, &env.resource) {
        Observed::Verified(Some(session), _) => session,
        Observed::Absent(_) | Observed::Verified(None, _) => return Ok(()),
        Observed::Unknown(_) => return Err(unavailable()),
    };
    let metadata = match run::metadata(&observe, context.root, session.run()) {
        Observed::Verified(metadata, _) => metadata,
        _ => return Err(unavailable()),
    };
    let identity = metadata.owner.ok_or_else(unavailable)?;
    match effigy_kernel::ident::process_identity(identity.pid) {
        Observed::Verified(current, _) if current == identity => {
            match effigy_kernel::ident::process_state(identity.pid) {
                Observed::Verified(effigy_kernel::ident::ProcessState::Zombie, _) => {}
                Observed::Verified(_, _) => return Ok(()),
                _ => return Err(unavailable()),
            }
        }
        Observed::Absent(_) => {}
        Observed::Verified(_, _) => {}
        Observed::Unknown(_) => return Err(unavailable()),
    }
    // A dead creator is not group closure. Its sentinel may still own the
    // private slot while settling. Wait only for positively observed exclusion
    // release, inside the original activation deadline, before exact recovery.
    loop {
        context.cx.operation().check("selected session recovery")?;
        match reader.run_owner_state(&observe, context.root, &session.run()) {
            Observed::Verified(OwnerState::Interrupted, _) => break,
            Observed::Verified(OwnerState::Closed, _) => return Ok(()),
            Observed::Verified(OwnerState::Held, _) => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            _ => return Err(unavailable()),
        }
    }
    let invocation = match run::summary(&reader, session.run()) {
        Observed::Verified(summary, _) => summary.invocation.ok_or_else(unavailable)?,
        _ => return Err(unavailable()),
    };
    drop(reader);
    let outcome = run::settle_interrupted(
        &context.cx,
        context.root,
        &invocation.spec().owner,
        session.run(),
        crate::runner::workspace_client::serving_host::recovery_invocation()?,
        &context.cleanup().map_err(|_| unavailable())?,
        &mut EnvSettlement,
    )?;
    if outcome.cleanup() != effigy_kernel::outcome::CleanupOutcome::Complete
        || !outcome.unresolved().is_empty()
    {
        return Err(unavailable());
    }
    Ok(())
}

fn unavailable() -> RunError {
    RunError::unavailable("selected workspace session recovery remains unverified; re-run up")
}
