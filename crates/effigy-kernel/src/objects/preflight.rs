//! Preflight sealing (§7.1–§7.4): verify the frozen closure an adapter is
//! about to apply, in one private owner-only tree, and hand back a typed
//! receipt. This call **never** publishes: nothing here writes canonical
//! `objects/`, so a helper that outlives its caller has no promotion path and
//! canonical promotion stays exactly where it was — the fenced publisher
//! transaction, which consumes the receipt under its objects gate.
//!
//! Layout under the checkout's private `state/staging/`:
//!
//! ```text
//! preflight-<token>.lock  0000600 owner lease, held by flock for as long as
//!                         the tree belongs to a live helper or a returned
//!                         receipt
//! preflight-<token>/      the staged closure, `<token>/<digest>/…`
//! ```
//!
//! The lease is created before the directory, so a tree a recovery finds
//! without a held lease is provably dead; a live helper's or a live receipt's
//! lease is never removed or raced. Staging and verification run on one helper
//! thread while the caller waits no longer than its deadline: a stalled write or
//! sync can finish late only inside that private tree, never in canonical
//! objects (K8's no detached canonical writer).
use super::*;
use crate::ctx::{Capability, Deadline, EffectClass, EffectContext};
use crate::fs::{self, FileName, Trust};
use crate::root::{Family, StateRoot};
use crate::run::RunError;
use std::io::Read as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Instant, SystemTime};

/// Trees and leases under `state/staging/` that this module owns. A run's
/// staging tree is named by its run id, so this prefix is never one and
/// recovery can tell them apart. Only this prefix is ever removed here.
pub(crate) const STAGING_PREFIX: &str = "preflight-";
/// Suffix of the owner lease that sits beside a seal's directory.
const LEASE_SUFFIX: &str = ".lock";
/// The token directory, the digest directory, then the closure's own bound.
const STAGING_DEPTH: usize = MAX_DEPTH + 2;
static NEXT_SEAL: AtomicU64 = AtomicU64::new(0);

/// A verified private preflight seal. It can only be minted by
/// [`Objects::seal_for_preflight`], it names no canonical object, and the only
/// way to make its closure canonical is the fenced publisher
/// ([`Objects::publish_sealed_desired`]), which consumes it under the objects
/// gate. The lease it holds keeps recovery from reclaiming the staged tree
/// while the receipt is alive.
#[derive(Debug)]
pub struct SealedPreflight {
    digest: String,
    token: String,
    verified_at: SystemTime,
    inventory: Vec<SealedEntry>,
    /// Held, not read: an flock that keeps recovery from reclaiming this
    /// receipt's private tree for as long as the receipt lives.
    _lease: std::fs::File,
}

impl SealedPreflight {
    /// The closure digest the verified tree carries.
    pub fn digest(&self) -> &str {
        &self.digest
    }
    /// The private staging token. Nothing outside this module can turn it into
    /// a canonical object.
    pub fn token(&self) -> &str {
        &self.token
    }
    /// When the tree's bytes, modes and entries finished verifying.
    pub fn verified_at(&self) -> SystemTime {
        self.verified_at
    }
    /// The verified entry inventory: root, directories and files with their
    /// private modes and byte lengths.
    pub fn inventory(&self) -> &[SealedEntry] {
        &self.inventory
    }
    /// Whether this receipt is exactly this closure: the digest it verified and
    /// the byte/mode inventory of every entry.
    pub fn matches(&self, closure: &Closure) -> bool {
        self.digest == closure.digest() && self.inventory == inventory_of(closure)
    }
    /// The private tree this receipt names, if it is still present.
    pub(crate) fn stage(&self, root: &StateRoot) -> Result<Option<DirFd>, StoreError> {
        let Some(staging) = root
            .read_family(Family::Staging)
            .map_err(StoreError::from)?
        else {
            return Ok(None);
        };
        let tree = tree_name(&self.token)?;
        staging
            .child(tree.as_str(), Trust::Private, false)
            .map_err(StoreError::from)
    }
}

