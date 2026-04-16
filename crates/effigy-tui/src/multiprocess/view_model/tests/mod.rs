use crate::core::{LogEntry, LogEntryKind};
use crate::multiprocess::state::SessionState;

pub(super) fn state_with_logs(lines: usize) -> SessionState {
    let mut state = SessionState::new(vec!["api".to_owned()], 2000, 240, 8000);
    let buffer = state.logs.get_mut("api").expect("api log buffer");
    for idx in 0..lines {
        buffer.push_back(LogEntry {
            kind: LogEntryKind::Stdout,
            line: format!("line-{idx}"),
        });
    }
    state
}

#[path = "cursor_tests.rs"]
mod cursor_tests;
#[path = "plain_tests.rs"]
mod plain_tests;
#[path = "vt_tests.rs"]
mod vt_tests;
