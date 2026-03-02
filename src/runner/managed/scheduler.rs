#[path = "scheduler/graph.rs"]
mod graph;
#[path = "scheduler/script.rs"]
mod script;

use super::super::ManifestManagedRunStep;

const DEFAULT_DAG_MAX_PARALLEL: usize = 4;

#[derive(Clone, Copy)]
pub(super) struct RunStepPolicy {
    timeout_ms: Option<u64>,
    retry: usize,
    retry_delay_ms: u64,
    fail_fast: bool,
}

impl Default for RunStepPolicy {
    fn default() -> Self {
        Self {
            timeout_ms: None,
            retry: 0,
            retry_delay_ms: 0,
            fail_fast: true,
        }
    }
}

impl RunStepPolicy {
    pub(super) fn is_default(self) -> bool {
        self.timeout_ms.is_none() && self.retry == 0 && self.retry_delay_ms == 0 && self.fail_fast
    }
}

pub(super) fn build_run_sequence_schedule(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
) -> Result<Option<Vec<Vec<usize>>>, super::super::RunnerError> {
    graph::build_run_sequence_schedule(task_name, steps)
}

pub(super) fn render_parallel_run_levels_with_policy(
    commands: &[String],
    levels: &[Vec<usize>],
    policies: &[RunStepPolicy],
) -> String {
    script::render_parallel_run_levels_with_policy(commands, levels, policies)
}

pub(super) fn step_policy_for(step: &ManifestManagedRunStep) -> RunStepPolicy {
    match step {
        ManifestManagedRunStep::Command(_) => RunStepPolicy::default(),
        ManifestManagedRunStep::Step(table) => RunStepPolicy {
            timeout_ms: table.timeout_ms,
            retry: table.retry.unwrap_or(0),
            retry_delay_ms: table.retry_delay_ms.unwrap_or(0),
            fail_fast: table.fail_fast.unwrap_or(true),
        },
    }
}
