//! Select the artifact source from declared policy and host provenance.

use super::artifact_receipt::{read_trimmed_identity_file, workspace_effigy_active_version_file};
use crate::runner::workspace_client::context::HOST_REPREPARE_COMMAND;
use crate::runner::RunnerError;
use effigy_core::repo_markers::has_task_manifest;
use std::path::{Path, PathBuf};
pub(super) const EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV: &str =
    "EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) enum LinuxWorkspaceTarget {
    X86_64Gnu,
    Aarch64Gnu,
    X86_64Musl,
    Aarch64Musl,
}

/// The declared source policy (§8.9). Auto follows proven host provenance;
/// Local builds the selected checkout; Download refuses local host builds.
/// Acquisition happens before publication and never at task entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinuxWorkspaceArtifactSource {
    Auto,
    Local,
    Download,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct EffigySourceConfig {
    pub(super) repo_root: String,
}

impl LinuxWorkspaceTarget {
    pub(in crate::runner) fn release_triple(self) -> &'static str {
        match self {
            Self::X86_64Gnu => "x86_64-unknown-linux-gnu",
            Self::Aarch64Gnu => "aarch64-unknown-linux-gnu",
            Self::X86_64Musl => "x86_64-unknown-linux-musl",
            Self::Aarch64Musl => "aarch64-unknown-linux-musl",
        }
    }

    /// The artifact family an image needs: its platform **and** its C library
    /// (§8.9). An artifact built against the other C library does not run, so a
    /// guest whose libc this host cannot prove has no selectable artifact.
    pub(in crate::runner) fn from_image(machine: &str, libc: &str) -> Result<Self, RunnerError> {
        let machine = match machine.trim() {
            "x86_64" | "amd64" => "x86_64",
            "aarch64" | "arm64" => "aarch64",
            other => {
                return Err(RunnerError::task_invocation(format!(
                    "workspace container image platform `{other}` has no pinned Linux guest \
                     artifact; run `{HOST_REPREPARE_COMMAND}` with a supported image"
                )))
            }
        };
        let libc = match libc.trim().to_ascii_lowercase().as_str() {
            "gnu" | "glibc" => "gnu",
            "musl" => "musl",
            other => {
                return Err(RunnerError::task_invocation(format!(
                    "the workspace container image reports no provable C library (`{other}`), so \
                     no guest artifact can be proven to run in it; an artifact matches the \
                     image's libc, never a guess. Run `{HOST_REPREPARE_COMMAND}` with an image \
                     that ships a glibc or musl loader"
                )))
            }
        };
        Ok(match (machine, libc) {
            ("x86_64", "gnu") => Self::X86_64Gnu,
            ("aarch64", "gnu") => Self::Aarch64Gnu,
            ("x86_64", "musl") => Self::X86_64Musl,
            _ => Self::Aarch64Musl,
        })
    }

    /// The image facts this artifact family was built for. The release triple in
    /// the producer's receipt is its own record, so this is the artifact side of
    /// the independent platform comparison, never a read of the image.
    pub(in crate::runner) fn image_platform(
        self,
    ) -> effigy_kernel::workspace_session::ImagePlatform {
        use effigy_kernel::workspace_session::{ImagePlatform, LoaderFamily};
        let (architecture, loader) = match self {
            Self::X86_64Gnu => ("amd64", LoaderFamily::Gnu),
            Self::Aarch64Gnu => ("arm64", LoaderFamily::Gnu),
            Self::X86_64Musl => ("amd64", LoaderFamily::Musl),
            Self::Aarch64Musl => ("arm64", LoaderFamily::Musl),
        };
        ImagePlatform {
            os: "linux".into(),
            architecture: architecture.into(),
            // A release triple names a machine type, never an optional CPU
            // variant, so the artifact claims no variant it was not built for.
            variant: String::new(),
            loader,
        }
    }

    pub(super) fn artifact_relative_path(self) -> PathBuf {
        PathBuf::from(".effigy/linux-release/artifacts")
            .join(format!("effigy-{}", self.release_triple()))
    }

    pub(super) fn cache_relative_dir(self) -> PathBuf {
        PathBuf::from(".effigy/workspace-bin/linux").join(self.release_triple())
    }
}

