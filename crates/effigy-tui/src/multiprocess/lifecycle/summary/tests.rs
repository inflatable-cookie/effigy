use std::collections::{HashMap, VecDeque};

use super::{collect_entry_states, failure_log_tail, format_process_status};
use crate::core::{LogEntry, LogEntryKind};
use crate::multiprocess::{SessionEntry, SessionEntryState};
use effigy_ui::theme::Theme;

#[test]
fn collect_entry_states_projects_every_supervised_entry_honestly() {
    let diagnostics = vec![
        ("cancelled".to_owned(), "signal=15".to_owned()),
        ("failed".to_owned(), "exit=7".to_owned()),
        ("never-started".to_owned(), "not-tracked".to_owned()),
        ("passed".to_owned(), "exit=0".to_owned()),
        ("unknown".to_owned(), "unknown exit".to_owned()),
    ];

    let entries = collect_entry_states(&diagnostics);

    assert_eq!(
        entries,
        vec![
            SessionEntry {
                name: "cancelled".to_owned(),
                state: SessionEntryState::Stopped {
                    detail: "signal=15".to_owned()
                },
            },
            SessionEntry {
                name: "failed".to_owned(),
                state: SessionEntryState::Failed {
                    detail: "exit=7".to_owned()
                },
            },
            SessionEntry {
                name: "never-started".to_owned(),
                state: SessionEntryState::NotStarted,
            },
            SessionEntry {
                name: "passed".to_owned(),
                state: SessionEntryState::Passed,
            },
            SessionEntry {
                name: "unknown".to_owned(),
                state: SessionEntryState::Failed {
                    detail: "unknown exit".to_owned()
                },
            },
        ]
    );
}

#[test]
fn a_terminated_entry_is_never_reported_as_a_pass() {
    for diagnostic in ["signal=9", "signal=15", "signal=11"] {
        let entries = collect_entry_states(&[(("suite").to_owned(), diagnostic.to_owned())]);
        assert_eq!(
            entries[0].state,
            SessionEntryState::Stopped {
                detail: diagnostic.to_owned()
            },
            "a signal is a stop, not a pass: {diagnostic}"
        );
    }
}

#[test]
fn format_process_status_names_the_recorded_state() {
    let theme = Theme::default();
    assert_eq!(
        format_process_status(&SessionEntryState::Passed, "12s", false, theme),
        "OK 12s".to_owned()
    );
    assert_eq!(
        format_process_status(
            &SessionEntryState::Stopped {
                detail: "signal=15".to_owned()
            },
            "3s",
            false,
            theme
        ),
        "signal=15 3s".to_owned()
    );
    assert_eq!(
        format_process_status(
            &SessionEntryState::Failed {
                detail: "exit=9".to_owned()
            },
            "5s",
            false,
            theme
        ),
        "exit=9 5s".to_owned()
    );
    assert_eq!(
        format_process_status(&SessionEntryState::NotStarted, "0s", false, theme),
        "not started 0s".to_owned()
    );
}

#[test]
fn failure_log_tail_returns_recent_lines_for_failed_process_only() {
    let logs = VecDeque::from([
        LogEntry {
            styles: None,
            kind: LogEntryKind::Stdout,
            line: "line one".to_owned(),
        },
        LogEntry {
            styles: None,
            kind: LogEntryKind::Stderr,
            line: "bad thing".to_owned(),
        },
        LogEntry {
            styles: None,
            kind: LogEntryKind::Exit,
            line: "exit=1".to_owned(),
        },
    ]);

    assert_eq!(
        failure_log_tail(&logs),
        vec!["line one".to_owned(), "bad thing".to_owned()]
    );
}

#[test]
fn failure_log_tail_keeps_the_styled_tail_a_program_drew() {
    let logs = VecDeque::from([LogEntry {
        styles: Some(Vec::new()),
        kind: LogEntryKind::Stdout,
        line: "drawn output".to_owned(),
    }]);

    assert_eq!(failure_log_tail(&logs), vec!["drawn output".to_owned()]);
}

#[test]
fn a_failed_entry_names_its_logs_and_a_stopped_entry_does_not() {
    let entries = vec![
        SessionEntry {
            name: "failed".to_owned(),
            state: SessionEntryState::Failed {
                detail: "exit=1".to_owned(),
            },
        },
        SessionEntry {
            name: "stopped".to_owned(),
            state: SessionEntryState::Stopped {
                detail: "signal=15".to_owned(),
            },
        },
    ];
    let logs = HashMap::from([
        (
            "failed".to_owned(),
            VecDeque::from([
                LogEntry {
                    styles: None,
                    kind: LogEntryKind::Stdout,
                    line: "failure detail".to_owned(),
                },
                LogEntry {
                    styles: None,
                    kind: LogEntryKind::Exit,
                    line: "exit=1".to_owned(),
                },
            ]),
        ),
        (
            "stopped".to_owned(),
            VecDeque::from([LogEntry {
                styles: None,
                kind: LogEntryKind::Stdout,
                line: "stopped detail".to_owned(),
            }]),
        ),
    ]);

    let failures = super::failure_output(&entries, &logs);

    assert_eq!(
        failures,
        vec![("failed", vec!["failure detail".to_owned()])]
    );
}
