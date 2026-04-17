use super::build_run_sequence_schedule;
use crate::ManagedError;
use effigy_manifest::ManifestManagedRunStep;
use effigy_manifest::ManifestManagedRunStepTable;

fn command_step(run: &str) -> ManifestManagedRunStep {
    ManifestManagedRunStep::Command(run.to_owned())
}

fn table_step(id: Option<&str>, depends_on: &[&str]) -> ManifestManagedRunStep {
    ManifestManagedRunStep::Step(Box::new(ManifestManagedRunStepTable {
        run: Some("printf ok".to_owned()),
        task: None,
        rhai: None,
        env: None,
        env_file: None,
        id: id.map(str::to_owned),
        depends_on: depends_on.iter().map(|value| value.to_string()).collect(),
        timeout_ms: None,
        retry: None,
        retry_delay_ms: None,
        fail_fast: None,
    }))
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
        ManagedError::TaskInvocation(message) => {
            assert!(message.contains("duplicate step id `dup`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn schedule_errors_on_dependency_cycle_with_named_path() {
    let steps = vec![table_step(Some("a"), &["b"]), table_step(Some("b"), &["a"])];

    let err = build_run_sequence_schedule("dev", &steps).expect_err("cycle error");
    match err {
        ManagedError::TaskInvocation(message) => {
            assert!(message.contains("dependency cycle"));
            assert!(message.contains("a -> b -> a"));
        }
        other => panic!("unexpected error: {other}"),
    }
}
