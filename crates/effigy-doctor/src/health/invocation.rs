use std::path::Path;
use std::time::Duration;

use effigy_cli::TaskInvocation;

use crate::{DoctorError, DoctorRuntimePorts};

pub(super) fn run_health_task_json(
    resolved_root: &Path,
    ports: &dyn DoctorRuntimePorts,
    remaining_budget: Option<Duration>,
) -> Result<String, DoctorError> {
    let invocation = TaskInvocation {
        name: "health".to_owned(),
        args: vec!["--json".to_owned()],
    };
    ports.run_manifest_task_bounded(&invocation, resolved_root.to_path_buf(), remaining_budget)
}
