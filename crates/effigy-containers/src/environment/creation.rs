//! One activation's engine creation receipt (effigy#510, contract 052 §8.12).
//!
//! A fresh workspace container belongs to the exact activation that created it.
//! That attribution cannot be read back from the engine afterwards: labels,
//! creation stamps and before/after inventories are all facts a competing
//! Compose actor reproduces (Tom's 2026-10-31 creator ruling). It also cannot
//! come from Compose's apply, which is itself a client of the engine's create:
//! the create response goes to Compose, and Compose's progress output names
//! containers rather than the ids it received.
//!
//! So a receipt is minted from exactly one thing — an engine create operation
//! whose successful response names the full container id it created
//! ([`EnvironmentBackend::create_workspace_container`]) — and it is journalled
//! as the applied result of the environment run's
//! `environment:engine-create` effect:
//!
//! * the create response's own id is read back (by that id) and must answer one
//!   container in the project, for the declared workspace service, with a
//!   `<project>-<service>-<index>` name, no one-off marker, and an immutable
//!   image id that the container's own image reference resolves to;
//! * the sealed [`CreationReceipt`] is the effect's result, and only that
//!   journal evidence answers [`created_by`] — the owning run, the `NoCreator`
//!   absence proof when every recorded create is attributed and none names the
//!   container, or an explicit unknown when an apply recorded no attribution at
//!   all;
//! * a retry reconciles the run's own effect or stays unknown. It never mints
//!   attribution from a later inventory, a name lookup or a progress line.
//!
//! No adapter issues such a create today: Compose owns the project's one apply
//! and does not surface the engine's create response, so both container
//! adapters answer `NoBackendRecord` and name the missing adapter. A first-up
//! therefore records *no attribution* rather than a weaker one, and the
//! attribution path below stays proven by deterministic fixtures until an
//! adapter can supply the create response.

use effigy_kernel::ctx::ObserveContext;
use effigy_kernel::id::{ResourceId, RunId};
use effigy_kernel::observe::{AbsenceProof, CreatorRef, Evidence, Observed, UnknownReason};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{EffectState, RunSummary};
use effigy_kernel::store::Store;
use serde::{Deserialize, Serialize};

use super::backend::{exact_image_id, CreatedContainerId, EnvironmentBackend, Workload};
use crate::runtime::scope::{COMPOSE_PROJECT_LABEL, PROJECT_LABEL};

/// The journal effect kind of one activation's engine create.
pub const CREATE_EFFECT: &str = "environment:engine-create";

/// The Compose service a container belongs to.
const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";
/// Compose marks `compose run` containers with this label.
const COMPOSE_ONEOFF_LABEL: &str = "com.docker.compose.oneoff";

/// How many of an owner's runs [`created_by`] reads before it stops looking.
/// A creation is journalled by the activation that just applied, so the answer
/// is among the newest runs of that environment.
pub const CREATION_LOOKBACK: usize = 64;

/// The schema tag of one stored creation record.
pub const CREATION_SCHEMA: &str = "effigy.environment.creation.v1";

/// One container an activation's engine create response named, verified against
/// the applying backend.
///
/// Its fields are private: only [`verify`] builds one, from the exact id the
/// create response returned and the engine's answer for that id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedContainer {
    container_id: String,
    name: String,
    service: String,
    image_id: String,
    image_reference: String,
}

impl CreatedContainer {
    /// The exact full container id the engine's create response named.
    pub fn container_id(&self) -> &str {
        &self.container_id
    }

    /// The container name the create request used.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The Compose service this container belongs to.
    pub fn service(&self) -> &str {
        &self.service
    }

    /// The container's own immutable image id, as the engine reported it.
    pub fn image_id(&self) -> &str {
        &self.image_id
    }

    /// The image reference the container was created from, which the admitted
    /// render declares.
    pub fn image_reference(&self) -> &str {
        &self.image_reference
    }
}

/// What one activation created, sealed to the environment run that created it.
///
/// There is no public constructor: only [`seal`] builds one, from ids an engine
/// create response returned and the read-back of those ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreationReceipt {
    environment: ResourceId,
    run: RunId,
    project: String,
    engine: String,
    context: String,
    created: Vec<CreatedContainer>,
    attached: Vec<String>,
}

impl CreationReceipt {
    /// The environment this activation applied.
    pub fn environment(&self) -> &ResourceId {
        &self.environment
    }

    /// The environment run whose create effect produced this receipt.
    pub fn run(&self) -> RunId {
        self.run
    }

    /// The Compose project the created containers belong to.
    pub fn project(&self) -> &str {
        &self.project
    }

    /// The engine the create ran on.
    pub fn engine(&self) -> &str {
        &self.engine
    }

    /// The engine's context (a Docker context or a Colima profile).
    pub fn context(&self) -> &str {
        &self.context
    }

