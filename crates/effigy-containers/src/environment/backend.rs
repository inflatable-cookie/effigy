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
use std::time::Instant;

use effigy_kernel::id::RunId;
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
    /// The image ID the container runs, as the engine reports it (`Image`).
    /// A creation window binds the container to the image it was created from
    /// (effigy#508).
    pub image: String,
    /// The container's own creation time (`Created`, RFC 3339). A container
    /// the window did not exist before cannot carry a stamp older than the
    /// apply it appeared in.
    pub created: String,
}

/// The seam every engine adapter implements. Applying and stopping run
/// through the caller's effect closure, so an adapter only observes.
pub trait EnvironmentBackend {
    fn selection(&self) -> &BackendSelection;
    /// Exact-identity inventory of one Compose project, stopped rows included.
    fn inventory(&mut self, workdir: &Path, project: &str) -> Observed<Vec<Workload>>;
    /// [`Self::inventory`] supervised by the caller's one absolute deadline,
    /// so a runtime that stops answering fails the read instead of holding the
    /// environment gate open (effigy#508).
    fn inventory_until(
        &mut self,
        workdir: &Path,
        project: &str,
        deadline: Instant,
    ) -> Observed<Vec<Workload>>;
    /// The run whose one Compose apply created `container_id` in `project`, as
    /// this backend's own creator record proves it (effigy#508).
    ///
    /// `Verified(run)` is the only answer a caller may treat as a creation
    /// receipt, and it must name the run whose apply the backend saw create
    /// *this exact full container id*. Every weaker fact is a fact a competing
    /// Compose actor reproduces — a creation timestamp, an inventory that was
    /// empty before and is populated after, Compose's own project and service
    /// labels, and a run id the caller supplies — so a backend that answered
    /// from those would hand the caller another actor's container.
    ///
    /// `Absent(AbsenceProof::NoCreator { .. })` is the proof a backend that
    /// keeps creator records gives when none names this container: another
    /// actor's container, a replacement, or an id a failed apply never
    /// created. `Unknown` means the backend has no such record at all, and its
    /// reason names the missing API.
    fn created_by(&mut self, project: &str, container_id: &str) -> Observed<RunId>;
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
            fn inventory_until(
                &mut self,
                workdir: &Path,
                project: &str,
                deadline: Instant,
            ) -> Observed<Vec<Workload>> {
                inventory_until(&self.selection, workdir, project, Some(deadline))
            }
            fn created_by(&mut self, project: &str, container_id: &str) -> Observed<RunId> {
                created_by(&self.selection, project, container_id)
            }
        }
    };
}
compose_adapter!(ColimaBackend);
compose_adapter!(DockerBackend);

/// The creator receipt a Compose adapter can honestly give: none (effigy#508).
///
/// Neither engine Effigy targets records *who* created a container. `inspect`
/// and `compose ps` expose the id, name, creation time and labels; the event
/// streams name the actor's id and labels; neither names a creating client.
/// The one authenticated fact is the `Id` in the engine's create response,
/// which goes to the client that issued the create — here the Compose CLI that
/// performs the environment's single apply, whose own creation report is a
/// human progress line naming the container, not an id it received. So this
/// adapter cannot distinguish this run's apply from a competing actor's
/// identical project, service and labels, and says exactly that instead of
/// minting a receipt from what it can observe.
fn created_by(selection: &BackendSelection, project: &str, container_id: &str) -> Observed<RunId> {
    Observed::Unknown(UnknownReason::NoBackendRecord(format!(
        "{} keeps no record of which client created a container, so `{container_id}` in project \
         `{project}` cannot be attributed to the run whose apply created it: creation goes through \
         the Compose CLI, and the new container's id exists only in the engine's create response to \
         that CLI",
        selection.kind.label()
    )))
}

fn inventory(
    selection: &BackendSelection,
    workdir: &Path,
    project: &str,
) -> Observed<Vec<Workload>> {
    inventory_until(selection, workdir, project, None)
}

/// [`inventory`] under the caller's one absolute deadline (effigy#508): the
/// listing and the inspect of its rows are supervised by the same deadline, so
/// a runtime that stops answering reports a timeout instead of holding the
/// caller.
fn inventory_until(
    selection: &BackendSelection,
    workdir: &Path,
    project: &str,
    deadline: Option<Instant>,
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
    let ids = match capture(workdir, &program, &args, "list project", deadline) {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        Err(error) => return Observed::Unknown(read_reason("list project", error)),
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
    match capture(workdir, &program, &args, "inspect project", deadline) {
        Ok(output) => match parse_inspect(&output.stdout) {
            Ok(rows) => Observed::Verified(
                rows,
                effigy_kernel::observe::Evidence::new("backend inspect".into(), None)
                    .expect("fixed evidence"),
            ),
            Err(reason) => Observed::Unknown(UnknownReason::Inconsistent(reason)),
        },
        Err(error) => Observed::Unknown(read_reason("inspect project", error)),
    }
}

/// One backend read on the caller's deadline, or unbounded when the caller set
/// none (the ordinary inventory).
fn capture(
    workdir: &Path,
    program: &std::ffi::OsStr,
    args: &[OsString],
    label: &str,
    deadline: Option<Instant>,
) -> Result<std::process::Output, crate::exec::ContainerExecError> {
    match deadline {
        Some(deadline) => {
            crate::exec::run_backend_invocation_until(workdir, program, args, label, &[], deadline)
        }
        None => crate::exec::run_backend_invocation(workdir, program, args, label, &[]),
    }
}

/// Why a backend read could not be taken. A read that timed out says so and
/// stays retryable; anything else is an engine answer this seam cannot use.
fn read_reason(label: &str, error: crate::exec::ContainerExecError) -> UnknownReason {
    if crate::exec::error_is_timeout(&error) {
        UnknownReason::TimedOut {
            phase: label.to_owned(),
        }
    } else {
        UnknownReason::Inconsistent(error.to_string())
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
        /// The image ID the container runs.
        image: String,
        /// The container's own creation time, RFC 3339.
        created: String,
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
            created: row.created,
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
    use super::{non_empty_lines, ProjectResources};

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