impl Objects {
    /// Verify `closure` into one private, uniquely tokened staging tree under
    /// the caller's `deadline` and return the receipt that describes it.
    ///
    /// The tree is written `O_EXCL`/0600, its content and directories are
    /// synced, and the kernel's one closure verifier reads it back inside the
    /// deadline. Nothing canonical is created or replaced: this never publishes,
    /// and it takes no `Gate(Env)`. Recovery of dead seal trees runs first, under
    /// the objects gate; the seal's own staging runs outside it because it only
    /// ever writes its own leased, private tree.
    pub fn seal_for_preflight(
        cx: &EffectContext,
        root: &StateRoot,
        closure: &Closure,
        deadline: Instant,
    ) -> Result<SealedPreflight, RunError> {
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
        // The seal's own deadline and the caller's operation budget both bind.
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
        // Recovery owns the objects gate: it removes only seal trees whose lease
        // proves them dead, and only while it holds that gate. The seal's own
        // private tree needs no gate and never blocks publishing behind it.
        recover_gated(&seal, root, deadline)?;
        let token = token()?;
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("effigy-object-preflight".into())
            .spawn({
                let cx = seal.clone();
                let root = root.clone();
                let closure = closure.clone();
                let token = token.clone();
                move || {
                    // The helper's whole authority is its own private tree: it
                    // never removes anything and cannot reach `objects/`.
                    let _ = sender.send(stage_object(&cx, &root, &closure, &token));
                }
            })
            .map_err(|error| {
                RunError::unavailable(format!("the preflight seal helper did not start ({error})"))
            })?;
        let staged = match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(staged) => staged?,
            Err(RecvTimeoutError::Timeout) => {
                return Err(RunError::unavailable("preflight seal deadline expired"))
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(RunError::unavailable(
                    "the preflight seal helper exited without a result",
                ))
            }
        };
        // The last decision the caller may take late: a receipt is only handed
        // out while its seal is still inside the caller's deadline and not
        // cancelled. A late tree stays private for recovery.
        if Instant::now() >= deadline {
            return Err(RunError::unavailable("preflight seal deadline expired"));
        }
        seal.operation()
            .check("preflight seal")
            .map_err(RunError::Interrupted)?;
        Ok(SealedPreflight {
            digest: closure.digest().to_owned(),
            token,
            verified_at: staged.verified_at,
            inventory: staged.inventory,
            _lease: staged.lease,
        })
    }
}

/// What the helper hands back: the verified inventory, when it finished, and
/// the owner lease it keeps open so recovery cannot reclaim a live tree.
struct Staged {
    verified_at: SystemTime,
    inventory: Vec<SealedEntry>,
    lease: std::fs::File,
}

/// The helper's whole authority: create its own leased token tree, write it,
/// sync it and verify it. It never removes anything and never touches
/// `objects/`.
fn stage_object(
    cx: &EffectContext,
    root: &StateRoot,
    closure: &Closure,
    token: &str,
) -> Result<Staged, RunError> {
    let name = validate_digest(closure.digest())?;
    let staging = root
        .family(Family::Staging)
        .map_err(StoreError::from)?
        .expect("created family");
    // The lease exists before the directory, so any tree a recovery can see is
    // either leased by a live owner or provably dead.
    let lease_name = lease_name(token)?;
    let (lease, _) = staging
        .open_file(
            &lease_name,
            libc::O_RDWR | libc::O_EXCL,
            Some(crate::fs::FILE_MODE),
            Trust::Private,
        )
        .map_err(StoreError::from)?
        .expect("created lease");
    if !fs::try_flock(&lease, libc::LOCK_EX).map_err(|error| {
        StoreError::from(FsError::io(
            "flock",
            &staging.join(lease_name.as_str()),
            &error,
        ))
    })? {
        return Err(RunError::unavailable(
            "a preflight seal lease could not be taken",
        ));
    }
    let tree = tree_name(token)?;
    let stage = staging
        .child(tree.as_str(), Trust::Private, true)
        .map_err(StoreError::from)?
        .expect("created staging");
    let dir = stage
        .child(name.as_str(), Trust::Private, true)
        .map_err(StoreError::from)?
        .expect("created closure");
    write(&dir, closure, cx)?;
    // The verified tree is durable before it is read back.
    stage.sync().map_err(StoreError::from)?;
    staging.sync().map_err(StoreError::from)?;
    #[cfg(test)]
    test_hooks::staged_point();
    // The kernel's one closure verifier decides the receipt: bytes, digest,
    // private modes, no hard link and no entry the digest cannot name.
    let observe = cx
        .operation()
        .observe()
        .map_err(|_| RunError::unavailable("a preflight seal needs a finite budget"))?;
    let verified = open(&stage, closure.digest(), &observe)?;
    if verified.closure != *closure {
        return Err(RunError::broken("the verified seal is not this closure"));
    }
    #[cfg(test)]
    test_hooks::verified_point();
    cx.operation()
        .check("preflight seal")
        .map_err(RunError::Interrupted)?;
    Ok(Staged {
        verified_at: SystemTime::now(),
        inventory: verified.inventory,
        lease,
    })
}

