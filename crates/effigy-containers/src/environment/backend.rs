//! The backend seam: "reconcile this environment from its render" (Q-004).
//!
//! One selection names the exact engine and context an environment runs on,
//! and why it was chosen. The engine and its precedence are
//! [`crate::backend::exact_backend`]'s; Prepare records them on the
//! environment's kernel resource row, and every transition targets that
//! backend instead of re-guessing from a profile name. Colima
//! and Docker (Desktop on macOS, Engine on Linux) implement the seam today; a
//! future Apple Containers adapter implements the same trait.

use std::ffi::OsString;
use std::path::Path;

use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::store::BackendMetadata;
use effigy_manifest::ManifestContainerDriver;
use serde::{Deserialize, Serialize};

use crate::{BackendId, EffectiveContainerPolicy};

/// The engine family an environment runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    /// One Lima VM per profile; containerd through `colima nerdctl`.
    Colima,
    /// Docker Desktop's LinuxKit VM through the host `docker` CLI.
    DockerDesktop,
    /// A Linux Docker Engine through the host `docker` CLI.
    DockerEngine,
}

impl BackendKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Colima => "Colima",
            Self::DockerDesktop => "Docker Desktop",
            Self::DockerEngine => "Docker Engine",
        }
    }
}

/// Why this backend was selected, highest precedence first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "source", content = "detail")]
pub enum BackendReason {
    /// `EFFIGY_COMPOSE_BACKEND` named it for this invocation.
    EnvironmentVariable(String),
    /// The environment record holds the backend Prepare recorded, with the
    /// label of the source that chose it then.
    Recorded(String),
    /// A v0.14.1 per-checkout backend metadata file named it (read-only).
    LegacyMetadata,
    /// `~/.effigy/config.toml` prefers this backend.
    UserPreference,
    /// The manifest's container driver defaults to it.
    ManifestDriver(String),
}

impl BackendReason {
    pub fn describe(&self) -> String {
        match self {
            Self::EnvironmentVariable(value) => {
                format!("EFFIGY_COMPOSE_BACKEND={value} selects it for this invocation")
            }
            Self::Recorded(source) => format!(
                "the environment record keeps the backend its generation was prepared on (chosen by: {source})"
            ),
            Self::LegacyMetadata => {
                "the checkout's v0.14 backend metadata names it".to_owned()
            }
            Self::UserPreference => "the user preference in ~/.effigy/config.toml".to_owned(),
            Self::ManifestDriver(driver) => format!("the manifest driver `{driver}` default"),
        }
    }

    /// The source label the kernel record carries (`resources.backend`),
    /// shared with [`crate::backend::BackendSource::label`].
    pub fn source_label(&self) -> String {
        match self {
            Self::EnvironmentVariable(_) => "environment".to_owned(),
            Self::Recorded(source) => source.clone(),
            Self::LegacyMetadata => "legacy-runtime-file".to_owned(),
            Self::UserPreference => "user-preference".to_owned(),
            Self::ManifestDriver(_) => "driver-default".to_owned(),
        }
    }
}

/// The exact engine, context and reason for one environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendSelection {
    pub kind: BackendKind,
    pub engine: BackendId,
    /// The Colima profile, or the Docker context the CLI resolves.
    pub context: String,
    pub reason: BackendReason,
}

impl BackendSelection {
    /// The kernel record of this selection (`resources.backend`), in the
    /// shape Prepare writes.
    pub fn metadata(&self) -> BackendMetadata {
        BackendMetadata {
            engine: self.engine.as_str().to_owned(),
            context: self.context.clone(),
            source: self.reason.source_label(),
        }
    }

    /// Reads a recorded selection. An unknown engine or an empty source is
    /// `Unknown`, never a default: destructive actions refuse until the
    /// backend is named (M4).
    pub fn from_metadata(metadata: &BackendMetadata) -> Observed<Self> {
        let engine = BackendId::new(metadata.engine.clone());
        let kind = if engine == BackendId::colima_nerdctl() {
            Some(BackendKind::Colima)
        } else if engine == BackendId::docker_compose() {
            Some(docker_kind())
        } else {
            None
        };
        match kind {
            Some(kind) if !metadata.source.trim().is_empty() => Observed::Verified(
                Self {
                    kind,
                    engine,
                    context: metadata.context.clone(),
                    reason: BackendReason::Recorded(metadata.source.clone()),
                },
                effigy_kernel::observe::Evidence::new("environment backend record".into(), None)
                    .expect("fixed evidence"),
            ),
            _ => Observed::Unknown(UnknownReason::Inconsistent(format!(
                "recorded backend `{}` is not a known engine",
                metadata.engine
            ))),
        }
    }

