//! Verified checkout and machine state roots (§7.1), with separate database
//! names. A machine root never implicitly opens the legacy central database.
use crate::ctx::{Capability, EffectContext, ObserveContext, Placement};
use crate::fs::{DirFd, FsError, Trust};
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use std::path::{Path, PathBuf};

/// Kernel-owned directories under the state root. `kernel.db` is family 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Gates,
    Sock,
    Runs,
    Objects,
    Staging,
}
impl Family {
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Gates => "gates",
            Self::Sock => "sock",
            Self::Runs => "runs",
            Self::Objects => "objects",
            Self::Staging => "staging",
        }
    }
}

/// An opened, verified state root. Opened for effects it is created, trusted
/// and local; opened for observation it is never created or changed.
#[derive(Debug, Clone)]
pub struct StateRoot {
    dir: DirFd,
    owner: DirFd,
    writable: bool,
    database: Database,
    adoption: bool,
}

#[derive(Debug, Clone)]
enum Database {
    Checkout,
    Machine,
}
impl StateRoot {
    /// The existing Effigy home rule: `$HOME/.effigy`. Test isolation uses a
    /// fresh `HOME`; there is no kernel-specific environment variable.
    pub fn home_from_env() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())
    }
    /// Opens a checkout's `.effigy/state/`, creating private directories as
    /// needed for Prepare, RefreshDerived and Mutate. `home` is the resolved
    /// owning checkout, not its invocation subdirectory.
    /// A guest placement is refused before any filesystem call (R1).
    pub fn effects(cx: &EffectContext, home: &Path) -> Result<Self, FsError> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| FsError::HostAuthorityRequired)?;
        let home = DirFd::walk(home, Trust::Operator)?;
        let effigy = home
            .child(".effigy", Trust::Operator, true)?
            .expect("created directory");
        let dir = effigy
            .child("state", Trust::Private, true)?
            .expect("created directory");
        dir.require_local()?;
        Ok(Self {
            dir,
            owner: home,
            writable: true,
            database: Database::Checkout,
            adoption: true,
        })
    }
    /// Read-only open. Missing is `Absent`; untrusted or unreadable is
    /// `Unknown`. A network filesystem is allowed for observation.
    pub fn observe(cx: &ObserveContext, home: &Path) -> Observed<Self> {
        if cx.placement() == Placement::Guest {
            return Observed::Unknown(UnknownReason::HostAuthorityRequired);
        }
        let opened = DirFd::walk(home, Trust::Operator).and_then(|home| {
            match home.child(".effigy", Trust::Operator, false)? {
                Some(effigy) => effigy
                    .child("state", Trust::Private, false)
                    .map(|state| state.map(|state| (home, state))),
                None => Ok(None),
            }
        });
        match opened {
            Ok(Some((owner, dir))) => Observed::Verified(
                Self {
                    dir,
                    owner,
                    writable: false,
                    database: Database::Checkout,
                    adoption: true,
                },
                Evidence::new("kernel state root".into(), None).expect("fixed source"),
            ),
            Ok(None) => Observed::Absent(AbsenceProof::NoStateRoot),
            Err(error) => Observed::Unknown(unknown(&error)),
        }
    }
    /// Shared-fact storage under `$HOME/.effigy/state/machine.db`. This never
    /// opens, initializes or repairs the legacy `kernel.db` beside it.
    pub fn machine_effects(cx: &EffectContext, home: &Path) -> Result<Self, FsError> {
        let mut root = Self::effects(cx, home)?;
        root.database = Database::Machine;
        Ok(root)
    }
    /// Pure machine-store observation. Absence never falls back to legacy data;
    /// importing a legacy source is a separate, explicitly fenced operation.
    pub fn machine_observe(cx: &ObserveContext, home: &Path) -> Observed<Self> {
        Self::observe(cx, home).map(|mut root| {
            root.database = Database::Machine;
            root
        })
    }
    /// An explicitly historical, read-only source for migration and fixture
    /// observation. This never grants authority to mutate a legacy journal.
    pub fn legacy_observe(cx: &ObserveContext, home: &Path) -> Observed<Self> {
        Self::observe(cx, home).map(|root| root.legacy_source())
    }
    pub(crate) fn legacy_source(&self) -> Self {
        let mut root = self.unpublished();
        root.database = Database::Checkout;
        root
    }
    pub(crate) fn unpublished(&self) -> Self {
        let mut root = self.clone();
        root.writable = false;
        root.adoption = false;
        root
    }
    pub(crate) fn requires_adoption(&self) -> bool {
        self.adoption
    }
    pub(crate) fn machine(&self) -> bool {
        matches!(self.database, Database::Machine)
    }
    pub(crate) fn database_name(&self) -> &'static str {
        match self.database {
            Database::Checkout => crate::store::DB_NAME,
            Database::Machine => "machine.db",
        }
    }
    pub(crate) fn verify_binding(&self) -> Result<(), FsError> {
        if DirFd::walk(self.owner.path(), Trust::Operator)?.identity()? != self.owner.identity()? {
            return Err(FsError::untrusted(
                self.owner.path(),
                "owning checkout directory changed",
            ));
        }
        let reopened = DirFd::walk(self.path(), Trust::Private)?;
        if self.dir.identity()? != reopened.identity()? {
            return Err(FsError::untrusted(
                self.path(),
                "state root identity changed",
            ));
        }
        Ok(())
    }
    /// An inert migration staging root inside this verified destination. It
    /// inherits database routing, but publication remains a separate authority
    /// switch; normal engine callers never receive this root.
    pub(crate) fn migration_stage(&self, name: &str) -> Result<Self, FsError> {
        if !self.writable {
            return Err(FsError::untrusted(self.path(), "read-only staging parent"));
        }
        let dir = self
            .dir
            .child(name, Trust::Private, true)?
            .expect("created staging directory");
        Ok(Self {
            dir,
            owner: self.owner.clone(),
            writable: true,
            database: self.database.clone(),
            adoption: false,
        })
    }
    /// Retained filesystem identity of the actual checkout or machine home.
    pub fn owning_directory(&self) -> &DirFd {
        &self.owner
    }
    /// Kernel file-record operations may use this already verified directory.
    pub fn state_directory(&self) -> &DirFd {
        &self.dir
    }
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
    pub(crate) fn dir(&self) -> &DirFd {
        &self.dir
    }
    pub(crate) fn writable(&self) -> bool {
        self.writable
    }
    /// A family directory. Read-only roots never create; `Ok(None)` is absence.
    pub(crate) fn read_family(&self, family: Family) -> Result<Option<DirFd>, FsError> {
        self.dir.child(family.dir_name(), Trust::Private, false)
    }
    pub(crate) fn family(&self, family: Family) -> Result<Option<DirFd>, FsError> {
        self.dir
            .child(family.dir_name(), Trust::Private, self.writable)
    }
}

pub(crate) fn unknown(error: &FsError) -> UnknownReason {
    match error {
        FsError::HostAuthorityRequired => UnknownReason::HostAuthorityRequired,
        FsError::Untrusted { reason, .. } => UnknownReason::Untrusted(reason.clone()),
        FsError::Io(io) if io.is_permission() => UnknownReason::PermissionDenied,
        other => UnknownReason::Inconsistent(other.to_string()),
    }
}
