use super::super::manifest::task_runtime::{ManifestManagedRun, ManifestTask};
use effigy_manifest::{
    resolve_task_execution_binding, LoadedCatalog, ResolvedTaskExecutionBinding,
};

pub(in crate::runner) fn task_run_preview(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> String {
    if let Some(run) = task.run.as_ref() {
        return match run {
            ManifestManagedRun::Command(command) => command.clone(),
            ManifestManagedRun::Sequence(steps) => format!("<sequence:{}>", steps.len()),
        };
    }
    if let Ok(Some(binding)) = resolve_task_execution_binding(&catalog.manifest, task_name, task) {
        return match binding {
            ResolvedTaskExecutionBinding::Host => "<host>".to_owned(),
            ResolvedTaskExecutionBinding::Workspace(binding) => {
                format!("<workspace:{}:{}>", binding.system, binding.workspace)
            }
        };
    }
    if let Some(mode) = task.mode.as_ref() {
        return format!("<managed:{mode}>");
    }
    "<none>".to_owned()
}

pub(in crate::runner) fn catalog_task_label(catalog: &LoadedCatalog, task_name: &str) -> String {
    if catalog.depth == 0 {
        task_name.to_owned()
    } else {
        format!("{}/{}", catalog.alias, task_name)
    }
}
