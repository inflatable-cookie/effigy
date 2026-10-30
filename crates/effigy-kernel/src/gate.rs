//! Gates (§5.4): a never-unlinked `flock` on a stable inode. Holding it is
//! the holder's liveness (K2); there are no heartbeats, staleness or unlinks.
mod checkout_fence;
mod foreground;
pub use checkout_fence::{CheckoutFenceGuard, CheckoutFenceIdentity};
pub use foreground::{ForegroundDiag, ForegroundHolder};

use crate::ctx::{Capability, EffectClass, EffectContext, Interrupt, ObserveContext};
use crate::fs::{self, FileName, FsError, IoError, Trust, FILE_MODE};
use crate::human::Category;
use crate::id::{ResourceId, ResourceKind, RunId, Scope};
use crate::ident::{self, ProcessState};
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::root::{Family, StateRoot};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// `FailFast` still polls briefly, absorbing a concurrent probe's shared lock.
pub const FAIL_FAST: Duration = Duration::from_millis(100);
const DIAG_MAX: u64 = 4096;
const POLL_MIN: Duration = Duration::from_millis(2);
const POLL_MAX: Duration = Duration::from_millis(25);

/// Every gate wait is bounded (K8): `Until` takes a finite instant, and the
/// context's own deadline still wins when earlier.
#[derive(Debug, Clone, Copy)]
pub enum Wait {
    FailFast,
    Until(Instant),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum GateState {
    Held(Box<Observed<HolderDiag>>),
    Free,
}

/// Diagnostics only (I-G2). The body is never authority; `state` is a fresh
/// observation of the named process after its start identity matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HolderDiag {
    pub foreground: Option<ForegroundDiag>,
    pub run: Option<String>,
    pub pid: u32,
    pub start: Option<String>,
    pub boot: Option<String>,
    pub argv0: Option<String>,
    pub state: Observed<ProcessState>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    foreground: Option<ForegroundDiag>,
    v: u8,
    run: Option<String>,
    pid: u32,
    start: Option<String>,
    boot: Option<String>,
    argv0: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateError {
    Busy {
        id: ResourceId,
        holder: Box<Observed<HolderDiag>>,
    },
    TimedOut {
        id: ResourceId,
        waited: Duration,
        holder: Box<Observed<HolderDiag>>,
    },
    HostAuthorityRequired,
    UnsupportedFilesystem {
        path: PathBuf,
        fs: String,
    },
    Untrusted {
        path: PathBuf,
        reason: String,
    },
    Io(IoError),
    /// The context's class may not acquire this resource's gate (§5.3).
    ClassRefused {
        id: ResourceId,
        class: EffectClass,
    },
    /// A store transaction is open on this thread (K14: txn before gate).
    InTransaction,
    Interrupted(Interrupt),
}
impl From<FsError> for GateError {
    fn from(error: FsError) -> Self {
        match error {
            FsError::HostAuthorityRequired => Self::HostAuthorityRequired,
            FsError::Untrusted { path, reason } => Self::Untrusted { path, reason },
            FsError::UnsupportedFilesystem { path, fs } => Self::UnsupportedFilesystem { path, fs },
            FsError::Io(error) => Self::Io(error),
        }
    }
}
impl GateError {
    pub fn category(&self) -> Category {
        match self {
            // A cancellation is not a rule refusing the request.
            Self::Interrupted(_) => Category::Unavailable,
            Self::Busy { .. } | Self::HostAuthorityRequired | Self::ClassRefused { .. } => {
                Category::Refused
            }
            Self::TimedOut { .. }
            | Self::UnsupportedFilesystem { .. }
            | Self::Untrusted { .. }
            | Self::Io(_) => Category::Unavailable,
            // K14: a gate inside a store transaction is Effigy's own ordering.
            Self::InTransaction => Category::Broken,
        }
    }
}
impl std::fmt::Display for GateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A cancellation says what ended the wait and asks nothing else (F7).
        if let Self::Interrupted(interrupt @ Interrupt::Cancelled { .. }) = self {
            return write!(f, "the gate was {interrupt}");
        }
        let detail = match self {
            Self::Busy { id, holder } => {
                format!("`{}` is held{}", id.label(), holder_note(holder))
            }
            Self::TimedOut { id, waited, holder } => format!(
                "`{}` was still held after {}{}",
                id.label(),
                crate::human::human_duration(*waited),
                holder_note(holder)
            ),
            Self::HostAuthorityRequired => {
                "a lock lives on the host, and a guest placement cannot take it".to_owned()
            }
            Self::UnsupportedFilesystem { path, fs } => format!(
                "{} is on a network filesystem ({fs}), and a lock needs a local disk",
                path.display()
            ),
            Self::Untrusted { path, reason } if reason == crate::fs::OPEN_MODE_REASON => {
                format!("{} is readable by other users", path.display())
            }
            Self::Untrusted { path, reason } => {
                format!("{} is not trusted gate state ({reason})", path.display())
            }
            Self::Io(error) => {
                return write!(
                    f,
                    "the gate could not proceed: {error}; {}",
                    self.category().next_step()
                )
            }
            Self::ClassRefused { id, class } => format!(
                "a {} operation may not take `{}`, which needs a prepare-or-mutate operation",
                crate::human::effect_class_label(*class),
                id.label()
            ),
            Self::InTransaction => {
                "a store transaction is already open on this thread, and taking a gate inside it \
                 would deadlock"
                    .to_owned()
            }
            Self::Interrupted(interrupt @ Interrupt::TimedOut { .. }) => interrupt.to_string(),
            // Cancellation rendered above.
            Self::Interrupted(Interrupt::Cancelled { .. }) => unreachable!(),
        };
        f.write_str(&self.category().render("the gate", &detail))
    }
}