    /// The containers this activation created.
    pub fn created(&self) -> &[CreatedContainer] {
        &self.created
    }

    /// The exact full ids this activation created.
    pub fn created_ids(&self) -> impl Iterator<Item = &str> {
        self.created
            .iter()
            .map(|container| container.container_id())
    }

    /// The containers the apply attached instead of creating. They are named so
    /// a caller can tell the two apart; no attached container ever carries this
    /// run's creation receipt.
    pub fn attached(&self) -> &[String] {
        &self.attached
    }

    /// Whether the engine create returned no container id at all.
    pub fn created_nothing(&self) -> bool {
        self.created.is_empty()
    }

    /// The created container with this exact full id, in this project.
    pub fn created_by(&self, project: &str, container_id: &str) -> Option<&CreatedContainer> {
        if project != self.project {
            return None;
        }
        self.created
            .iter()
            .find(|container| container.container_id() == container_id)
    }
}

/// What one create effect needs from the caller besides its backend selection.
#[derive(Debug, Clone, Copy)]
pub struct CreationRequest<'a> {
    /// The checkout the engine create and its read-back run from.
    pub workdir: &'a std::path::Path,
    /// The Compose project the workspace container belongs to.
    pub project: &'a str,
    /// The service the environment's declared workspace runs the guest binary
    /// in: the one container attribution is required for.
    pub primary_service: &'a str,
    /// The rendered Compose files the workspace service is declared in.
    pub compose_files: &'a [std::path::PathBuf],
}

/// Why an engine create could not become a receipt. Every variant names a fact
/// the activation could not prove, so the run records the effect as `Unknown`
/// and publishes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationRefusal {
    /// The create response's own id answered no container, or another one.
    Missing(String),
    /// Two created containers claim one service, so the created set is not the
    /// one the render declares.
    DuplicateService(String),
    /// The created set does not carry the environment's workspace service.
    Partial(String),
    /// A created container's own image field is a mutable reference, so the
    /// engine cannot prove which image it runs.
    ReferenceOnlyImage(String),
    /// The container's image id is not the one its reference resolves to.
    ImageMismatch(String),
    /// A created container is marked a one-off (`compose run`) container.
    OneOff(String),
    /// A created container names no service, or a name or project that
    /// disagrees with the labels it carries.
    Service(String),
    /// The engine's answer for the created ids could not be read.
    Unknown(String),
}

impl std::fmt::Display for CreationRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(detail) => write!(
                f,
                "the create response named a container the engine did not answer for: {detail}"
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

/// What one activation's engine create recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RecordedCreation {
    /// The engine create's response named these exact containers, verified
    /// against the applying backend.
    Attributed {
        engine: String,
        context: String,
        project: String,
        primary_service: String,
        created: Vec<CreatedContainer>,
        attached: Vec<String>,
    },
    /// The apply ran, but no engine create response named what it created: the
    /// backend has no such API. This is an explicit absence of attribution, so
    /// `created_by` answers unknown instead of claiming nothing was created.
    Unattributed { project: String, reason: String },
}

/// The durable shape one run's create effect result carries. It is written as
/// the effect's applied receipt, so a later read sees exactly what this
/// activation journalled and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCreation {
    schema: String,
    record: RecordedCreation,
}

impl StoredCreation {
    /// The containers this record says its run created, if it attributed any.
    pub fn created(&self) -> &[CreatedContainer] {
        match &self.record {
            RecordedCreation::Attributed { created, .. } => created,
            RecordedCreation::Unattributed { .. } => &[],
        }
    }

    /// The project this record is about.
    pub fn project(&self) -> &str {
        match &self.record {
            RecordedCreation::Attributed { project, .. } => project,
            RecordedCreation::Unattributed { project, .. } => project,
        }
    }

    /// The engine and context the create ran on, when it was attributed.
    pub fn backend(&self) -> Option<(&str, &str)> {
        match &self.record {
            RecordedCreation::Attributed {
                engine, context, ..
            } => Some((engine, context)),
            RecordedCreation::Unattributed { .. } => None,
        }
    }

    /// The containers the apply attached instead of creating.
    pub fn attached(&self) -> &[String] {
        match &self.record {
            RecordedCreation::Attributed { attached, .. } => attached,
            RecordedCreation::Unattributed { .. } => &[],
        }
    }

    /// The recorded result itself.
    pub fn record(&self) -> &RecordedCreation {
        &self.record
    }
}

/// What one activation's create recorded, as the environment run returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationOutcome {
    /// The engine create's response named exact containers, verified and sealed
    /// to this run.
    Attributed(CreationReceipt),
    /// No engine create response named anything: the backend keeps no record
    /// that answers this, and the detail names the missing API.
    Unattributed { project: String, reason: String },
}

