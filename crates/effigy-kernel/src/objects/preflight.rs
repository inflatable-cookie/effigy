//! Preflight sealing (§7.1–§7.4): publish the frozen closure an adapter is
//! about to apply, before it applies it, without creating a run, a journal
//! record or a reference. Publication still owns the reference; this only makes
//! the exact bytes observable read-only and then reconcilable by the same
//! trusted digest.
//!
//! Staging is synchronous filesystem work that can stall, so it runs on one
//! helper thread while the caller keeps the objects gate and waits no longer
//! than its deadline. A helper that outlives its caller may not begin a
//! promotion: the promotion gate re-reads the caller's cancellation and the
//! deadline immediately before the single no-replace rename, and the next seal
//! removes an abandoned private staging tree.
use super::*;
use crate::ctx::{Capability, Deadline, EffectClass, EffectContext};
use crate::fs::{FileName, Trust};
use crate::root::{Family, StateRoot};
use crate::run::RunError;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Trees under `state/staging/` that this module owns. A run's staging tree is
/// named by its run id, so this prefix is never one and recovery can tell them
/// apart. Removal is only ever of this prefix.
pub(crate) const STAGING_PREFIX: &str = "preflight-";
/// The stage directory, the digest directory, then the closure's own bound.
const STAGING_DEPTH: usize = MAX_DEPTH + 2;
static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

/// One helper's right to promote. Only the holder may move a staged tree into
/// `objects/`, and only while the caller is still waiting for it.
struct Promotion {
    held: Mutex<()>,
    cancelled: AtomicBool,
    deadline: Instant,
}

impl Objects {
    /// Seal `closure` into the checkout's immutable objects under the caller's
    /// `deadline`, returning the trusted digest the existing publisher records.
    ///
    /// An object already present is accepted only when the kernel's closure
    /// verifier reads back exactly these bytes, modes and entries. Otherwise a
    /// private, synced staging tree is promoted with one exclusive no-replace
    /// rename, and a concurrent winner is verified the same way. No promotion
    /// can begin after the caller's deadline: a helper still working then is
    /// abandoned and its private tree is removed by recovery.
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
        let promotion = Arc::new(Promotion {
            held: Mutex::new(()),
            cancelled: AtomicBool::new(false),
            deadline,
        });
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("effigy-object-preflight".into())
            .spawn({
                let cx = seal.clone();
                let root = root.clone();
                let closure = closure.clone();
                let promotion = Arc::clone(&promotion);
                move || {
                    // A helper that outlives its caller has nobody to answer.
                    let _ = sender.send(seal_object(&cx, &root, &closure, &promotion));
                }
            })
            .map_err(|error| {
                RunError::unavailable(format!("the preflight seal helper did not start ({error})"))
            })?;
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                // No promotion may begin after this point. One already inside
                // the gate is one atomic no-replace rename of the exact
                // closure: it completed or it did not, and the staged tree
                // stays private until the helper or recovery removes it.
                promotion.cancelled.store(true, Ordering::SeqCst);
                Err(RunError::unavailable("preflight seal deadline expired"))
            }
            Err(RecvTimeoutError::Disconnected) => Err(RunError::unavailable(
                "the preflight seal helper exited without a result",
            )),
        }
    }
}

