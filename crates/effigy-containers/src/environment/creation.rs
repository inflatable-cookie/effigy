//! One gated apply's creation window (effigy#508, contract 052 §8.12).
//!
//! #506 may only attribute a fresh Compose target to this activation if the
//! environment backend proves which run created the container. No engine
//! records that (`EnvironmentBackend::created_by`), so this bounded first
//! attempt reads creation off the engine's own inventory on both sides of the
//! one Compose apply — the *bounded Option C* the supervisor ruling of
//! 2026-10-31 authorises:
//!
//! * the window opens under the exclusive `Gate(Env)` the apply already holds,
//!   so no other Effigy activation of this project can interleave;
//! * it takes one exact-identity inventory of the project's containers
//!   (`ps -a --no-trunc` by project label, then one `inspect` of exactly those
//!   full ids: labels, image id and reference, and the container's own
//!   `Created`), and refuses an inspect that does not answer exactly the listed
//!   ids once each;
//! * it records the apply's start and its own successful outcome;
//! * it takes the same inventory again immediately after the apply;
//! * the project after the apply holds **exactly** the services the render
//!   declares, one container each: an extra container, a missing service or two
//!   containers for one service refuses, so the created set is complete rather
//!   than merely non-empty;
//! * a container absent before and present after appeared inside this window,
//!   and must then match the **admitted render's own declared values** for its
//!   service: Effigy's managed, project, scope and checkout labels exactly as
//!   the render declares them, a service the render declares, the image
//!   reference the render declares, the **exact image id** that reference
//!   names on the applying backend, and a `Created` stamp no older than the
//!   apply start. Anything else refuses, and the run publishes no receipt.
//!
//! **The image id.** The render declares a reference (`postgres:16`); the
//! backend's image table is the only authority that turns it into an image id,
//! so the window resolves every declared reference inside its own deadline,
//! **immediately before and immediately after** the one apply, and requires the
//! two readings to be the same id. A tag that moved inside the window, an image
//! that is missing on either side, or an incomplete or timed-out read refuses.
//!
//! The container's own identity is then compared with that id. Docker reports
//! the container's `Image` as an exact image id, and that id — immutable, and
//! the container's own — must equal it (`ImageIdSource::Reported`). nerdctl
//! reports `Image`/`Config.Image` as a reference and exposes the image id
//! nowhere in the container record (`inspect`, `inspect --mode=native` and
//! `ps --format json` all carry only the reference, traced in the fixtures), so
//! there the bind is the render's reference resolved on the same backend on
//! both sides of the apply, with the container's reported reference required to
//! be that same reference (`ImageIdSource::ResolvedByReference`). That second
//! reading is recorded as *resolved by reference, stable across the window* and
//! is never described as an engine-returned container image id: a tag that moves
//! and moves back inside the window is beyond what either engine reports, which
//! is the bounded limit this provisional ruling accepts (supervisor, 2026-10-31,
//! pending Tom's confirmation).
//!
//! **Threat boundary** (contract 052 §8.12): this excludes a foreign process
//! that deliberately creates a container carrying Effigy's own managed,
//! project, scope and checkout labels inside the one gated apply window. The
//! canonical `Gate(Env)` excludes concurrent Effigy activations of the same
//! project; a foreign actor that forges Effigy's labels is a different threat,
//! and no protection against it is claimed or implied here. No label is
//! injected into the render: the apply is still the one Compose invocation it
//! was, and this window only reads.
//!
//! A timestamp, before/after absence and matching labels are each insufficient
//! on their own — that is why the receipt binds all of them *to the run whose
//! gated apply produced them* and to the render's exact expected values, why
//! the receipt has no public constructor, and why each refusal leaves the run
//! with no receipt rather than a weaker one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use effigy_kernel::id::{ResourceId, RunId};
use effigy_kernel::objects::Closure;
use effigy_kernel::observe::{AbsenceProof, CreatorRef, Evidence, Observed, UnknownReason};

use super::backend::{exact_image_id, BackendSelection, EnvironmentBackend, Workload};
use crate::runtime::scope::{
    CHECKOUT_LABEL, COMPOSE_PROJECT_LABEL, MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL,
};

/// Compose labels the container Compose created for a service.
const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";
/// Compose marks `compose run` containers with this label.
const COMPOSE_ONEOFF_LABEL: &str = "com.docker.compose.oneoff";

/// The ownership labels the render must declare for every service this window
/// attributes.
const OWNERSHIP_LABELS: [&str; 4] = [MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL, CHECKOUT_LABEL];

/// How long the window may take: one absolute deadline covers the before read,
/// the apply and the after read. A first apply that outlasts it publishes no
/// receipt (a safe refusal) rather than a partial window; the apply itself
/// stays bounded by the run's own budget (K8).
pub const CREATION_WINDOW: Duration = Duration::from_secs(30 * 60);

/// How the window bound a created container to an image id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageIdSource {
    /// The engine reported the container's own image id (Docker's `Image`).
    /// The id belongs to that container and is immutable.
    Reported,
    /// The engine reports only a reference for the container (nerdctl), so the
    /// id is the render's declared reference resolved on this same backend
    /// immediately before and after the apply, stable across the window, with
    /// the container reporting that same reference. It is **not** an
    /// engine-returned container image id: a tag that moves and moves back
    /// inside the window is beyond what the engine reports.
    ResolvedByReference,
}

