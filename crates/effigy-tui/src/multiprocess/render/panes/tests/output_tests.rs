use std::time::Duration;

use crate::core::{LogEntry, LogEntryKind};
use crate::multiprocess::state::ProcessStartupState;

use super::super::output::{output_lines, waiting_for_output_lines};

#[test]
fn output_lines_include_runtime_meta_for_non_shell_processes() {
    let logs = vec![
        LogEntry {
            styles: None,
            kind: LogEntryKind::Stdout,
            line: "hello".to_owned(),
        },
        LogEntry {
            styles: None,
            kind: LogEntryKind::Stderr,
            line: "oops".to_owned(),
        },
        LogEntry {
            styles: None,
            kind: LogEntryKind::Exit,
            line: "code=1".to_owned(),
        },
    ];

    let lines = output_lines(&logs, false, Duration::from_secs(2), 0);
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].spans[0].content.as_ref(), "started: ");
    assert_eq!(lines[2].spans[0].content.as_ref(), "oops");
    assert_eq!(lines[3].spans[0].content.as_ref(), "[exit] ");
}

#[test]
fn output_lines_render_stderr_lines_plainly() {
    let logs = vec![
        LogEntry {
            styles: None,
            kind: LogEntryKind::Stderr,
            line: "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s"
                .to_owned(),
        },
        LogEntry {
            styles: None,
            kind: LogEntryKind::Stderr,
            line: "     Running `target/debug/acme-api`".to_owned(),
        },
    ];

    let lines = output_lines(&logs, false, Duration::from_secs(2), 0);
    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines[1].spans[0].content.as_ref(),
        "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s"
    );
    assert_eq!(
        lines[2].spans[0].content.as_ref(),
        "     Running `target/debug/acme-api`"
    );
}

#[test]
fn output_lines_skip_runtime_meta_for_shell_processes() {
    let logs = vec![LogEntry {
        styles: None,
        kind: LogEntryKind::Stdout,
        line: "shell-output".to_owned(),
    }];

    let lines = output_lines(&logs, true, Duration::from_secs(3), 1);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].spans[0].content.as_ref(), "shell-output");
}

#[test]
fn waiting_for_output_lines_uses_spinner_frame_and_message() {
    let lines = waiting_for_output_lines(
        3,
        Duration::from_secs(1),
        0,
        ProcessStartupState::Running,
        None,
    );
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].spans[0].content.as_ref(), "started: ");
    assert_eq!(lines[1].spans[0].content.as_ref(), "⠸");
    assert_eq!(
        lines[1].spans[1].content.as_ref(),
        " waiting for first output..."
    );
}

#[test]
fn output_lines_render_writer_styles_without_ansi_bytes() {
    use effigy_core::terminal_text::StyledSpan;
    let green =
        effigy_core::terminal_text::style::apply_sgr(Default::default(), "32", Default::default());
    let logs = vec![LogEntry {
        kind: LogEntryKind::Stdout,
        line: "ready [REDACTED]\n".into(),
        styles: Some(vec![
            StyledSpan {
                text: "ready ".into(),
                style: green,
            },
            StyledSpan {
                text: "[REDACTED]\n".into(),
                style: Default::default(),
            },
        ]),
    }];
    let lines = output_lines(&logs, true, Duration::ZERO, 0);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].spans[0].style, green);
    assert_eq!(lines[0].spans[0].content, "ready ");
    assert_eq!(lines[0].spans[1].style, Default::default());
    assert_eq!(lines[0].spans[1].content, "[REDACTED]");
}

#[test]
fn a_starting_pane_names_its_wait_and_the_elapsed_time_not_just_a_glyph() {
    let frame = |elapsed| {
        let lines = waiting_for_output_lines(
            0,
            Duration::from_secs(elapsed),
            0,
            ProcessStartupState::Starting,
            None,
        );
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let early = frame(2);
    let later = frame(45);
    assert_ne!(early, later, "a fixed label under a moving glyph is noise");
    assert!(early.contains("startup: starting (2s elapsed)"), "{early}");
    assert!(later.contains("starting process..."), "{later}");
    assert!(later.contains("startup: starting (45s elapsed)"), "{later}");
}

#[test]
fn an_entry_with_its_own_state_does_not_borrow_the_session_clock() {
    use crate::waits::{EntryPhase, EntryState};

    let entry = EntryState {
        entry: "web".to_owned(),
        phase: EntryPhase::Starting,
        waiting_on: None,
        not_started_because: None,
        detail: None,
        elapsed_ms: 7_000,
    };
    let lines = waiting_for_output_lines(
        0,
        Duration::from_secs(60),
        0,
        ProcessStartupState::Starting,
        Some(&entry),
    );
    let text = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("startup: starting (7s elapsed)"), "{text}");
    assert!(!text.contains("60s"), "{text}");
}
