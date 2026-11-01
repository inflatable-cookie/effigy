//! A complete ID difference scoped to one gated Compose apply.
//! The exclusive environment gate excludes competing Effigy activations.
//! A same-user actor forging identical labels inside the window is explicitly
//! outside v0.15's guarantee, as is same-user retag/restore inside the gate.
//! This inference is never an engine create response.

mod image;
use image::same_reference;
pub use image::{ImageCapability, ImageIdentity};
mod window;
pub use window::{AttachedPrimaryImage, CreationPreflight};
pub(super) use window::{CreationWindow, InventoryDifference};

use effigy_kernel::ctx::ObserveContext;
use effigy_kernel::id::{ResourceId, RunId};
use effigy_kernel::observe::{Evidence, Observed, UnknownReason};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{EffectState, RunSummary};
use effigy_kernel::store::Store;
use serde::{Deserialize, Serialize};

use super::backend::{exact_image_id, EnvironmentBackend, Workload};
use crate::runtime::scope::{COMPOSE_PROJECT_LABEL, PROJECT_LABEL};

fn frozen_config_hashes(
    backend: &mut dyn EnvironmentBackend,
    workdir: &std::path::Path,
    project: &str,
    files: &[std::path::PathBuf],
    services: &std::collections::BTreeSet<String>,
    deadline: std::time::Instant,
) -> Result<std::collections::BTreeMap<String, String>, CreationRefusal> {
    if std::time::Instant::now() >= deadline {
        return Err(CreationRefusal::Unknown(
            "config hash deadline expired".into(),
        ));
    }
    let hashes = match backend.service_config_hashes_until(workdir, project, files, deadline) {
        Observed::Verified(hashes, _) => hashes,
        _ => {
            return Err(CreationRefusal::Unknown(
                "frozen Compose config hashes unavailable".into(),
            ))
        }
    };
    if std::time::Instant::now() >= deadline
        || hashes
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            != *services
    {
        return Err(CreationRefusal::Unknown(
            "frozen Compose config hashes incomplete or deadline expired".into(),
        ));
    }
    Ok(hashes)
}

pub const CREATE_EFFECT: &str = "environment:gate-scoped-id-difference";

const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";
const COMPOSE_ONEOFF_LABEL: &str = "com.docker.compose.oneoff";

pub const CREATION_LOOKBACK: usize = 64;

pub const CREATION_SCHEMA: &str = "effigy.environment.gate-scoped-creation.v5";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedContainer {
    container_id: String,
    name: String,
    service: String,
    image_id: String,
    image_reference: String,
    image_capability: ImageCapability,
}