    /// `Colima (profile `effigy`)`, `Docker Desktop (context `default`)`.
    pub fn label(&self) -> String {
        match self.kind {
            BackendKind::Colima => format!("Colima (profile `{}`)", self.context),
            BackendKind::DockerDesktop | BackendKind::DockerEngine => {
                format!("{} (context `{}`)", self.kind.label(), self.context)
            }
        }
    }

    pub fn describe(&self) -> String {
        format!("{}: {}", self.label(), self.reason.describe())
    }

    /// The adapter that targets exactly this selection.
    pub fn adapter(&self) -> Box<dyn EnvironmentBackend> {
        match self.kind {
            BackendKind::Colima => Box::new(ColimaBackend::new(self.clone())),
            BackendKind::DockerDesktop | BackendKind::DockerEngine => {
                Box::new(DockerBackend::new(self.clone()))
            }
        }
    }

    /// The program and arguments for one Compose invocation on this backend.
    pub fn compose_invocation(&self, compose_args: &[OsString]) -> (OsString, Vec<OsString>) {
        match self.kind {
            BackendKind::Colima => {
                let mut args = vec![
                    OsString::from("nerdctl"),
                    OsString::from("--profile"),
                    OsString::from(&self.context),
                    OsString::from("--"),
                ];
                args.extend(compose_args.iter().cloned());
                (OsString::from("colima"), args)
            }
            BackendKind::DockerDesktop | BackendKind::DockerEngine => {
                (OsString::from("docker"), compose_args.to_vec())
            }
        }
    }
}

/// The selection for `policy`: [`crate::backend::exact_backend`]'s engine
/// and precedence, with the context and reason this seam shows. Observe
/// only: reads the store, the legacy file and configuration, writes nothing.
pub fn select(repo_root: &Path, policy: &EffectiveContainerPolicy) -> BackendSelection {
    use crate::backend::BackendSource;
    let exact = crate::backend::exact_backend(repo_root, &policy.name, policy.driver);
    let reason = match exact.source {
        BackendSource::Environment => BackendReason::EnvironmentVariable(
            std::env::var("EFFIGY_COMPOSE_BACKEND").unwrap_or_default(),
        ),
        BackendSource::Recorded(source) => BackendReason::Recorded(source),
        BackendSource::Legacy => BackendReason::LegacyMetadata,
        BackendSource::UserPreference => BackendReason::UserPreference,
        BackendSource::DriverDefault => match policy.driver {
            ManifestContainerDriver::Colima => {
                BackendReason::ManifestDriver(crate::driver_label(policy.driver).to_owned())
            }
        },
    };
    selection_for(exact.id, &policy.profile, reason)
}

pub(super) fn docker_kind() -> BackendKind {
    if cfg!(target_os = "macos") {
        BackendKind::DockerDesktop
    } else {
        BackendKind::DockerEngine
    }
}