/// One container the window proved this run's apply created.
///
/// It is read-only outside this module: only the gated window that took the
/// inventories builds one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedContainer {
    container_id: String,
    service: String,
    image: String,
    image_id: String,
    image_id_source: ImageIdSource,
    image_reference: String,
    created: String,
}

impl CreatedContainer {
    /// The exact full container id the engine reported for this create.
    pub fn container_id(&self) -> &str {
        &self.container_id
    }

    /// The Compose service the container belongs to, as the render declares it.
    pub fn service(&self) -> &str {
        &self.service
    }

    /// The image field the engine reported for the container (`Image`): the
    /// container's own image id on Docker, the reference on nerdctl. It is what
    /// the window bound, never the expectation.
    pub fn image(&self) -> &str {
        &self.image
    }

    /// The exact image id this container was bound to: the id the admitted
    /// render's declared reference names on the applying backend, stable across
    /// the window (see [`ImageIdSource`] for how it was learned).
    pub fn image_id(&self) -> &str {
        &self.image_id
    }

    /// How the window learned the container's image id (see [`ImageIdSource`]).
    pub fn image_id_source(&self) -> ImageIdSource {
        self.image_id_source
    }

    /// The image reference Compose created the container from
    /// (`Config.Image`), which the admitted render declares for this service.
    pub fn image_reference(&self) -> &str {
        &self.image_reference
    }

    /// The container's own `Created` stamp, RFC 3339.
    pub fn created(&self) -> &str {
        &self.created
    }
}

/// What the window read on both sides of the one apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreationEvidence {
    source: String,
    before: usize,
    after: usize,
}

impl CreationEvidence {
    /// The inventories and the apply this receipt is built from, in words.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Containers in the project immediately before the apply.
    pub fn before(&self) -> usize {
        self.before
    }

    /// Containers in the project immediately after it.
    pub fn after(&self) -> usize {
        self.after
    }
}

/// The receipt of one gated apply's creation window.
///
/// It binds the environment run, the exact full container ids, the backend and
/// context, the project, the apply's own start and completion, the containers
/// the window adopted and the inventory evidence it read.
///
/// There is **no public constructor**: the only fields are set by the gated
/// before/apply/after window in this module, so a caller cannot pair its own
/// run id with someone else's container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreationReceipt {
    environment: ResourceId,
    run: RunId,
    engine: String,
    context: String,
    project: String,
    apply_started: String,
    apply_completed: String,
    created: Vec<CreatedContainer>,
    attached: Vec<String>,
    evidence: CreationEvidence,
}

impl CreationReceipt {
    /// The environment this window belongs to.
    pub fn environment(&self) -> &ResourceId {
        &self.environment
    }

    /// The environment run whose one gated apply produced this window.
    pub fn run(&self) -> RunId {
        self.run
    }

    /// The exact engine the apply targeted.
    pub fn engine(&self) -> &str {
        &self.engine
    }

    /// The Colima profile, or the Docker context or host.
    pub fn context(&self) -> &str {
        &self.context
    }

    /// The Compose project.
    pub fn project(&self) -> &str {
        &self.project
    }

    /// When the apply started, RFC 3339, from this host's clock.
    pub fn apply_started(&self) -> &str {
        &self.apply_started
    }

    /// When the apply reported success, RFC 3339.
    pub fn apply_completed(&self) -> &str {
        &self.apply_completed
    }

    /// The exact containers this window proved were created, by service.
    pub fn created(&self) -> &[CreatedContainer] {
        &self.created
    }

    /// Containers already present before the apply: adopted, never created
    /// here.
    pub fn attached(&self) -> &[String] {
        &self.attached
    }

    /// The inventories and the apply this receipt was built from.
    pub fn evidence(&self) -> &CreationEvidence {
        &self.evidence
    }

    /// The run this receipt attributes `container_id` to.
    ///
    /// `Verified` only for an exact full id this window proved its run
    /// created in this project. Everything else is `Absent(NoCreator)`: the
    /// window holds no creation of it, which is what an adopted container, a
    /// replacement and another project's container all are here.
    pub fn created_by(&self, project: &str, container_id: &str) -> Observed<RunId> {
        if self.project == project
            && self
                .created
                .iter()
                .any(|created| created.container_id == container_id)
        {
            return Observed::Verified(
                self.run,
                Evidence::new(
                    format!(
                        "environment creation receipt of run {} ({} containers created)",
                        self.run,
                        self.created.len()
                    ),
                    None,
                )
                .expect("fixed evidence"),
            );
        }
        Observed::Absent(AbsenceProof::NoCreator {
            creator: CreatorRef {
                project: project.to_owned(),
                container_id: container_id.to_owned(),
            },
        })
    }

    /// The container this window created for `service`, if it created one.
    pub fn for_service(&self, service: &str) -> Option<&CreatedContainer> {
        self.created
            .iter()
            .find(|created| created.service == service)
    }
}