/// Which process holds a gate, when the probe could name it.
fn holder_note(holder: &Observed<HolderDiag>) -> String {
    match holder.verified() {
        Some(diag) => match &diag.run {
            Some(run) => format!(" by pid {} of run {run}", diag.pid),
            None => format!(" by pid {}", diag.pid),
        },
        None => String::new(),
    }
}
impl std::error::Error for GateError {}

/// Held gates. Dropping releases resource gates before their scope gates, so
/// a scope removal never runs while a resource gate is still held.
pub struct GateGuard {
    gates: Vec<(ResourceId, GateFile)>,
    scopes: Vec<(String, GateFile)>,
    /// Actions released with this guard: session listener/key custody is freed
    /// when the owning run settles *or* is dropped un-settled, because either
    /// way the gate is released here.
    release_actions: Vec<Box<dyn FnOnce() + Send>>,
}

/// A lock belongs to the open description, including copies in a concurrent
/// fork. Explicit unlock makes release independent of the child's close/exec.
#[derive(Debug)]
struct GateFile {
    file: File,
    unlock_on_drop: bool,
}
impl Drop for GateFile {
    fn drop(&mut self) {
        if self.unlock_on_drop {
            fs::unlock(&self.file);
        }
    }
}
/// Keep inherited gate sources above the helper's 20..84 target range.
/// Ordinary `try_clone` may allocate inside that range and clobber a later
/// source during pre-exec dup2. These copies remain CLOEXEC in the owner.
pub(crate) fn duplicate_custody(
    file: &impl std::os::fd::AsRawFd,
) -> std::io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    // SAFETY: the borrow keeps the source descriptor live. fcntl duplicates
    // without consuming it and returns a fresh CLOEXEC fd >=128 or -1.
    let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 128) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: successful fcntl transfers this fresh descriptor exactly once.
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
}
impl GateGuard {
    /// Foreground backup custody retains these same open descriptions. Once
    /// shared, closing a custodian must not explicitly unlock the other's
    /// flock. Workload forks still inherit only CLOEXEC copies.
    pub(crate) fn share_custody(&mut self) -> std::io::Result<Vec<std::os::fd::OwnedFd>> {
        let mut copies = Vec::new();
        for file in self
            .gates
            .iter()
            .map(|(_, f)| f)
            .chain(self.scopes.iter().map(|(_, f)| f))
        {
            copies.push(duplicate_custody(&file.file)?);
        }
        for file in self
            .gates
            .iter_mut()
            .map(|(_, f)| f)
            .chain(self.scopes.iter_mut().map(|(_, f)| f))
        {
            file.unlock_on_drop = false;
        }
        Ok(copies)
    }

