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

pub(in crate::runner) use binding::{
    resolve_container_execution_binding, ContainerExecutionBinding,
};
pub(super) use describe::{catalog_task_label, task_run_preview};
pub(super) use entry::{run_manifest_task, run_manifest_task_with_cwd};