/// What one environment run's gated apply proved about container creation.
///
/// `Proven` is the only variant that can attribute a container, and it can
/// only be built by the gated window: `CreationReceipt` has no public
/// constructor. `NothingCreated` is the proof that this run's apply reached no
/// backend (a refused permit or a fault before the effect) and `Unproven` is
/// the reason a window could not be taken. A caller compares `created_by` with
/// its own activation run and refuses on everything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Creation {
    /// A complete window, boxed: the receipt is much larger than the refusals
    /// beside it, and nothing on this path holds two of them.
    Proven(Box<CreationReceipt>),
    NothingCreated,
    Unproven(UnknownReason),
}

impl Creation {
    /// The run whose one gated apply created `container_id` in `project`.
    pub fn created_by(&self, project: &str, container_id: &str) -> Observed<RunId> {
        match self {
            Self::Proven(receipt) => receipt.created_by(project, container_id),
            Self::NothingCreated => Observed::Absent(AbsenceProof::NoCreator {
                creator: CreatorRef {
                    project: project.to_owned(),
                    container_id: container_id.to_owned(),
                },
            }),
            Self::Unproven(reason) => Observed::Unknown(reason.clone()),
        }
    }

    /// The receipt, when this run's window completed.
    pub fn receipt(&self) -> Option<&CreationReceipt> {
        match self {
            Self::Proven(receipt) => Some(receipt.as_ref()),
            Self::NothingCreated | Self::Unproven(_) => None,
        }
    }

    /// A run that has not applied the environment yet has no window. It is
    /// unproven, never an absence: this run cannot speak for anyone else's
    /// containers.
    pub fn not_applied() -> Self {
        Self::Unproven(UnknownReason::Inconsistent(
            "this environment run has not applied the environment, so it has no creation window"
                .into(),
        ))
    }
}

/// The exact values the admitted render declares for one service.
#[derive(Debug, Default)]
struct DeclaredService {
    /// The ownership labels the render itself writes for the service.
    labels: BTreeMap<String, String>,
    /// The image reference the render declares for the service.
    image: Option<String>,
}

/// Read the render's own declarations: which services it declares, the
/// ownership labels it writes for each and the image reference it names.
///
/// The render is the one authority for these values. A value declared twice
/// with different spellings refuses, because the window cannot then say which
/// one the apply used.
fn declared_services(render: &Closure) -> Result<BTreeMap<String, DeclaredService>, UnknownReason> {
    let mut declared: BTreeMap<String, DeclaredService> = BTreeMap::new();
    for (path, bytes) in render.files() {
        let lower = path.to_ascii_lowercase();
        if !(lower.ends_with(".yml") || lower.ends_with(".yaml")) {
            continue;
        }
        let document: serde_yaml::Value = serde_yaml::from_slice(bytes).map_err(|error| {
            inconsistent(format!(
                "the admitted render file `{path}` is not Compose YAML: {error}"
            ))
        })?;
        let Some(services) = document
            .get("services")
            .and_then(|value| value.as_mapping())
        else {
            continue;
        };
        for (name, body) in services {
            let Some(name) = name.as_str() else { continue };
            let Some(body) = body.as_mapping() else {
                continue;
            };
            let service = declared.entry(name.to_owned()).or_default();
            if let Some(labels) = body.get("labels") {
                for (key, value) in label_pairs(labels)? {
                    if !OWNERSHIP_LABELS.contains(&key.as_str()) {
                        continue;
                    }
                    match service.labels.get(&key) {
                        Some(existing) if existing != &value => {
                            return Err(inconsistent(format!(
                                "the admitted render declares `{key}` as both `{existing}` and `{value}` for service `{name}`"
                            )))
                        }
                        Some(_) => {}
                        None => {
                            service.labels.insert(key, value);
                        }
                    }
                }
            }
            if let Some(image) = body.get("image").and_then(|value| value.as_str()) {
                match &service.image {
                    Some(existing) if existing != image => {
                        return Err(inconsistent(format!(
                            "the admitted render declares image `{existing}` and `{image}` for service `{name}`"
                        )))
                    }
                    Some(_) => {}
                    None => service.image = Some(image.to_owned()),
                }
            }
        }
    }
    Ok(declared)
}

/// Every `key=value` pair a Compose `labels:` value declares, as a mapping or
/// as a list of `key=value` strings.
fn label_pairs(value: &serde_yaml::Value) -> Result<Vec<(String, String)>, UnknownReason> {
    let mut pairs = Vec::new();
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            for (key, value) in mapping {
                let Some(key) = key.as_str() else { continue };
                let Some(value) = yaml_scalar(value) else {
                    continue;
                };
                pairs.push((key.to_owned(), value));
            }
        }
        serde_yaml::Value::Sequence(entries) => {
            for entry in entries {
                let Some(text) = entry.as_str() else { continue };
                let Some((key, value)) = text.split_once('=') else {
                    continue;
                };
                pairs.push((key.to_owned(), value.to_owned()));
            }
        }
        _ => {}
    }
    Ok(pairs)
}

fn yaml_scalar(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(text) => Some(text.clone()),
        serde_yaml::Value::Bool(flag) => Some(flag.to_string()),
        serde_yaml::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// How the one apply ended, as the window reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ApplyOutcome {
    /// The backend converged.
    Completed,
    /// The backend was never contacted: nothing in this window was created.
    NotStarted,
    /// The apply started and its outcome is unproven.
    Uncertain(String),
}

