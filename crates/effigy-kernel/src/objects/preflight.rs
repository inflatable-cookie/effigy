//! Preflight sealing (§7.1–§7.4): publish the frozen closure an adapter is
//! about to apply, before it applies it, without creating a run, a journal
//! record or a reference. Publication still owns the reference; this only makes
//! the exact bytes observable read-only and then reconcilable by the same
//! trusted digest.
//!
//! Staging is synchronous filesystem work that can stall, so it runs on one
//! helper thread while the caller waits no longer than its deadline. The
//! promotion, however, runs on the caller's own thread, after the helper
//! reports a durable stage and while the caller still holds the objects gate:
//! a helper can only write inside its private tree, so a helper that outlives
//! its caller has no promotion path at all. No promotion can therefore begin
//! after the caller has returned, and no canonical objects write happens
//! outside the gate this operation took.
//!
//! The one interval this cannot close is the instruction boundary between the
//! last deadline/cancellation check and the `renameat2`/`renameatx_np` call:
//! a thread descheduled there renames when it runs again, and every in-flight
//! filesystem syscall (stage write, fsync, rename, or a `renameat_np` blocked
//! on a stalled filesystem) outlasts any deadline. K8 bounds waits, not
//! syscalls, exactly as `record::atomic_replace` assumes (its deadline check
//! precedes its open/write/fsync/rename with no check between). Closing that
//! interval needs a lease-bound or revocable exclusive move — a rename that
//! fails once the operation's deadline has passed — which the kernel's gate and
//! record contracts do not provide; see effigy#509.
use super::*;
use crate::ctx::{Capability, Deadline, EffectClass, EffectContext};
use crate::fs::{FileName, Trust};
use crate::root::{Family, StateRoot};
use crate::run::RunError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Instant;

/// Trees under `state/staging/` that this module owns. A run's staging tree is
/// named by its run id, so this prefix is never one and recovery can tell them
/// apart. Removal is only ever of this prefix.
pub(crate) const STAGING_PREFIX: &str = "preflight-";
/// The stage directory, the digest directory, then the closure's own bound.
const STAGING_DEPTH: usize = MAX_DEPTH + 2;
static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

/// What the caller's helper found.
enum Sealed {
    /// The trusted object was already published and verified in place.
    Present,
    /// A private, synced stage is ready for the caller to promote.
    Staged,
}

impl Objects {
    /// Seal `closure` into the checkout's immutable objects under the caller's
    /// `deadline`, returning the trusted digest the existing publisher records.
    ///
    /// The objects gate is held for the whole call, so nothing else publishes,
    /// settles or recovers an object while this seal decides. An object already
    /// present is accepted only when the kernel's closure verifier reads back
    /// exactly these bytes, modes and entries; otherwise a private, synced
    /// staging tree is promoted here on the caller's thread with one exclusive
    /// no-replace rename, and a concurrent winner is verified the same way.
    ///
    /// The deadline bounds every wait: the gate wait, the helper's staging and
    /// verification, and the check immediately before the rename. A helper
    /// still working when the caller returns is abandoned — it holds no gate
    /// and has no promotion path — and its private tree is left for the next
    /// seal's recovery, which runs under the gate and stops at its own
    /// deadline. A stage the caller does not promote, because the deadline
    /// passed or it was cancelled, is likewise left to that recovery rather
    /// than cleaned up late.
    pub fn seal_for_preflight(
        cx: &EffectContext,
        root: &StateRoot,
        closure: &Closure,
        deadline: Instant,
    ) -> Result<String, RunError> {
        cx.require(Capability::SourceStore).map_err(|_| {
            RunError::refused("a preflight seal needs a Prepare or Mutate operation")
        })?;
        if !matches!(
            cx.operation().class(),
            EffectClass::Prepare | EffectClass::Mutate
        ) {
            return Err(RunError::refused(
                "a preflight seal needs a Prepare or Mutate operation",
            ));
        }
        if root.is_machine() {
            return Err(RunError::refused(
                "immutable objects belong in a checkout journal",
            ));
        }
        // The seal's own deadline and the caller's operation budget both bind,
        // and the objects gate wait is subject to the same instant (K8).
        let deadline = match cx.operation().deadline() {
            Deadline::At(at) => deadline.min(at),
            Deadline::Open(_) => deadline,
        };
        let seal = cx.child(deadline.saturating_duration_since(Instant::now()));
        if Instant::now() >= deadline {
            return Err(RunError::unavailable("preflight seal deadline expired"));
        }
        seal.operation()
            .check("preflight seal")
            .map_err(RunError::Interrupted)?;
        let _guard = lock(&seal, root)?;
        // Recovery runs here, under the gate the whole call holds: an abandoned
        // helper's tree is then provably unpromotable, and any run-owned tree
        // is never touched.
        let staging = root
            .family(Family::Staging)
            .map_err(StoreError::from)?
            .expect("created family");
        recover(&staging, deadline);
        let stage_name = stage_name()?;
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("effigy-object-preflight".into())
            .spawn({
                let cx = seal.clone();
                let root = root.clone();
                let closure = closure.clone();
                let stage_name = stage_name.clone();
                move || {
                    // A helper never removes a stage its caller may have
                    // abandoned: removal is recovery's, under the gate, so no
                    // staging or objects write ever happens outside custody.
                    let _ = sender.send(stage_object(&cx, &root, &closure, &stage_name));
                }
            })
            .map_err(|error| {
                RunError::unavailable(format!("the preflight seal helper did not start ({error})"))
            })?;
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(Sealed::Present)) => Ok(closure.digest().to_owned()),
            Ok(Ok(Sealed::Staged)) => finish(&seal, root, closure, &stage_name, deadline),
            Ok(Err(error)) => Err(error),
            Err(RecvTimeoutError::Timeout) => {
                Err(RunError::unavailable("preflight seal deadline expired"))
            }
            Err(RecvTimeoutError::Disconnected) => Err(RunError::unavailable(
                "the preflight seal helper exited without a result",
            )),
        }
    }
}

