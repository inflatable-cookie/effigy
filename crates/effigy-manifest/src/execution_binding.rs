use crate::config_sections::{
    ManifestContainersConfig, ManifestInlineWorkspaceContainerConfig, ManifestSystemsConfig,
    ManifestWorkspaceConfig, ManifestWorkspaceContainerRef,
};
use crate::{ManifestTask, ManifestTaskRunIn, TaskManifest};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedTaskExecutionBinding {
    Host,
    Workspace(Box<ResolvedWorkspaceBinding>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedWorkspaceBinding {
    pub system: String,
    pub workspace: String,
    pub working_dir: Option<String>,
    pub workspace_config: ManifestWorkspaceConfig,
    pub container: Option<ResolvedWorkspaceContainer>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedWorkspaceContainer {
    Named(String),
    Inline(ResolvedInlineWorkspaceContainer),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedInlineWorkspaceContainer {
    pub synthetic_name: String,
    pub image: Option<String>,
    pub mount: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionBindingResolveError {
    detail: String,
}

impl ExecutionBindingResolveError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for ExecutionBindingResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for ExecutionBindingResolveError {}

pub fn resolve_task_execution_binding(
    manifest: &TaskManifest,
    task_name: &str,
    task: &ManifestTask,
) -> Result<Option<ResolvedTaskExecutionBinding>, ExecutionBindingResolveError> {
    resolve_task_execution_binding_from_parts(
        manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        manifest.systems.as_ref(),
        manifest.containers.as_ref(),
        task_name,
        task,
    )
}

pub fn resolve_task_execution_binding_from_systems(
    default_run_in: Option<ManifestTaskRunIn>,
    systems: Option<&ManifestSystemsConfig>,
    task_name: &str,
    task: &ManifestTask,
) -> Result<Option<ResolvedTaskExecutionBinding>, ExecutionBindingResolveError> {
    resolve_task_execution_binding_from_parts(default_run_in, systems, None, task_name, task)
}

pub fn resolve_task_execution_binding_from_parts(
    default_run_in: Option<ManifestTaskRunIn>,
    systems: Option<&ManifestSystemsConfig>,
    containers: Option<&ManifestContainersConfig>,
    task_name: &str,
    task: &ManifestTask,
) -> Result<Option<ResolvedTaskExecutionBinding>, ExecutionBindingResolveError> {
    let has_workspace_binding = task.system.is_some() || task.workspace.is_some();
    let implicit_default_target_available = has_implicit_default_target(containers);
    let effective_run_in = task.effective_run_in(default_run_in);
    if effective_run_in == ManifestTaskRunIn::Host {
        if has_workspace_binding {
            return Err(ExecutionBindingResolveError::new(format!(
                "task `{task_name}` cannot combine `run_in = \"host\"` with container or workspace execution binding"
            )));
        }
        return Ok(Some(ResolvedTaskExecutionBinding::Host));
    }

    let Some(systems) = systems else {
        if task.workspace.is_some() && task.system.is_none() {
            return Err(ExecutionBindingResolveError::new(format!(
                "task `{task_name}` sets `workspace` but no task `system` or `[systems].default` is defined"
            )));
        }
        if has_workspace_binding {
            return Err(ExecutionBindingResolveError::new(format!(
                "task `{task_name}` references `system` or `workspace`, but the manifest does not define `[systems]`"
            )));
        }
        return Ok(None);
    };

    let resolved_system = match task
        .system
        .as_ref()
        .or(systems.default.as_ref())
        .or_else(|| {
            if implicit_default_target_available {
                sole_entry_name(&systems.systems)
            } else {
                None
            }
        }) {
        Some(name) => name,
        None => {
            if task.workspace.is_some() {
                return Err(ExecutionBindingResolveError::new(format!(
                    "task `{task_name}` sets `workspace` but no task `system`, `[systems].default`, or sole system entry is defined"
                )));
            }
            return Ok(None);
        }
    };

    let system_config = systems.systems.get(resolved_system).ok_or_else(|| {
        ExecutionBindingResolveError::new(format!(
            "task `{task_name}` resolved system `{resolved_system}`, but `[systems.{resolved_system}]` is not defined"
        ))
    })?;

    let (resolved_workspace, workspace_config) = resolve_workspace_config(
        system_config,
        containers,
        task.workspace.as_ref(),
        implicit_default_target_available,
    )
    .ok_or_else(|| {
        ExecutionBindingResolveError::new(format!(
            "task `{task_name}` resolved system `{resolved_system}`, but no task `workspace`, `[systems.{resolved_system}].default_workspace`, sole workspace entry, or implied `default` workspace is defined"
        ))
    })?;

    let container = workspace_container(
        resolved_system,
        &resolved_workspace,
        &workspace_config,
        containers,
    );

    Ok(Some(ResolvedTaskExecutionBinding::Workspace(Box::new(
        ResolvedWorkspaceBinding {
            system: resolved_system.clone(),
            workspace: resolved_workspace,
            working_dir: workspace_config.working_dir.clone(),
            workspace_config: workspace_config.clone(),
            container,
        },
    ))))
}

fn workspace_container(
    system_name: &str,
    workspace_name: &str,
    workspace: &ManifestWorkspaceConfig,
    containers: Option<&ManifestContainersConfig>,
) -> Option<ResolvedWorkspaceContainer> {
    let container_ref = workspace
        .container
        .as_ref()
        .cloned()
        .or_else(|| default_workspace_container(containers))?;
    match container_ref {
        ManifestWorkspaceContainerRef::Named(name) => Some(ResolvedWorkspaceContainer::Named(name)),
        ManifestWorkspaceContainerRef::Inline(ManifestInlineWorkspaceContainerConfig {
            image,
            mount,
            ..
        }) => Some(ResolvedWorkspaceContainer::Inline(
            ResolvedInlineWorkspaceContainer {
                synthetic_name: format!("{system_name}__{workspace_name}"),
                image: image.clone(),
                mount: mount.clone(),
            },
        )),
    }
}

fn default_workspace_container(
    containers: Option<&ManifestContainersConfig>,
) -> Option<ManifestWorkspaceContainerRef> {
    let containers = containers?;
    containers
        .default
        .clone()
        .or_else(|| sole_dev_context_container_name(containers))
        .map(ManifestWorkspaceContainerRef::Named)
}

fn resolve_workspace_config(
    system_config: &crate::config_sections::ManifestSystemConfig,
    containers: Option<&ManifestContainersConfig>,
    requested_workspace: Option<&String>,
    implicit_default_target_available: bool,
) -> Option<(String, ManifestWorkspaceConfig)> {
    let resolved_workspace = requested_workspace
        .cloned()
        .or(system_config.default_workspace.clone())
        .or_else(|| {
            if implicit_default_target_available {
                sole_entry_name(&system_config.workspaces).cloned()
            } else {
                None
            }
        })
        .or_else(|| implied_default_workspace_name(system_config, containers))?;

    if let Some(workspace_config) = system_config.workspaces.get(&resolved_workspace) {
        return Some((
            resolved_workspace,
            merge_workspace_config(system_config, workspace_config),
        ));
    }

    implied_default_workspace_config(system_config, containers, &resolved_workspace)
        .map(|workspace_config| (resolved_workspace, workspace_config))
}

fn implied_default_workspace_name(
    system_config: &crate::config_sections::ManifestSystemConfig,
    containers: Option<&ManifestContainersConfig>,
) -> Option<String> {
    if !system_config.workspaces.is_empty() {
        return None;
    }
    default_workspace_container(containers).map(|_| "default".to_owned())
}

fn implied_default_workspace_config(
    system_config: &crate::config_sections::ManifestSystemConfig,
    containers: Option<&ManifestContainersConfig>,
    workspace_name: &str,
) -> Option<ManifestWorkspaceConfig> {
    if workspace_name != "default" || !system_config.workspaces.is_empty() {
        return None;
    }
    let mut workspace = ManifestWorkspaceConfig {
        container: system_config.container.clone(),
        working_dir: system_config.working_dir.clone(),
        mounts: system_config.mounts.clone(),
        user: system_config.user.clone(),
        home: system_config.home.clone(),
        isolation: system_config.isolation.clone(),
    };
    if workspace.container.is_none() {
        workspace.container = default_workspace_container(containers);
    }
    workspace.container.as_ref()?;
    Some(workspace)
}

fn merge_workspace_config(
    system_config: &crate::config_sections::ManifestSystemConfig,
    workspace: &crate::config_sections::ManifestWorkspaceConfig,
) -> ManifestWorkspaceConfig {
    let mut merged = ManifestWorkspaceConfig {
        container: system_config.container.clone(),
        working_dir: system_config.working_dir.clone(),
        mounts: system_config.mounts.clone(),
        user: system_config.user.clone(),
        home: system_config.home.clone(),
        isolation: system_config.isolation.clone(),
    };
    if workspace.container.is_some() {
        merged.container = workspace.container.clone();
    }
    if workspace.working_dir.is_some() {
        merged.working_dir = workspace.working_dir.clone();
    }
    if workspace.user.is_some() {
        merged.user = workspace.user.clone();
    }
    if workspace.home.is_some() {
        merged.home = workspace.home.clone();
    }
    if !workspace.mounts.is_empty() {
        merged.mounts = workspace.mounts.clone();
    }
    merged
}

fn sole_entry_name<T>(entries: &BTreeMap<String, T>) -> Option<&String> {
    if entries.len() == 1 {
        entries.keys().next()
    } else {
        None
    }
}

fn has_implicit_default_target(containers: Option<&ManifestContainersConfig>) -> bool {
    containers.is_some_and(|containers| {
        containers.default.is_some() || sole_dev_context_container_name(containers).is_some()
    })
}

fn sole_dev_context_container_name(containers: &ManifestContainersConfig) -> Option<String> {
    let mut matches = containers
        .environments
        .iter()
        .filter(|(_, config)| config.context.as_deref() == Some("dev"))
        .map(|(name, _)| name.clone());
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_task_execution_binding, ResolvedInlineWorkspaceContainer,
        ResolvedTaskExecutionBinding, ResolvedWorkspaceBinding, ResolvedWorkspaceContainer,
    };
    use crate::{
        load_task_manifest_with_inspection, ManifestInlineWorkspaceContainerConfig,
        ManifestWorkspaceConfig, ManifestWorkspaceContainerRef,
    };
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn parse_manifest(text: &str) -> crate::TaskManifest {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "effigy-manifest-execution-binding-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&root).expect("mkdir root");
        let manifest_path = root.join("effigy.toml");
        fs::write(&manifest_path, text).expect("write manifest");
        load_task_manifest_with_inspection(&manifest_path)
            .expect("parse manifest")
            .manifest
    }

    #[test]
    fn resolves_workspace_binding_from_defaults() {
        let manifest = parse_manifest(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "app"
working_dir = "/workspace"

[tasks.dev]
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let resolved = resolve_task_execution_binding(&manifest, "dev", task).expect("resolve");

        assert_eq!(
            resolved,
            Some(ResolvedTaskExecutionBinding::Workspace(Box::new(
                ResolvedWorkspaceBinding {
                    system: "dev".to_owned(),
                    workspace: "app".to_owned(),
                    working_dir: Some("/workspace".to_owned()),
                    workspace_config: ManifestWorkspaceConfig {
                        container: Some(ManifestWorkspaceContainerRef::Named("app".to_owned())),
                        working_dir: Some("/workspace".to_owned()),
                        mounts: vec![],
                        user: None,
                        home: None,
                        isolation: vec![],
                    },
                    container: Some(ResolvedWorkspaceContainer::Named("app".to_owned())),
                }
            )))
        );
    }

    #[test]
    fn resolves_workspace_binding_from_sole_system_and_workspace_without_defaults() {
        let manifest = parse_manifest(
            r#"
[containers.app]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"

[systems.dev.workspaces.app]
container = "app"
working_dir = "/workspace"

[tasks.dev]
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let resolved = resolve_task_execution_binding(&manifest, "dev", task).expect("resolve");

        assert_eq!(
            resolved,
            Some(ResolvedTaskExecutionBinding::Workspace(Box::new(
                ResolvedWorkspaceBinding {
                    system: "dev".to_owned(),
                    workspace: "app".to_owned(),
                    working_dir: Some("/workspace".to_owned()),
                    workspace_config: ManifestWorkspaceConfig {
                        container: Some(ManifestWorkspaceContainerRef::Named("app".to_owned())),
                        working_dir: Some("/workspace".to_owned()),
                        mounts: vec![],
                        user: None,
                        home: None,
                        isolation: vec![],
                    },
                    container: Some(ResolvedWorkspaceContainer::Named("app".to_owned())),
                }
            )))
        );
    }

    #[test]
    fn resolves_workspace_binding_from_sole_container_when_workspace_container_is_omitted() {
        let manifest = parse_manifest(
            r#"
[containers.app]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"

[systems.dev.workspaces.app]
working_dir = "/workspace"

[tasks.dev]
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let resolved = resolve_task_execution_binding(&manifest, "dev", task).expect("resolve");

        assert_eq!(
            resolved,
            Some(ResolvedTaskExecutionBinding::Workspace(Box::new(
                ResolvedWorkspaceBinding {
                    system: "dev".to_owned(),
                    workspace: "app".to_owned(),
                    working_dir: Some("/workspace".to_owned()),
                    workspace_config: ManifestWorkspaceConfig {
                        container: None,
                        working_dir: Some("/workspace".to_owned()),
                        mounts: vec![],
                        user: None,
                        home: None,
                        isolation: vec![],
                    },
                    container: Some(ResolvedWorkspaceContainer::Named("app".to_owned())),
                }
            )))
        );
    }

    #[test]
    fn resolves_workspace_binding_from_implied_default_workspace() {
        let manifest = parse_manifest(
            r#"
[containers.app]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"

[systems.dev]

[tasks.dev]
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let resolved = resolve_task_execution_binding(&manifest, "dev", task).expect("resolve");

        assert_eq!(
            resolved,
            Some(ResolvedTaskExecutionBinding::Workspace(Box::new(
                ResolvedWorkspaceBinding {
                    system: "dev".to_owned(),
                    workspace: "default".to_owned(),
                    working_dir: None,
                    workspace_config: ManifestWorkspaceConfig {
                        container: Some(ManifestWorkspaceContainerRef::Named("app".to_owned())),
                        working_dir: None,
                        mounts: vec![],
                        user: None,
                        home: None,
                        isolation: vec![],
                    },
                    container: Some(ResolvedWorkspaceContainer::Named("app".to_owned())),
                }
            )))
        );
    }

    #[test]
    fn does_not_infer_sole_system_workspace_without_eligible_default_target() {
        let manifest = parse_manifest(
            r#"
[containers.release]
compose_file = "infra/release/linux/docker-compose.yml"
primary_service = "builder"

[systems.release.workspaces.linux]
container = "release"

[tasks.bootstrap]
run = "cargo install --path ."
"#,
        );
        let task = manifest.tasks.get("bootstrap").expect("task");
        let resolved =
            resolve_task_execution_binding(&manifest, "bootstrap", task).expect("resolve");

        assert_eq!(resolved, None);
    }

    #[test]
    fn resolves_inline_workspace_container_to_synthetic_identity() {
        let manifest = parse_manifest(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace", shell = "bash" }

[tasks.dev]
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let resolved = resolve_task_execution_binding(&manifest, "dev", task).expect("resolve");

        assert_eq!(
            resolved,
            Some(ResolvedTaskExecutionBinding::Workspace(Box::new(
                ResolvedWorkspaceBinding {
                    system: "dev".to_owned(),
                    workspace: "app".to_owned(),
                    working_dir: None,
                    workspace_config: ManifestWorkspaceConfig {
                        container: Some(ManifestWorkspaceContainerRef::Inline(
                            ManifestInlineWorkspaceContainerConfig {
                                image: Some("node:22".to_owned()),
                                mount: Some("./:/workspace".to_owned()),
                                extra: [(
                                    "shell".to_owned(),
                                    toml::Value::String("bash".to_owned())
                                )]
                                .into_iter()
                                .collect(),
                            }
                        )),
                        working_dir: None,
                        mounts: vec![],
                        user: None,
                        home: None,
                        isolation: vec![],
                    },
                    container: Some(ResolvedWorkspaceContainer::Inline(
                        ResolvedInlineWorkspaceContainer {
                            synthetic_name: "dev__app".to_owned(),
                            image: Some("node:22".to_owned()),
                            mount: Some("./:/workspace".to_owned()),
                        }
                    )),
                }
            )))
        );
    }

    #[test]
    fn errors_when_workspace_cannot_resolve_system() {
        let manifest = parse_manifest(
            r#"
[tasks.dev]
workspace = "app"
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let error = resolve_task_execution_binding(&manifest, "dev", task).expect_err("error");

        assert!(error
            .to_string()
            .contains("sets `workspace` but no task `system` or `[systems].default` is defined"));
    }

    #[test]
    fn errors_when_host_and_workspace_bindings_mix() {
        let manifest = parse_manifest(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "app"

[tasks.dev]
run_in = "host"
workspace = "app"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let error = resolve_task_execution_binding(&manifest, "dev", task).expect_err("error");

        assert!(error.to_string().contains(
            "cannot combine `run_in = \"host\"` with container or workspace execution binding"
        ));
    }

    #[test]
    fn manifest_task_defaults_run_in_applies_to_tasks_without_explicit_override() {
        let manifest = parse_manifest(
            r#"
[task_defaults]
run_in = "host"

[tasks.dev]
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let resolved = resolve_task_execution_binding(&manifest, "dev", task).expect("resolve");

        assert_eq!(resolved, Some(ResolvedTaskExecutionBinding::Host));
    }

    #[test]
    fn task_run_in_overrides_manifest_task_default() {
        let manifest = parse_manifest(
            r#"
[task_defaults]
run_in = "host"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "app"

[tasks.dev]
run_in = "either"
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let resolved = resolve_task_execution_binding(&manifest, "dev", task).expect("resolve");

        assert!(matches!(
            resolved,
            Some(ResolvedTaskExecutionBinding::Workspace(_))
        ));
    }

    #[test]
    fn container_lifecycle_beats_manifest_task_default_run_in_host() {
        let manifest = parse_manifest(
            r#"
[task_defaults]
run_in = "host"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "app"

[tasks.dev]
container_lifecycle = true
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let resolved = resolve_task_execution_binding(&manifest, "dev", task).expect("resolve");

        assert!(matches!(
            resolved,
            Some(ResolvedTaskExecutionBinding::Workspace(_))
        ));
    }

    #[test]
    fn explicit_host_run_in_still_beats_container_lifecycle() {
        let manifest = parse_manifest(
            r#"
[task_defaults]
run_in = "either"

[tasks.dev]
container_lifecycle = true
run_in = "host"
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let resolved = resolve_task_execution_binding(&manifest, "dev", task).expect("resolve");

        assert_eq!(resolved, Some(ResolvedTaskExecutionBinding::Host));
    }
}