    pub fn ids(&self) -> impl Iterator<Item = &ResourceId> {
        self.gates.iter().map(|(id, _)| id)
    }
    /// Admission releases auxiliary resources before adapters (K14), retaining
    /// every checkout alias of the same logical owner across token creation.
    pub(crate) fn retain_owner(&mut self, owner: &ResourceId) {
        self.gates
            .retain(|(id, _)| id == owner || id.checkout_alias_of(owner));
        self.scopes.retain(|(scope, _)| {
            self.gates
                .iter()
                .any(|(id, _)| *scope == scope_name(id.scope()))
        });
    }
    pub fn covers(&self, id: &ResourceId) -> bool {
        self.gates.iter().any(|(held, _)| held == id)
    }
    /// Records the owning run in every diagnostic body. Failure loses only
    /// diagnostics.
    pub fn set_run(&self, run: &RunId) {
        let body = body(Some(run.to_string()));
        for (_, file) in &self.gates {
            let _ = fs::rewrite(&file.file, &body);
        }
    }
}
impl GateGuard {
    /// Register an action released with this guard, exactly once.
    pub(crate) fn on_release(&mut self, action: impl FnOnce() + Send + 'static) {
        self.release_actions.push(Box::new(action));
    }
    /// Run everything registered with [`GateGuard::on_release`], exactly once.
    pub(crate) fn release_actions(&mut self) {
        for action in std::mem::take(&mut self.release_actions) {
            action();
        }
    }
}
impl std::fmt::Debug for GateGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GateGuard")
            .field("gates", &self.gates.len())
            .field("scopes", &self.scopes.len())
            .finish()
    }
}
impl Drop for GateGuard {
    fn drop(&mut self) {
        // A run dropped without settling releases its holders before the gate
        // goes: a retained listener must not outlive the owner's slot.
        self.release_actions();
        self.gates.clear();
        self.scopes.clear();
    }
}

pub struct Gate;
impl std::fmt::Debug for Gate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Gate")
    }
}
impl Gate {
    /// A named mutex for derived private files, colocated with their directory.
    /// Uses the same stable inode, identity diagnostics and bounded waits as
    /// journal gates, without opening or creating a runtime state root.
    pub fn files(
        cx: &EffectContext,
        directory: &crate::fs::DirFd,
        name: &str,
        wait: Wait,
    ) -> Result<GateGuard, GateError> {
        let root = StateRoot::file_gate_root(cx, directory)?;
        let resource =
            ResourceId::store(Scope::Machine, name).map_err(|error| GateError::Untrusted {
                path: directory.path().into(),
                reason: error.to_string(),
            })?;
        Self::acquire(cx, &root, &[resource], wait)
    }

