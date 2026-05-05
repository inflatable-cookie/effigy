use crate::runner::entrypoints::run_command_with_context;
use crate::runner::tests::prelude::{assert_file_text_equals, temp_workspace, write_root_manifest};
use effigy_cli::{Command, TaskInvocation};
use effigy_context::EffigyRuntimeContext;

#[test]
fn direct_task_dispatch_runs_through_execution_request_boundary() {
    let root = temp_workspace("direct-task-execution-request");
    let marker = root.join("direct-task.out");
    write_root_manifest(
        &root,
        &format!(
            "[tasks.echo]\nrun = \"printf direct-request > '{}'\"\n",
            marker.display()
        ),
    );
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    run_command_with_context(
        Command::Task(TaskInvocation {
            name: "echo".to_owned(),
            args: Vec::new(),
        }),
        &context,
    )
    .expect("direct task");

    assert_file_text_equals(&marker, "direct-request");
}