impl CreatedContainer {
    pub fn container_id(&self) -> &str {
        &self.container_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    /// The image-inspect resolution; capability says whether the container
    /// itself also attested this immutable ID.
    pub fn image_id(&self) -> &str {
        &self.image_id
    }

    pub fn image_capability(&self) -> ImageCapability {
        self.image_capability
    }

    pub fn image_reference(&self) -> &str {
        &self.image_reference
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreationReceipt {
    environment: ResourceId,
    run: RunId,
    project: String,
    primary_service: String,
    engine: String,
    context: String,
    generation: u64,
    observed: Vec<Workload>,
    workdir: std::path::PathBuf,
    expected_created: std::collections::BTreeMap<String, Option<String>>,
    workspace_predecessor: Option<String>,
    image_identities: std::collections::BTreeMap<String, ImageIdentity>,
    reference_resolutions: std::collections::BTreeMap<String, String>,
    compose_files: Vec<std::path::PathBuf>,
    created: Vec<CreatedContainer>,
    attached: Vec<String>,
}

impl CreationReceipt {
    pub fn workspace_id(&self) -> Option<&str> {
        self.observed
            .iter()
            .find(|row| {
                row.labels.get(COMPOSE_SERVICE_LABEL).map(String::as_str)
                    == Some(self.primary_service.as_str())
            })
            .map(|row| row.id.as_str())
    }

    pub fn primary_service(&self) -> &str {
        &self.primary_service
    }
    /// Re-observe the complete attributed/attached inventory under the same
    /// activation gate. This receipt is still an inference scoped to that gate.
    pub fn reobserve(
        &self,
        backend: &mut dyn EnvironmentBackend,
        deadline: std::time::Instant,
    ) -> Result<(), CreationRefusal> {
        if std::time::Instant::now() >= deadline {
            return Err(CreationRefusal::Unknown(
                "target recheck deadline expired".into(),
            ));
        }
        if backend.selection().engine.as_str() != self.engine
            || backend.selection().context != self.context
        {
            return Err(CreationRefusal::Unknown(
                "backend differs from the applying backend".into(),
            ));
        }
        let mut current = match backend.inventory_until(&self.workdir, &self.project, deadline) {
            Observed::Verified(rows, _) => rows,
            Observed::Absent(proof) => {
                return Err(CreationRefusal::Unknown(format!(
                    "inventory unavailable: {proof}"
                )))
            }
            Observed::Unknown(reason) => return Err(CreationRefusal::Unknown(reason.to_string())),
        };
        let mut expected = self.observed.clone();
        current.sort_by(|left, right| left.id.cmp(&right.id));
        expected.sort_by(|left, right| left.id.cmp(&right.id));
        if current != expected {
            return Err(CreationRefusal::Unknown(
                "project inventory changed after apply".into(),
            ));
        }
        let hashes =
            if self.image_identities.values().any(|identity| {
                identity.capability() == ImageCapability::AttachedReferenceConfigHash
            }) {
                frozen_config_hashes(
                    backend,
                    &self.workdir,
                    &self.project,
                    &self.compose_files,
                    &self.image_identities.keys().cloned().collect(),
                    deadline,
                )?
            } else {
                std::collections::BTreeMap::new()
            };
        for row in current {
            let service = row
                .labels
                .get(COMPOSE_SERVICE_LABEL)
                .ok_or_else(|| CreationRefusal::Service(row.name.clone()))?;
            let expected = self
                .image_identities
                .get(service)
                .ok_or_else(|| CreationRefusal::ImageMismatch(row.id.clone()))?;
            let resolved = match backend.image_id_until(
                &self.workdir,
                expected.rendered_reference(),
                deadline,
            ) {
                Observed::Verified(id, _) => exact_image_id(&id),
                _ => None,
            };
            if resolved.as_ref()
                != self
                    .reference_resolutions
                    .get(expected.rendered_reference())
            {
                return Err(CreationRefusal::ImageMismatch(row.id));
            }
            let resolved =
                resolved.ok_or_else(|| CreationRefusal::ImageMismatch(row.id.clone()))?;
            let created = self.created_by(&self.project, &row.id).is_some();
            let observed = ImageIdentity::observe(
                backend,
                &row,
                expected.rendered_reference(),
                &resolved,
                created,
                hashes.get(service).map(String::as_str),
            )?;
            if &observed != expected {
                return Err(CreationRefusal::ImageMismatch(row.id));
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err(CreationRefusal::Unknown(
                "target recheck deadline expired".into(),
            ));
        }
        Ok(())
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn environment(&self) -> &ResourceId {
        &self.environment
    }

    pub fn run(&self) -> RunId {
        self.run
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    pub fn engine(&self) -> &str {
        &self.engine
    }

    pub fn context(&self) -> &str {
        &self.context
    }

    /// Frozen references, resolved IDs and the backend's actual observation capability.
    pub fn image_identities(&self) -> &std::collections::BTreeMap<String, ImageIdentity> {
        &self.image_identities
    }

    /// Required new services fixed before apply; a value names the replaced ID.
    pub fn expected_created(&self) -> &std::collections::BTreeMap<String, Option<String>> {
        &self.expected_created
    }

    /// Pre-apply workspace ID, allowed to remain attached or be replaced once.
    pub fn workspace_predecessor(&self) -> Option<&str> {
        self.workspace_predecessor.as_deref()
    }

    pub fn created(&self) -> &[CreatedContainer] {
        &self.created
    }

    pub fn created_ids(&self) -> impl Iterator<Item = &str> {
        self.created
            .iter()
            .map(|container| container.container_id())
    }

    pub fn attached(&self) -> &[String] {
        &self.attached
    }

    pub fn created_nothing(&self) -> bool {
        self.created.is_empty()
    }

    pub fn created_by(&self, project: &str, container_id: &str) -> Option<&CreatedContainer> {
        if project != self.project {
            return None;
        }
        self.created
            .iter()
            .find(|container| container.container_id() == container_id)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CreationRequest<'a> {
    pub workdir: &'a std::path::Path,
    pub project: &'a str,
    pub primary_service: &'a str,
    pub render: Option<&'a effigy_kernel::objects::Closure>,
    pub deadline: std::time::Instant,
    pub(crate) required: bool,
    pub preflight: Option<&'a CreationPreflight>,
    pub compose_files: &'a [std::path::PathBuf],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationRefusal {
    Missing(String),
    DuplicateService(String),
    Partial(String),
    ReferenceOnlyImage(String),
    ImageMismatch(String),
    OneOff(String),
    Service(String),
    Unknown(String),
}

impl std::fmt::Display for CreationRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(detail) => write!(
                f,
                "the inventory did not answer the exact requested ID set: {detail}"
            ),
            Self::DuplicateService(service) => write!(
                f,
                "the create named two containers for service `{service}`, so the created set is \
                 ambiguous"
            ),
            Self::Partial(detail) => write!(
                f,
                "the created set is incomplete: {detail}; nothing is attributed from a partial \
                 create"
            ),
            Self::ReferenceOnlyImage(detail) => write!(
                f,
                "the engine reports no immutable image id for the created container: {detail}; a \
                 mutable reference is not image identity"
            ),
            Self::ImageMismatch(detail) => write!(
                f,
                "the created container's image identity disagrees with its own reference: {detail}"
            ),
            Self::OneOff(container) => write!(
                f,
                "container `{container}` is a one-off `compose run` container, never an \
                 environment target"
            ),
            Self::Service(detail) => write!(
                f,
                "the created container does not name the service the render declares: {detail}"
            ),
            Self::Unknown(detail) => {
                write!(f, "the created containers could not be read back: {detail}")
            }
        }
    }
}

impl std::error::Error for CreationRefusal {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RecordedCreation {
    Attributed(Box<AttributedCreation>),
    Unproven { project: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributedCreation {
    pub(super) engine: String,
    pub(super) context: String,
    pub(super) project: String,
    pub(super) primary_service: String,
    pub(super) generation: u64,
    pub(super) observed: Vec<Workload>,
    pub(super) workdir: std::path::PathBuf,
    pub(super) expected_created: std::collections::BTreeMap<String, Option<String>>,
    pub(super) workspace_predecessor: Option<String>,
    pub(super) image_identities: std::collections::BTreeMap<String, ImageIdentity>,
    pub(super) reference_resolutions: std::collections::BTreeMap<String, String>,
    pub(super) compose_files: Vec<std::path::PathBuf>,
    pub(super) created: Vec<CreatedContainer>,
    pub(super) attached: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCreation {
    schema: String,
    record: RecordedCreation,
}

impl StoredCreation {
    pub fn created(&self) -> &[CreatedContainer] {
        match &self.record {
            RecordedCreation::Attributed(record) => &record.created,
            RecordedCreation::Unproven { .. } => &[],
        }
    }

    pub fn project(&self) -> &str {
        match &self.record {
            RecordedCreation::Attributed(record) => &record.project,
            RecordedCreation::Unproven { project, .. } => project,
        }
    }

    pub fn backend(&self) -> Option<(&str, &str)> {
        match &self.record {
            RecordedCreation::Attributed(record) => Some((&record.engine, &record.context)),
            RecordedCreation::Unproven { .. } => None,
        }
    }

    pub fn attached(&self) -> &[String] {
        match &self.record {
            RecordedCreation::Attributed(record) => &record.attached,
            RecordedCreation::Unproven { .. } => &[],
        }
    }

    pub fn record(&self) -> &RecordedCreation {
        &self.record
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationOutcome {
    Attributed(Box<CreationReceipt>),
    Unproven { project: String, reason: String },
}

impl CreationOutcome {
    pub fn receipt(&self) -> Option<&CreationReceipt> {
        match self {
            Self::Attributed(receipt) => Some(receipt),
            Self::Unproven { .. } => None,
        }
    }

    pub fn unproven(&self) -> Option<&str> {
        match self {
            Self::Attributed(_) => None,
            Self::Unproven { reason, .. } => Some(reason),
        }
    }

    pub fn created_ids(&self) -> Vec<String> {
        match self {
            Self::Attributed(receipt) => receipt.created_ids().map(str::to_owned).collect(),
            Self::Unproven { .. } => Vec::new(),
        }
    }
}

pub(super) fn outcome(
    environment: &ResourceId,
    run: RunId,
    record: RecordedCreation,
) -> CreationOutcome {
    match record {
        RecordedCreation::Attributed(record) => {
            let AttributedCreation {
                engine,
                context,
                project,
                primary_service,
                expected_created,
                workspace_predecessor,
                image_identities,
                reference_resolutions,
                compose_files,
                created,
                attached,
                generation,
                observed,
                workdir,
                ..
            } = *record;
            CreationOutcome::Attributed(Box::new(CreationReceipt {
                environment: environment.clone(),
                run,
                project,
                primary_service,
                engine,
                context,
                expected_created,
                workspace_predecessor,
                image_identities,
                reference_resolutions,
                compose_files,
                created,
                attached,
                generation,
                observed,
                workdir,
            }))
        }
        RecordedCreation::Unproven { project, reason } => {
            CreationOutcome::Unproven { project, reason }
        }
    }
}

pub(super) fn stored(record: RecordedCreation) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(StoredCreation {
        schema: CREATION_SCHEMA.to_owned(),
        record,
    })
}

pub(super) fn decode_stored(value: &serde_json::Value) -> Result<StoredCreation, String> {
    let stored: StoredCreation =
        serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
    if stored.schema != CREATION_SCHEMA {
        return Err(format!(
            "unknown creation receipt schema `{}`",
            stored.schema
        ));
    }
    Ok(stored)
}

pub fn created_by(
    observe: &ObserveContext,
    root: &StateRoot,
    environment: &ResourceId,
    project: &str,
    container_id: &str,
) -> Observed<RunId> {
    if !super::backend::full_container_id(container_id) {
        return Observed::Unknown(UnknownReason::Inconsistent(format!(
            "`{container_id}` is not a full container id, so no create can name it"
        )));
    }
    let reader = match Store::read(observe, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    let runs: Vec<RunSummary> =
        match effigy_kernel::run::history(&reader, environment, CREATION_LOOKBACK) {
            Observed::Verified(runs, _) => runs,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
    let mut found: Option<RunId> = None;
    let mut unproven: Option<String> = None;
    for run in runs {
        let effects = match effigy_kernel::run::effects(&reader, run.id) {
            Observed::Verified(effects, _) => effects,
            Observed::Absent(_) => continue,
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        // `Settlement::Retain` keeps an applied effect's receipt at settle and
        // only rewrites its state, so both states name a create this run
        // actually performed. An `Unknown` create recorded no gate-scoped
        // result, so its reason is kept: an unverified creation must never read
        // as the absence of one.
        for effect in effects
            .iter()
            .filter(|effect| effect.spec.kind == CREATE_EFFECT)
        {
            if !matches!(effect.state, EffectState::Applied | EffectState::Retained) {
                unproven.get_or_insert_with(|| {
                    effect
                        .reason
                        .clone()
                        .unwrap_or_else(|| "the create effect has no gate-scoped result".to_owned())
                });
                continue;
            }
            let Some(receipt) = &effect.receipt else {
                continue;
            };
            let stored = match decode_stored(receipt) {
                Ok(stored) => stored,
                Err(reason) => return Observed::Unknown(UnknownReason::Inconsistent(reason)),
            };
            if stored.project() != project {
                continue;
            }
            match stored.record() {
                RecordedCreation::Attributed(record) => {
                    if !record
                        .created
                        .iter()
                        .any(|container| container.container_id() == container_id)
                    {
                        continue;
                    }
                    match found {
                        Some(existing) if existing != run.id => {
                            return Observed::Unknown(UnknownReason::Inconsistent(format!(
                                "container `{container_id}` in project `{project}` is named by two \
                                 activations' creation receipts"
                            )))
                        }
                        _ => found = Some(run.id),
                    }
                }
                RecordedCreation::Unproven { reason, .. } => {
                    unproven.get_or_insert_with(|| reason.clone());
                }
            }
        }
    }
    if let Some(reason) = unproven {
        return Observed::Unknown(UnknownReason::Inconsistent(reason));
    }
    if let Some(run) = found {
        return Observed::Verified(
            run,
            Evidence::new("environment create effect journal".into(), None)
                .expect("fixed evidence"),
        );
    }
    Observed::Unknown(UnknownReason::Inconsistent(
        "no gate-scoped attribution for this exact ID in the bounded journal history".into(),
    ))
}
