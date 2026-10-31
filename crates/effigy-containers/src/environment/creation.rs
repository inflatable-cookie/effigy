//! One activation's engine creation effect (effigy#510, contract 052 §8.12).
//!
//! A fresh workspace container belongs to the exact activation that created it.
//! That attribution cannot be read back from the engine afterwards: labels,
//! creation stamps and before/after inventories are all facts a competing
//! Compose actor reproduces (Tom's 2026-10-31 creator ruling). So the fact is
//! taken from the apply's own create report, inside the one gated apply, and
//! journalled under the environment run that performed it:
//!
//! * the apply's create report names each container it created, by name. A
//!   container name is unique in one engine, so a create that succeeded for
//!   that name owns the container the name resolves to — a competing create for
//!   the same name fails instead of appearing, and a competing create under
//!   another name is never named by this report at all;
//! * each named container is read back by that exact name and must answer one
//!   full container id, in this project, for a declared service, with an
//!   immutable image id that the container's own reference resolves to on the
//!   applying backend;
//! * the resulting [`CreationReceipt`] is journalled as the applied result of
//!   the run's `environment:engine-create` effect. Only that journal evidence
//!   answers [`created_by`]: a container is attributed to a run exactly when a
//!   receipt that run journalled names its exact full id.
//!
//! There is no public constructor for a receipt: only [`seal`] builds one, from
//! a create report this module read back and verified. A retry reconciles the
//! run's own journalled effect or stays `Unknown`; it never mints attribution
//! from a later inventory.

use std::path::Path;

use effigy_kernel::ctx::ObserveContext;
use effigy_kernel::id::{ResourceId, RunId};
use effigy_kernel::observe::{AbsenceProof, CreatorRef, Evidence, Observed, UnknownReason};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{EffectState, RunSummary};
use effigy_kernel::store::Store;
use serde::{Deserialize, Serialize};

use super::backend::{exact_image_id, EnvironmentBackend, Workload};
use crate::runtime::scope::{COMPOSE_PROJECT_LABEL, MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL};

/// The journal effect kind of one activation's engine create.
pub const CREATE_EFFECT: &str = "environment:engine-create";

/// The Compose service a container belongs to.
const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";
/// Compose marks `compose run` containers with this label.
const COMPOSE_ONEOFF_LABEL: &str = "com.docker.compose.oneoff";

/// How many of an owner's runs [`created_by`] reads before it stops looking.
/// A creation is journalled by the activation that just applied, so the answer
/// is among the newest runs of that environment; an older journal is never
/// consulted to attribute a new container.
pub const CREATION_LOOKBACK: usize = 64;

/// What one apply's create report said it did, in container names.
///
/// The apply feeds it as the runtime reports progress; the environment run
/// reads it back once the effect returned. It is the only input to a receipt's
/// container set, which is why it never carries an id an engine did not name.
#[derive(Debug, Default, Clone)]
pub struct CreateReport {
    created: Vec<String>,
    attached: Vec<String>,
}

impl CreateReport {
    /// The runtime said it created this container. A create report names the
    /// container, so the name is kept exactly as printed.
    pub fn created(&mut self, name: &str) {
        let name = name.trim();
        if !name.is_empty() && !self.created.iter().any(|known| known == name) {
            self.created.push(name.to_owned());
        }
    }

    /// The runtime said this container already existed and was started: it is
    /// attached to this apply, never created by it.
    pub fn attached(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty()
            || self.created.iter().any(|known| known == name)
            || self.attached.iter().any(|known| known == name)
        {
            return;
        }
        self.attached.push(name.to_owned());
    }

    /// The containers the create report named as created, in report order.
    pub fn created_names(&self) -> &[String] {
        &self.created
    }

    /// The containers the apply attached rather than created.
    pub fn attached_names(&self) -> &[String] {
        &self.attached
    }

    /// Whether the apply reported creating nothing: an unchanged re-up.
    pub fn created_nothing(&self) -> bool {
        self.created.is_empty()
    }
}

/// One container this activation's create report named, verified against the
/// applying backend.
///
/// Its fields are private: only [`resolve`] builds one, from a name the create
/// report printed and an engine answer for exactly that name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedContainer {
    container_id: String,
    name: String,
    service: String,
    image_id: String,
    image_reference: String,
}

impl CreatedContainer {
    /// The exact full container id the engine answered for the created name.
    pub fn container_id(&self) -> &str {
        &self.container_id
    }

    /// The container name the create report printed.
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

/// What one activation created, sealed to the environment run that applied it.
///
/// Only the journalled effect result (see [`StoredCreation`]) can answer which
/// run created a container: [`CreationReceipt::created_by`] exists for the
/// caller that already holds the receipt, and the durable answer is
/// [`created_by`].
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

