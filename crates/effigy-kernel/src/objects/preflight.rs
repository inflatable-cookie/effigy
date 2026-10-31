//! Preflight sealing (§7.1–§7.4): verify the frozen closure an adapter is
//! about to apply, in one private owner-only tree, and hand back a typed
//! receipt. This call **never** publishes: nothing in it writes canonical
//! `objects/`, takes a gate, or acquires `Gate(Env)`, so a helper that outlives
//! its caller has no promotion path and canonical promotion stays exactly where
//! it was — the fenced publisher transaction, which consumes the receipt under
//! `Gate(Env)`.
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
//! The helper creates and locks the lease **before** it creates the tree, and
//! recovery only ever inspects a tree, never a lease on its own. A live helper
//! can therefore never have its lease unlinked before it locks it, and a tree a
//! recovery can see is either leased by a live owner or provably dead.
//!
//! Recovery removes a tree only on positive proof: its lease is absent, or its
//! lease is present and acquirable. A lease it cannot open or inspect (a trust
//! failure, a hard link or symlink, an I/O error, descriptor exhaustion) proves
//! nothing about its owner, so that tree is left untouched for a later
//! publication. The fenced publisher runs recovery after its own commit and
//! swallows every failure, so no residue state can turn a committed publication
//! into a failure, and the seal's calling thread never runs any of this: it
//! performs no filesystem work at all and waits only within its deadline.
use super::*;
use crate::ctx::{Capability, Deadline, EffectClass, EffectContext};
use crate::fs::{self, FileName, Trust};
use crate::ident::ProcessIdentity;
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
    /// takes no gate and no `Gate(Env)`, and this thread only waits for its
    /// helper. Recovery of dead seal trees belongs to the fenced publisher, which
    /// already holds the objects gate.
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
        // Everything below is the helper's: the token, the lease, the tree and
        // the verification read. This thread only waits, bounded by the deadline.
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("effigy-object-preflight".into())
            .spawn({
                let cx = seal.clone();
                let root = root.clone();
                let closure = closure.clone();
                move || {
                    // The helper's whole authority is its own private tree: it
                    // cannot reach `objects/`, and a tree it abandons is dead
                    // residue for the fenced publisher's recovery.
                    let _ = sender.send(stage_object(&cx, &root, &closure));
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
            token: staged.token,
            verified_at: staged.verified_at,
            inventory: staged.inventory,
            _lease: staged.lease,
        })
    }
}

/// What the helper hands back: the token it minted, the verified inventory, when
/// it finished, and the owner lease it keeps open so recovery cannot reclaim a
/// live tree.
struct Staged {
    token: String,
    verified_at: SystemTime,
    inventory: Vec<SealedEntry>,
    lease: std::fs::File,
}

/// The helper's whole authority: mint its token, create and lock its own lease,
/// create, write, sync and verify its tree. It never removes anything and never
/// touches `objects/`.
fn stage_object(
    cx: &EffectContext,
    root: &StateRoot,
    closure: &Closure,
) -> Result<Staged, RunError> {
    let name = validate_digest(closure.digest())?;
    // A seal that cannot record its own incarnation could never be proven dead,
    // so it refuses rather than leaving unclaimable residue behind.
    let identity = match crate::ident::process_identity(std::process::id()) {
        Observed::Verified(identity, _) => identity,
        Observed::Absent(_) | Observed::Unknown(_) => {
            return Err(RunError::unavailable(
                "a preflight seal needs its own process identity",
            ))
        }
    };
    let token = token(&identity)?;
    let staging = root
        .family(Family::Staging)
        .map_err(StoreError::from)?
        .expect("created family");
    // The lease is created and locked before the tree, so no tree can exist
    // before its owner holds the lease and recovery can never see a tree whose
    // ownership it cannot decide.
    let lease_path = lease_name(&token)?;
    let (lease, _) = staging
        .open_file(
            &lease_path,
            libc::O_RDWR | libc::O_EXCL,
            Some(crate::fs::FILE_MODE),
            Trust::Private,
        )
        .map_err(StoreError::from)?
        .expect("created lease");
    #[cfg(test)]
    test_hooks::lease_point();
    if !fs::try_flock(&lease, libc::LOCK_EX).map_err(|error| {
        StoreError::from(FsError::io(
            "flock",
            &staging.join(lease_path.as_str()),
            &error,
        ))
    })? {
        // Nobody else can hold a fresh, treeless lease, but a lease this helper
        // cannot own must not be left behind as if it were live.
        let _ = staging.remove(&lease_path, false);
        return Err(RunError::unavailable(
            "a preflight seal lease could not be taken",
        ));
    }
    let tree = tree_name(&token)?;
    let stage = match staging
        .child(tree.as_str(), Trust::Private, true)
        .map_err(StoreError::from)?
    {
        Some(stage) => stage,
        None => {
            // Nothing of this seal exists yet: only its own lease is removed.
            let _ = staging.remove(&lease_path, false);
            return Err(RunError::broken("the preflight stage was not created"));
        }
    };
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
        token,
        verified_at: SystemTime::now(),
        inventory: verified.inventory,
        lease,
    })
}

