use crate::runner::error::RunnerError;
use effigy_cli::DraftsArgs;

pub(in crate::runner) fn run_drafts(_args: DraftsArgs) -> Result<String, RunnerError> {
    Err(RunnerError::task_invocation("`effigy drafts` was removed in v0.15. Use `effigy tasks --all` or `effigy tasks --json`; legacy `[drafts]` definitions are translated to internal tasks for one release."))
}
