use std::path::Path;

use effigy_containers::EffectiveContainerPolicy;
use effigy_runtime_plan::{RuntimeActivationPlan, RuntimeLeasePolicy};

use crate::runner::error::RunnerError;

pub(super) fn refresh_runtime_lease_stage(
    plan: &RuntimeActivationPlan,
    policy: &EffectiveContainerPolicy,
    system_was_running: bool,
    refresh_host_container_lease: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        bool,
    ) -> Result<bool, RunnerError>,
) -> Result<bool, RunnerError> {
    if plan.lease.policy != RuntimeLeasePolicy::RefreshOnActivation {
        return Ok(false);
    }
    refresh_host_container_lease(plan.request.repo_root.as_path(), policy, system_was_running)
}