impl CreationOutcome {
    /// The attributed receipt, if this activation has one.
    pub fn receipt(&self) -> Option<&CreationReceipt> {
        match self {
            Self::Attributed(receipt) => Some(receipt),
            Self::Unattributed { .. } => None,
        }
    }

    /// Whether the activation's creation is unattributed, and why.
    pub fn unattributed(&self) -> Option<&str> {
        match self {
            Self::Attributed(_) => None,
            Self::Unattributed { reason, .. } => Some(reason),
        }
    }

    /// The exact ids this activation created; empty when unattributed.
    pub fn created_ids(&self) -> Vec<String> {
        match self {
            Self::Attributed(receipt) => receipt.created_ids().map(str::to_owned).collect(),
            Self::Unattributed { .. } => Vec::new(),
        }
    }
}

/// Verify the containers one engine create's response named.
///
/// The ids come from the create response alone. Each is read back by that same
/// id and must answer exactly one container in the project, for the declared
/// workspace service, with an immutable image id its own reference resolves to.
/// A container the create did not name is never read, so no inventory, label
/// match or timestamp can stand in for a creation this activation did not
/// perform.
pub fn verify(
    backend: &mut dyn EnvironmentBackend,
    request: CreationRequest<'_>,
    created: &[CreatedContainerId],
) -> Result<Vec<CreatedContainer>, CreationRefusal> {
    if created.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = created
        .iter()
        .map(|created| created.container_id().to_owned())
        .collect();
    let rows = match backend.containers_by_id(request.workdir, &ids) {
        Observed::Verified(rows, _) => rows,
        Observed::Absent(proof) => return Err(CreationRefusal::Missing(proof.to_string())),
        Observed::Unknown(reason) => return Err(CreationRefusal::Unknown(reason.to_string())),
    };
    let mut verified: Vec<CreatedContainer> = Vec::with_capacity(rows.len());
    let mut services: Vec<String> = Vec::with_capacity(rows.len());
    for row in &rows {
        let container = verify_row(backend, request, row)?;
        if services.iter().any(|service| service == &container.service) {
            return Err(CreationRefusal::DuplicateService(container.service.clone()));
        }
        services.push(container.service.clone());
        verified.push(container);
    }
    if !verified
        .iter()
        .any(|container| container.service == request.primary_service)
    {
        return Err(CreationRefusal::Partial(format!(
            "it created {} container(s) and none of them is the declared workspace service `{}`",
            verified.len(),
            request.primary_service
        )));
    }
    Ok(verified)
}

/// One created row, verified as the container whose id the create response
/// named.
fn verify_row(
    backend: &mut dyn EnvironmentBackend,
    request: CreationRequest<'_>,
    row: &Workload,
) -> Result<CreatedContainer, CreationRefusal> {
    let name = row.name.as_str();
    let service = match row.labels.get(COMPOSE_SERVICE_LABEL).map(String::as_str) {
        Some(service) if !service.trim().is_empty() => service.to_owned(),
        _ => {
            return Err(CreationRefusal::Service(format!(
                "container `{name}` names no Compose service"
            )))
        }
    };
    if row
        .labels
        .get(COMPOSE_ONEOFF_LABEL)
        .is_some_and(|oneoff| oneoff.eq_ignore_ascii_case("true"))
    {
        return Err(CreationRefusal::OneOff(name.to_owned()));
    }
    match row.labels.get(COMPOSE_PROJECT_LABEL) {
        Some(project) if project == request.project => {}
        Some(project) => {
            return Err(CreationRefusal::Service(format!(
                "container `{name}` belongs to project `{project}`, not `{}`",
                request.project
            )))
        }
        None => {
            return Err(CreationRefusal::Service(format!(
                "container `{name}` names no Compose project"
            )))
        }
    }
    if let Some(project) = row.labels.get(PROJECT_LABEL) {
        if project != request.project {
            return Err(CreationRefusal::Service(format!(
                "container `{name}` carries Effigy project `{project}`, not `{}`",
                request.project
            )));
        }
    }
    // The name Compose gave this container is derived from the project, the
    // service and the replica index: a name that does not carry both cannot be
    // the container the labels claim.
    let Some(index) = name
        .strip_prefix(&format!("{}-{service}-", request.project))
        .and_then(|index| index.parse::<u32>().ok())
    else {
        return Err(CreationRefusal::Service(format!(
            "container `{name}` is not `{project}-{service}-<index>`",
            project = request.project
        )));
    };
    if index == 0 {
        return Err(CreationRefusal::Service(format!(
            "container `{name}` has no replica index"
        )));
    }
    let image_id = exact_image_id(&row.image).ok_or_else(|| {
        CreationRefusal::ReferenceOnlyImage(format!(
            "container `{name}` reports `{}` where an image id belongs",
            row.image
        ))
    })?;
    if row.image_reference.trim().is_empty() {
        return Err(CreationRefusal::ImageMismatch(format!(
            "container `{name}` names no image reference"
        )));
    }
    match backend.image_id(request.workdir, &row.image_reference) {
        Observed::Verified(resolved, _) => {
            if resolved != image_id {
                return Err(CreationRefusal::ImageMismatch(format!(
                    "container `{name}` runs `{image_id}` while `{}` currently resolves to `{resolved}`",
                    row.image_reference
                )));
            }
        }
        Observed::Absent(proof) => {
            return Err(CreationRefusal::ImageMismatch(format!(
                "`{}` resolves to no image on this backend: {proof}",
                row.image_reference
            )))
        }
        Observed::Unknown(reason) => return Err(CreationRefusal::Unknown(reason.to_string())),
    }
    Ok(CreatedContainer {
        container_id: row.id.clone(),
        name: name.to_owned(),
        service,
        image_id,
        image_reference: row.image_reference.clone(),
    })
}