    /// The environment run that owned the create effect.
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

    /// The containers this activation created, in report order.
    pub fn created(&self) -> &[CreatedContainer] {
        &self.created
    }

    /// The exact full ids this activation created.
    pub fn created_ids(&self) -> impl Iterator<Item = &str> {
        self.created
            .iter()
            .map(|container| container.container_id())
    }

    /// The containers the apply attached instead of creating. They are named
    /// so a caller can tell the two apart; no attached container ever carries
    /// this run's creation receipt.
    pub fn attached(&self) -> &[String] {
        &self.attached
    }

    /// Whether this apply created nothing: an unchanged re-up.
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
    /// The checkout the backend commands run from.
    pub workdir: &'a Path,
    /// The Compose project the apply converges.
    pub project: &'a str,
    /// The service the environment's declared workspace needs: when the apply
    /// created containers, exactly one of them is this service.
    pub primary_service: &'a str,
}

/// Why a create report cannot become a receipt. Every variant names a fact the
/// activation could not prove, so the environment run records the effect as
/// `Unknown` and publishes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationRefusal {
    /// The create report named a container the engine answered with another
    /// id, or answered no container at all.
    Missing(String),
    /// Two created containers claim one service, so the created set is not
    /// the one the render declares.
    DuplicateService(String),
    /// The created set does not carry the environment's primary service.
    Partial(String),
    /// A created container's own image field is a mutable reference, so the
    /// engine cannot prove which image it runs.
    ReferenceOnlyImage(String),
    /// The container's image id is not the one its reference resolves to.
    ImageMismatch(String),
    /// A created container is marked a one-off (`compose run`) container.
    OneOff(String),
    /// A created container names no service, or a service the name disagrees
    /// with, so it cannot be the container the render declares.
    Service(String),
    /// The engine's answer for the named containers could not be read.
    Unknown(String),
}

impl std::fmt::Display for CreationRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(detail) => write!(
                f,
                "the apply reported creating a container the engine did not answer for: {detail}"
            ),
            Self::DuplicateService(service) => write!(
                f,
                "the apply reported creating two containers for service `{service}`, so the created \
                 set is ambiguous"
            ),
            Self::Partial(detail) => write!(
                f,
                "the apply's created set is incomplete: {detail}; nothing is attributed from a \
                 partial apply"
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
            Self::Unknown(detail) => write!(
                f,
                "the created containers could not be read back: {detail}"
            ),
        }
    }
}

impl std::error::Error for CreationRefusal {}

/// The durable shape one run's create effect result carries. It is written as
/// the effect's applied receipt, so a later read sees exactly what this
/// activation journalled and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCreation {
    schema: String,
    engine: String,
    context: String,
    project: String,
    primary_service: String,
    created: Vec<CreatedContainer>,
    attached: Vec<String>,
}

/// The schema tag of one stored creation receipt.
pub const CREATION_SCHEMA: &str = "effigy.environment.creation.v1";

impl StoredCreation {
    /// The containers this record says its run created.
    pub fn created(&self) -> &[CreatedContainer] {
        &self.created
    }

    /// The project this record is about.
    pub fn project(&self) -> &str {
        &self.project
    }

    /// The engine and context the create ran on.
    pub fn backend(&self) -> (&str, &str) {
        (&self.engine, &self.context)
    }

    /// The containers the apply attached instead of creating.
    pub fn attached(&self) -> &[String] {
        &self.attached
    }
}

/// Read the exact containers `report` named, verify each against the applying
/// backend, and keep them in report order.
///
/// The project, service, one-off marker, name shape and immutable image
/// identity are all checked against the container's own engine record: the
/// report says *which* containers this activation created, and this reads *what*
/// they are. A container the report did not name is never read, so no
/// inventory, label match, timestamp or image reference can stand in for a
/// creation this activation did not perform.
pub fn resolve(
    backend: &mut dyn EnvironmentBackend,
    request: CreationRequest<'_>,
    report: &CreateReport,
) -> Result<Vec<CreatedContainer>, CreationRefusal> {
    if report.created_nothing() {
        return Ok(Vec::new());
    }
    let names = report.created_names().to_vec();
    let rows = match backend.containers_named(request.workdir, request.project, &names) {
        Observed::Verified(rows, _) => rows,
        Observed::Absent(proof) => return Err(CreationRefusal::Missing(proof.to_string())),
        Observed::Unknown(reason) => return Err(CreationRefusal::Unknown(reason.to_string())),
    };
    let mut created: Vec<CreatedContainer> = Vec::with_capacity(rows.len());
    let mut services: Vec<String> = Vec::with_capacity(rows.len());
    for row in &rows {
        let container = verify(backend, request, row)?;
        if services.iter().any(|service| service == &container.service) {
            return Err(CreationRefusal::DuplicateService(container.service.clone()));
        }
        services.push(container.service.clone());
        created.push(container);
    }
    if !created
        .iter()
        .any(|container| container.service == request.primary_service)
    {
        return Err(CreationRefusal::Partial(format!(
            "it created {} container(s) and none of them is the declared primary service `{}`",
            created.len(),
            request.primary_service
        )));
    }
    Ok(created)
}