/// The helper's whole authority: verify an existing object, or write and sync a
/// private stage. It never moves anything into `objects/`.
fn stage_object(
    cx: &EffectContext,
    root: &StateRoot,
    closure: &Closure,
    stage_name: &FileName,
) -> Result<Sealed, RunError> {
    let name = validate_digest(closure.digest())?;
    let objects = root
        .family(Family::Objects)
        .map_err(StoreError::from)?
        .expect("created family");
    let staging = root
        .family(Family::Staging)
        .map_err(StoreError::from)?
        .expect("created family");
    let observe = cx
        .operation()
        .observe()
        .map_err(|_| RunError::unavailable("a preflight seal needs a finite budget"))?;
    #[cfg(test)]
    test_hooks::verification_point();
    if verified(&objects, closure, &observe)?.is_some() {
        return Ok(Sealed::Present);
    }
    cx.operation()
        .check("preflight seal")
        .map_err(RunError::Interrupted)?;
    let stage = staging
        .child(stage_name.as_str(), Trust::Private, true)
        .map_err(StoreError::from)?
        .expect("created staging");
    let staged = (|| -> Result<(), RunError> {
        let dir = stage
            .child(name.as_str(), Trust::Private, true)
            .map_err(StoreError::from)?
            .expect("created closure");
        write(&dir, closure, cx)?;
        // The rename later publishes a durable tree: content and both
        // directories are already synced.
        stage.sync().map_err(StoreError::from)?;
        staging.sync().map_err(StoreError::from)?;
        cx.operation()
            .check("preflight seal")
            .map_err(RunError::Interrupted)?;
        #[cfg(test)]
        test_hooks::staging_point();
        Ok(())
    })();
    match staged {
        Ok(()) => Ok(Sealed::Staged),
        Err(error) => {
            let _ = staging.remove_tree(stage_name, Trust::Private, STAGING_DEPTH);
            let _ = staging.sync();
            Err(error)
        }
    }
}

/// The promotion, on the caller's thread and under the objects gate it took.
fn finish(
    seal: &EffectContext,
    root: &StateRoot,
    closure: &Closure,
    stage_name: &FileName,
    deadline: Instant,
) -> Result<String, RunError> {
    let name = validate_digest(closure.digest())?;
    let objects = root
        .family(Family::Objects)
        .map_err(StoreError::from)?
        .expect("created family");
    let staging = root
        .family(Family::Staging)
        .map_err(StoreError::from)?
        .expect("created family");
    let Some(stage) = staging
        .child(stage_name.as_str(), Trust::Private, false)
        .map_err(StoreError::from)?
    else {
        return Err(RunError::broken(
            "the preflight stage vanished before its promotion",
        ));
    };
    // The last decision the caller may take late is this one: nothing is
    // promoted once the seal's deadline has passed or it was cancelled, and
    // such a stage is left to the next seal's recovery instead of being
    // cleaned up late.
    if Instant::now() >= deadline {
        return Err(RunError::unavailable("preflight seal deadline expired"));
    }
    seal.operation()
        .check("preflight seal")
        .map_err(RunError::Interrupted)?;
    let observe = seal
        .operation()
        .observe()
        .map_err(|_| RunError::unavailable("a preflight seal needs a finite budget"))?;
    #[cfg(test)]
    test_hooks::promotion_point();
    let promoted = promote(&objects, &stage, &name, closure, &observe);
    // The decision is taken, so the private tree this seal staged is its own to
    // remove whatever that decision was.
    let _ = staging.remove_tree(stage_name, Trust::Private, STAGING_DEPTH);
    let _ = staging.sync();
    promoted
}

