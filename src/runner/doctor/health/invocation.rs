use std::path::Path;

use effigy_cli::TaskInvocation;

use super::super::super::execute::run_manifest_task_with_cwd;
use crate::runner::error::RunnerError;

pub(super) fn run_health_task_json(resolved_root: &Path) -> Result<String, RunnerError> {
    let invocation = TaskInvocation {
        name: "health".to_owned(),
        args: vec!["--json".to_owned()],
    };
    run_manifest_task_with_cwd(&invocation, resolved_root.to_path_buf())
}
