use std::path::Path;

use super::RunnerError;

#[test]
fn task_invocation_constructor_preserves_message() {
    let err = RunnerError::task_invocation("message contract");
    match err {
        RunnerError::TaskInvocation(message) => assert_eq!(message, "message contract"),
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn task_invocation_path_message_constructors_are_stable() {
    let path = Path::new("/tmp/effigy.toml");
    let read = RunnerError::task_invocation_failed_read(path, "read-failed");
    let parse = RunnerError::task_invocation_failed_parse(path, "parse-failed");
    let write = RunnerError::task_invocation_failed_write(path, "write-failed");
    let render = RunnerError::task_invocation_failed_render(path, "render-failed");

    match read {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(message, "failed to read /tmp/effigy.toml: read-failed")
        }
        other => panic!("unexpected error variant: {other}"),
    }
    match parse {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(message, "failed to parse /tmp/effigy.toml: parse-failed")
        }
        other => panic!("unexpected error variant: {other}"),
    }
    match write {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(message, "failed to write /tmp/effigy.toml: write-failed")
        }
        other => panic!("unexpected error variant: {other}"),
    }
    match render {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(message, "failed to render /tmp/effigy.toml: render-failed")
        }
        other => panic!("unexpected error variant: {other}"),
    }
}
