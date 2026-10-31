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
//! * a container absent before and present after appeared inside this window,
//!   and must then match the **admitted render's own declared values** for its
//!   service: Effigy's managed, project, scope and checkout labels exactly as
//!   the render declares them, a service the render declares, the image
//!   reference the render declares, and a `Created` stamp no older than the
//!   apply start. Anything else refuses, and the run publishes no receipt.
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

use super::backend::{BackendSelection, EnvironmentBackend, Workload};
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

/// One container the window proved this run's apply created.
///
/// It is read-only outside this module: only the gated window that took the
/// inventories builds one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedContainer {
    container_id: String,
    service: String,
    image: String,
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

    /// The image the engine reports for the container (`Image`): the image id
    /// on Docker, the resolved reference on nerdctl.
    pub fn image(&self) -> &str {
        &self.image
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

/// One gated apply's open window: the inventory it opened with, the render it
/// binds and the apply start it recorded.
pub(super) struct CreationWindow<'a> {
    backend: &'a mut dyn EnvironmentBackend,
    workdir: &'a Path,
    selection: &'a BackendSelection,
    project: &'a str,
    render: Option<&'a Closure>,
    environment: ResourceId,
    run: RunId,
    deadline: Instant,
    before: Observed<Vec<Workload>>,
    apply_started: String,
    apply_started_at: DateTime<Utc>,
}

impl<'a> CreationWindow<'a> {
    /// Take the inventory immediately before the apply and record its start.
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
        let started = Utc::now();
        Self {
            backend,
            workdir,
            selection,
            project,
            render,
            environment,
            run,
            deadline,
            before,
            apply_started: stamp(started),
            apply_started_at: started,
        }
    }

    /// Take the inventory immediately after the apply and build the receipt.
    pub(super) fn close(self, outcome: ApplyOutcome) -> Creation {
        let Self {
            backend,
            workdir,
            selection,
            project,
            render,
            environment,
            run,
            deadline,
            before,
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
        let Some(render) = render else {
            // No admitted render means no exact expected values for scope,
            // checkout, service or image, and the window refuses rather than
            // trust what a container reports about itself.
            return Creation::Unproven(UnknownReason::Inconsistent(format!(
                "the apply of run {run} has no admitted render, so no service, label or image expectation can be bound"
            )));
        };
        let declared = match declared_services(render) {
            Ok(declared) => declared,
            Err(reason) => return Creation::Unproven(reason),
        };
        match receipt(
            selection,
            environment,
            run,
            project,
            &declared,
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
    selection: &BackendSelection,
    environment: ResourceId,
    run: RunId,
    project: &str,
    declared: &BTreeMap<String, DeclaredService>,
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
    let mut services: BTreeSet<String> = BTreeSet::new();
    for row in candidates {
        let service = row
            .labels
            .get(COMPOSE_SERVICE_LABEL)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                inconsistent(format!(
                    "container `{}` appeared in this window without a Compose service identity",
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
        if row.image.trim().is_empty() {
            return Err(inconsistent(format!(
                "container `{}` created in this window reports no image id",
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
        // One container per service: a second container for the service the
        // candidate names is either a duplicate the window cannot attribute or
        // a recreate that did not finish.
        let sharing = after
            .iter()
            .filter(|other| {
                other.labels.get(COMPOSE_SERVICE_LABEL) == row.labels.get(COMPOSE_SERVICE_LABEL)
            })
            .count();
        if sharing != 1 {
            return Err(inconsistent(format!(
                "service `{service}` has {sharing} containers after the apply; a creation window names exactly one"
            )));
        }
        if !services.insert(service.to_owned()) {
            return Err(inconsistent(format!(
                "two containers created in this window name service `{service}`"
            )));
        }
        created.push(CreatedContainer {
            container_id: row.id.clone(),
            service: service.to_owned(),
            image: row.image.clone(),
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
                "backend inventory of project `{project}` on {} (context `{}`) immediately before and after the one environment:apply of run {run}, bound to the admitted render",
                selection.kind.label(),
                selection.context
            ),
            before: before.len(),
            after: after.len(),
        },
    })
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
    use crate::environment::backend::parse_inspect;
    use crate::environment::tests::{env as test_env, selection};
    use crate::runtime::scope::{CHECKOUT_LABEL, MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL};
    use std::collections::BTreeMap;

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

    /// A native trace, not a fake: the container below was created by
    /// `colima nerdctl --profile effigy -- compose up -d` on the machine this
    /// task ran on (project `effigy-508-trace-1793419616-36063`, image
    /// `alpine:3.19`), captured with `nerdctl inspect`, then removed. The
    /// window binds it to a render that declares the short reference, which is
    /// what Compose resolves before it creates the container.
    #[test]
    fn a_native_nerdctl_container_binds_to_the_renders_short_image_reference() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/environment/colima-nerdctl-compose.inspect.json"
        ));
        let rows = parse_inspect(bytes).expect("the native inspect parses");
        assert_eq!(rows.len(), 1, "one container in the trace");
        let row = &rows[0];
        let project = row.labels[PROJECT_LABEL].clone();
        let declared = BTreeMap::from([(
            "app".to_owned(),
            super::DeclaredService {
                labels: BTreeMap::from([
                    (MANAGED_LABEL.to_owned(), "true".to_owned()),
                    (PROJECT_LABEL.to_owned(), project.clone()),
                    (SCOPE_LABEL.to_owned(), "508trace".to_owned()),
                    (
                        CHECKOUT_LABEL.to_owned(),
                        row.labels[CHECKOUT_LABEL].clone(),
                    ),
                ]),
                image: Some("alpine:3.19".to_owned()),
            },
        )]);
        let started = chrono::DateTime::parse_from_rfc3339("2026-10-31T04:07:00Z")
            .expect("start")
            .with_timezone(&chrono::Utc);
        let completed = chrono::DateTime::parse_from_rfc3339("2026-10-31T04:08:00Z")
            .expect("completion")
            .with_timezone(&chrono::Utc);
        let receipt = super::receipt(
            &selection(),
            test_env("stack").resource,
            effigy_kernel::id::RunId::new(1_760_000_000_000, [1; 10]).expect("run"),
            &project,
            &declared,
            &[],
            &rows,
            "2026-10-31T04:07:00.000000000Z",
            started,
            completed,
        )
        .expect("the native container binds to the render");
        assert_eq!(receipt.created().len(), 1);
        let created = &receipt.created()[0];
        assert_eq!(created.container_id(), row.id);
        assert_eq!(created.service(), "app");
        assert_eq!(
            created.image_reference(),
            "docker.io/library/alpine:3.19",
            "Compose resolves the render's short reference"
        );
        assert_eq!(
            created.image(),
            "docker.io/library/alpine:3.19",
            "nerdctl reports the resolved reference in `Image`"
        );
        assert_eq!(created.created(), "2026-10-31T04:07:43.603235578Z");
    }
}
