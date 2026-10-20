//! The backend seam: "reconcile this environment from its render" (Q-004).
//!
//! One selection names the exact engine and context an environment runs on,
//! and why it was chosen. The selection is recorded on the environment's
//! kernel resource row, so every later lifecycle, volume and data action
//! targets that backend instead of re-guessing from a profile name. Colima
//! and Docker (Desktop on macOS, Engine on Linux) implement the seam today; a
//! future Apple Containers adapter implements the same trait.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

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
    /// The environment record holds the backend its stack was applied on.
    Recorded(Box<BackendReason>),
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
            Self::Recorded(original) => format!(
                "the environment record keeps the backend its stack was applied on (originally: {})",
                original.describe()
            ),
            Self::LegacyMetadata => {
                "the checkout's v0.14 backend metadata names it".to_owned()
            }
            Self::UserPreference => "the user preference in ~/.effigy/config.toml".to_owned(),
            Self::ManifestDriver(driver) => format!("the manifest driver `{driver}` default"),
        }
    }

    /// The original policy source, without the recorded wrapper.
    pub fn origin(&self) -> &BackendReason {
        match self {
            Self::Recorded(original) => original.origin(),
            other => other,
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
    /// The kernel record of this selection (`resources.backend`).
    pub fn metadata(&self) -> BackendMetadata {
        BackendMetadata {
            engine: self.engine.as_str().to_owned(),
            context: self.context.clone(),
            source: serde_json::to_string(&(self.kind, self.reason.origin()))
                .expect("backend reason serializes"),
        }
    }

    /// Reads a recorded selection. An unreadable source is `Unknown`, never a
    /// default: destructive actions refuse until the backend is named (M4).
    pub fn from_metadata(metadata: &BackendMetadata) -> Observed<Self> {
        let decoded = serde_json::from_str::<(BackendKind, BackendReason)>(&metadata.source);
        let engine = BackendId::new(metadata.engine.clone());
        match decoded {
            Ok((kind, reason))
                if engine == BackendId::colima_nerdctl()
                    || engine == BackendId::docker_compose() =>
            {
                Observed::Verified(
                    Self {
                        kind,
                        engine,
                        context: metadata.context.clone(),
                        reason: BackendReason::Recorded(Box::new(reason)),
                    },
                    effigy_kernel::observe::Evidence::new(
                        "environment backend record".into(),
                        None,
                    )
                    .expect("fixed evidence"),
                )
            }
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

/// Resolves the selection for `policy` from policy sources, ignoring any
/// record. Pure: reads environment, user config and manifest facts only.
pub fn select(repo_root: &Path, policy: &EffectiveContainerPolicy) -> BackendSelection {
    let (engine, reason) = if let Some((value, engine)) = std::env::var("EFFIGY_COMPOSE_BACKEND")
        .ok()
        .and_then(|value| crate::backend_override_from_env_value(&value).map(|id| (value, id)))
    {
        (engine, BackendReason::EnvironmentVariable(value))
    } else if let Some(engine) =
        crate::load_runtime_backend_override(repo_root, Some(policy.name.as_str()))
    {
        (engine, BackendReason::LegacyMetadata)
    } else if let Some(engine) = crate::user_global_backend_preference() {
        (engine, BackendReason::UserPreference)
    } else {
        match policy.driver {
            ManifestContainerDriver::Colima => (
                BackendId::colima_nerdctl(),
                BackendReason::ManifestDriver(crate::driver_label(policy.driver).to_owned()),
            ),
        }
    };
    selection_for(engine, &policy.profile, reason)
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
            kind: if cfg!(target_os = "macos") {
                BackendKind::DockerDesktop
            } else {
                BackendKind::DockerEngine
            },
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

/// An explicit backend for this invocation outranks the record; the record
/// outranks every other policy source (exact targeting).
pub fn effective(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    recorded: Option<&BackendSelection>,
) -> BackendSelection {
    let selected = select(repo_root, policy);
    match (recorded, &selected.reason) {
        (_, BackendReason::EnvironmentVariable(_)) | (None, _) => selected,
        (Some(recorded), _) => recorded.clone(),
    }
}

/// One rendered environment, as the backend reconciles it.
#[derive(Debug, Clone)]
pub struct RenderRef<'a> {
    pub project: &'a str,
    pub workdir: &'a Path,
    /// `compose -f … -p …` prefix plus the operation's own arguments.
    pub compose_files: &'a [PathBuf],
    pub env: &'a [(String, OsString)],
}

impl RenderRef<'_> {
    fn compose(&self, tail: &[&str]) -> Vec<OsString> {
        let mut args = vec![OsString::from("compose")];
        for file in self.compose_files {
            args.push(OsString::from("-f"));
            args.push(file.as_os_str().to_os_string());
        }
        args.push(OsString::from("-p"));
        args.push(OsString::from(self.project));
        args.extend(tail.iter().map(OsString::from));
        args
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
}

/// The seam every engine adapter implements.
pub trait EnvironmentBackend {
    fn selection(&self) -> &BackendSelection;
    /// Converge the backend on the render: create missing workloads and
    /// recreate only changed ones (`compose up -d`).
    fn reconcile(&mut self, render: &RenderRef<'_>) -> Result<(), BackendError>;
    /// Stop the render's workloads, keeping volumes.
    fn stop(&mut self, render: &RenderRef<'_>) -> Result<(), BackendError>;
    /// Exact-identity inventory of one Compose project, stopped rows included.
    fn inventory(&mut self, workdir: &Path, project: &str) -> Observed<Vec<Workload>>;
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
            fn reconcile(&mut self, render: &RenderRef<'_>) -> Result<(), BackendError> {
                run(
                    &self.selection,
                    render,
                    &["up", "-d", "--remove-orphans"],
                    "compose up",
                )
            }
            fn stop(&mut self, render: &RenderRef<'_>) -> Result<(), BackendError> {
                run(&self.selection, render, &["stop"], "compose stop")
            }
            fn inventory(&mut self, workdir: &Path, project: &str) -> Observed<Vec<Workload>> {
                inventory(&self.selection, workdir, project)
            }
        }
    };
}
compose_adapter!(ColimaBackend);
compose_adapter!(DockerBackend);

fn run(
    selection: &BackendSelection,
    render: &RenderRef<'_>,
    tail: &[&str],
    label: &str,
) -> Result<(), BackendError> {
    let (program, args) = selection.compose_invocation(&render.compose(tail));
    crate::exec::run_backend_invocation(render.workdir, &program, &args, label, render.env)
        .map(|_| ())
        .map_err(|error| match error {
            crate::exec::ContainerExecError::Launch { .. } => {
                BackendError::NotStarted(error.to_string())
            }
            other => BackendError::Uncertain(other.to_string()),
        })
}

fn inventory(
    selection: &BackendSelection,
    workdir: &Path,
    project: &str,
) -> Observed<Vec<Workload>> {
    let filter = format!("label=com.docker.compose.project={project}");
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
            binds: row
                .mounts
                .into_iter()
                .filter(|mount| mount.kind == "bind" && !mount.source.is_empty())
                .map(|mount| mount.source)
                .collect(),
        })
        .collect())
}
