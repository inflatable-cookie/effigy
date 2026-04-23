#[path = "execute/binding.rs"]
mod binding;
#[path = "execute/cache_hit.rs"]
mod cache_hit;
#[path = "execute/context.rs"]
mod context;
#[path = "execute/describe.rs"]
mod describe;
#[path = "execute/entry.rs"]
mod entry;
#[path = "execute/json_payload.rs"]
mod json_payload;
#[path = "execute/pipeline.rs"]
mod pipeline;
#[path = "execute/preflight.rs"]
mod preflight;
#[path = "execute/process.rs"]
mod process;
#[path = "execute/process_run.rs"]
mod process_run;
#[path = "execute/routing.rs"]
mod routing;
#[path = "execute/selection.rs"]
mod selection;
#[path = "execute/sequence_run.rs"]
mod sequence_run;

use std::path::PathBuf;

use effigy_cli::TaskInvocation;
use effigy_manifest::{ManifestManagedRun, ManifestTask, ManifestTaskRunIn, TaskSelection};
use effigy_tasks::CatalogSelectionMode;

pub(in crate::runner) use binding::{
    resolve_container_execution_binding, ContainerExecutionBinding,
};
pub(super) use describe::{catalog_task_label, task_run_preview};
pub(super) use entry::{run_manifest_task, run_manifest_task_with_cwd};

pub(super) fn run_managed_run_with_cwd(
    run: &ManifestManagedRun,
    cwd: PathBuf,
    label: &str,
) -> Result<String, crate::runner::error::RunnerError> {
    let invocation = TaskInvocation {
        name: label.to_owned(),
        args: Vec::new(),
    };
    let preflight = preflight::build_execution_preflight(&invocation, cwd)?;
    let root_catalog = preflight
        .catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == preflight.resolved.resolved_root)
        .min_by_key(|catalog| catalog.depth)
        .ok_or_else(|| {
            crate::runner::error::RunnerError::task_invocation(format!(
                "bootstrap run could not resolve root catalog for {}",
                preflight.resolved.resolved_root.display()
            ))
        })?;

    let synthetic_task = ManifestTask {
        run: Some(run.clone()),
        run_in: Some(ManifestTaskRunIn::Host),
        ..Default::default()
    };
    let selection = TaskSelection {
        catalog: root_catalog,
        task: &synthetic_task,
        mode: CatalogSelectionMode::RootShallowest,
        evidence: vec!["bootstrap-local run".to_owned()],
    };
    pipeline::standard::run_standard_task(&preflight, &selection)
}
