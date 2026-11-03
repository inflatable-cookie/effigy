use crate::tests::prelude::{parse_command, Command, TaskInvocation};

/// A generic selector keeps its own flags in order behind the task-args
/// boundary; this is what makes deleting the retired `repo-pulse` cases safe.
#[test]
fn parse_runtime_task_passthrough() {
    let cmd = parse_command(vec![
        "snapshot".to_owned(),
        "--json".to_owned(),
        "--repo".to_owned(),
        ".".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Task(TaskInvocation {
            name: "snapshot".to_owned(),
            args: vec![
                effigy_core::task_args::TASK_ARGS_BOUNDARY.to_owned(),
                "--json".to_owned(),
                "--repo".to_owned(),
                ".".to_owned(),
            ],
        })
    );
}
