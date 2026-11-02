//! Qualification-only host down: retire the selected session using the
//! kernel's ordinary coordination, then let the stop transition recheck R4.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{self, Phase, RequestId};
use effigy_kernel::store::{Store, StoreReader};
use effigy_kernel::workspace_session::{DownCoordination, DownStart, SessionRef};

use crate::runner::{environment, error::RunnerError};

pub(in crate::runner) fn retire_selected_session(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let context = environment::context(repo_root)?;
    let env = environment::env_ref(policy)?;
    // One bounded context covers observation, requests and waiting. A missing
    // mapping is not an exemption: the ordinary stop still checks every holder.
    let effect = context.cx.child(Duration::from_secs(30));
    let effigy_kernel::ctx::Deadline::At(until) = effect.operation().deadline() else {
        return Err(failure("down requires a finite deadline"));
    };
    super::recovery::selected_session(policy, until).map_err(failure)?;
    let observe = effect.operation().observe().map_err(failure)?;
    let read = || match Store::read(&observe, context.root) {
        Observed::Verified(reader, _) => Ok(Some(reader)),
        Observed::Absent(_) => Ok(None),
        Observed::Unknown(reason) => Err(failure(reason)),
    };
    let Some(reader) = read()? else { return Ok(()) };
    let session = match SessionRef::resolve(&reader, &env.resource) {
        Observed::Verified(Some(session), _) => session,
        Observed::Absent(_) | Observed::Verified(None, _) => return Ok(()),
        Observed::Unknown(reason) => return Err(failure(reason)),
    };
    let children = match DownCoordination::begin(&observe, context.root, &reader, &session) {
        Observed::Verified(DownStart::Ready(children), _) => children,
        Observed::Verified(DownStart::Refused { .. }, _) => {
            return Err(failure(
                "another live or unresolved run holds the workspace",
            ));
        }
        other => return Err(failure(format!("down coordination unavailable: {other:?}"))),
    };
    let request = || -> Result<RequestId, RunnerError> {
        let mut bytes = [0; 16];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(failure)?;
        Ok(RequestId::new(bytes))
    };
    for child in children.children() {
        children
            .stop(&effect, context.root, &reader, &request()?, *child)
            .map_err(failure)?;
    }
    drop(reader);
    let read_required = || -> Result<StoreReader, RunnerError> {
        read()?.ok_or_else(|| failure("workspace journal disappeared during down"))
    };
    loop {
        effect
            .operation()
            .check("workspace down children")
            .map_err(failure)?;
        let reader = read_required()?;
        match children.unresolved(&reader) {
            Observed::Verified(open, _) if open.is_empty() => {
                for child in children.children() {
                    if !matches!(run::summary(&reader, *child), Observed::Verified(summary, _) if summary.cleanup_complete == Some(true))
                    {
                        return Err(failure("workspace child closed with unresolved cleanup"));
                    }
                }
                children
                    .settle_session(&effect, context.root, &reader, &request()?)
                    .map_err(failure)?;
                break;
            }
            Observed::Verified(_, _) => {}
            other => {
                return Err(failure(format!(
                    "workspace child closure unavailable: {other:?}"
                )))
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    loop {
        effect
            .operation()
            .check("workspace down session")
            .map_err(failure)?;
        let reader = read_required()?;
        match run::summary(&reader, session.run()) {
            Observed::Verified(summary, _) if summary.phase == Phase::Closed => {
                if summary.cleanup_complete == Some(true) {
                    return Ok(());
                }
                return Err(failure("selected session closed with unresolved cleanup"));
            }
            Observed::Verified(_, _) => {}
            other => {
                return Err(failure(format!(
                    "workspace session closure unavailable: {other:?}"
                )))
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn failure(error: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(format!("workspace down refused: {error}"))
}