/// One exclusive no-replace move of the exact staged closure, then a verified
/// read of any concurrent winner.
fn promote(
    objects: &DirFd,
    stage: &DirFd,
    name: &FileName,
    closure: &Closure,
    observe: &ObserveContext,
) -> Result<String, RunError> {
    if !stage
        .move_to_exclusive(name, objects, name)
        .map_err(StoreError::from)?
    {
        // Another sealer already published the name. Trust it only if the
        // verifier reads back exactly this closure.
        if verified(objects, closure, observe)?.is_none() {
            return Err(RunError::broken("preflight object collision"));
        }
    }
    objects.sync().map_err(StoreError::from)?;
    Ok(closure.digest().to_owned())
}

/// The kernel's one trusted closure reader: bytes, private modes, no hard link
/// and no entry the digest cannot name. `Ok(None)` is absence.
fn verified(
    objects: &DirFd,
    closure: &Closure,
    cx: &ObserveContext,
) -> Result<Option<Object>, StoreError> {
    let name = validate_digest(closure.digest())?;
    if objects.entry(&name)?.is_none() {
        return Ok(None);
    }
    let object = open(objects, closure.digest(), cx)?;
    if object.closure != *closure {
        return Err(invalid("published object is not this closure"));
    }
    Ok(Some(object))
}

/// Removes every abandoned seal tree, stopping at the caller's deadline so
/// recovery can never spend the budget the seal needs. Only the seal prefix is
/// ever removed, so a run's own staging tree stays its settle's to remove.
fn recover(staging: &DirFd, deadline: Instant) {
    let Ok(entries) = staging.list(MAX_FILES) else {
        return;
    };
    let mut removed = false;
    for entry in entries {
        if Instant::now() >= deadline {
            // The rest is a later seal's recovery; this seal keeps its budget.
            return;
        }
        let Some(name) = entry
            .to_str()
            .filter(|name| name.starts_with(STAGING_PREFIX))
            .and_then(|name| FileName::new(name).ok())
        else {
            continue;
        };
        removed |= staging
            .remove_tree(&name, Trust::Private, STAGING_DEPTH)
            .is_ok();
    }
    if removed {
        let _ = staging.sync();
    }
}

/// A private name no other seal, process or retry can collide with.
fn stage_name() -> Result<FileName, StoreError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    FileName::new(&format!(
        "{STAGING_PREFIX}{}-{}-{nanos}",
        std::process::id(),
        NEXT_STAGE.fetch_add(1, Ordering::Relaxed),
    ))
    .map_err(StoreError::from)
}

/// Deterministic seams for the kernel's own proofs. Fault injection arms
/// thread-local state, and the staging helper is another thread, so these are
/// process-wide and only ever compiled into a test build.
#[cfg(test)]
pub(crate) mod test_hooks {
    use std::sync::Mutex;

    /// The staging helper, after durable staging and its last deadline check.
    static STAGING: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
    /// The staging helper, before it verifies an existing object.
    static VERIFICATION: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
    /// The caller, inside the objects gate, immediately before its rename.
    static PROMOTION: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);

    pub(crate) fn stall_staging(hook: impl FnOnce() + Send + 'static) {
        *STAGING
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
    }
    pub(crate) fn stall_verification(hook: impl FnOnce() + Send + 'static) {
        *VERIFICATION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
    }
    pub(crate) fn promotion(hook: impl FnOnce() + Send + 'static) {
        *PROMOTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
    }
    pub(super) fn staging_point() {
        run(&STAGING);
    }
    pub(super) fn verification_point() {
        run(&VERIFICATION);
    }
    pub(super) fn promotion_point() {
        run(&PROMOTION);
    }
    fn run(hook: &Mutex<Option<Box<dyn FnOnce() + Send>>>) {
        let hook = hook
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(hook) = hook {
            hook();
        }
    }
}
