use std::collections::BTreeMap;

use crate::ManagedError;
use effigy_manifest::LoadedCatalog;
use effigy_manifest::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedRunStep, ManifestRunStepEnv,
};

use super::RunSpecContext;
use projection::project_run_sequence;
use rendering::render_projected_run_sequence;

mod dotenv;
mod env_files;
mod env_resolution;
mod pathing;
mod projection;
mod rendering;

pub use env_resolution::StepEnvAccumulator;

pub fn render_run_sequence(
    steps: &[ManifestManagedRunStep],
    context: RunSpecContext<'_>,
) -> Result<String, ManagedError> {
    if steps.is_empty() {
        return Err(ManagedError::task_invocation(format!(
            "task `{}` has an empty run array",
            context.task_name
        )));
    }

    let projected = project_run_sequence(steps, context)?;
    render_projected_run_sequence(context.task_name, steps, &projected)
}

pub fn resolve_standalone_env(
    owner_label: &str,
    env: Option<&ManifestRunStepEnv>,
    env_file: Option<&ManifestEnvFileDirective>,
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
    repo_root: &std::path::Path,
    catalogs: &[LoadedCatalog],
    runtime_env_schema_override: Option<&std::path::Path>,
) -> Result<BTreeMap<String, String>, ManagedError> {
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
