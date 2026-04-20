use crate::runner::error::RunnerError;
use effigy_containers::{
    load_container_policy, load_inline_workspace_container_policy,
    resolve_inline_workspace_exec_working_dir, EffectiveContainerPolicy,
};
use effigy_manifest::{
    resolve_task_execution_binding_from_systems, ManifestInlineWorkspaceContainerConfig,
    ManifestSystemsConfig, ManifestTask, ResolvedTaskExecutionBinding, ResolvedWorkspaceContainer,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(in crate::runner) enum ContainerExecutionBinding {
    None,
    Host,
    Container {
        name: Option<String>,
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
            Self::Container { name } => name.as_deref(),
            Self::Inline { .. } | Self::None | Self::Host => None,
        }
    }

    pub(in crate::runner) fn requested_container_name(&self) -> Option<Option<&str>> {
        match self {
            Self::Container { name } => Some(name.as_deref()),
            Self::Inline { .. } | Self::None | Self::Host => None,
        }
    }

    pub(in crate::runner) fn load_effective_policy(
        &self,
        repo_root: &Path,
    ) -> Result<Option<EffectiveContainerPolicy>, RunnerError> {
        match self {
            Self::Container { name } => load_container_policy(repo_root, name.as_deref())
                .map(Some)
                .map_err(|error| RunnerError::task_invocation(error.to_string())),
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
            Self::Container { name } => {
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

pub(in crate::runner) fn resolve_container_execution_binding(
    systems: Option<&ManifestSystemsConfig>,
    task_name: &str,
    task: &ManifestTask,
    runtime_surface: &str,
) -> Result<ContainerExecutionBinding, RunnerError> {
    match resolve_task_execution_binding_from_systems(systems, task_name, task)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    {
        Some(ResolvedTaskExecutionBinding::Workspace(binding)) => {
            match binding.container {
                Some(ResolvedWorkspaceContainer::Named(name)) => {
                    Ok(ContainerExecutionBinding::Container { name: Some(name) })
                }
                Some(ResolvedWorkspaceContainer::Inline(inline)) => Ok(
                    ContainerExecutionBinding::Inline {
                        synthetic_name: inline.synthetic_name,
                        container: ManifestInlineWorkspaceContainerConfig {
                            image: inline.image,
                            mount: inline.mount,
                            extra: Default::default(),
                        },
                        workdir: binding.workdir,
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