    /// Canonical-order, all-or-nothing acquisition (K14). Guest placement is
    /// refused before the state root is touched (R1).
    pub fn acquire(
        cx: &EffectContext,
        root: &StateRoot,
        ids: &[ResourceId],
        wait: Wait,
    ) -> Result<GateGuard, GateError> {
        if crate::store::in_transaction() {
            return Err(GateError::InTransaction);
        }
        cx.require(Capability::HostAuthority)
            .map_err(|_| GateError::HostAuthorityRequired)?;
        let class = cx.operation().class();
        for id in ids {
            let allowed = match (id.kind(), class) {
                (_, EffectClass::Mutate) => true,
                (ResourceKind::Store, EffectClass::RefreshDerived) => {
                    cx.require(Capability::DerivedStore).is_ok()
                }
                (ResourceKind::Store, EffectClass::Prepare) => {
                    cx.require(Capability::SourceStore).is_ok()
                }
                _ => false,
            };
            if !allowed {
                return Err(GateError::ClassRefused {
                    id: id.clone(),
                    class,
                });
            }
        }
        if !root.writable() {
            return Err(GateError::Untrusted {
                path: root.path().into(),
                reason: "state root opened read-only".into(),
            });
        }
        let started = Instant::now();
        let mut deadline = match wait {
            Wait::FailFast => started + FAIL_FAST,
            Wait::Until(at) => at,
        };
        if let Some(left) = cx.operation().remaining() {
            deadline = deadline.min(started + left);
        }
        let mut ids = ids.to_vec();
        ids.sort_by_key(ResourceId::canonical);
        ids.dedup();
        let machine = if ids.iter().any(|id| id.scope() == Scope::Machine) {
            root.machine_gate_effects(cx)?
        } else {
            None
        };
        let mut scope_names: Vec<String> = ids.iter().map(|id| scope_name(id.scope())).collect();
        scope_names.sort();
        scope_names.dedup();

        let mut guard = GateGuard {
            gates: Vec::new(),
            scopes: Vec::new(),
            release_actions: Default::default(),
        };
        let mut scope_dirs = Vec::new();
        for name in &scope_names {
            let domain = if name == "machine" {
                machine.as_ref().unwrap_or(root)
            } else {
                root
            };
            let gates = domain.family(Family::Gates)?.expect("created directory");
            let file =
                open_gate(&gates, &format!("{name}.scope"), true)?.expect("created gate file");
            let first = ids
                .iter()
                .find(|id| &scope_name(id.scope()) == name)
                .expect("scope of an id");
            let file = lock(cx, file, libc::LOCK_SH, deadline, started, wait, first)?;
            guard.scopes.push((name.clone(), file));
            // Recreated after a removal; removal cannot run while we hold SH.
            let dir = gates.child(name, Trust::Private, true)?.expect("created");
            scope_dirs.push((name.clone(), dir));
        }
        for id in &ids {
            let scope = scope_name(id.scope());
            let (_, dir) = scope_dirs
                .iter()
                .find(|(name, _)| *name == scope)
                .expect("opened scope");
            let file =
                open_gate(dir, &format!("{}.gate", id.name()), true)?.expect("created gate file");
            let file = lock(cx, file, libc::LOCK_EX, deadline, started, wait, id)?;
            guard.gates.push((id.clone(), file));
        }
        crate::fault_point!(GateAcquire)
            .map_err(|e| GateError::Io(IoError::new("gate acquire", root.path(), &e)))?;
        let body = body(None);
        for (_, file) in &guard.gates {
            let _ = fs::rewrite(&file.file, &body);
        }
        Ok(guard)
    }

    /// Pure read: no create, at most a momentary `LOCK_SH | LOCK_NB`.
    pub fn probe(cx: &ObserveContext, root: &StateRoot, id: &ResourceId) -> Observed<GateState> {
        probe_names(cx, root, &scope_name(id.scope()), &id.name().to_string())
    }

    /// Signals the live holder of `id`'s gate, only when the gate body names
    /// `run` and the holder's start and boot identity match it again right
    /// before the signal (I-P4). A free gate, another run's holder or any
    /// unverified evidence signals nothing.
    pub fn signal_holder(
        cx: &EffectContext,
        root: &StateRoot,
        id: &ResourceId,
        run: &RunId,
        signal: i32,
    ) -> Observed<u32> {
        signal_named_holder(
            cx,
            root,
            &scope_name(id.scope()),
            &id.name().to_string(),
            ExpectedHolder::Run(run),
            signal,
        )
    }

    /// Removes a scope's gate directory under `LOCK_EX | LOCK_NB` on its
    /// scope gate (§7.4). The `.scope` inode itself is never unlinked, so an
    /// acquirer always meets the same inode a remover locked.
    pub(crate) fn remove_scope(
        cx: &EffectContext,
        root: &StateRoot,
        scope: Scope,
    ) -> Result<ScopeRemoval, GateError> {
        Self::remove_named_scope(cx, root, &scope_name(scope))
    }
    /// The same removal by the gate directory's own name, for a scope whose
    /// checkout row is already gone. `machine` is never removed.
    pub(crate) fn remove_named_scope(
        cx: &EffectContext,
        root: &StateRoot,
        name: &str,
    ) -> Result<ScopeRemoval, GateError> {
        if crate::store::in_transaction() {
            return Err(GateError::InTransaction);
        }
        cx.require(Capability::HostAuthority)
            .map_err(|_| GateError::HostAuthorityRequired)?;
        let machine = scope_name(Scope::Machine);
        if cx.operation().class() != EffectClass::Mutate || name == machine {
            return Err(GateError::ClassRefused {
                id: ResourceId::store(Scope::Machine, "gates").expect("fixed name"),
                class: cx.operation().class(),
            });
        }
        let name = name.to_owned();
        let gates = root.family(Family::Gates)?.expect("created directory");
        let Some(file) = open_gate(&gates, &format!("{name}.scope"), false)? else {
            return Ok(ScopeRemoval::Absent);
        };
        if !fs::try_flock(&file, libc::LOCK_EX)
            .map_err(|e| GateError::Io(IoError::new("flock", root.path(), &e)))?
        {
            return Ok(ScopeRemoval::Busy);
        }
        // Also release on every early return or filesystem error below.
        let file = GateFile {
            file,
            unlock_on_drop: true,
        };
        let Some(dir) = gates.child(&name, Trust::Private, false)? else {
            return Ok(ScopeRemoval::Absent);
        };
        for entry in dir.list(1 << 16)? {
            let entry = entry
                .to_str()
                .and_then(|entry| FileName::new(entry).ok())
                .filter(|entry| entry.as_str().ends_with(".gate"))
                .ok_or_else(|| GateError::Untrusted {
                    path: dir.path().into(),
                    reason: "unexpected entry in a gate scope".into(),
                })?;
            dir.remove(&entry, false)?;
        }
        gates.remove(&FileName::new(&name)?, true)?;
        gates.sync()?;
        drop(file);
        Ok(ScopeRemoval::Removed)
    }
}