fn seal_object(
    cx: &EffectContext,
    root: &StateRoot,
    closure: &Closure,
    promotion: &Promotion,
) -> Result<String, RunError> {
    let digest = closure.digest().to_owned();
    let name = validate_digest(&digest)?;
    let objects = root
        .family(Family::Objects)
        .map_err(StoreError::from)?
        .expect("created family");
    let staging = root
        .family(Family::Staging)
        .map_err(StoreError::from)?
        .expect("created family");
    // A helper whose caller timed out leaves nothing promotable behind. The
    // objects gate this caller holds means no live helper owns such a tree.
    recover(&staging);
    let observe = cx
        .operation()
        .observe()
        .map_err(|_| RunError::unavailable("a preflight seal needs a finite budget"))?;
    #[cfg(test)]
    test_hooks::verification_point();
    if let Some(object) = verified(&objects, closure, &observe)? {
        return Ok(object.closure.digest().to_owned());
    }
    if promotion.cancelled.load(Ordering::SeqCst) || Instant::now() >= promotion.deadline {
        return Err(RunError::unavailable("preflight seal deadline expired"));
    }
    let stage_name = stage_name()?;
    let stage = staging
        .child(stage_name.as_str(), Trust::Private, true)
        .map_err(StoreError::from)?
        .expect("created staging");
    let staged = (|| -> Result<String, RunError> {
        let dir = stage
            .child(name.as_str(), Trust::Private, true)
            .map_err(StoreError::from)?
            .expect("created closure");
        write(&dir, closure, cx)?;
        // The rename publishes a durable tree: content and both directories.
        stage.sync().map_err(StoreError::from)?;
        staging.sync().map_err(StoreError::from)?;
        #[cfg(test)]
        test_hooks::staging_point();
        promote(
            cx, &objects, &stage, &name, &digest, closure, &observe, promotion,
        )
    })();
    // Whatever happened, this tree is private and this seal owns it.
    let _ = staging.remove_tree(&stage_name, Trust::Private, STAGING_DEPTH);
    let _ = staging.sync();
    staged
}

#[allow(clippy::too_many_arguments)]
fn promote(
    cx: &EffectContext,
    objects: &DirFd,
    stage: &DirFd,
    name: &FileName,
    digest: &str,
    closure: &Closure,
    observe: &ObserveContext,
    promotion: &Promotion,
) -> Result<String, RunError> {
    {
        // Only the gate holder promotes, and only while the caller waits.
        let _held = promotion
            .held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if promotion.cancelled.load(Ordering::SeqCst) || Instant::now() >= promotion.deadline {
            return Err(RunError::unavailable("preflight seal deadline expired"));
        }
        cx.operation()
            .check("preflight seal")
            .map_err(RunError::Interrupted)?;
        #[cfg(test)]
        test_hooks::promotion_point();
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
    }
    objects.sync().map_err(StoreError::from)?;
    Ok(digest.to_owned())
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

/// Removes every abandoned seal tree. Best effort: a tree a cancelled helper is
/// still writing is its own to finish, and a tree that is not trusted state is
/// never the reason a seal fails.
fn recover(staging: &DirFd) {
    let Ok(entries) = staging.list(MAX_FILES) else {
        return;
    };
    for entry in entries {
        let Some(name) = entry
            .to_str()
            .filter(|name| name.starts_with(STAGING_PREFIX))
            .and_then(|name| FileName::new(name).ok())
        else {
            continue;
        };
        let _ = staging.remove_tree(&name, Trust::Private, STAGING_DEPTH);
    }
    let _ = staging.sync();
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

/// Deterministic seams for the kernel's own proofs. The staged helper runs on
/// its own thread and a fault harness arms thread-local state, so these are
/// process-wide and only ever compiled into a test build.
#[cfg(test)]
pub(crate) mod test_hooks {
    use std::sync::Mutex;
    use std::time::Duration;

    static STALL_VERIFICATION: Mutex<Option<Duration>> = Mutex::new(None);
    static STALL_STAGING: Mutex<Option<Duration>> = Mutex::new(None);
    static BEFORE_PROMOTION: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);

    /// Stalls the helper in the fast-path verification, as a synchronous read
    /// of a large or slow object would.
    pub(crate) fn stall_verification(stall: Option<Duration>) {
        *STALL_VERIFICATION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = stall;
    }
    /// Stalls the helper after durable staging and before the promotion gate.
    pub(crate) fn stall_staging(stall: Option<Duration>) {
        *STALL_STAGING
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = stall;
    }
    /// Runs once, inside the promotion gate, immediately before the rename.
    pub(crate) fn before_promotion(hook: impl FnOnce() + Send + 'static) {
        *BEFORE_PROMOTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
    }
    pub(super) fn verification_point() {
        sleep(&STALL_VERIFICATION);
    }
    pub(super) fn staging_point() {
        sleep(&STALL_STAGING);
    }
    pub(super) fn promotion_point() {
        if let Some(hook) = BEFORE_PROMOTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            hook();
        }
    }
    fn sleep(stall: &Mutex<Option<Duration>>) {
        let stall = *stall
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(stall) = stall {
            std::thread::sleep(stall);
        }
    }
}
