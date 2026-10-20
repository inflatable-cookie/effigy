//! The one record per environment (contract 053 §2): the kernel resource
//! row, its object references, attachments and the engine's own fields.
//! Reading it is pure (R2): no store, gate or file is created.

use effigy_kernel::ctx::ObserveContext;
use effigy_kernel::id::{Generation, ResourceId};
use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{Attachments, Holder};
use effigy_kernel::store::Store;
use serde::{Deserialize, Serialize};

use super::backend::BackendSelection;

pub const RECORD_SCHEMA: &str = "effigy.environment.v1";

/// Who keeps an environment running when no run holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Retention {
    /// An explicit `container up` owns it until `container down`.
    Persistent,
    /// Started for its holders; idle stop runs when the last one settles.
    Held,
}

impl Retention {
    pub fn label(self) -> &'static str {
        match self {
            Self::Persistent => "persistent (explicit `container up`)",
            Self::Held => "held (stops when its last holder ends)",
        }
    }
}

/// Provenance of an import from v0.14.1 marker files and labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyImport {
    /// Each marker file read, with its SHA-256 (M2).
    pub markers: Vec<ImportedMarker>,
    /// Whether running workloads' ownership labels were verified (M7).
    pub labels: LabelEvidence,
    /// Set until the import's adoption step committed; a killed import
    /// resumes instead of being skipped.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedMarker {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "state", content = "detail")]
pub enum LabelEvidence {
    /// Every project workload carries this checkout's ownership labels.
    Verified { workloads: usize },
    /// No workload exists for the project.
    Absent,
    /// Labels could not be read or did not prove ownership: blocks
    /// destructive action until an explicit, label-verified `down` (M4).
    Unknown(String),
}

/// The environment engine's fields on its kernel resource row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineRecord {
    pub schema: String,
    pub retention: Retention,
    /// The Compose project the environment's workloads run under.
    pub project: String,
    /// The desired digest the reconciler started applying and has not yet
    /// promoted: a partial activation (replaces `.partial.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applying: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported: Option<LegacyImport>,
}

impl EngineRecord {
    pub fn new(retention: Retention, project: &str) -> Self {
        Self {
            schema: RECORD_SCHEMA.to_owned(),
            retention,
            project: project.to_owned(),
            applying: None,
            imported: None,
        }
    }
}

/// The lifecycle state the kernel row publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnvState {
    /// Applied and attachable at the current generation.
    Active,
    /// Stopped by a transition; volumes kept.
    Stopped,
    /// A run is or was in progress and has not published a state since.
    Pending,
}

/// A pure projection of one environment's record.
#[derive(Debug)]
pub struct EnvRecord {
    pub resource: ResourceId,
    pub generation: Generation,
    pub state: EnvState,
    pub desired: Option<String>,
    pub last_applied: Option<String>,
    pub superseded: Vec<String>,
    pub backend: Option<Observed<BackendSelection>>,
    pub engine: Option<Observed<EngineRecord>>,
    pub holders: Observed<Vec<Holder>>,
}

impl EnvRecord {
    /// `Absent` only with proof (no store or no row); unreadable is `Unknown`.
    pub fn read(cx: &ObserveContext, root: &StateRoot, env: &ResourceId) -> Observed<Self> {
        let reader = match Store::read(cx, root) {
            Observed::Verified(reader, _) => reader,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        let resource = match reader.resource(env) {
            Observed::Verified(resource, _) => resource,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        let refs = match reader.object_refs(env) {
            Observed::Verified(refs, _) => refs,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        let digest = |role: &str| {
            refs.iter()
                .filter(|reference| reference.role == role)
                .max_by_key(|reference| reference.seq)
                .map(|reference| reference.digest.clone())
        };
        let mut superseded = refs
            .iter()
            .filter(|reference| reference.role == "superseded")
            .collect::<Vec<_>>();
        superseded.sort_by_key(|reference| std::cmp::Reverse(reference.seq));
        let state = match resource.state.as_deref() {
            Some("active") => EnvState::Active,
            Some("stopped") => EnvState::Stopped,
            _ => EnvState::Pending,
        };
        let engine = resource.engine.as_ref().map(|value| {
            match serde_json::from_value::<EngineRecord>(value.clone()) {
                Ok(record) if record.schema == RECORD_SCHEMA => {
                    Observed::Verified(record, evidence())
                }
                Ok(record) => Observed::Unknown(UnknownReason::Inconsistent(format!(
                    "unknown environment record schema `{}`",
                    record.schema
                ))),
                Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
            }
        });
        Observed::Verified(
            Self {
                resource: env.clone(),
                generation: resource.generation,
                state,
                desired: digest("desired"),
                last_applied: digest("last_applied"),
                superseded: superseded
                    .into_iter()
                    .map(|reference| reference.digest.clone())
                    .collect(),
                backend: resource
                    .backend
                    .as_ref()
                    .map(BackendSelection::from_metadata),
                engine,
                holders: Attachments::holders(cx, root, &reader, env),
            },
            evidence(),
        )
    }

    pub fn engine(&self) -> Option<&EngineRecord> {
        self.engine.as_ref().and_then(Observed::verified)
    }

    pub fn recorded_backend(&self) -> Option<&BackendSelection> {
        self.backend.as_ref().and_then(Observed::verified)
    }

    /// Attachable at its current generation.
    pub fn active(&self) -> bool {
        self.state == EnvState::Active
    }

    /// A started apply that never promoted (replaces `.partial.json`).
    pub fn partial(&self) -> Option<&str> {
        self.engine().and_then(|record| record.applying.as_deref())
    }

    /// Holders that protect the environment (R4): every attached run,
    /// whether live, interrupted or unknown. An unreadable list protects.
    pub fn protected(&self) -> bool {
        match &self.holders {
            Observed::Verified(holders, _) => !holders.is_empty(),
            _ => true,
        }
    }
}

pub(super) fn evidence() -> effigy_kernel::observe::Evidence {
    effigy_kernel::observe::Evidence::new("environment record".into(), None)
        .expect("fixed evidence")
}