pub(crate) enum ExpectedHolder<'a> {
    Run(&'a RunId),
}
/// The holder is matched by exact run, start and boot identity, never by name
/// or PID alone.
pub(crate) fn signal_named_holder(
    cx: &EffectContext,
    root: &StateRoot,
    scope: &str,
    resource: &str,
    target: ExpectedHolder<'_>,
    signal: i32,
) -> Observed<u32> {
    if cx.operation().check("gate holder signal").is_err() {
        return Observed::Unknown(UnknownReason::TimedOut {
            phase: "gate holder signal".into(),
        });
    }
    if cx.require(Capability::HostAuthority).is_err() {
        return Observed::Unknown(UnknownReason::HostAuthorityRequired);
    }
    let observe = match cx.child(Duration::from_secs(10)).operation().observe() {
        Ok(observe) => observe,
        Err(_) => {
            return Observed::Unknown(UnknownReason::TimedOut {
                phase: "machine gate holder signal".into(),
            })
        }
    };
    let machine = if scope == "machine" {
        match root.machine_gate_observe(&observe) {
            Observed::Verified(root, _) => root,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        }
    } else {
        None
    };
    let root = machine.as_ref().unwrap_or(root);
    let gates = match root.family(Family::Gates) {
        Ok(Some(gates)) => gates,
        Ok(None) => return Observed::Absent(AbsenceProof::NoGateInode),
        Err(error) => return Observed::Unknown(crate::root::unknown(&error)),
    };
    let file = match gates
        .child(scope, Trust::Private, false)
        .and_then(|dir| match dir {
            Some(dir) => open_gate(&dir, &format!("{resource}.gate"), false),
            None => Ok(None),
        }) {
        Ok(Some(file)) => file,
        Ok(None) => return Observed::Absent(AbsenceProof::NoGateInode),
        Err(error) => return Observed::Unknown(crate::root::unknown(&error)),
    };
    match fs::try_flock(&file, libc::LOCK_SH) {
        Ok(true) => {
            fs::unlock(&file);
            return Observed::Absent(AbsenceProof::NoGateInode);
        }
        Ok(false) => {}
        Err(error) => return Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
    }
    let diag = match holder(&file) {
        Observed::Verified(diag, _) => diag,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    let matches_target = match target {
        ExpectedHolder::Run(run) => diag.run.as_deref() == Some(run.to_string().as_str()),
    };
    if !matches_target || !matches!(diag.state, Observed::Verified(..)) {
        return Observed::Unknown(UnknownReason::Inconsistent(
            "the gate holder is not the expected verified owner".into(),
        ));
    }
    match ident::process_identity(diag.pid) {
        Observed::Verified(identity, _)
            if Some(identity.start.text()) == diag.start
                && Some(identity.boot.as_str()) == diag.boot.as_deref() => {}
        _ => {
            return Observed::Unknown(UnknownReason::Inconsistent(
                "the gate holder changed before the signal".into(),
            ))
        }
    }
    let pid = match i32::try_from(diag.pid) {
        Ok(pid) if pid > 0 => pid,
        _ => return Observed::Unknown(UnknownReason::Unparsable("invalid holder pid".into())),
    };
    // SAFETY: kill takes integer PID/signal values and no memory. The
    // positive, representable PID selects one process, never a group; its
    // exact boot/start identity was rechecked immediately above (I-P4).
    if unsafe { libc::kill(pid, signal) } != 0 {
        return Observed::Unknown(UnknownReason::Inconsistent(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    Observed::Verified(
        diag.pid,
        Evidence::new("kernel gate holder signal".into(), None).expect("fixed source"),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeRemoval {
    Removed,
    Absent,
    Busy,
}

/// Run phase as the store records it; `Interrupted` is never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RunPhaseFact {
    NonClosed,
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OwnerState {
    /// Non-closed with its owner gate held: the run is live.
    Held,
    /// Non-closed with a free owner gate (D4). A successor settles it first.
    Interrupted,
    Closed,
}
/// D4: a free gate with a non-closed run reads `Interrupted`. Either side
/// `Unknown` stays `Unknown` (R3); a reader never repairs the record.
pub fn owner_state(run: Observed<RunPhaseFact>, gate: Observed<GateState>) -> Observed<OwnerState> {
    let evidence = || Evidence::new("kernel run row and owner gate".into(), None).expect("fixed");
    match (run, gate) {
        (Observed::Unknown(reason), _) => Observed::Unknown(reason),
        (Observed::Absent(proof), _) => Observed::Absent(proof),
        (Observed::Verified(RunPhaseFact::Closed, _), _) => {
            Observed::Verified(OwnerState::Closed, evidence())
        }
        (Observed::Verified(RunPhaseFact::NonClosed, _), Observed::Unknown(reason)) => {
            Observed::Unknown(reason)
        }
        (Observed::Verified(RunPhaseFact::NonClosed, _), Observed::Absent(_))
        | (
            Observed::Verified(RunPhaseFact::NonClosed, _),
            Observed::Verified(GateState::Free, _),
        ) => Observed::Verified(OwnerState::Interrupted, evidence()),
        (
            Observed::Verified(RunPhaseFact::NonClosed, _),
            Observed::Verified(GateState::Held(_), _),
        ) => Observed::Verified(OwnerState::Held, evidence()),
    }
}

pub(crate) fn scope_name(scope: Scope) -> String {
    match scope {
        Scope::Machine => "machine".into(),
        Scope::Checkout(checkout) => checkout.resource().name().to_string(),
    }
}
pub(crate) fn probe_names(
    cx: &ObserveContext,
    root: &StateRoot,
    scope: &str,
    rid: &str,
) -> Observed<GateState> {
    #[cfg(all(test, feature = "fault-injection"))]
    if let Some(injected) = crate::fault::injected() {
        return injected;
    }
    if cx.check("gate probe").is_err() {
        return Observed::Unknown(UnknownReason::TimedOut {
            phase: "gate probe".into(),
        });
    }
    if cx.placement() == crate::ctx::Placement::Guest {
        return Observed::Unknown(UnknownReason::HostAuthorityRequired);
    }
    let machine = if scope == "machine" {
        match root.machine_gate_observe(cx) {
            Observed::Verified(root, _) => root,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        }
    } else {
        None
    };
    let root = machine.as_ref().unwrap_or(root);
    let gates = match root.family(Family::Gates) {
        Ok(Some(gates)) => gates,
        Ok(None) => return Observed::Absent(AbsenceProof::NoGateInode),
        Err(error) => return Observed::Unknown(crate::root::unknown(&error)),
    };
    let dir = match gates.child(scope, Trust::Private, false) {
        Ok(Some(dir)) => dir,
        Ok(None) => return Observed::Absent(AbsenceProof::NoGateInode),
        Err(error) => return Observed::Unknown(crate::root::unknown(&error)),
    };
    let file = match open_gate(&dir, &format!("{rid}.gate"), false) {
        Ok(Some(file)) => file,
        Ok(None) => return Observed::Absent(AbsenceProof::NoGateInode),
        Err(error) => return Observed::Unknown(crate::root::unknown(&error)),
    };
    let evidence = Evidence::new("kernel gate probe".into(), None).expect("fixed source");
    match fs::try_flock(&file, libc::LOCK_SH) {
        Ok(true) => {
            fs::unlock(&file);
            Observed::Verified(GateState::Free, evidence)
        }
        Ok(false) => Observed::Verified(GateState::Held(Box::new(holder(&file))), evidence),
        Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
    }
}
fn open_gate(dir: &fs::DirFd, name: &str, create: bool) -> Result<Option<File>, FsError> {
    let name = FileName::new(name)?;
    let flags = if create { libc::O_RDWR } else { libc::O_RDONLY };
    Ok(dir
        .open_file(&name, flags, create.then_some(FILE_MODE), Trust::Private)?
        .map(|(file, _)| file))
}

fn lock(
    cx: &EffectContext,
    file: File,
    op: i32,
    deadline: Instant,
    started: Instant,
    wait: Wait,
    id: &ResourceId,
) -> Result<GateFile, GateError> {
    let mut pause = POLL_MIN;
    loop {
        match fs::try_flock(&file, op) {
            Ok(true) => {
                return Ok(GateFile {
                    file,
                    unlock_on_drop: true,
                })
            }
            Ok(false) => {}
            Err(error) => {
                return Err(GateError::Io(IoError::new(
                    "flock",
                    std::path::Path::new(&id.name().to_string()),
                    &error,
                )))
            }
        }
        if let Err(interrupt @ Interrupt::Cancelled { .. }) = cx.operation().check("gate wait") {
            return Err(GateError::Interrupted(interrupt));
        }
        let now = Instant::now();
        if now >= deadline {
            let holder = Box::new(holder(&file));
            return Err(match wait {
                Wait::FailFast => GateError::Busy {
                    id: id.clone(),
                    holder,
                },
                Wait::Until(_) => GateError::TimedOut {
                    id: id.clone(),
                    waited: now - started,
                    holder,
                },
            });
        }
        std::thread::sleep(pause.min(deadline - now));
        pause = (pause * 2).min(POLL_MAX);
    }
}

fn body(run: Option<String>) -> Vec<u8> {
    serde_json::to_vec(&current_body(run)).expect("serializable body")
}
fn current_body(run: Option<String>) -> Body {
    let pid = std::process::id();
    let start = ident::process_identity(pid)
        .verified()
        .map(|identity| identity.start.text());
    let boot = ident::boot_identity()
        .verified()
        .map(|boot| boot.as_str().to_string());
    let argv0 = std::env::args_os()
        .next()
        .map(|arg| arg.to_string_lossy().chars().take(512).collect());
    Body {
        foreground: None,
        v: 1,
        run,
        pid,
        start,
        boot,
        argv0,
    }
}

/// Reads the diagnostic body. Torn, missing or mismatched: `Unknown`.
fn holder(file: &File) -> Observed<HolderDiag> {
    let bytes = match fs::read_bounded(file, DIAG_MAX) {
        Ok(Some(bytes)) if !bytes.is_empty() => bytes,
        Ok(Some(_)) => {
            return Observed::Unknown(UnknownReason::Unparsable("empty gate body".into()))
        }
        Ok(None) => {
            return Observed::Unknown(UnknownReason::Unparsable("oversized gate body".into()))
        }
        Err(error) => return Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
    };
    let body = match serde_json::from_slice::<Body>(&bytes) {
        Ok(body) if body.v == 1 => body,
        _ => return Observed::Unknown(UnknownReason::Unparsable("torn gate body".into())),
    };
    let state = match ident::process_identity(body.pid) {
        Observed::Verified(identity, _)
            if Some(identity.start.text()) == body.start
                && Some(identity.boot.as_str()) == body.boot.as_deref() =>
        {
            ident::process_state(body.pid)
        }
        Observed::Unknown(reason) => Observed::Unknown(reason),
        _ => Observed::Unknown(UnknownReason::Inconsistent(
            "gate body names a different process".into(),
        )),
    };
    Observed::Verified(
        HolderDiag {
            foreground: body.foreground,
            run: body.run,
            pid: body.pid,
            start: body.start,
            boot: body.boot,
            argv0: body.argv0,
            state,
        },
        Evidence::new("kernel gate diagnostic body".into(), None).expect("fixed source"),
    )
}