/// One gated apply's open window: the inventory it opened with, the render's
/// own declarations, the image ids it resolved before the apply, and the apply
/// start it recorded.
pub(super) struct CreationWindow<'a> {
    backend: &'a mut dyn EnvironmentBackend,
    workdir: &'a Path,
    selection: &'a BackendSelection,
    project: &'a str,
    environment: ResourceId,
    run: RunId,
    deadline: Instant,
    before: Observed<Vec<Workload>>,
    /// The admitted render's declarations, or why they cannot be read. Parsed
    /// before the apply because the window resolves the image references it
    /// declares on both sides of the one apply.
    declared: Option<Result<BTreeMap<String, DeclaredService>, UnknownReason>>,
    /// Every declared reference's exact image id immediately before the apply.
    before_images: Option<Result<BTreeMap<String, String>, UnknownReason>>,
    apply_started: String,
    apply_started_at: DateTime<Utc>,
}

impl<'a> CreationWindow<'a> {
    /// Take the inventory and the declared references' image ids immediately
    /// before the apply, and record its start.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn open(
        backend: &'a mut dyn EnvironmentBackend,
        workdir: &'a Path,
        selection: &'a BackendSelection,
        project: &'a str,
        render: Option<&'a Closure>,
        environment: ResourceId,
        run: RunId,
        deadline: Instant,
    ) -> Self {
        let before = backend.inventory_until(workdir, project, deadline);
        // The render's declared references are resolved on this side of the
        // apply too: the bind needs the tag's exact image id before *and* after
        // it, so a tag that moves inside the window cannot pass.
        let declared = render.map(declared_services);
        let before_images = match declared.as_ref() {
            Some(Ok(declared)) => Some(resolve_declared_images(
                backend, workdir, deadline, declared,
            )),
            _ => None,
        };
        let started = Utc::now();
        Self {
            backend,
            workdir,
            selection,
            project,
            environment,
            run,
            deadline,
            before,
            declared,
            before_images,
            apply_started: stamp(started),
            apply_started_at: started,
        }
    }

    /// Take the inventory and the image ids immediately after the apply, then
    /// build the receipt.
    pub(super) fn close(self, outcome: ApplyOutcome) -> Creation {
        let Self {
            backend,
            workdir,
            selection,
            project,
            environment,
            run,
            deadline,
            before,
            declared,
            before_images,
            apply_started,
            apply_started_at,
        } = self;
        match outcome {
            ApplyOutcome::NotStarted => return Creation::NothingCreated,
            ApplyOutcome::Uncertain(detail) => {
                return Creation::Unproven(UnknownReason::Inconsistent(format!(
                    "the environment apply did not report success ({detail}), so its creation window is incomplete"
                )))
            }
            ApplyOutcome::Completed => {}
        }
        let before = match before {
            Observed::Verified(rows, _) => rows,
            Observed::Absent(proof) => {
                return Creation::Unproven(UnknownReason::Inconsistent(format!(
                    "the project inventory before the apply is absent ({proof})"
                )))
            }
            Observed::Unknown(reason) => return Creation::Unproven(reason),
        };
        let after = match backend.inventory_until(workdir, project, deadline) {
            Observed::Verified(rows, _) => rows,
            Observed::Absent(proof) => {
                return Creation::Unproven(UnknownReason::Inconsistent(format!(
                    "the project inventory after the apply is absent ({proof})"
                )))
            }
            Observed::Unknown(reason) => return Creation::Unproven(reason),
        };
        let Some(declared) = declared else {
            // No admitted render means no exact expected values for scope,
            // checkout, service or image, and the window refuses rather than
            // trust what a container reports about itself.
            return Creation::Unproven(UnknownReason::Inconsistent(format!(
                "the apply of run {run} has no admitted render, so no service, label or image expectation can be bound"
            )));
        };
        let declared = match declared {
            Ok(declared) => declared,
            Err(reason) => return Creation::Unproven(reason),
        };
        let before_images = match before_images {
            Some(Ok(images)) => images,
            Some(Err(reason)) => return Creation::Unproven(reason),
            None => {
                return Creation::Unproven(UnknownReason::Inconsistent(format!(
                    "the apply of run {run} has no admitted render, so no image id could be resolved before it"
                )))
            }
        };
        let mut after_images = match resolve_declared_images(backend, workdir, deadline, &declared)
        {
            Ok(images) => images,
            Err(reason) => return Creation::Unproven(reason),
        };
        match receipt(
            backend,
            workdir,
            deadline,
            selection,
            environment,
            run,
            project,
            &declared,
            &before_images,
            &mut after_images,
            &before,
            &after,
            &apply_started,
            apply_started_at,
            Utc::now(),
        ) {
            Ok(receipt) => Creation::Proven(Box::new(receipt)),
            Err(reason) => Creation::Unproven(reason),
        }
    }
}