/// Recovery of dead seal trees, on the caller and under the objects gate: a tree
/// is removed only when its lease proves no helper and no returned receipt owns
/// it. A live lease is left exactly as it is, so a helper still writing after
/// its caller timed out is never raced.
fn recover_gated(cx: &EffectContext, root: &StateRoot, deadline: Instant) -> Result<(), RunError> {
    let _guard = lock(cx, root)?;
    let Some(staging) = root
        .read_family(Family::Staging)
        .map_err(StoreError::from)?
    else {
        return Ok(());
    };
    let entries = staging.list(MAX_FILES).map_err(StoreError::from)?;
    for entry in entries {
        if Instant::now() >= deadline {
            // The rest is a later seal's recovery; this seal keeps its budget.
            return Ok(());
        }
        let Some(name) = entry
            .to_str()
            .filter(|name| name.starts_with(STAGING_PREFIX))
        else {
            continue;
        };
        let Ok(name) = FileName::new(name) else {
            continue;
        };
        if let Some(token) = token_of_lease(name.as_str()) {
            // A lease with no tree is residue of a completed consumption — but
            // only a *free* one: an owner creates its lease before its tree, so
            // a held lease with no tree yet is a live helper mid-create.
            if staging
                .entry(&tree_name(token)?)
                .map_err(StoreError::from)?
                .is_none()
            {
                if let Some((lease, free)) = lease_state(&staging, token)? {
                    if free {
                        let _ = staging.remove(&name, false);
                        let _ = staging.sync();
                    }
                    drop(lease);
                }
            }
            continue;
        }
        let Some(token) = token_of_tree(name.as_str()) else {
            continue;
        };
        let Some((lease, free)) = lease_state(&staging, token)? else {
            // No lease can exist for this tree in this layout, so it is dead
            // residue from an interrupted create or an older build.
            let _ = staging.remove_tree(&name, Trust::Private, STAGING_DEPTH);
            let _ = staging.sync();
            continue;
        };
        if !free {
            // A live helper or a live receipt owns this tree: never touch it.
            continue;
        }
        let _ = staging.remove_tree(&name, Trust::Private, STAGING_DEPTH);
        let _ = staging.remove(&lease_name(token)?, false);
        let _ = staging.sync();
        // The lease is held until the tree is gone, so no second recovery and
        // no late helper can interleave.
        drop(lease);
    }
    Ok(())
}

/// Whether a seal tree's lease is free (dead) or held (live), with the lease
/// file kept open so the caller can hold it across removal.
fn lease_state(staging: &DirFd, token: &str) -> Result<Option<(std::fs::File, bool)>, StoreError> {
    let name = lease_name(token)?;
    let Some((lease, _)) = staging
        .open_file(&name, libc::O_RDWR, None, Trust::Private)
        .map_err(StoreError::from)?
    else {
        return Ok(None);
    };
    let free = fs::try_flock(&lease, libc::LOCK_EX)
        .map_err(|error| FsError::io("flock", &staging.join(name.as_str()), &error))?;
    Ok(Some((lease, free)))
}

