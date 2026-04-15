use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_manifest::{
    load_task_manifest, ManifestContainerConfig, ManifestContainerDriver,
    ManifestContainerOnTaskExit, ManifestContainerShutdownMode, ManifestContainerStartup,
    ManifestContainersConfig, ManifestError,
};

const DEFAULT_COLIMA_PROFILE: &str = "default";
const DEFAULT_ATTACH_TIMEOUT_SECS: u64 = 10;
const DEFAULT_HEALTH_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveAttachMode {
    Attached,
    Detached,
}

#[derive(Debug, Clone)]
pub struct EffectiveContainerPolicy {
    pub name: String,
    pub driver: ManifestContainerDriver,
    pub startup: ManifestContainerStartup,
    pub profile: String,
    pub compose_file: PathBuf,
    pub compose_file_display: String,
    pub project_name: String,
    pub primary_service: String,
    pub declared_ports: Vec<String>,
    pub declared_mounts: Vec<String>,
    pub health_check: Option<String>,
    pub health_timeout_secs: u64,
    pub ui_tabs: Vec<String>,
    pub on_task_exit: ManifestContainerOnTaskExit,
    pub shutdown: ManifestContainerShutdownMode,
    pub detach_timeout_secs: u64,
}

#[derive(Debug)]
pub enum ContainerPolicyError {
    Manifest(ManifestError),
    TaskInvocation(String),
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
}

impl std::fmt::Display for ContainerPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(error) => write!(f, "{error}"),
            Self::TaskInvocation(message) => write!(f, "{message}"),
            Self::Read { path, error } => {
                write!(f, "failed to read {}: {error}", path.display())
            }
        }
    }
}

impl std::error::Error for ContainerPolicyError {}

impl From<ManifestError> for ContainerPolicyError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

pub fn load_container_policy(
    repo_root: &Path,
    requested_name: Option<&str>,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    let manifest_path = repo_root.join("effigy.toml");
    let manifest = load_task_manifest(&manifest_path)?;
    let containers = manifest.containers.ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(
            "manifest does not define a `[containers]` registry".to_owned(),
        )
    })?;
    let name = requested_name
        .map(str::to_owned)
        .or_else(|| containers.default.clone())
        .ok_or_else(|| {
            ContainerPolicyError::TaskInvocation(
                "container name omitted but `[containers].default` is not defined".to_owned(),
            )
        })?;
    let config = containers.environments.get(&name).ok_or_else(|| {
        let available = containers
            .environments
            .keys()
            .map(|entry| format!("`{entry}`"))
            .collect::<Vec<_>>()
            .join(", ");
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` is not defined in `[containers]` (available: {available})"
        ))
    })?;
    build_effective_policy(repo_root, &containers, &name, config)
}

pub fn validate_container_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if policy.driver != ManifestContainerDriver::Colima {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{}` uses unsupported driver `{}`; v1 only supports `colima`",
            policy.name,
            driver_label(policy.driver)
        )));
    }
    if !policy.compose_file.is_file() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{}` compose_file not found: {}",
            policy.name,
            policy.compose_file.display()
        )));
    }
    validate_declared_mounts(repo_root, &policy.name, &policy.declared_mounts)?;
    Ok(())
}

pub fn effective_attach_mode(
    policy: &EffectiveContainerPolicy,
    attach: bool,
    detach: bool,
) -> EffectiveAttachMode {
    if attach {
        return EffectiveAttachMode::Attached;
    }
    if detach {
        return EffectiveAttachMode::Detached;
    }
    match policy.startup {
        ManifestContainerStartup::Attached => EffectiveAttachMode::Attached,
        ManifestContainerStartup::Detached => EffectiveAttachMode::Detached,
    }
}

fn build_effective_policy(
    repo_root: &Path,
    containers: &ManifestContainersConfig,
    name: &str,
    config: &ManifestContainerConfig,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    let compose_file =
        repo_relative_path(repo_root, &config.compose_file, "containers.*.compose_file")?;
    let compose_file_display = path_relative_to_repo(repo_root, &compose_file);
    let project_name = config.project_name.clone().unwrap_or_else(|| {
        let repo = repo_root
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("repo")
            .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
        format!("{repo}-{name}-dev")
    });
    let primary_service = config.primary_service.clone().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` must declare `primary_service`"
        ))
    })?;
    let host = config.host.as_ref().cloned().unwrap_or_default();
    let health = config.health.as_ref().cloned().unwrap_or_default();
    let ui_tabs = config
        .ui
        .as_ref()
        .map(|value| value.tabs.clone())
        .unwrap_or_default();
    let lifecycle = config.lifecycle.as_ref();
    let _ = containers;

    Ok(EffectiveContainerPolicy {
        name: name.to_owned(),
        driver: config.driver.unwrap_or(ManifestContainerDriver::Colima),
        startup: config.startup.unwrap_or(ManifestContainerStartup::Attached),
        profile: config
            .profile
            .clone()
            .unwrap_or_else(|| DEFAULT_COLIMA_PROFILE.to_owned()),
        compose_file,
        compose_file_display,
        project_name,
        primary_service,
        declared_ports: host.ports,
        declared_mounts: host.mounts,
        health_check: health.check,
        health_timeout_secs: health.timeout_secs.unwrap_or(DEFAULT_HEALTH_TIMEOUT_SECS),
        ui_tabs,
        on_task_exit: lifecycle
            .and_then(|value| value.on_task_exit)
            .unwrap_or(ManifestContainerOnTaskExit::Stop),
        shutdown: lifecycle
            .and_then(|value| value.shutdown)
            .unwrap_or(ManifestContainerShutdownMode::Graceful),
        detach_timeout_secs: lifecycle
            .and_then(|value| value.detach_timeout_secs)
            .unwrap_or(DEFAULT_ATTACH_TIMEOUT_SECS),
    })
}