/// The receipt one complete before/apply/after window proves.
#[allow(clippy::too_many_arguments)]
fn receipt(
    backend: &mut dyn EnvironmentBackend,
    workdir: &Path,
    deadline: Instant,
    selection: &BackendSelection,
    environment: ResourceId,
    run: RunId,
    project: &str,
    declared: &BTreeMap<String, DeclaredService>,
    before_images: &BTreeMap<String, String>,
    after_images: &mut BTreeMap<String, String>,
    before: &[Workload],
    after: &[Workload],
    apply_started: &str,
    apply_started_at: DateTime<Utc>,
    apply_completed: DateTime<Utc>,
) -> Result<CreationReceipt, UnknownReason> {
    // Every row both inventories returned must name this project, or this is
    // not the exact-identity inventory the window claims to have read.
    for (side, rows) in [("before", before), ("after", after)] {
        for row in rows {
            if row.labels.get(COMPOSE_PROJECT_LABEL).map(String::as_str) != Some(project) {
                return Err(inconsistent(format!(
                    "container `{}` in the {side} inventory does not carry Compose project label `{project}`",
                    row.id
                )));
            }
        }
    }
    let before_ids: BTreeSet<&str> = before.iter().map(|row| row.id.as_str()).collect();

    // The created set is exact or the window proves nothing: after the apply
    // the project holds exactly one container for every service the render
    // declares, and none for anything else. An extra container is a target the
    // apply did not converge on, a missing service means it did not converge,
    // and two containers for one service are an ambiguity the window cannot
    // attribute.
    let mut after_by_service: BTreeMap<&str, Vec<&Workload>> = BTreeMap::new();
    for row in after {
        let service = row
            .labels
            .get(COMPOSE_SERVICE_LABEL)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                inconsistent(format!(
                    "container `{}` in the inventory after the apply has no Compose service identity, so it is not a target the render declares",
                    row.id
                ))
            })?;
        if row
            .labels
            .get(COMPOSE_ONEOFF_LABEL)
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        {
            return Err(inconsistent(format!(
                "container `{}` is a one-off container: the environment apply created no such target",
                row.id
            )));
        }
        if !declared.contains_key(service) {
            return Err(inconsistent(format!(
                "service `{service}` of container `{}` is not a service the admitted render declares, so the project holds a target this window did not expect",
                row.id
            )));
        }
        after_by_service.entry(service).or_default().push(row);
    }
    for service in declared.keys() {
        match after_by_service.get(service.as_str()).map(Vec::len) {
            Some(1) => {}
            Some(count) => {
                return Err(inconsistent(format!(
                    "service `{service}` has {count} containers after the apply while the render declares exactly one target for it"
                )))
            }
            None => {
                return Err(inconsistent(format!(
                    "service `{service}` has no container after the apply, so the project does not hold exactly the services the admitted render declares"
                )))
            }
        }
    }

    let mut attached: Vec<String> = after
        .iter()
        .filter(|row| before_ids.contains(row.id.as_str()))
        .map(|row| row.id.clone())
        .collect();
    attached.sort();
    // Only an id absent before the apply and present after it appeared inside
    // this window. Everything else was adopted and is never created here.
    let candidates: Vec<&Workload> = after
        .iter()
        .filter(|row| !before_ids.contains(row.id.as_str()))
        .collect();
    let mut created: Vec<CreatedContainer> = Vec::with_capacity(candidates.len());
    for row in candidates {
        let service = row
            .labels
            .get(COMPOSE_SERVICE_LABEL)
            .map(|value| value.trim())
            .unwrap_or_default();
        // The render is the authority for every expected value below. A
        // container's own labels are only ever compared with it, never trusted.
        let expected = declared.get(service).ok_or_else(|| {
            inconsistent(format!(
                "service `{service}` of container `{}` is not a service the admitted render declares",
                row.id
            ))
        })?;
        for label in OWNERSHIP_LABELS {
            let Some(declared_value) = expected.labels.get(label) else {
                return Err(inconsistent(format!(
                    "the admitted render declares no {label} label for service `{service}`, so container `{}` cannot be bound to it",
                    row.id
                )));
            };
            let actual = row.labels.get(label).map(String::as_str).unwrap_or("");
            if actual != declared_value {
                return Err(inconsistent(format!(
                    "container `{}` carries {label} `{actual}` while the admitted render declares `{declared_value}` for service `{service}`",
                    row.id
                )));
            }
        }
        let expected_image = expected.image.as_ref().ok_or_else(|| {
            inconsistent(format!(
                "the admitted render declares no image for service `{service}`, so container `{}` cannot be bound to an intended image",
                row.id
            ))
        })?;
        if !reference_matches(expected_image, &row.image_reference) {
            return Err(inconsistent(format!(
                "container `{}` was created from image `{}` while the admitted render declares `{expected_image}` for service `{service}`",
                row.id, row.image_reference
            )));
        }
        // The exact image id the render's reference names on this backend,
        // required to be the same immediately before and after the apply.
        let expected_image_id =
            stable_image_id(service, expected_image, before_images, after_images)?;
        let reported_image = row.image.trim();
        if reported_image.is_empty() {
            return Err(inconsistent(format!(
                "container `{}` created in this window reports no image identity",
                row.id
            )));
        }
        let (actual_image_id, image_id_source) = match exact_image_id(reported_image) {
            // Docker: the engine reports the container's own immutable id.
            Some(id) => (id, ImageIdSource::Reported),
            // nerdctl: the engine reports only a reference, so the bind is the
            // render's reference resolved on this same backend across the
            // window, with the container required to report that reference.
            None => {
                if !reference_matches(expected_image, reported_image) {
                    return Err(inconsistent(format!(
                        "container `{}` reports image `{reported_image}` while the admitted render declares `{expected_image}` for service `{service}`, so it was not created from the render's image",
                        row.id
                    )));
                }
                let id =
                    resolved_image_id(backend, workdir, deadline, after_images, reported_image)?;
                (id, ImageIdSource::ResolvedByReference)
            }
        };
        if actual_image_id != expected_image_id {
            return Err(inconsistent(format!(
                "container `{}` was created from image id `{actual_image_id}` while the admitted render declares `{expected_image}` for service `{service}`, which resolved to image id `{expected_image_id}` inside this window",
                row.id
            )));
        }
        let created_at = parse_stamp(&row.created).ok_or_else(|| {
            inconsistent(format!(
                "container `{}` created in this window reports no parsable Created stamp",
                row.id
            ))
        })?;
        if created_at < apply_started_at {
            return Err(inconsistent(format!(
                "container `{}` reports it was created at {} before this window's apply started at {apply_started}",
                row.id,
                stamp(created_at)
            )));
        }
        created.push(CreatedContainer {
            container_id: row.id.clone(),
            service: service.to_owned(),
            image: row.image.clone(),
            image_id: actual_image_id,
            image_id_source,
            image_reference: row.image_reference.clone(),
            created: row.created.clone(),
        });
    }
    created.sort_by(|left, right| {
        left.service
            .cmp(&right.service)
            .then_with(|| left.container_id.cmp(&right.container_id))
    });
    Ok(CreationReceipt {
        environment,
        run,
        engine: selection.engine.as_str().to_owned(),
        context: selection.context.clone(),
        project: project.to_owned(),
        apply_started: apply_started.to_owned(),
        apply_completed: stamp(apply_completed),
        created,
        attached,
        evidence: CreationEvidence {
            source: format!(
                "backend inventory of project `{project}` on {} (context `{}`) immediately before and after the one environment:apply of run {run}, bound to the admitted render, whose declared image references were resolved on the same backend immediately before and after that apply",
                selection.kind.label(),
                selection.context
            ),
            before: before.len(),
            after: after.len(),
        },
    })
}

