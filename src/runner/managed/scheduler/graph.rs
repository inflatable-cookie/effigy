#[path = "graph/cycle.rs"]
mod cycle;
#[path = "graph/dependencies.rs"]
mod dependencies;
#[path = "graph/index.rs"]
mod index;
#[path = "graph/topological.rs"]
mod topological;

use super::super::super::{ManifestManagedRunStep, RunnerError};

use cycle::detect_dependency_cycle;
use dependencies::{build_step_dependencies, build_step_dependents};
use index::build_step_index;
use topological::build_schedule_levels;

pub(super) fn build_run_sequence_schedule(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
) -> Result<Option<Vec<Vec<usize>>>, RunnerError> {
    let step_index = build_step_index(task_name, steps)?;
    if !step_index.has_explicit_dependencies {
        return Ok(None);
    }

    let dependencies = build_step_dependencies(task_name, steps, &step_index.id_to_index)?;
    let dependents = build_step_dependents(&dependencies);

    if let Some(cycle) = detect_dependency_cycle(&dependencies, &step_index.display_names) {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` run sequence contains dependency cycle: {}",
            cycle.join(" -> ")
        )));
    }

    let Some(levels) = build_schedule_levels(steps.len(), &dependencies, &dependents) else {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` run sequence contains dependency cycle"
        )));
    };

    Ok(Some(levels))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::super::manifest::ManifestManagedRunStepTable;

    fn command_step(run: &str) -> ManifestManagedRunStep {
        ManifestManagedRunStep::Command(run.to_owned())
    }

    fn table_step(id: Option<&str>, depends_on: &[&str]) -> ManifestManagedRunStep {
        ManifestManagedRunStep::Step(ManifestManagedRunStepTable {
            run: Some("printf ok".to_owned()),
            task: None,
            env: None,
            env_file: None,
            id: id.map(str::to_owned),
            depends_on: depends_on.iter().map(|value| value.to_string()).collect(),
            timeout_ms: None,
            retry: None,
            retry_delay_ms: None,
            fail_fast: None,
        })
    }

    #[test]
    fn schedule_returns_none_without_explicit_dependencies() {
        let steps = vec![command_step("printf first"), command_step("printf second")];
        let schedule = build_run_sequence_schedule("dev", &steps).expect("schedule");
        assert!(schedule.is_none());
    }

    #[test]
    fn schedule_builds_parallel_levels_for_explicit_dag() {
        let steps = vec![
            table_step(Some("a"), &[]),
            table_step(Some("b"), &["a"]),
            table_step(Some("c"), &["a"]),
            table_step(Some("d"), &["b", "c"]),
        ];

        let schedule = build_run_sequence_schedule("dev", &steps)
            .expect("schedule")
            .expect("explicit dependencies should produce levels");
        assert_eq!(schedule, vec![vec![0], vec![1, 2], vec![3]]);
    }

    #[test]
    fn schedule_errors_on_duplicate_step_id() {
        let steps = vec![table_step(Some("dup"), &[]), table_step(Some("dup"), &[])];
        let err = build_run_sequence_schedule("dev", &steps).expect_err("duplicate id error");
        match err {
            RunnerError::TaskInvocation(message) => {
                assert!(message.contains("duplicate step id `dup`"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn schedule_errors_on_dependency_cycle_with_named_path() {
        let steps = vec![
            table_step(Some("a"), &["b"]),
            table_step(Some("b"), &["a"]),
        ];

        let err = build_run_sequence_schedule("dev", &steps).expect_err("cycle error");
        match err {
            RunnerError::TaskInvocation(message) => {
                assert!(message.contains("dependency cycle"));
                assert!(message.contains("a -> b -> a"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