/// One created row, verified as the container the create report named.
fn verify(
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
    // The name Compose gave this container is derived from the project, the
    // service and the replica index: a name that does not carry both cannot be
    // the container the report and the labels claim.
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
    if let Some(project) = row.labels.get(PROJECT_LABEL) {
        if project != request.project {
            return Err(CreationRefusal::Service(format!(
                "container `{name}` names project `{project}`, not `{project_expected}`",
                project_expected = request.project
            )));
        }
    }
    // Effigy's own ownership labels are the render's: a managed container it
    // started carries them, and a container without them was not started by
    // this render.
    for label in [MANAGED_LABEL, SCOPE_LABEL] {
        if !row.labels.contains_key(label) {
            return Err(CreationRefusal::Service(format!(
                "container `{name}` carries no `{label}` label, so it is not this render's container"
            )));
        }
    }
    if !row.labels.contains_key(COMPOSE_PROJECT_LABEL) {
        return Err(CreationRefusal::Service(format!(
            "container `{name}` names no Compose project"
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

/// Seal a verified created set to the exact environment run that applied it.
///
/// Crate-private construction is the whole point: no caller can pair its own
/// run with a container another activation created.
pub(super) fn seal(
    environment: &ResourceId,
    run: RunId,
    project: &str,
    engine: &str,
    context: &str,
    created: Vec<CreatedContainer>,
    attached: Vec<String>,
) -> CreationReceipt {
    CreationReceipt {
        environment: environment.clone(),
        run,
        project: project.to_owned(),
        engine: engine.to_owned(),
        context: context.to_owned(),
        created,
        attached,
    }
}

/// The effect result one run's create journals.
pub(super) fn stored(
    primary_service: &str,
    receipt: &CreationReceipt,
) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(StoredCreation {
        schema: CREATION_SCHEMA.to_owned(),
        engine: receipt.engine().to_owned(),
        context: receipt.context().to_owned(),
        project: receipt.project().to_owned(),
        primary_service: primary_service.to_owned(),
        created: receipt.created.clone(),
        attached: receipt.attached.clone(),
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
/// own create effect journalled this exact container as created.
///
/// The answer is durable journal evidence and nothing else: an applied
/// `environment:engine-create` receipt that names this exact full id in this
/// project. A readable journal that names no such creation is
/// `Absent(NoCreator)`, which also covers a replacement container (a new id this
/// environment never created) and a container another actor made. An unreadable
/// store is `Unknown`.
pub fn created_by(
    observe: &ObserveContext,
    root: &StateRoot,
    environment: &ResourceId,
    project: &str,
    container_id: &str,
) -> Observed<RunId> {
    if !super::backend::full_container_id(container_id) {
        return Observed::Unknown(UnknownReason::Inconsistent(format!(
            "`{container_id}` is not a full container id, so no creation can name it"
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
    for run in runs {
        let effects = match effigy_kernel::run::effects(&reader, run.id) {
            Observed::Verified(effects, _) => effects,
            Observed::Absent(_) => continue,
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        // `Settlement::Retain` keeps an applied effect's receipt at settle and
        // only rewrites its state, so both states name a create this run
        // actually performed.
        for effect in effects.iter().filter(|effect| {
            effect.spec.kind == CREATE_EFFECT
                && matches!(effect.state, EffectState::Applied | EffectState::Retained)
        }) {
            let Some(receipt) = &effect.receipt else {
                continue;
            };
            let stored = match decode_stored(receipt) {
                Ok(stored) => stored,
                Err(reason) => return Observed::Unknown(UnknownReason::Inconsistent(reason)),
            };
            if stored.project != project {
                continue;
            }
            if !stored
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
    }
    match found {
        Some(run) => Observed::Verified(
            run,
            Evidence::new("environment create effect journal".into(), None)
                .expect("fixed evidence"),
        ),
        None => Observed::Absent(AbsenceProof::NoCreator {
            creator: CreatorRef {
                project: project.to_owned(),
                container_id: container_id.to_owned(),
            },
        }),
    }
}
