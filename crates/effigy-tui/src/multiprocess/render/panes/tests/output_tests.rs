use std::time::Duration;

use crate::core::{LogEntry, LogEntryKind};

use super::super::output::{output_lines, waiting_for_output_lines};

#[test]
fn output_lines_include_runtime_meta_for_non_shell_processes() {
    let logs = vec![
        LogEntry {
            kind: LogEntryKind::Stdout,
            line: "hello".to_owned(),
        },
        LogEntry {
            kind: LogEntryKind::Stderr,
            line: "oops".to_owned(),
        },
        LogEntry {
            kind: LogEntryKind::Exit,
            line: "code=1".to_owned(),
        },
    ];

    let lines = output_lines(&logs, false, Duration::from_secs(2), 0);
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].spans[0].content.as_ref(), "started: ");
    assert_eq!(lines[2].spans[0].content.as_ref(), "[stderr] ");
    assert_eq!(lines[3].spans[0].content.as_ref(), "[exit] ");
}

#[test]
fn output_lines_skip_runtime_meta_for_shell_processes() {
    let logs = vec![LogEntry {
        kind: LogEntryKind::Stdout,
        line: "shell-output".to_owned(),
    }];

    let lines = output_lines(&logs, true, Duration::from_secs(3), 1);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].spans[0].content.as_ref(), "shell-output");
}

#[test]
fn waiting_for_output_lines_uses_spinner_frame_and_message() {
    let lines = waiting_for_output_lines(3, Duration::from_secs(1), 0);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].spans[0].content.as_ref(), "started: ");
    assert_eq!(lines[1].spans[0].content.as_ref(), "⠸");
    assert_eq!(
        lines[1].spans[1].content.as_ref(),
        " waiting for first output..."
    );
}