/// The selected producer: a proven development checkout or a published release.
/// A nearby checkout alone never establishes host provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArtifactPlan {
    Build { checkout: PathBuf },
    Download,
}

/// The host facts that decide the [`ArtifactPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum HostProvenance {
    /// Installed by `install:local` or built in a checkout's `target/` tree.
    Checkout(PathBuf),
    /// Stamped as a `+local.` build whose checkout cannot be proven.
    UnprovenLocal,
    /// A published release or a plain install.
    Release,
}

/// Select the declared local checkout or follow host provenance.
/// A local host build never substitutes a published release.
pub(super) fn artifact_plan(
    host_binary: &Path,
    workspace_repo_root: &Path,
    cwd: &Path,
    source: LinuxWorkspaceArtifactSource,
) -> Result<ArtifactPlan, RunnerError> {
    match (source, host_provenance(host_binary)) {
        (LinuxWorkspaceArtifactSource::Local, _) => {
            let checkout = resolve_local_effigy_repo_root_from_paths(
                workspace_repo_root,
                host_binary.parent(),
                Some(cwd),
            )
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "{EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV}=local found no Effigy checkout to \
                     build the workspace artifact from; run from that checkout, or use `auto` or \
                     `download`"
                ))
            })?;
            Ok(ArtifactPlan::Build { checkout })
        }
        (LinuxWorkspaceArtifactSource::Auto, HostProvenance::Checkout(checkout)) => {
            Ok(ArtifactPlan::Build { checkout })
        }
        (
            LinuxWorkspaceArtifactSource::Auto | LinuxWorkspaceArtifactSource::Download,
            HostProvenance::Release,
        ) => Ok(ArtifactPlan::Download),
        (LinuxWorkspaceArtifactSource::Download, HostProvenance::Checkout(_)) => {
            Err(RunnerError::task_invocation(format!(
                "{EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV}=download would substitute the published \
                 release for this local Effigy build, which has no release artifact; use `auto` \
                 to build this checkout's artifact, or `local`"
            )))
        }
        (
            LinuxWorkspaceArtifactSource::Auto | LinuxWorkspaceArtifactSource::Download,
            HostProvenance::UnprovenLocal,
        ) => Err(RunnerError::task_invocation(
            "this local Effigy build has no provable checkout, so no workspace artifact can be \
             built for it, and the published release is not its artifact; run `effigy \
             install:local` from its checkout",
        )),
    }
}

/// The provenance of the running host, from its executable's location and its
/// install stamp. Nearby checkouts are not provenance.
fn host_provenance(host_binary: &Path) -> HostProvenance {
    if let Some(checkout) =
        checkout_of_local_install(host_binary).or_else(|| checkout_of_build_tree(host_binary))
    {
        return HostProvenance::Checkout(checkout);
    }
    let stamped = read_trimmed_identity_file(&workspace_effigy_active_version_file(host_binary));
    if stamped.is_some_and(|version| version.contains("+local.")) {
        return HostProvenance::UnprovenLocal;
    }
    HostProvenance::Release
}

/// `<checkout>/.local-install/bin/effigy`, the path `install:local` activates.
fn checkout_of_local_install(host_binary: &Path) -> Option<PathBuf> {
    let bin = host_binary.parent()?;
    let install = bin.parent()?;
    if !path_named(bin, "bin") || !path_named(install, ".local-install") {
        return None;
    }
    let checkout = install.parent()?;
    looks_like_effigy_repo_root(checkout).then(|| checkout.to_path_buf())
}

/// `<checkout>/target/{debug,release}/effigy`, a checkout's own build output.
fn checkout_of_build_tree(host_binary: &Path) -> Option<PathBuf> {
    let profile = host_binary.parent()?;
    let target = profile.parent()?;
    let checkout = target.parent()?;
    let profile_is_build = path_named(profile, "debug") || path_named(profile, "release");
    if !profile_is_build || !path_named(target, "target") {
        return None;
    }
    looks_like_effigy_repo_root(checkout).then(|| checkout.to_path_buf())
}