/// Removes one consumed receipt's private tree and lease, under the objects gate
/// the publisher holds. Absence is success: a replay of the same receipt finds
/// nothing to remove.
pub(crate) fn consume(staging: &DirFd, token: &str) -> Result<(), StoreError> {
    let _ = staging.remove_tree(&tree_name(token)?, Trust::Private, STAGING_DEPTH);
    let _ = staging.remove(&lease_name(token)?, false);
    staging.sync().map_err(StoreError::from)
}

/// The inventory a verified tree of this closure must have. It is derived from
/// the closure alone, so a receipt minted from a tree that disagrees with the
/// closure can never match it.
fn inventory_of(closure: &Closure) -> Vec<SealedEntry> {
    let mut entries = vec![SealedEntry {
        path: String::new(),
        kind: SealedKind::Directory,
        mode: crate::fs::DIR_MODE,
        len: 0,
    }];
    let mut directories: BTreeMap<String, ()> = BTreeMap::new();
    for (path, bytes) in &closure.files {
        let mut prefix = String::new();
        let parts: Vec<_> = path.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            directories.insert(prefix.clone(), ());
        }
        entries.push(SealedEntry {
            path: path.clone(),
            kind: SealedKind::File,
            mode: crate::fs::FILE_MODE,
            len: bytes.len() as u64,
        });
    }
    entries.extend(directories.keys().map(|path| SealedEntry {
        path: path.clone(),
        kind: SealedKind::Directory,
        mode: crate::fs::DIR_MODE,
        len: 0,
    }));
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    entries
}

/// A private name no other seal, process or retry can collide with: this
/// process and helper identity, a per-process counter and a random token.
fn token() -> Result<String, RunError> {
    let mut random = [0u8; 8];
    // A seal whose token could be guessed cannot be uniquely owned, so a host
    // without its randomness device refuses instead of inventing a name.
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut random))
        .map_err(|error| {
            RunError::unavailable(format!("a preflight seal needs a random token ({error})"))
        })?;
    let random = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    Ok(format!(
        "{random}-{nanos:x}-{}-{}",
        std::process::id(),
        NEXT_SEAL.fetch_add(1, Ordering::Relaxed)
    ))
}

fn tree_name(token: &str) -> Result<FileName, StoreError> {
    FileName::new(&format!("{STAGING_PREFIX}{token}")).map_err(StoreError::from)
}

fn lease_name(token: &str) -> Result<FileName, StoreError> {
    FileName::new(&format!("{STAGING_PREFIX}{token}{LEASE_SUFFIX}")).map_err(StoreError::from)
}

/// The token a directory name carries, if it is one of ours.
fn token_of_tree(name: &str) -> Option<&str> {
    name.strip_prefix(STAGING_PREFIX)
}

/// The token a lease file name carries, if it is one of ours.
fn token_of_lease(name: &str) -> Option<&str> {
    token_of_tree(name)?.strip_suffix(LEASE_SUFFIX)
}

/// Deterministic seams for the kernel's own proofs. Fault injection arms
/// thread-local state and the staging helper is another thread, so these are
/// process-wide and only ever compiled into a test build.
#[cfg(test)]
pub(crate) mod test_hooks {
    use std::sync::Mutex;

    /// The helper, after durable staging and before its verification read.
    static STAGED: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
    /// The helper, after verification and before it hands the receipt back.
    static VERIFIED: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
    /// The publisher, inside the objects gate, before it consumes a receipt.
    static CONSUME: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);

    pub(crate) fn staged(hook: impl FnOnce() + Send + 'static) {
        *STAGED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
    }
    pub(crate) fn verified(hook: impl FnOnce() + Send + 'static) {
        *VERIFIED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
    }
    pub(crate) fn consume(hook: impl FnOnce() + Send + 'static) {
        *CONSUME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
    }
    pub(super) fn staged_point() {
        run(&STAGED);
    }
    pub(super) fn verified_point() {
        run(&VERIFIED);
    }
    pub(crate) fn consume_point() {
        run(&CONSUME);
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