/// Recovers at most one dead seal artifact. Best effort by construction: the
/// fenced publisher calls this *after* its desired-reference transaction has
/// committed, so no recovery outcome — an over-full or unreadable staging
/// family, a trust refusal, an I/O error, a stalled removal — can turn that
/// committed publication into a failure. Residue simply waits for a later
/// publication, which keeps making progress, and each call removes at most one
/// provably dead artifact so the gate-held critical section stays bounded.
///
/// Removal needs positive proof, never an inference from a failure:
/// - the token records the minting process's pid and incarnation *before* any
///   file is created, so a dead or reused pid proves the owner is gone even when
///   the crash happened between creating a lease and locking it (which is how
///   crash-window leases are reclaimed rather than accumulating);
/// - a live minting process is only ever removed from when its lease is free and
///   a tree exists, i.e. the helper thread or the returned receipt that owned the
///   name dropped it; a live process's bare lease is skipped as the
///   create-before-lock instant;
/// - a lease that is held, or that cannot be opened or inspected, leaves
///   everything exactly as it is.
pub(crate) fn recover(staging: &DirFd) {
    let Ok(entries) = staging.list(MAX_FILES) else {
        // An over-full or unreadable family is not a publication failure.
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
        let is_lease = token_of_lease(name.as_str()).is_some();
        let Some(token) = (if is_lease {
            token_of_lease(name.as_str())
        } else {
            token_of_tree(name.as_str())
        }) else {
            continue;
        };
        // Liveness first, from the token's recorded process incarnation: a live
        // helper is never raced, even in the instants between creating its lease
        // and locking it, and its bare lease or tree is left alone.
        let owner = owner_of(token);
        if owner == Owner::Unknown {
            // Never remove what cannot be proven dead.
            continue;
        }
        // Positive proof of a free lease, in every case that can remove.
        let lease = match lease_state(staging, token) {
            Ok(None) => None,
            // Held: a live helper or a live returned receipt owns it.
            Ok(Some((lease, false))) => {
                drop(lease);
                continue;
            }
            // Unopenable or uninspectable (trust failure, hard link, symlink,
            // I/O error, descriptor exhaustion) proves nothing.
            Err(_) => continue,
            Ok(Some((lease, true))) => Some(lease),
        };
        if owner == Owner::Live && is_lease {
            // A bare lease with a free lock under a live minting process is the
            // instant between creating the lease and locking it: skip it. With a
            // tree present, a free lock means the helper thread or returned
            // receipt that owned the name is gone.
            let tree_present = match tree_name(token) {
                Ok(tree) => staging.entry(&tree).ok().flatten().is_some(),
                Err(_) => true,
            };
            if !tree_present {
                drop(lease);
                continue;
            }
        }
        // Remove the name's tree (when it has one) and its lease together: the
        // listing order is arbitrary, so finishing the pair here stops a later
        // call from finding a tree whose lease already went.
        if let Ok(tree) = tree_name(token) {
            let _ = staging.remove_tree(&tree, Trust::Private, STAGING_DEPTH);
        }
        if let Ok(lease_path) = lease_name(token) {
            let _ = staging.remove(&lease_path, false);
        }
        let _ = staging.sync();
        // The lease stays held until the name is gone, so no second recovery and
        // no late helper can interleave.
        drop(lease);
        return;
    }
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
    let free = fs::try_flock(&lease, libc::LOCK_EX).map_err(|error| {
        StoreError::from(FsError::io("flock", &staging.join(name.as_str()), &error))
    })?;
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
fn token(identity: &ProcessIdentity) -> Result<String, RunError> {
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
        "{:x}-{}-{random}-{nanos:x}-{:x}",
        identity.pid,
        identity_fingerprint(identity),
        NEXT_SEAL.fetch_add(1, Ordering::Relaxed)
    ))
}

/// A stable fingerprint of one process incarnation: its boot and start
/// identity, exactly the evidence a gate body records. Two reads of the same
/// process agree; a reused pid differs.
fn identity_fingerprint(identity: &ProcessIdentity) -> String {
    let mut hash = Sha256::new();
    hash.update(b"effigy.preflight.owner\0\x01");
    hash.update(identity.boot.as_str().as_bytes());
    hash.update([0u8]);
    hash.update(identity.start.text().as_bytes());
    hash.finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The owner a token names: its pid and process-incarnation fingerprint.
fn token_owner(token: &str) -> Option<(u32, &str)> {
    let (pid, rest) = token.split_once('-')?;
    let (fingerprint, _) = rest.split_once('-')?;
    Some((u32::from_str_radix(pid, 16).ok()?, fingerprint))
}

/// Whether the process that minted this token is still the same incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owner {
    /// The minting process is gone, or its pid now names another incarnation.
    Dead,
    /// The minting process is still this same incarnation.
    Live,
    /// Liveness could not be proven either way; nothing may be removed.
    Unknown,
}
fn owner_of(token: &str) -> Owner {
    let Some((pid, fingerprint)) = token_owner(token) else {
        return Owner::Unknown;
    };
    match crate::ident::process_identity(pid) {
        Observed::Verified(current, _) if identity_fingerprint(&current) == fingerprint => {
            Owner::Live
        }
        Observed::Verified(_, _) | Observed::Absent(_) => Owner::Dead,
        Observed::Unknown(_) => Owner::Unknown,
    }
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

    /// The helper, between creating its lease and locking it.
    static LEASE: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
    /// The helper, after durable staging and before its verification read.
    static STAGED: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
    /// The helper, after verification and before it hands the receipt back.
    static VERIFIED: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
    /// The publisher, inside the objects gate, immediately before its
    /// exclusive no-replace move.
    static PROMOTE: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
    /// The publisher, inside the objects gate, before it consumes a receipt.
    static CONSUME: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);

    pub(crate) fn lease(hook: impl FnOnce() + Send + 'static) {
        *LEASE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
    }
    pub(crate) fn promote(hook: impl FnOnce() + Send + 'static) {
        *PROMOTE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
    }
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
    pub(super) fn lease_point() {
        run(&LEASE);
    }
    pub(crate) fn promote_point() {
        run(&PROMOTE);
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