fn selection_for(engine: BackendId, profile: &str, reason: BackendReason) -> BackendSelection {
    if engine == BackendId::docker_compose() {
        let context = std::env::var("DOCKER_CONTEXT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                std::env::var("DOCKER_HOST")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| "default".to_owned());
        BackendSelection {
            kind: docker_kind(),
            engine,
            context,
            reason,
        }
    } else {
        BackendSelection {
            kind: BackendKind::Colima,
            engine: BackendId::colima_nerdctl(),
            context: profile.to_owned(),
            reason,
        }
    }
}

/// [`select`], with the recorded context when the record is what chose it:
/// an explicit backend for this invocation outranks the record, and the
/// record outranks every other policy source (exact targeting).
pub fn effective(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    recorded: Option<&BackendSelection>,
) -> BackendSelection {
    let selected = select(repo_root, policy);
    match (recorded, &selected.reason) {
        (Some(recorded), BackendReason::Recorded(_)) if recorded.engine == selected.engine => {
            recorded.clone()
        }
        _ => selected,
    }
}

/// The adapter outcome. `Uncertain` means the engine may have acted: the
/// journal records it `Unknown`, never `Failed` (R3, R5).
#[derive(Debug)]
pub enum BackendError {
    NotStarted(String),
    Uncertain(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStarted(message) | Self::Uncertain(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for BackendError {}

/// What one engine create is asked to create: the environment's workspace
/// service, described by the admitted render (effigy#510).
#[derive(Debug, Clone, Copy)]
pub struct WorkspaceCreation<'a> {
    /// The checkout the engine create runs from.
    pub workdir: &'a Path,
    /// The Compose project the workspace container belongs to.
    pub project: &'a str,
    /// The service the guest binary runs in.
    pub service: &'a str,
    /// The rendered Compose files the service specification is declared in.
    pub compose_files: &'a [std::path::PathBuf],
}

/// One container an engine's create response named: the exact full id the
/// operation returned and the name its request used.
///
/// Only an engine create builds one, so an inventory, a label match or a
/// progress line can never stand in for a creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedContainerId {
    container_id: String,
    name: String,
}

impl CreatedContainerId {
    /// The create response's own full container id.
    pub fn container_id(&self) -> &str {
        &self.container_id
    }

    /// The container name the create request used.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// A create response, for a fixture backend that must report one. No
    /// production caller builds this: an adapter receives these fields from the
    /// engine's own create response.
    pub fn from_response(container_id: &str, name: &str) -> Self {
        Self {
            container_id: container_id.to_owned(),
            name: name.to_owned(),
        }
    }
}

/// One workload the backend observes, with the labels that prove ownership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workload {
    pub id: String,
    pub name: String,
    pub running: bool,
    pub labels: std::collections::BTreeMap<String, String>,
    /// Host sources of bind mounts: ownership evidence for pre-label stacks.
    #[serde(default)]
    pub binds: Vec<String>,
    /// The image field the engine reports for the container (`Image`): the
    /// immutable image id on Docker, and the resolved reference where an
    /// engine reports only that (effigy#510).
    #[serde(default)]
    pub image: String,
    /// The image reference the container was created from (`Config.Image`).
    #[serde(default)]
    pub image_reference: String,
}

/// The seam every engine adapter implements. Applying and stopping run
/// through the caller's effect closure, so an adapter only observes.
pub trait EnvironmentBackend {
    fn selection(&self) -> &BackendSelection;
    /// Exact-identity inventory of one Compose project, stopped rows included.
    fn inventory(&mut self, workdir: &Path, project: &str) -> Observed<Vec<Workload>>;

    /// Create the environment's workspace container with the engine's own
    /// create API, and return exactly the full ids that operation's successful
    /// response named (effigy#510).
    ///
    /// This is the only creation fact that can become a receipt. Compose is a
    /// client of the engine's create: its apply creates the container, but the
    /// create response goes to Compose and never to Effigy, and Compose's
    /// progress output names containers rather than the ids it received. A
    /// backend that cannot issue the create itself therefore answers
    /// `NoBackendRecord` naming the missing adapter; `Verified` carries exactly
    /// the ids the create response returned, never an inventory, a name lookup,
    /// a timestamp or a label match.
    fn create_workspace_container(
        &mut self,
        request: &WorkspaceCreation<'_>,
    ) -> Observed<Vec<CreatedContainerId>> {
        let _ = request;
        Observed::Unknown(UnknownReason::NoBackendRecord(
            "this adapter issues no create of its own, so it never receives the engine's create \
             response"
                .to_owned(),
        ))
    }

    /// The exact containers these full ids name, read back for verification
    /// (effigy#510). Each requested id must answer exactly one row; a missing,
    /// duplicated or unexpected row is `Unknown`, because a partial answer
    /// cannot say what one create returned.
    fn containers_by_id(&mut self, workdir: &Path, ids: &[String]) -> Observed<Vec<Workload>> {
        let _ = (workdir, ids);
        Observed::Unknown(UnknownReason::ToolUnavailable(
            "this backend adapter cannot read containers back by id".to_owned(),
        ))
    }

    /// The exact image id `reference` resolves to in this backend's image
    /// table (effigy#510).
    ///
    /// A render declares an image *reference*; the engine's image table is the
    /// only authority that turns it into the exact image id a container created
    /// from it carries. A container bound only to a mutable reference is never
    /// immutable image identity, so `Absent`/`Unknown` refuse the receipt
    /// instead of falling back to the reference.
    fn image_id(&mut self, workdir: &Path, reference: &str) -> Observed<String> {
        let _ = (workdir, reference);
        Observed::Unknown(UnknownReason::ToolUnavailable(
            "this backend adapter cannot read its image table".to_owned(),
        ))
    }
}

/// Colima: `colima nerdctl --profile <profile> -- compose …`.
#[derive(Debug)]
pub struct ColimaBackend {
    selection: BackendSelection,
}

impl ColimaBackend {
    pub fn new(selection: BackendSelection) -> Self {
        Self { selection }
    }
}

/// Docker Desktop or Engine: the host `docker` CLI on its resolved context.
#[derive(Debug)]
pub struct DockerBackend {
    selection: BackendSelection,
}

impl DockerBackend {
    pub fn new(selection: BackendSelection) -> Self {
        Self { selection }
    }
}

macro_rules! compose_adapter {
    ($adapter:ty) => {
        impl EnvironmentBackend for $adapter {
            fn selection(&self) -> &BackendSelection {
                &self.selection
            }
            fn inventory(&mut self, workdir: &Path, project: &str) -> Observed<Vec<Workload>> {
                inventory(&self.selection, workdir, project)
            }
            fn containers_by_id(
                &mut self,
                workdir: &Path,
                ids: &[String],
            ) -> Observed<Vec<Workload>> {
                containers_by_id(&self.selection, workdir, ids)
            }
            fn image_id(&mut self, workdir: &Path, reference: &str) -> Observed<String> {
                image_id(&self.selection, workdir, reference)
            }
        }
    };
}
compose_adapter!(ColimaBackend);
compose_adapter!(DockerBackend);

/// The exact containers `ids` name, one row each.
fn containers_by_id(
    selection: &BackendSelection,
    workdir: &Path,
    ids: &[String],
) -> Observed<Vec<Workload>> {
    if ids.is_empty() {
        return Verified::rows(Vec::new(), "backend created ids");
    }
    if let Err(reason) = unique_ids(ids) {
        return Observed::Unknown(UnknownReason::Inconsistent(reason));
    }
    let mut inspect = vec![OsString::from("inspect")];
    inspect.extend(ids.iter().map(OsString::from));
    let (program, args) = runtime_invocation(selection, &inspect);
    match crate::exec::run_backend_invocation(workdir, &program, &args, "inspect created", &[]) {
        Ok(output) => match parse_inspect(&output.stdout).and_then(|rows| exact_id_rows(ids, rows))
        {
            Ok(rows) => Verified::rows(rows, "backend inspect by id"),
            Err(reason) => Observed::Unknown(UnknownReason::Inconsistent(reason)),
        },
        Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
    }
}

/// The exact image id `reference` resolves to on `selection`.
fn image_id(selection: &BackendSelection, workdir: &Path, reference: &str) -> Observed<String> {
    let args = ["image", "inspect", reference].map(OsString::from);
    let (program, args) = runtime_invocation(selection, &args);
    match crate::exec::run_backend_invocation(workdir, &program, &args, "inspect image", &[]) {
        Ok(output) => match parse_image_inspect(&output.stdout) {
            Ok(id) => Verified::value(id, "backend image inspect"),
            Err(reason) => Observed::Unknown(UnknownReason::Inconsistent(reason)),
        },
        Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
    }
}

/// A small constructor for the one evidence line every exact backend read
/// carries, so a caller can tell which reads produced a value.
struct Verified;

impl Verified {
    fn value<T>(value: T, source: &str) -> Observed<T> {
        Observed::Verified(
            value,
            effigy_kernel::observe::Evidence::new(source.into(), None).expect("fixed evidence"),
        )
    }
    fn rows(value: Vec<Workload>, source: &str) -> Observed<Vec<Workload>> {
        Self::value(value, source)
    }
}

/// The created ids are distinct: a create response that names one id twice
/// cannot say how many containers were created.
fn unique_ids(ids: &[String]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        if !full_container_id(id) {
            return Err(format!(
                "the create response named `{id}`, which is not a full container id"
            ));
        }
        if !seen.insert(id.to_ascii_lowercase()) {
            return Err(format!("the create response named container `{id}` twice"));
        }
    }
    Ok(())
}

