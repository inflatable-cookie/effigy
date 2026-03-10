use std::collections::BTreeMap;

use crate::runner::error::RunnerError;
use crate::runner::manifest::task_runtime::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedRunStep, ManifestRunStepEnv,
};
use crate::runner::model::catalog::LoadedCatalog;

use super::RunSpecContext;
use env_resolution::StepEnvAccumulator;
use projection::project_run_sequence;
use rendering::render_projected_run_sequence;

mod dotenv;
mod env_files;
mod env_resolution;
mod pathing;
mod projection;
mod rendering;

pub(super) fn render_run_sequence(
    steps: &[ManifestManagedRunStep],
    context: RunSpecContext<'_>,
) -> Result<String, RunnerError> {
    if steps.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "task `{}` has an empty run array",
            context.task_name
        )));
    }

    let projected = project_run_sequence(steps, context)?;
    render_projected_run_sequence(context.task_name, steps, &projected)
}

pub(super) fn resolve_standalone_env(
    owner_label: &str,
    env: Option<&ManifestRunStepEnv>,
    env_file: Option<&ManifestEnvFileDirective>,
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
    repo_root: &std::path::Path,
    catalogs: &[LoadedCatalog],
    runtime_env_schema_override: Option<&std::path::Path>,
) -> Result<BTreeMap<String, String>, RunnerError> {
    StepEnvAccumulator::resolve_standalone_env(
        owner_label,
        env,
        env_file,
        env_profiles,
        repo_root,
        catalogs,
        runtime_env_schema_override,
    )
}
