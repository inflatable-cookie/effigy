use crate::runner::error::RunnerError;
use effigy_containers::{
    load_container_policy_with_workspace, load_inline_workspace_container_policy,
    resolve_inline_workspace_exec_working_dir, EffectiveContainerPolicy,
};
use effigy_execution::{
    ExecutionBindingInput, ExecutionBindingKind as SharedExecutionBindingKind, ExecutionBindingPlan,
};
use effigy_manifest::{
    resolve_task_execution_binding_from_parts, ManifestContainersConfig,
    ManifestInlineWorkspaceContainerConfig, ManifestSystemsConfig, ManifestTask,
    ManifestWorkspaceConfig, ResolvedTaskExecutionBinding, ResolvedWorkspaceContainer,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(in crate::runner) enum ContainerExecutionBinding {
    None,
    Host,
    Container {
        name: Option<String>,
        workspace: Option<ManifestWorkspaceConfig>,
    },
    Inline {
        synthetic_name: String,
        container: ManifestInlineWorkspaceContainerConfig,
        workdir: Option<String>,
    },
}

impl ContainerExecutionBinding {
    pub(in crate::runner) fn container_name(&self) -> Option<&str> {
        match self {
            Self::Container { name, .. } => name.as_deref(),
            Self::Inline { .. } | Self::None | Self::Host => None,
        }
    }

    pub(in crate::runner) fn requested_container_name(&self) -> Option<Option<&str>> {
        match self {
            Self::Container { name, .. } => Some(name.as_deref()),
            Self::Inline { .. } | Self::None | Self::Host => None,
        }
    }

    pub(in crate::runner) fn load_effective_policy(
        &self,
        repo_root: &Path,
    ) -> Result<Option<EffectiveContainerPolicy>, RunnerError> {
        match self {
            Self::Container { name, workspace } => {
                load_container_policy_with_workspace(repo_root, name.as_deref(), workspace.as_ref())
                    .map(Some)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))
            }
            Self::Inline {
                synthetic_name,
                container,
                workdir,
            } => load_inline_workspace_container_policy(
                repo_root,
                synthetic_name,
                container,
                workdir.as_deref(),
            )
            .map(Some)
            .map_err(|error| RunnerError::task_invocation(error.to_string())),
            Self::None | Self::Host => Ok(None),
        }
    }

    pub(in crate::runner) fn exec_working_dir(
        &self,
        repo_root: &Path,
    ) -> Result<Option<PathBuf>, RunnerError> {
        match self {
            Self::Container { name, .. } => {
                effigy_containers::load_container_exec_working_dir(repo_root, name.as_deref())
                    .map(Some)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))
            }
            Self::Inline {
                synthetic_name,
                container,
                workdir,
            } => resolve_inline_workspace_exec_working_dir(
                repo_root,
                synthetic_name,
                container,
                workdir.as_deref(),
            )
            .map(Some)
            .map_err(|error| RunnerError::task_invocation(error.to_string())),
            Self::None | Self::Host => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum ExecutionBindingKind {
    None,
    Host,
    NamedContainer,
    InlineContainer,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct ExecutionBindingResolution {
    binding: ContainerExecutionBinding,
    kind: ExecutionBindingKind,
    requested_container_name: Option<String>,
}

impl ExecutionBindingResolution {
    pub(in crate::runner) fn binding(&self) -> &ContainerExecutionBinding {
        &self.binding
    }

    pub(in crate::runner) fn kind(&self) -> ExecutionBindingKind {
        self.kind
    }

    pub(in crate::runner) fn is_inline_container(&self) -> bool {
        self.kind == ExecutionBindingKind::InlineContainer
    }

    pub(in crate::runner) fn requested_container_name(&self) -> Option<&str> {
        self.requested_container_name.as_deref()
    }

    pub(in crate::runner) fn effective_policy(
        &self,
        repo_root: &Path,
    ) -> Result<Option<EffectiveContainerPolicy>, RunnerError> {
        self.binding.load_effective_policy(repo_root)
    }

    pub(in crate::runner) fn exec_working_dir(
        &self,
        repo_root: &Path,
    ) -> Result<Option<PathBuf>, RunnerError> {
        self.binding.exec_working_dir(repo_root)
    }

    pub(in crate::runner) fn plan(&self, input: ExecutionBindingInput) -> ExecutionBindingPlan {
        ExecutionBindingPlan::new(
            input,
            match self.kind {
                ExecutionBindingKind::None => SharedExecutionBindingKind::None,
                ExecutionBindingKind::Host => SharedExecutionBindingKind::Host,
                ExecutionBindingKind::NamedContainer => SharedExecutionBindingKind::NamedContainer,
                ExecutionBindingKind::InlineContainer => {
                    SharedExecutionBindingKind::InlineContainer
                }
            },
            self.requested_container_name.clone(),
            self.is_inline_container(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum InlineWorkspaceCapabilitySurface<'a> {
    StandardTaskRouting { task_name: &'a str },
    ManagedAttachedSession { task_name: &'a str },
    PublicWorkspaceCommand { surface: &'a str },
}

pub(in crate::runner) fn ensure_inline_workspace_supported(
    binding: &ContainerExecutionBinding,
    surface: InlineWorkspaceCapabilitySurface<'_>,
) -> Result<(), RunnerError> {
    if !matches!(binding, ContainerExecutionBinding::Inline { .. }) {
        return Ok(());
    }

    let message = match surface {
        InlineWorkspaceCapabilitySurface::StandardTaskRouting { task_name } => format!(
            "task `{task_name}` uses an inline workspace container, but standard task routing does not support inline workspace containers yet"
        ),
        InlineWorkspaceCapabilitySurface::ManagedAttachedSession { task_name } => format!(
            "task `{task_name}` uses an inline workspace container, but non-managed attached container sessions do not support inline workspace containers yet"
        ),
        InlineWorkspaceCapabilitySurface::PublicWorkspaceCommand { surface } => {
            format!("`effigy {surface}` does not support inline workspace containers yet")
        }
    };
    Err(RunnerError::task_invocation(message))
}

pub(in crate::runner) fn resolve_container_execution_binding(
    default_run_in: Option<effigy_manifest::ManifestTaskRunIn>,
    systems: Option<&ManifestSystemsConfig>,
    containers: Option<&ManifestContainersConfig>,
    task_name: &str,
    task: &ManifestTask,
    runtime_surface: &str,
) -> Result<ContainerExecutionBinding, RunnerError> {
    match resolve_task_execution_binding_from_parts(
        default_run_in,
        systems,
        containers,
        task_name,
        task,
    )
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    {
        Some(ResolvedTaskExecutionBinding::Workspace(binding)) => {
            match binding.container {
                Some(ResolvedWorkspaceContainer::Named(name)) => Ok(
                    ContainerExecutionBinding::Container {
                        name: Some(name),
                        workspace: Some(binding.workspace_config),
                    },
                ),
                Some(ResolvedWorkspaceContainer::Inline(inline)) => Ok(
                    ContainerExecutionBinding::Inline {
                        synthetic_name: inline.synthetic_name,
                        container: ManifestInlineWorkspaceContainerConfig {
                            image: inline.image,
                            mount: inline.mount,
                            extra: Default::default(),
                        },
                        workdir: binding.working_dir,
                    },
                ),
                None => Err(RunnerError::task_invocation(format!(
                    "task `{task_name}` uses workspace execution binding `{}.{}`, but {runtime_surface} requires that workspace to declare a backing container",
                    binding.system, binding.workspace
                ))),
            }
        }
        Some(ResolvedTaskExecutionBinding::Host) => Ok(ContainerExecutionBinding::Host),
        None => Ok(ContainerExecutionBinding::None),
    }
}

pub(in crate::runner) fn resolve_execution_binding_resolution(
    default_run_in: Option<effigy_manifest::ManifestTaskRunIn>,
    systems: Option<&ManifestSystemsConfig>,
    containers: Option<&ManifestContainersConfig>,
    task_name: &str,
    task: &ManifestTask,
    runtime_surface: &str,
) -> Result<ExecutionBindingResolution, RunnerError> {
    let binding = resolve_container_execution_binding(
        default_run_in,
        systems,
        containers,
        task_name,
        task,
        runtime_surface,
    )?;
    let kind = match &binding {
        ContainerExecutionBinding::None => ExecutionBindingKind::None,
        ContainerExecutionBinding::Host => ExecutionBindingKind::Host,
        ContainerExecutionBinding::Container { .. } => ExecutionBindingKind::NamedContainer,
        ContainerExecutionBinding::Inline { .. } => ExecutionBindingKind::InlineContainer,
    };
    let requested_container_name = binding
        .requested_container_name()
        .flatten()
        .map(str::to_owned);
    Ok(ExecutionBindingResolution {
        binding,
        kind,
        requested_container_name,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_inline_workspace_supported, resolve_execution_binding_resolution,
        ContainerExecutionBinding, ExecutionBindingKind, InlineWorkspaceCapabilitySurface,
    };
    use effigy_manifest::{ManifestInlineWorkspaceContainerConfig, ManifestTask};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_repo(manifest: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "effigy-binding-tests-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(root.join("infra/dev")).expect("mkdir repo");
        std::fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
        std::fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n")
            .expect("write compose");
        root
    }

    fn inline_binding() -> ContainerExecutionBinding {
        ContainerExecutionBinding::Inline {
            synthetic_name: "dev__app".to_owned(),
            container: ManifestInlineWorkspaceContainerConfig {
                image: Some("node:22".to_owned()),
                mount: Some("./:/workspace".to_owned()),
                extra: Default::default(),
            },
            workdir: Some("/workspace".to_owned()),
        }
    }

    #[test]
    fn standard_routing_inline_capability_error_is_stable() {
        let error = ensure_inline_workspace_supported(
            &inline_binding(),
            InlineWorkspaceCapabilitySurface::StandardTaskRouting { task_name: "build" },
        )
        .expect_err("inline binding should be rejected");

        assert!(error
            .to_string()
            .contains("standard task routing does not support inline workspace containers yet"));
    }

    #[test]
    fn managed_attached_inline_capability_error_is_stable() {
        let error = ensure_inline_workspace_supported(
            &inline_binding(),
            InlineWorkspaceCapabilitySurface::ManagedAttachedSession { task_name: "dev" },
        )
        .expect_err("inline binding should be rejected");

        assert!(error.to_string().contains(
            "non-managed attached container sessions do not support inline workspace containers yet"
        ));
    }

    #[test]
    fn public_workspace_inline_capability_error_is_stable() {
        let error = ensure_inline_workspace_supported(
            &inline_binding(),
            InlineWorkspaceCapabilitySurface::PublicWorkspaceCommand {
                surface: "workspace",
            },
        )
        .expect_err("inline binding should be rejected");

        assert_eq!(
            error.to_string(),
            "`effigy workspace` does not support inline workspace containers yet"
        );
    }

    #[test]
    fn execution_binding_resolution_materializes_inline_policy_and_workdir() {
        let root = temp_repo(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace" }
"#,
        );
        let task = ManifestTask {
            workspace: Some("app".to_owned()),
            ..Default::default()
        };
        let manifest =
            effigy_manifest::load_task_manifest(&root.join("effigy.toml")).expect("manifest");

        let resolution = resolve_execution_binding_resolution(
            None,
            manifest.systems.as_ref(),
            None,
            "build",
            &task,
            "binding test",
        )
        .expect("resolve binding");

        assert_eq!(resolution.kind(), ExecutionBindingKind::InlineContainer);
        assert!(resolution.is_inline_container());
        assert!(resolution.requested_container_name().is_none());
        assert_eq!(
            resolution
                .effective_policy(&root)
                .expect("policy lookup")
                .expect("resolved policy")
                .name,
            "dev__app"
        );
        assert_eq!(
            resolution
                .exec_working_dir(&root)
                .expect("working dir lookup")
                .expect("resolved working dir")
                .to_string_lossy(),
            "/workspace"
        );
    }
}