/// The inspect answered exactly the created ids, once each.
fn exact_id_rows(ids: &[String], rows: Vec<Workload>) -> Result<Vec<Workload>, String> {
    if rows.len() != ids.len() {
        return Err(format!(
            "the create response named {} containers and the engine answered {} for them",
            ids.len(),
            rows.len()
        ));
    }
    let requested: std::collections::BTreeSet<String> =
        ids.iter().map(|id| id.to_ascii_lowercase()).collect();
    let mut seen = std::collections::BTreeSet::new();
    for row in &rows {
        if !full_container_id(&row.id) {
            return Err(format!(
                "the engine reported `{}`, which is not a full container id",
                row.id
            ));
        }
        let id = row.id.to_ascii_lowercase();
        if !requested.contains(&id) {
            return Err(format!(
                "the engine answered container `{}`, which no create response named",
                row.id
            ));
        }
        if !seen.insert(id) {
            return Err(format!("the engine answered container `{}` twice", row.id));
        }
    }
    Ok(rows)
}

/// A full container id: sixty-four hexadecimal digits.
pub(crate) fn full_container_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The exact image id a value spells, or `None` when it is not one.
///
/// An image id is the engine's own `sha256:` and sixty-four hexadecimal
/// digits. A bare sixty-four-digit spelling is normalised to the `sha256:`
/// form; a short id, a reference or an empty value is not an image id, and
/// nothing here treats a mutable reference as one.
pub(crate) fn exact_image_id(value: &str) -> Option<String> {
    let value = value.trim();
    let digits = value.strip_prefix("sha256:").unwrap_or(value);
    if digits.len() != 64 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("sha256:{}", digits.to_ascii_lowercase()))
}