/// Seal a verified created set to the exact environment run that created it.
///
/// Crate-private construction is the whole point: no caller can pair its own
/// run with a container another activation created.
pub(super) fn seal(
    environment: &ResourceId,
    run: RunId,
    request: CreationRequest<'_>,
    engine: &str,
    context: &str,
    created: Vec<CreatedContainer>,
    attached: Vec<String>,
) -> CreationReceipt {
    CreationReceipt {
        environment: environment.clone(),
        run,
        project: request.project.to_owned(),
        engine: engine.to_owned(),
        context: context.to_owned(),
        created,
        attached,
    }
}

/// The outcome one journalled record names for the run that wrote it.
pub(super) fn outcome(
    environment: &ResourceId,
    run: RunId,
    record: RecordedCreation,
) -> CreationOutcome {
    match record {
        RecordedCreation::Attributed {
            engine,
            context,
            project,
            created,
            attached,
            ..
        } => CreationOutcome::Attributed(CreationReceipt {
            environment: environment.clone(),
            run,
            project,
            engine,
            context,
            created,
            attached,
        }),
        RecordedCreation::Unattributed { project, reason } => {
            CreationOutcome::Unattributed { project, reason }
        }
    }
}

/// The effect result one run's create journals.
pub(super) fn stored(record: RecordedCreation) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(StoredCreation {
        schema: CREATION_SCHEMA.to_owned(),
        record,
    })
}

/// Read one stored creation from an effect receipt, refusing a value this
/// build does not understand instead of reading past its schema.
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

/// `created_by(project, full_id) -> Observed<RunId>`: the environment run whose
/// own engine create response named this exact container.
///
/// The answer is durable journal evidence and nothing else. An attributed
/// create receipt naming this exact full id in this project answers
/// `Verified(run)`; a readable journal whose every create is attributed and
/// names no such container answers `Absent(NoCreator)`, which also covers a
/// replacement container (a new id this environment never created) and a
/// container another actor made. An apply that recorded no attribution leaves
/// the answer `Unknown(NoBackendRecord)`, because nothing proves that
/// activation did not create this container. A store or a receipt that cannot be
/// read is `Unknown`.
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
    let mut unattributed: Option<String> = None;
    for run in runs {
        let effects = match effigy_kernel::run::effects(&reader, run.id) {
            Observed::Verified(effects, _) => effects,
            Observed::Absent(_) => continue,
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        // `Settlement::Retain` keeps an applied effect's receipt at settle and
        // only rewrites its state, so both states name a create this run
        // actually performed. An `Unknown` create recorded no authenticated
        // result, so its reason is kept: an unverified creation must never read
        // as the absence of one.
        for effect in effects.iter().filter(|effect| {
            effect.spec.kind == CREATE_EFFECT
                && matches!(
                    effect.state,
                    EffectState::Applied | EffectState::Retained | EffectState::Unknown
                )
        }) {
            if effect.state == EffectState::Unknown {
                unattributed.get_or_insert_with(|| {
                    effect.reason.clone().unwrap_or_else(|| {
                        "the create effect has no authenticated result".to_owned()
                    })
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
                RecordedCreation::Attributed { created, .. } => {
                    if !created
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
                RecordedCreation::Unattributed { reason, .. } => {
                    unattributed.get_or_insert_with(|| reason.clone());
                }
            }
        }
    }
    if let Some(run) = found {
        return Observed::Verified(
            run,
            Evidence::new("environment create effect journal".into(), None)
                .expect("fixed evidence"),
        );
    }
    match unattributed {
        Some(reason) => Observed::Unknown(UnknownReason::NoBackendRecord(reason)),
        None => Observed::Absent(AbsenceProof::NoCreator {
            creator: CreatorRef {
                project: project.to_owned(),
                container_id: container_id.to_owned(),
            },
        }),
    }
}
