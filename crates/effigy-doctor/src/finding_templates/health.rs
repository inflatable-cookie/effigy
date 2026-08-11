use crate::contracts::{check_id, remediation};
use crate::DoctorState;

pub(crate) enum HealthFinding {
    DiscoveryMissing,
    DiscoveryFound { catalogs: String },
    HeavyAggregate { evidence: String },
    ExecutionSuccess { summary: String },
    ExecutionFailure { evidence: String },
}

impl HealthFinding {
    pub(crate) fn discovery_missing() -> Self {
        Self::DiscoveryMissing
    }

    pub(crate) fn discovery_found(catalogs: &[String]) -> Self {
        Self::DiscoveryFound {
            catalogs: catalogs.join(", "),
        }
    }

    pub(crate) fn execution_success(summary: String) -> Self {
        Self::ExecutionSuccess { summary }
    }

    pub(crate) fn heavy_aggregate(evidence: String) -> Self {
        Self::HeavyAggregate { evidence }
    }

    pub(crate) fn execution_failure(evidence: String) -> Self {
        Self::ExecutionFailure { evidence }
    }

    pub(crate) fn emit(self, state: &mut DoctorState) {
        match self {
            Self::DiscoveryMissing => {
                state.add_check_fixable_warning(
                    check_id::HEALTH_TASK_DISCOVERY,
                    "no `health` task found in effective catalogs",
                    remediation::DEFINE_HEALTH_TASK,
                );
            }
            Self::DiscoveryFound { catalogs } => {
                state.add_check_info(
                    check_id::HEALTH_TASK_DISCOVERY,
                    format!("discovered `health` task in: {catalogs}"),
                    remediation::NO_ACTION_REQUIRED,
                );
            }
            Self::HeavyAggregate { evidence } => {
                state.add_check_warning(
                    check_id::HEALTH_TASK_POSTURE,
                    format!("`tasks.health` reaches heavy validation: {evidence}"),
                    remediation::KEEP_HEALTH_CHEAP,
                );
            }
            Self::ExecutionSuccess { summary } => {
                state.add_check_info(
                    check_id::HEALTH_TASK_EXECUTE,
                    summary,
                    remediation::NO_ACTION_REQUIRED,
                );
            }
            Self::ExecutionFailure { evidence } => {
                state.add_check_error(
                    check_id::HEALTH_TASK_EXECUTE,
                    evidence,
                    remediation::FIX_HEALTH_TASK_FAILURES,
                );
            }
        }
    }
}
