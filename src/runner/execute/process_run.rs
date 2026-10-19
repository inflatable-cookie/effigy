use super::context::ExecutionTaskContext;
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{run_plain, EnvOverride, PlainCommand, TaskRun};
use effigy_env::secret::SecretString;
use effigy_kernel::outcome::WorkloadOutcome;

/// A plain host command under kernel custody. Text and JSON run the same
/// way; JSON only captures stdout and stderr for its envelope (K11).
pub(super) fn run_task_process(
    output_json: bool,
    verbose_root: bool,
    context: &ExecutionTaskContext<'_>,
    secret_env: Option<&[(&str, &SecretString)]>,
    run: &mut TaskRun,
    workload: &mut Option<WorkloadOutcome>,
) -> Result<String, RunnerError> {
    let env = secret_env
        .unwrap_or_default()
        .iter()
        .map(|(key, secret)| {
            (
                (*key).to_owned(),
                EnvOverride::Secret(secret.expose().to_owned()),
            )
        })
        .collect();
    let exit = run_plain(
        run,
        PlainCommand {
            command: context.command(),
            cwd: context.repo_for_task(),
            env,
            capture: output_json,
        },
    )?;
    *workload = Some(exit.workload.clone());
    if output_json {
        let stdout = String::from_utf8_lossy(&exit.stdout);
        let stderr = String::from_utf8_lossy(&exit.stderr);
        let rendered = super::json_payload::render_task_command_json(
            context.selector,
            context.repo_for_task(),
            context.command(),
            Some(exit.code),
            &stdout,
            &stderr,
        )?;
        if exit.workload == WorkloadOutcome::Succeeded {
            return Ok(rendered);
        }
        return Err(RunnerError::CommandJsonFailure { rendered });
    }
    if exit.workload == WorkloadOutcome::Succeeded {
        if verbose_root {
            return Ok(context.render_resolution_trace());
        }
        return Ok(String::new());
    }
    Err(RunnerError::TaskCommandFailure {
        command: context.command().to_owned(),
        code: Some(exit.code),
        stdout: String::new(),
        stderr: String::new(),
    })
}
