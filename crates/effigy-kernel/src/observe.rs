//! Unknown is preserved as uncertainty; it never becomes an absence proof.
use crate::id::{Generation, ResourceId};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
}
/// Evidence is diagnostic metadata, never raw subprocess output or credentials.
/// Its source is bounded; process/network capture bounds belong to cut 6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum AbsenceProof {
    NoGateInode,
    GateFreeRunClosed,
    BootChanged { recorded: String, current: String },
    NoSuchProcess { pid: u32 },
    StartIdentityDiffers { pid: u32 },
    GroupEmpty { pgid: u32 },
    NotInInventory { inventory: InventoryRef },
    ContainerGone { id: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InventoryRef {
    pub source: String,
    pub generation: Generation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum UnknownReason {
    TimedOut { phase: String },
    PermissionDenied,
    ToolUnavailable(String),
    Unparsable(String),
    Inconsistent(String),
    GuardUnavailable,
    Injected,
}