/// The exact image id `reference` resolves to on the applying backend, under
/// the window's one deadline.
///
/// Resolved once per declared reference per window. This is the *expectation*
/// side of the image bind — what the render's reference names on this backend
/// inside the window — and is never used to stand in for a container's own
/// identity. An answer that is not an exact image id, or a backend that has no
/// answer, refuses the whole window: a reference alone is not the image a
/// container runs.
fn resolved_image_id(
    backend: &mut dyn EnvironmentBackend,
    workdir: &Path,
    deadline: Instant,
    resolved: &mut BTreeMap<String, String>,
    reference: &str,
) -> Result<String, UnknownReason> {
    if let Some(id) = resolved.get(reference) {
        return Ok(id.clone());
    }
    let id = match backend.image_id_until(workdir, reference, deadline) {
        Observed::Verified(id, _) => exact_image_id(&id).ok_or_else(|| {
            inconsistent(format!(
                "the image inspect of `{reference}` answered `{id}`, which is not an exact image id"
            ))
        })?,
        Observed::Absent(proof) => {
            return Err(inconsistent(format!(
                "the backend resolved no image for `{reference}` ({proof}), so no exact image id can be bound"
            )))
        }
        Observed::Unknown(reason) => return Err(reason),
    };
    resolved.insert(reference.to_owned(), id.clone());
    Ok(id)
}

/// Every declared reference's exact image id, read once at this point in the
/// window.
///
/// The window runs this immediately before the apply and again immediately
/// after it: a tag that names a different image on the two sides has moved
/// inside the window, and nothing the container reports can be bound to it.
fn resolve_declared_images(
    backend: &mut dyn EnvironmentBackend,
    workdir: &Path,
    deadline: Instant,
    declared: &BTreeMap<String, DeclaredService>,
) -> Result<BTreeMap<String, String>, UnknownReason> {
    let mut resolved = BTreeMap::new();
    for service in declared.values() {
        let Some(image) = &service.image else {
            continue;
        };
        resolved_image_id(backend, workdir, deadline, &mut resolved, image)?;
    }
    Ok(resolved)
}