fn path_named(path: &Path, name: &str) -> bool {
    path.file_name().is_some_and(|file| file == name)
}

pub(super) fn resolve_local_effigy_repo_root_from_paths(
    workspace_repo_root: &Path,
    current_exe_parent: Option<&Path>,
    cwd: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(effigy_repo_root) = discover_effigy_repo_root(current_exe_parent) {
        return Some(effigy_repo_root);
    }

    if let Some(effigy_repo_root) = discover_effigy_repo_root(cwd) {
        return Some(effigy_repo_root);
    }

    if let Some(effigy_repo_root) = sibling_effigy_repo_root(workspace_repo_root) {
        return Some(effigy_repo_root);
    }

    configured_effigy_repo_root()
}

pub(super) fn configured_linux_workspace_artifact_source(
) -> Result<LinuxWorkspaceArtifactSource, RunnerError> {
    let Some(raw) = std::env::var_os(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV) else {
        return Ok(LinuxWorkspaceArtifactSource::Auto);
    };
    let normalized = raw.to_string_lossy().trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "auto" => Ok(LinuxWorkspaceArtifactSource::Auto),
        "local" => Ok(LinuxWorkspaceArtifactSource::Local),
        "download" | "github" | "release" => Ok(LinuxWorkspaceArtifactSource::Download),
        _ => Err(RunnerError::task_invocation(format!(
            "{EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV} must be one of `auto`, `local`, or `download`; got `{normalized}`"
        ))),
    }
}

pub(super) fn sibling_effigy_repo_root(workspace_repo_root: &Path) -> Option<PathBuf> {
    for ancestor in workspace_repo_root.ancestors().skip(1) {
        for candidate in nearby_effigy_repo_candidates(ancestor) {
            if looks_like_effigy_repo_root(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

pub(super) fn nearby_effigy_repo_candidates(base: &Path) -> [PathBuf; 2] {
    [base.join("effigy"), base.join("projects").join("effigy")]
}

pub(super) fn configured_effigy_repo_root() -> Option<PathBuf> {
    let path = effigy_source_config_path().ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let config = toml::from_str::<EffigySourceConfig>(&raw).ok()?;
    let repo_root = PathBuf::from(config.repo_root);
    looks_like_effigy_repo_root(&repo_root).then_some(repo_root)
}

#[cfg(test)]
pub(super) fn persist_effigy_source_repo_root(repo_root: &Path) -> Result<(), RunnerError> {
    let path = effigy_source_config_path()?;
    let parent = path.parent().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "effigy source config path has no parent: `{}`",
            path.display()
        ))
    })?;
    std::fs::create_dir_all(parent).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create effigy source config directory `{}`: {error}",
            parent.display()
        ))
    })?;
    let body = toml::to_string_pretty(&EffigySourceConfig {
        repo_root: repo_root.display().to_string(),
    })
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to serialize effigy source config for `{}`: {error}",
            repo_root.display()
        ))
    })?;
    std::fs::write(&path, body).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write effigy source config `{}`: {error}",
            path.display()
        ))
    })
}

pub(super) fn effigy_source_config_path() -> Result<PathBuf, RunnerError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        RunnerError::task_invocation("HOME is not set; cannot resolve effigy source config path")
    })?;
    Ok(PathBuf::from(home).join(".effigy").join("source.toml"))
}

pub(super) fn discover_effigy_repo_root(start: Option<&Path>) -> Option<PathBuf> {
    let mut current = start?;
    loop {
        if looks_like_effigy_repo_root(current) {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

pub(super) fn looks_like_effigy_repo_root(path: &Path) -> bool {
    has_task_manifest(path)
        && path.join("config/tasks.toml").is_file()
        && path.join("config/containers.toml").is_file()
}