fn validate_declared_mounts(
    repo_root: &Path,
    container_name: &str,
    mounts: &[String],
) -> Result<(), ContainerPolicyError> {
    let repo_root = repo_root
        .canonicalize()
        .map_err(|error| ContainerPolicyError::Read {
            path: repo_root.to_path_buf(),
            error,
        })?;
    for mount in mounts {
        let source = mount.split(':').next().unwrap_or_default().trim();
        if source.is_empty() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` has invalid mount `{mount}`; expected `<repo-relative-source>:<target>`"
            )));
        }
        let source_path = Path::new(source);
        if source_path.is_absolute() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount `{mount}` must use a repo-relative source path"
            )));
        }
        let resolved = repo_root.join(source_path);
        let canonical = resolved.canonicalize().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount source `{source}` is invalid: {error}"
            ))
        })?;
        if canonical.strip_prefix(&repo_root).is_err() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount `{mount}` escapes the repo root"
            )));
        }
    }
    Ok(())
}

fn repo_relative_path(
    repo_root: &Path,
    raw: &str,
    field: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let path = Path::new(raw);
    if path.is_absolute() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "`{field}` must stay repo-relative in v1"
        )));
    }
    Ok(repo_root.join(path))
}

fn path_relative_to_repo(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn driver_label(driver: ManifestContainerDriver) -> &'static str {
    match driver {
        ManifestContainerDriver::Colima => "colima",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        effective_attach_mode, load_container_policy, validate_container_policy,
        ContainerPolicyError, EffectiveAttachMode,
    };
    use std::fs;
    use std::path::PathBuf;

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-containers-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    #[test]
    fn load_container_policy_uses_default_container() {
        let root = temp_repo("default");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let policy = load_container_policy(&root, None).expect("policy");

        assert_eq!(policy.name, "web");
        assert_eq!(policy.compose_file_display, "infra/dev/docker-compose.yml");
    }

    #[test]
    fn validate_container_policy_rejects_mounts_outside_repo() {
        let root = temp_repo("mount");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.host]
mounts = ["../outside:/workspace/outside"]
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let policy = load_container_policy(&root, None).expect("policy");
        let error = validate_container_policy(&root, &policy).expect_err("should fail");
        assert!(error.to_string().contains("escapes the repo root"));
    }

    #[test]
    fn effective_attach_mode_respects_flags_before_policy() {
        let root = temp_repo("attach-mode");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
startup = "detached"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let policy = load_container_policy(&root, None).expect("policy");
        assert_eq!(
            effective_attach_mode(&policy, false, false),
            EffectiveAttachMode::Detached
        );
        assert_eq!(
            effective_attach_mode(&policy, true, false),
            EffectiveAttachMode::Attached
        );
    }

    #[test]
    fn load_container_policy_requires_registry() {
        let root = temp_repo("missing-registry");
        fs::write(root.join("effigy.toml"), "[tasks]\n").expect("write manifest");
        let error = load_container_policy(&root, None).expect_err("should fail");
        assert!(matches!(error, ContainerPolicyError::TaskInvocation(_)));
        assert!(error.to_string().contains("[containers]"));
    }
}
