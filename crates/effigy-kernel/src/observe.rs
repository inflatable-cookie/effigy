//! Unknown is preserved as uncertainty; it never becomes an absence proof.
use crate::id::{Generation, ResourceId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Observed<T> {
    Verified(T, Evidence),
    Absent(AbsenceProof),
    Unknown(UnknownReason),
}
impl<T> Observed<T> {
    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Observed<U> {
        match self {
            Self::Verified(value, evidence) => Observed::Verified(map(value), evidence),
            Self::Absent(proof) => Observed::Absent(proof),
            Self::Unknown(reason) => Observed::Unknown(reason),
        }
    }
    pub fn verified(&self) -> Option<&T> {
        match self {
            Self::Verified(value, _) => Some(value),
            _ => None,
        }
    }
    /// Why this observation is not a value, for an error that has to say what
    /// it saw without borrowing a `Display` from `T`.
    pub fn unproven(&self) -> String {
        match self {
            Self::Verified(..) => {
                "the observation found a value where another was expected".to_owned()
            }
            Self::Absent(proof) => proof.to_string(),
            Self::Unknown(reason) => reason.to_string(),
        }
    }
}
/// Evidence is diagnostic metadata, never raw subprocess output or credentials.
/// Its source is bounded; process/network capture bounds belong to cut 6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    source: String,
    generation: Option<(ResourceId, Generation)>,
}
impl Evidence {
    pub const MAX_SOURCE_BYTES: usize = 4096;
    pub fn new(
        source: String,
        generation: Option<(ResourceId, Generation)>,
    ) -> Result<Self, UnknownReason> {
        if source.is_empty() || source.len() > Self::MAX_SOURCE_BYTES {
            return Err(UnknownReason::Unparsable(
                "invalid evidence source length".into(),
            ));
        }
        Ok(Self { source, generation })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn generation(&self) -> Option<&(ResourceId, Generation)> {
        self.generation.as_ref()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbsenceProof {
    NoGateInode,
    GateFreeRunClosed,
    NoStateRoot,
    NoStore,
    NoFile {
        name: String,
    },
    NoRow {
        table: String,
    },
    BootChanged {
        recorded: String,
        current: String,
    },
    NoSuchProcess {
        pid: u32,
    },
    StartIdentityDiffers {
        pid: u32,
    },
    GroupEmpty {
        pgid: u32,
    },
    NotInInventory {
        inventory: InventoryRef,
    },
    ContainerGone {
        id: String,
    },
    /// No creation record covers this exact container: no activation that
    /// journalled its created ids named it. The proof is scoped to the record
    /// that was asked, so it never becomes a creation receipt (effigy#510).
    NoCreator {
        creator: CreatorRef,
    },
}
/// The exact container, in its project, one creation record was asked about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatorRef {
    pub project: String,
    pub container_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryRef {
    pub source: String,
    pub generation: Generation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnknownReason {
    TimedOut {
        phase: String,
    },
    /// The observation could not be taken because the thing observed held its
    /// lock for the whole window. It means "try again", never a failure.
    Busy {
        phase: String,
    },
    /// A stop request — a signal or an operator's own stop — ended the work
    /// before the observation could be taken. Nothing is wrong with the state:
    /// the work was stopped on purpose. The detail names what was stopped.
    Cancelled(String),
    PermissionDenied,
    ToolUnavailable(String),
    Unparsable(String),
    Inconsistent(String),
    GuardUnavailable,
    Injected,
    Untrusted(String),
    HostAuthorityRequired,
    StoreUnreadable(String),
}

/// What the kernel could not prove, what that means, and the next step.
impl std::fmt::Display for UnknownReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut { phase } => write!(
                f,
                "could not tell: the observation timed out during {phase}; retry once the host is less busy"
            ),
            Self::Busy { phase } => write!(
                f,
                "could not tell: the state was being written throughout {phase}; retry once nothing else is writing it"
            ),
            Self::Cancelled(detail) => write!(
                f,
                "could not tell: {detail}; nothing further was attempted"
            ),
            Self::PermissionDenied => f.write_str(
                "could not tell: this user may not read the state; check the checkout's `.effigy/state` ownership and permissions",
            ),
            Self::ToolUnavailable(tool) => write!(
                f,
                "could not tell: `{tool}` is not available on this host; install it and retry, or run `effigy doctor` to see what is missing"
            ),
            Self::Unparsable(detail) => write!(
                f,
                "could not tell: a recorded value could not be parsed ({detail}); run `effigy doctor` to inspect the state root"
            ),
            Self::Inconsistent(detail) => write!(
                f,
                "could not tell: the evidence contradicts itself ({detail}); retry, and if it persists run `effigy doctor`"
            ),
            Self::GuardUnavailable => f.write_str(
                "could not tell: the guard that makes the observation safe is unavailable; retry when the host is quieter",
            ),
            Self::Injected => f.write_str("could not tell: a test fault was injected"),
            Self::Untrusted(reason) => write!(
                f,
                "could not tell: the state root is not trusted ({reason}); Effigy only reads state it owns, so fix the mode or ownership it names and retry",
            ),
            Self::HostAuthorityRequired => f.write_str(
                "could not tell: this is host state, and a guest placement cannot read it",
            ),
            Self::StoreUnreadable(detail) => write!(
                f,
                "could not tell: the store is unreadable ({detail}); run `effigy doctor` to inspect the state root"
            ),
        }
    }
}

/// What an absence proves, in words a person reads instead of a variant name.
impl std::fmt::Display for AbsenceProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoGateInode => f.write_str("no gate file exists"),
            Self::GateFreeRunClosed => f.write_str("the run closed and left no gate"),
            Self::NoStateRoot => f.write_str("there is no state root"),
            Self::NoStore => f.write_str("there is no store"),
            Self::NoFile { name } => write!(f, "`{name}` does not exist"),
            Self::NoRow { table } => write!(f, "table `{table}` has no such row"),
            Self::BootChanged { recorded, current } => write!(
                f,
                "the host rebooted (recorded boot {recorded}, current {current})"
            ),
            Self::NoSuchProcess { pid } => write!(f, "process {pid} does not exist"),
            Self::StartIdentityDiffers { pid } => write!(
                f,
                "process {pid} exists but is a different process than the one recorded"
            ),
            Self::GroupEmpty { pgid } => write!(f, "process group {pgid} is empty"),
            Self::NotInInventory { .. } => {
                f.write_str("the process is not in the runtime inventory")
            }
            Self::ContainerGone { id } => write!(f, "container `{id}` no longer exists"),
            Self::NoCreator { creator } => write!(
                f,
                "no creation record names container `{}` in project `{}`",
                creator.container_id, creator.project
            ),
        }
    }
}

/// An observation reads as its value, or as the reason it could not be proved.
impl<T: std::fmt::Display> std::fmt::Display for Observed<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Verified(value, _) => value.fmt(f),
            Self::Absent(proof) => proof.fmt(f),
            Self::Unknown(reason) => reason.fmt(f),
        }
    }
}
