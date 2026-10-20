//! Observe kernel-owned writers before removing a private fixture HOME.
use effigy_kernel::custody::test_support as custody;
use effigy_kernel::observe::Observed;
use std::{
    fs, io,
    path::Path,
    time::{Duration, Instant},
};

/// A killed fixture client can leave its kernel sentinel finishing settlement.
/// Wait for those recorded generations to stop writing before deleting HOME.
pub fn wait_kernel_helpers(home: &Path) -> io::Result<()> {
    use effigy_kernel::{ctx::*, ident, root::StateRoot};
    match fs::symlink_metadata(home.join(".effigy/state/kernel.db")) {
        Ok(_) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }
    let operation = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 1024,
            log_keep: 0,
            line_max: 1024,
            queue_bytes: 1024,
            probe_concurrency: 1,
        },
        EffectClass::Observe,
        Placement::Host,
    );
    let cx = operation.observe().map_err(|_| {
        io::Error::other("fixture helper observation requires a finite observe context")
    })?;
    let root = match StateRoot::observe(&cx, home) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(()),
        Observed::Unknown(reason) => {
            return Err(io::Error::other(format!("fixture state: {reason:?}")))
        }
    };
    let mut last_observation = None;
    loop {
        operation.check("fixture kernel helpers").map_err(|error| {
            io::Error::other(format!(
                "fixture helper wait: {error:?}; last observation={last_observation:?}"
            ))
        })?;
        // A killed client can leave a hot journal until its sentinel recovers
        // it. Observation may retry, but never repairs or infers absence.
        let helpers = match custody::run_helpers(&cx, &root) {
            Ok(helpers) => {
                last_observation = None;
                helpers
            }
            Err(error) => {
                last_observation = Some(error);
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
        };
        let mut live = false;
        for identity in helpers {
            // The calling test owns its synchronous cleanup; only other
            // recorded processes can race this thread's directory removal.
            if identity.pid == std::process::id() {
                continue;
            }
            match ident::process_identity(identity.pid) {
                Observed::Absent(_) => continue,
                Observed::Verified(current, _) if current != identity => continue,
                Observed::Verified(..) => (),
                Observed::Unknown(reason) => {
                    return Err(io::Error::other(format!("fixture helper: {reason:?}")))
                }
            }
            match ident::process_state(identity.pid) {
                Observed::Absent(_) | Observed::Verified(ident::ProcessState::Zombie, _) => (),
                Observed::Verified(..) => live = true,
                Observed::Unknown(reason) => {
                    return Err(io::Error::other(format!(
                        "fixture helper state: {reason:?}"
                    )))
                }
            }
        }
        if !live {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
