use std::collections::BTreeMap;

use crate::ManagedError;
use effigy_manifest::LoadedCatalog;
use effigy_manifest::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedRunStep, ManifestRunStepEnv,
};

use super::{RenderedRun, RunSpecContext};
use projection::project_run_sequence;

mod dotenv;
mod env_files;
mod env_resolution;
mod pathing;
mod projection;

pub use env_resolution::StepEnvAccumulator;

pub fn render_run_sequence(
    steps: &[ManifestManagedRunStep],
    context: RunSpecContext<'_>,
) -> Result<RenderedRun, ManagedError> {
    if steps.is_empty() {
        return Err(ManagedError::task_invocation(format!(
            "task `{}` has an empty run array",
            context.task_name
        )));
    }

    let projected = project_run_sequence(steps, context)?;
    Ok(RenderedRun {
        env: projected.env,
        command: projected
            .commands
            .iter()
            .zip(&projected.timeouts)
            .map(|(command, timeout)| render_timed_step(command, *timeout))
            .collect::<Vec<_>>()
            .join(" && "),
    })
}

/// Renders one projected step. A step without a declared timeout keeps its
/// command; a timed step is wrapped in a `python3` timeout shim, the rendering
/// the container-only sequence path has always produced until the remote
/// custody cutover (#236) replaces it.
fn render_timed_step(command: &str, timeout_ms: Option<u64>) -> String {
    match timeout_ms {
        None => command.to_owned(),
        Some(ms) => format!(
            "python3 -c 'import subprocess,sys\ntry:\n r=subprocess.run([\"sh\",\"-lc\",sys.argv[2]], timeout=float(sys.argv[1]))\n sys.exit(r.returncode)\nexcept subprocess.TimeoutExpired:\n sys.exit(124)' {} {}",
            (ms as f64) / 1000.0,
            effigy_core::shell::shell_quote(command)
        ),
    }
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
