use crate::config_sections::{
    ManifestInlineWorkspaceContainerConfig, ManifestSystemsConfig, ManifestWorkspaceConfig,
    ManifestWorkspaceContainerRef,
};
use crate::{ManifestTask, TaskManifest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTaskExecutionBinding {
    Host,
    Workspace(ResolvedWorkspaceBinding),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorkspaceBinding {
    pub system: String,
    pub workspace: String,
    pub workdir: Option<String>,
    pub container: Option<ResolvedWorkspaceContainer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedWorkspaceContainer {
    Named(String),
    Inline(ResolvedInlineWorkspaceContainer),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    resolve_task_execution_binding_from_systems(manifest.systems.as_ref(), task_name, task)
}

pub fn resolve_task_execution_binding_from_systems(
    systems: Option<&ManifestSystemsConfig>,
    task_name: &str,
    task: &ManifestTask,
) -> Result<Option<ResolvedTaskExecutionBinding>, ExecutionBindingResolveError> {
    let has_workspace_binding = task.system.is_some() || task.workspace.is_some();
    if task.host.unwrap_or(false) {
        if has_workspace_binding {
            return Err(ExecutionBindingResolveError::new(format!(
                "task `{task_name}` cannot combine `host = true` with container or workspace execution binding"
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

    let resolved_system = match task.system.as_ref().or(systems.default.as_ref()) {
        Some(name) => name,
        None => {
            if task.workspace.is_some() {
                return Err(ExecutionBindingResolveError::new(format!(
                    "task `{task_name}` sets `workspace` but no task `system` or `[systems].default` is defined"
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

    let resolved_workspace = task
        .workspace
        .as_ref()
        .or(system_config.default_workspace.as_ref())
        .ok_or_else(|| {
            ExecutionBindingResolveError::new(format!(
                "task `{task_name}` resolved system `{resolved_system}`, but no task `workspace` or `[systems.{resolved_system}].default_workspace` is defined"
            ))
        })?;

    let workspace_config = system_config
        .workspaces
        .get(resolved_workspace)
        .ok_or_else(|| {
            ExecutionBindingResolveError::new(format!(
                "task `{task_name}` resolved workspace `{resolved_workspace}` in system `{resolved_system}`, but `[systems.{resolved_system}.workspaces.{resolved_workspace}]` is not defined"
            ))
        })?;

    Ok(Some(ResolvedTaskExecutionBinding::Workspace(
        ResolvedWorkspaceBinding {
            system: resolved_system.clone(),
            workspace: resolved_workspace.clone(),
            workdir: workspace_config.workdir.clone(),
            container: workspace_container(resolved_system, resolved_workspace, workspace_config),
        },
    )))
}

fn workspace_container(
    system_name: &str,
    workspace_name: &str,
    workspace: &ManifestWorkspaceConfig,
) -> Option<ResolvedWorkspaceContainer> {
    match workspace.container.as_ref()? {
        ManifestWorkspaceContainerRef::Named(name) => {
            Some(ResolvedWorkspaceContainer::Named(name.clone()))
        }
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

#[cfg(test)]
mod tests {
    use super::{
        resolve_task_execution_binding, ResolvedInlineWorkspaceContainer,
        ResolvedTaskExecutionBinding, ResolvedWorkspaceBinding, ResolvedWorkspaceContainer,
    };
    use crate::load_task_manifest_with_inspection;
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
workdir = "/workspace"

[tasks.dev]
run = "npm run dev"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let resolved = resolve_task_execution_binding(&manifest, "dev", task).expect("resolve");

        assert_eq!(
            resolved,
            Some(ResolvedTaskExecutionBinding::Workspace(
                ResolvedWorkspaceBinding {
                    system: "dev".to_owned(),
                    workspace: "app".to_owned(),
                    workdir: Some("/workspace".to_owned()),
                    container: Some(ResolvedWorkspaceContainer::Named("app".to_owned())),
                }
            ))
        );
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
            Some(ResolvedTaskExecutionBinding::Workspace(
                ResolvedWorkspaceBinding {
                    system: "dev".to_owned(),
                    workspace: "app".to_owned(),
                    workdir: None,
                    container: Some(ResolvedWorkspaceContainer::Inline(
                        ResolvedInlineWorkspaceContainer {
                            synthetic_name: "dev__app".to_owned(),
                            image: Some("node:22".to_owned()),
                            mount: Some("./:/workspace".to_owned()),
                        }
                    )),
                }
            ))
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
host = true
workspace = "app"
"#,
        );
        let task = manifest.tasks.get("dev").expect("task");
        let error = resolve_task_execution_binding(&manifest, "dev", task).expect_err("error");

        assert!(error.to_string().contains(
            "cannot combine `host = true` with container or workspace execution binding"
        ));
    }
}