/// Parses `docker image inspect` / `nerdctl image inspect` JSON into the one
/// exact image id it answers for the inspected reference. Empty, multiple or
/// non-id answers are refused: the receipt binds an id, never a guess.
pub(crate) fn parse_image_inspect(bytes: &[u8]) -> Result<String, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Row {
        id: String,
    }
    let rows: Vec<Row> = serde_json::from_slice(bytes)
        .map_err(|error| format!("unparsable image inspect: {error}"))?;
    let [row] = rows.as_slice() else {
        return Err(format!(
            "the image inspect answered {} images where one reference names exactly one",
            rows.len()
        ));
    };
    exact_image_id(&row.id).ok_or_else(|| {
        format!(
            "the image inspect reported `{}`, which is not an image id",
            row.id
        )
    })
}

fn inventory(
    selection: &BackendSelection,
    workdir: &Path,
    project: &str,
) -> Observed<Vec<Workload>> {
    let filter = format!("label={COMPOSE_PROJECT_LABEL}={project}");
    let ps = [
        "ps",
        "--all",
        "--no-trunc",
        "--filter",
        filter.as_str(),
        "--format",
        "{{.ID}}",
    ]
    .map(OsString::from);
    let (program, args) = runtime_invocation(selection, &ps);
    let ids =
        match crate::exec::run_backend_invocation(workdir, &program, &args, "list project", &[]) {
            Ok(output) => String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>(),
            Err(error) => return Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        };
    if ids.is_empty() {
        return Observed::Verified(
            Vec::new(),
            effigy_kernel::observe::Evidence::new("backend inventory".into(), None)
                .expect("fixed evidence"),
        );
    }
    let mut inspect = vec![OsString::from("inspect")];
    inspect.extend(ids.iter().map(OsString::from));
    let (program, args) = runtime_invocation(selection, &inspect);
    match crate::exec::run_backend_invocation(workdir, &program, &args, "inspect project", &[]) {
        Ok(output) => match parse_inspect(&output.stdout) {
            Ok(rows) => Observed::Verified(
                rows,
                effigy_kernel::observe::Evidence::new("backend inspect".into(), None)
                    .expect("fixed evidence"),
            ),
            Err(reason) => Observed::Unknown(UnknownReason::Inconsistent(reason)),
        },
        Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
    }
}

/// Compose labels every container, network and volume it creates with its
/// project name.
const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";
/// Effigy labels the volumes it creates for one project with its own name.
const EFFIGY_PROJECT_LABEL: &str = "com.effigy.project";

/// What one Compose project owns on one backend, observed before any effect.
/// A stop asks this before rendering: an environment this checkout never
/// prepared has nothing to stop, and its never-assigned ports must not reach
/// Compose at all (effigy#286).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProjectResources {
    pub containers: usize,
    pub networks: usize,
    /// Compose-created and Effigy-managed volumes that name the project.
    pub volumes: usize,
}

