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
//!   (`ps -a --no-trunc` by project label, then one `inspect` of those exact
//!   ids: full id, labels, image id and the container's own `Created` stamp);
//! * it records the apply's start and its own successful outcome;
//! * it takes the same inventory again immediately after the apply;
//! * a container absent before and present after appeared inside this window,
//!   and must then carry Effigy's managed, project, scope and checkout labels,
//!   its Compose project and a single service identity, an image id (and a
//!   `com.docker.compose.image` label that agrees when Compose wrote one) and a
//!   `Created` stamp no older than the apply start. Anything else refuses, and
//!   the run publishes no receipt.
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
//! gated apply produced them*, and why each refusal leaves the run with no
//! receipt rather than a weaker one.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use effigy_kernel::id::{ResourceId, RunId};
use effigy_kernel::observe::{AbsenceProof, Evidence, Observed, UnknownReason};

use super::backend::{BackendSelection, EnvironmentBackend, Workload};
use crate::runtime::scope::{
    CHECKOUT_LABEL, COMPOSE_PROJECT_LABEL, MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL,
};

/// Compose labels the container Compose created for a service.
const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";
/// Compose marks `compose run` containers with this label.
const COMPOSE_ONEOFF_LABEL: &str = "com.docker.compose.oneoff";
/// Compose records the image it created the container from, when it does.
const COMPOSE_IMAGE_LABEL: &str = "com.docker.compose.image";

/// How long the window may take: one absolute deadline covers the before read,
/// the apply and the after read. A first apply that outlasts it publishes no
/// receipt (a safe refusal) rather than a partial window; the apply itself
/// stays bounded by the run's own budget (K8).
pub const CREATION_WINDOW: Duration = Duration::from_secs(30 * 60);

/// One container the window proved this run's apply created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedContainer {
    /// The exact full container id the engine returned for this create.
    pub container_id: String,
    /// The Compose service the container belongs to.
    pub service: String,
    /// The image id the container runs.
    pub image: String,
    /// The container's own `Created` stamp, RFC 3339.
    pub created: String,
}

/// What the window read on both sides of the one apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreationEvidence {
    /// The inventories and the apply this receipt is built from, in words.
    pub source: String,
    /// Containers in the project immediately before the apply.
    pub before: usize,
    /// Containers in the project immediately after it.
    pub after: usize,
}

/// The receipt of one gated apply's creation window.
///
/// It binds the environment run, the exact full container ids, the backend and
/// context, the project, the apply's own start and completion, the containers
/// the window adopted and the inventory evidence it read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreationReceipt {
    /// The environment this window belongs to.
    pub environment: ResourceId,
    /// The environment run whose one gated apply produced this window.
    pub run: RunId,
    /// The exact engine the apply targeted.
    pub engine: String,
    /// The Colima profile, or the Docker context or host.
    pub context: String,
    /// The Compose project.
    pub project: String,
    /// When the apply started, RFC 3339, from this host's clock.
    pub apply_started: String,
    /// When the apply reported success, RFC 3339.
    pub apply_completed: String,
    /// The exact containers this window proved were created, by service.
    pub created: Vec<CreatedContainer>,
    /// Containers already present before the apply: adopted, never created
    /// here.
    pub attached: Vec<String>,
    pub evidence: CreationEvidence,
}

impl CreationReceipt {
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
            creator: effigy_kernel::observe::CreatorRef {
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
/// `Proven` is the only variant that can attribute a container; `NothingCreated`
/// is the proof that this run's apply reached no backend (a refused permit or a
/// fault before the effect) and `Unproven` is the reason a window could not be
/// taken. A caller compares `created_by` with its own activation run and
/// refuses on everything else.
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
                creator: effigy_kernel::observe::CreatorRef {
                    project: project.to_owned(),
                    container_id: container_id.to_owned(),
                },
            }),
            Self::Unproven(reason) => Observed::Unknown(reason.clone()),
        }
    }

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

/// One gated apply's open window: the inventory it opened with and the apply
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
    apply_started: String,
    apply_started_at: DateTime<Utc>,
}

impl<'a> CreationWindow<'a> {
    /// Take the inventory immediately before the apply and record its start.
    pub(super) fn open(
        backend: &'a mut dyn EnvironmentBackend,
        workdir: &'a Path,
        selection: &'a BackendSelection,
        project: &'a str,
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
        match receipt(
            selection,
            environment,
            run,
            project,
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
        for (label, expected) in [
            (MANAGED_LABEL, Some("true")),
            (PROJECT_LABEL, Some(project)),
            (COMPOSE_PROJECT_LABEL, Some(project)),
        ] {
            if row.labels.get(label).map(String::as_str) != expected {
                return Err(inconsistent(format!(
                    "container `{}` created in this window carries {label} `{}`, not `{}`",
                    row.id,
                    row.labels.get(label).map(String::as_str).unwrap_or(""),
                    expected.unwrap_or("")
                )));
            }
        }
        for label in [SCOPE_LABEL, CHECKOUT_LABEL] {
            if row
                .labels
                .get(label)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(inconsistent(format!(
                    "container `{}` created in this window carries no {label} label",
                    row.id
                )));
            }
        }
        if row.image.trim().is_empty() {
            return Err(inconsistent(format!(
                "container `{}` created in this window reports no image id",
                row.id
            )));
        }
        if let Some(compose_image) = row.labels.get(COMPOSE_IMAGE_LABEL) {
            if compose_image != &row.image {
                return Err(inconsistent(format!(
                    "container `{}` runs image `{}` but its Compose image label names `{compose_image}`",
                    row.id, row.image
                )));
            }
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
                "backend inventory of project `{project}` on {} (context `{}`) immediately before and after the one environment:apply of run {run}",
                selection.kind.label(),
                selection.context
            ),
            before: before.len(),
            after: after.len(),
        },
    })
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
