use std::path::Path;

use effigy_cli::TaskInvocation;

use crate::{DoctorError, DoctorRuntimePorts};

pub(super) fn run_health_task_json(
    resolved_root: &Path,
    ports: &dyn DoctorRuntimePorts,
) -> Result<String, DoctorError> {
    let invocation = TaskInvocation {
        name: "health".to_owned(),
        args: vec!["--json".to_owned()],
    };
    ports.run_manifest_task(&invocation, resolved_root.to_path_buf())
}