impl ProjectResources {
    /// Proven to own nothing: no container, network or volume names the
    /// project. Only `Observed::Verified` emptiness is proof; an unavailable
    /// backend never is.
    pub fn empty(&self) -> bool {
        self.containers == 0 && self.networks == 0 && self.volumes == 0
    }
}

/// What `project` owns on `selection`. Every listing runs through the selected
/// backend, and any listing that cannot be read makes the whole answer
/// `Unknown`: a stop treats only proven emptiness as nothing to tear down.
pub fn project_resources(
    selection: &BackendSelection,
    workdir: &Path,
    project: &str,
) -> Observed<ProjectResources> {
    let containers = match inventory(selection, workdir, project) {
        Observed::Verified(rows, _) => rows.len(),
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    let count = |kind: &str, label: &str| {
        count_project_rows(selection, workdir, kind, label, project)
            .map_err(|error| UnknownReason::Inconsistent(error.to_string()))
    };
    let networks = match count("network", COMPOSE_PROJECT_LABEL) {
        Ok(count) => count,
        Err(reason) => return Observed::Unknown(reason),
    };
    let compose_volumes = match count("volume", COMPOSE_PROJECT_LABEL) {
        Ok(count) => count,
        Err(reason) => return Observed::Unknown(reason),
    };
    let managed_volumes = match count("volume", EFFIGY_PROJECT_LABEL) {
        Ok(count) => count,
        Err(reason) => return Observed::Unknown(reason),
    };
    Observed::Verified(
        ProjectResources {
            containers,
            networks,
            // A volume may carry both labels; only emptiness matters here.
            volumes: compose_volumes.max(managed_volumes),
        },
        effigy_kernel::observe::Evidence::new("backend project resources".into(), None)
            .expect("fixed evidence"),
    )
}

/// How many rows of one backend listing carry the project label.
fn count_project_rows(
    selection: &BackendSelection,
    workdir: &Path,
    kind: &str,
    label: &str,
    project: &str,
) -> Result<usize, crate::exec::ContainerExecError> {
    let filter = format!("label={label}={project}");
    let args = [
        kind,
        "ls",
        "--filter",
        filter.as_str(),
        "--format",
        "{{.Name}}",
    ]
    .map(OsString::from);
    let (program, args) = runtime_invocation(selection, &args);
    let output =
        crate::exec::run_backend_invocation(workdir, &program, &args, "list project rows", &[])?;
    Ok(non_empty_lines(&output.stdout))
}

fn non_empty_lines(bytes: &[u8]) -> usize {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

fn runtime_invocation(
    selection: &BackendSelection,
    tail: &[OsString],
) -> (OsString, Vec<OsString>) {
    match selection.kind {
        BackendKind::Colima => {
            let mut args = vec![
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from(&selection.context),
                OsString::from("--"),
            ];
            args.extend(tail.iter().cloned());
            (OsString::from("colima"), args)
        }
        BackendKind::DockerDesktop | BackendKind::DockerEngine => {
            (OsString::from("docker"), tail.to_vec())
        }
    }
}

/// Parses `docker inspect` / `nerdctl inspect` JSON into workloads. Fields
/// the seam does not use (environment, volumes, networks) are never retained.
pub fn parse_inspect(bytes: &[u8]) -> Result<Vec<Workload>, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Row {
        id: String,
        name: String,
        /// The image id (Docker) or resolved reference (nerdctl) the container
        /// runs, and the reference it was created from.
        #[serde(default)]
        image: String,
        state: State,
        config: Config,
        #[serde(default)]
        mounts: Vec<Mount>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Mount {
        #[serde(default, rename = "Type")]
        kind: String,
        #[serde(default)]
        source: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct State {
        running: bool,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Config {
        #[serde(default)]
        labels: Option<std::collections::BTreeMap<String, String>>,
        /// The image reference the container was created from.
        #[serde(default)]
        image: String,
    }
    let rows: Vec<Row> =
        serde_json::from_slice(bytes).map_err(|error| format!("unparsable inspect: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|row| Workload {
            id: row.id,
            name: row.name.trim_start_matches('/').to_owned(),
            running: row.state.running,
            labels: row.config.labels.unwrap_or_default(),
            image: row.image,
            image_reference: row.config.image,
            binds: row
                .mounts
                .into_iter()
                .filter(|mount| mount.kind == "bind" && !mount.source.is_empty())
                .map(|mount| mount.source)
                .collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{
        exact_id_rows, exact_image_id, non_empty_lines, parse_image_inspect, unique_ids,
        ProjectResources, Workload,
    };

    /// An image id is the engine's `sha256:` and sixty-four hex digits; a short
    /// id, a reference or an empty value is not one, and the bare spelling is
    /// normalised rather than refused.
    #[test]
    fn an_image_id_is_exactly_one_image_id() {
        let digits = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            exact_image_id(&format!("sha256:{digits}")),
            Some(format!("sha256:{digits}"))
        );
        assert_eq!(exact_image_id(digits), Some(format!("sha256:{digits}")));
        assert_eq!(
            exact_image_id(&format!("  sha256:{}  ", digits.to_uppercase())),
            Some(format!("sha256:{digits}"))
        );
        for value in [
            "",
            "postgres:16",
            "0123456789abcdef",
            &format!("sha256:{digits}0"),
            "sha256:zz",
        ] {
            assert_eq!(exact_image_id(value), None, "{value}");
        }
    }

    /// An image inspect is exact or it is nothing: one image row carrying an
    /// exact image id.
    #[test]
    fn an_image_inspect_must_answer_exactly_one_image_id() {
        let digits = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            parse_image_inspect(format!(r#"[{{"Id": "sha256:{digits}"}}]"#).as_bytes()),
            Ok(format!("sha256:{digits}"))
        );
        for malformed in [
            "[]".to_owned(),
            format!(r#"[{{"Id": "sha256:{digits}"}}, {{"Id": "sha256:{digits}"}}]"#),
            r#"[{"Id": "alpine:3.19"}]"#.to_owned(),
            r#"[{"Id": "0123456789abcdef"}]"#.to_owned(),
            "not json".to_owned(),
            String::new(),
        ] {
            assert!(
                parse_image_inspect(malformed.as_bytes()).is_err(),
                "{malformed}"
            );
        }
    }

    fn row(name: &str, id: &str, project: &str) -> Workload {
        Workload {
            id: id.to_owned(),
            name: name.to_owned(),
            running: true,
            labels: [(super::COMPOSE_PROJECT_LABEL.to_owned(), project.to_owned())]
                .into_iter()
                .collect(),
            binds: Vec::new(),
            image: format!("sha256:{}", "a".repeat(64)),
            image_reference: "fixture:latest".to_owned(),
        }
    }

    /// The create response's ids are distinct full ids, and the engine answers
    /// exactly them, once each: a missing, extra, repeated or short answer is
    /// never a partial success.
    #[test]
    fn an_id_read_is_exact_or_refused() {
        let id = "a".repeat(64);
        let other = "b".repeat(64);
        let ids = vec![id.clone()];
        assert!(unique_ids(&ids).is_ok());
        assert!(unique_ids(&[id.clone(), id.clone()]).is_err());
        assert!(unique_ids(&["abc".to_owned()]).is_err());

        let rows = vec![row("demo-app-1", &id, "demo")];
        assert!(exact_id_rows(&ids, rows).is_ok());
        for (label, rows) in [
            ("no row", Vec::new()),
            ("another id", vec![row("demo-app-1", &other, "demo")]),
            ("a short id", vec![row("demo-app-1", "abc", "demo")]),
        ] {
            assert!(exact_id_rows(&ids, rows).is_err(), "{label} was accepted");
        }
    }

    /// A listing that names nothing is an empty listing; a listing the backend
    /// could not answer is never counted as one (`project_resources` refuses
    /// with `Unknown` before this point).
    #[test]
    fn only_a_listing_with_a_row_names_a_resource() {
        assert_eq!(non_empty_lines(b""), 0);
        assert_eq!(non_empty_lines(b"\n  \n\n"), 0);
        assert_eq!(non_empty_lines(b"demo-web-dev_default\n"), 1);
        assert_eq!(non_empty_lines(b"a\nb\n"), 2);
    }

    /// effigy#286: only a project proven to own no container, network or
    /// volume may be called empty.
    #[test]
    fn a_project_is_empty_only_with_no_container_network_or_volume() {
        assert!(ProjectResources::default().empty());
        for resources in [
            ProjectResources {
                containers: 1,
                ..ProjectResources::default()
            },
            ProjectResources {
                networks: 1,
                ..ProjectResources::default()
            },
            ProjectResources {
                volumes: 1,
                ..ProjectResources::default()
            },
        ] {
            assert!(!resources.empty(), "{resources:?}");
        }
    }
}