/// The exact image id a service's declared reference names, required to be the
/// same immediately before and after the apply.
///
/// A reference that resolved to one id before the apply and another after it
/// moved inside the window, so no container can be bound to "the image the
/// render declares": the window refuses and names both readings.
fn stable_image_id(
    service: &str,
    declared_image: &str,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Result<String, UnknownReason> {
    let before_id = before.get(declared_image).ok_or_else(|| {
        inconsistent(format!(
            "the image `{declared_image}` the admitted render declares for service `{service}` resolved to no image id before the apply"
        ))
    })?;
    let after_id = after.get(declared_image).ok_or_else(|| {
        inconsistent(format!(
            "the image `{declared_image}` the admitted render declares for service `{service}` resolved to no image id after the apply"
        ))
    })?;
    if before_id != after_id {
        return Err(inconsistent(format!(
            "the tag `{declared_image}` of service `{service}` moved inside this window: it resolved to image id `{before_id}` before the apply and `{after_id}` after it"
        )));
    }
    Ok(before_id.clone())
}

/// Whether the container's image reference is the render's declared one.
///
/// Exact text first, then the same reference in its fully-qualified spelling:
/// Compose resolves a short reference before it creates the container, so
/// `postgres:16` and `docker.io/library/postgres:16` name the same image. Only
/// spellings of one reference match; nothing here widens what may be
/// attributed.
fn reference_matches(declared: &str, actual: &str) -> bool {
    !declared.trim().is_empty()
        && !actual.trim().is_empty()
        && normalize_reference(declared) == normalize_reference(actual)
}

/// The fully-qualified spelling of one image reference: `docker.io/library/`
/// for a bare name, the registry when the first component is a host, `latest`
/// when no tag or digest is named.
fn normalize_reference(reference: &str) -> String {
    let reference = reference.trim();
    let (name, digest) = match reference.split_once('@') {
        Some((name, digest)) => (name, Some(digest)),
        None => (reference, None),
    };
    let after_slash = name.rfind('/').map_or(0, |index| index + 1);
    let (repository, tag) = match name[after_slash..].rfind(':') {
        Some(offset) => {
            let index = after_slash + offset;
            (name[..index].to_owned(), Some(name[index + 1..].to_owned()))
        }
        None => (name.to_owned(), None),
    };
    let repository = match repository.split_once('/') {
        None => format!("docker.io/library/{repository}"),
        Some((first, _)) if first.contains('.') || first.contains(':') || first == "localhost" => {
            repository
        }
        Some(_) => format!("docker.io/{repository}"),
    };
    match (tag, digest) {
        (_, Some(digest)) => format!("{repository}@{digest}"),
        (Some(tag), None) => format!("{repository}:{tag}"),
        (None, None) => format!("{repository}:latest"),
    }
}

fn inconsistent(detail: String) -> UnknownReason {
    UnknownReason::Inconsistent(detail)
}

fn stamp(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn parse_stamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::{normalize_reference, reference_matches};
    use crate::environment::backend::{parse_image_inspect, parse_inspect};
    use crate::environment::tests::{env as test_env, selection};
    use crate::environment::{BackendSelection, EnvironmentBackend, Workload, CREATION_WINDOW};
    use crate::runtime::scope::{CHECKOUT_LABEL, MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL};
    use effigy_kernel::id::RunId;
    use effigy_kernel::observe::{Evidence, Observed};
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::time::Instant;

    /// Compose resolves a short reference before it creates a container, so
    /// the fully-qualified spelling of one reference is the same image.
    #[test]
    fn one_image_reference_matches_in_its_fully_qualified_spellings() {
        for (declared, actual) in [
            ("postgres:16", "docker.io/library/postgres:16"),
            ("postgres", "docker.io/library/postgres:latest"),
            ("library/redis:7", "docker.io/library/redis:7"),
            ("ghcr.io/acme/api:1", "ghcr.io/acme/api:1"),
            ("localhost:5000/acme/api:1", "localhost:5000/acme/api:1"),
            ("acme/api@sha256:abc", "docker.io/acme/api@sha256:abc"),
        ] {
            assert!(reference_matches(declared, actual), "{declared} / {actual}");
            assert!(reference_matches(actual, declared), "{actual} / {declared}");
        }
        // A different image or tag never matches, however it is spelled.
        for (declared, actual) in [
            ("postgres:16", "postgres:17"),
            ("postgres:16", "docker.io/library/mysql:16"),
            ("ghcr.io/acme/api:1", "ghcr.io/acme/web:1"),
            ("postgres:16", ""),
            ("", "postgres:16"),
        ] {
            assert!(
                !reference_matches(declared, actual),
                "{declared} / {actual}"
            );
        }
        assert_eq!(
            normalize_reference("acme/api:2"),
            "docker.io/acme/api:2",
            "a repository with a namespace is not in `library/`"
        );
    }

    /// A backend whose image table is the native `nerdctl image inspect` trace
    /// below: every spelling of the one reference the trace names resolves to
    /// the exact image id the engine reported, which is exactly what the real
    /// adapter reads.
    struct NativeImages {
        selection: BackendSelection,
        image_id: String,
    }

    impl EnvironmentBackend for NativeImages {
        fn selection(&self) -> &BackendSelection {
            &self.selection
        }
        fn inventory(&mut self, _: &Path, project: &str) -> Observed<Vec<Workload>> {
            panic!("the native trace supplies its own inventory ({project})")
        }
        fn inventory_until(
            &mut self,
            _: &Path,
            project: &str,
            _: Instant,
        ) -> Observed<Vec<Workload>> {
            panic!("the native trace supplies its own inventory ({project})")
        }
        fn image_id_until(&mut self, _: &Path, reference: &str, _: Instant) -> Observed<String> {
            assert!(
                reference == "alpine:3.19" || reference == "docker.io/library/alpine:3.19",
                "the trace names one image: {reference}"
            );
            Observed::Verified(
                self.image_id.clone(),
                Evidence::new("native image inspect".to_owned(), None).expect("evidence"),
            )
        }
        fn created_by(&mut self, project: &str, container_id: &str) -> Observed<RunId> {
            panic!("a proof does not ask the raw backend about `{container_id}` in `{project}`")
        }
    }

    /// A native trace, not a fake: the container below was created by
    /// `colima nerdctl --profile effigy -- compose -p … -f - up -d` on the
    /// machine this task ran on (project `effigy-508-v4-8292789880`, image
    /// `alpine:3.19`), captured with `nerdctl inspect` and `nerdctl image
    /// inspect`, then removed.
    ///
    /// The engine's own bytes are the finding: nerdctl reports the container's
    /// image as a reference in `Image`/`Config.Image` and exposes no image id
    /// for the container at all (no `ImageID`, and no field of the container
    /// record carries a digest), while `nerdctl image inspect` on that same
    /// reference answers one exact id both immediately before and immediately
    /// after the apply. So the bind a nerdctl container can honestly support is
    /// the render's declared reference resolved on this backend and stable
    /// across the window, with the container reporting that reference — which
    /// this proof takes from the real bytes and records as
    /// `ResolvedByReference`, never as an engine-returned container image id.
    #[test]
    fn a_native_nerdctl_container_binds_to_its_reference_resolved_stably_across_the_apply() {
        let rows = parse_inspect(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/environment/colima-nerdctl-compose.inspect.json"
        )))
        .expect("the native inspect parses");
        assert_eq!(rows.len(), 1, "one container in the trace");
        let row = &rows[0];
        assert_eq!(
            row.image, "docker.io/library/alpine:3.19",
            "nerdctl reports the container's image as a reference, not an id"
        );
        assert_eq!(
            row.image_reference, "docker.io/library/alpine:3.19",
            "`Config.Image` is the same reference"
        );
        assert!(
            super::exact_image_id(&row.image).is_none(),
            "the reference is not an image id, so the container carries none"
        );
        // The image table answered the same exact id before and after the apply.
        let image_id = parse_image_inspect(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/environment/colima-nerdctl-alpine-3.19.image.inspect.json"
        )))
        .expect("the native image inspect parses");
        assert_eq!(
            image_id, "sha256:6cf065f724d522cbb3e1898559cd88f015db8a1a4210d2d048a0853b69b57ba9",
            "the engine's own image id for alpine:3.19"
        );
        let images = BTreeMap::from([("alpine:3.19".to_owned(), image_id.clone())]);
        let project = row.labels[PROJECT_LABEL].clone();
        let declared = BTreeMap::from([(
            "app".to_owned(),
            super::DeclaredService {
                labels: BTreeMap::from([
                    (MANAGED_LABEL.to_owned(), "true".to_owned()),
                    (PROJECT_LABEL.to_owned(), project.clone()),
                    (SCOPE_LABEL.to_owned(), "508v4".to_owned()),
                    (
                        CHECKOUT_LABEL.to_owned(),
                        row.labels[CHECKOUT_LABEL].clone(),
                    ),
                ]),
                image: Some("alpine:3.19".to_owned()),
            },
        )]);
        let started = chrono::DateTime::parse_from_rfc3339("2026-10-31T05:04:00Z")
            .expect("start")
            .with_timezone(&chrono::Utc);
        let completed = chrono::DateTime::parse_from_rfc3339("2026-10-31T05:05:00Z")
            .expect("completion")
            .with_timezone(&chrono::Utc);
        let mut after_images = images.clone();
        let mut backend = NativeImages {
            selection: selection(),
            image_id: image_id.clone(),
        };
        let receipt = super::receipt(
            &mut backend,
            Path::new("/fixtures/checkout"),
            Instant::now() + CREATION_WINDOW,
            &selection(),
            test_env("stack").resource,
            effigy_kernel::id::RunId::new(1_760_000_000_000, [1; 10]).expect("run"),
            &project,
            &declared,
            &images,
            &mut after_images,
            &[],
            &rows,
            "2026-10-31T05:04:00.000000000Z",
            started,
            completed,
        )
        .expect("the native container binds to its reference resolved across the window");
        assert_eq!(receipt.created().len(), 1);
        let created = &receipt.created()[0];
        assert_eq!(created.container_id(), row.id);
        assert_eq!(created.service(), "app");
        assert_eq!(
            created.image_reference(),
            "docker.io/library/alpine:3.19",
            "Compose records the resolved reference"
        );
        assert_eq!(
            created.image(),
            "docker.io/library/alpine:3.19",
            "nerdctl reports the reference in `Image`"
        );
        assert_eq!(
            created.image_id(),
            image_id,
            "the bound id is the reference resolved stably across the apply"
        );
        assert_eq!(
            created.image_id_source(),
            super::ImageIdSource::ResolvedByReference,
            "recorded as resolved by reference, never as an engine-returned container id"
        );
        assert_eq!(created.created(), "2026-10-31T05:04:10.522659842Z");
    }
}
