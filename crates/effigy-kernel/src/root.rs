//! The host state root `~/.effigy/state/` (§7.1) and its kernel-owned path
//! families (§7.3, families 1–6).
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
    writable: bool,
}
impl StateRoot {
    /// The existing Effigy home rule: `$HOME/.effigy`. Test isolation uses a
    /// fresh `HOME`; there is no kernel-specific environment variable.
    pub fn home_from_env() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())
    }
    /// Opens (creating 0700 as needed) for Prepare, RefreshDerived and Mutate.
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
            writable: true,
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
                Some(effigy) => effigy.child("state", Trust::Private, false),
                None => Ok(None),
            }
        });
        match opened {
            Ok(Some(dir)) => Observed::Verified(
                Self {
                    dir,
                    writable: false,
                },
                Evidence::new("kernel state root".into(), None).expect("fixed source"),
            ),
            Ok(None) => Observed::Absent(AbsenceProof::NoStateRoot),
            Err(error) => Observed::Unknown(unknown(&error)),
        }
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
