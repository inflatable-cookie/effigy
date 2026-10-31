use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::core::{LogEntry, LogEntryKind};

use super::state::SessionState;

pub(super) fn export_active_process_transcript(state: &SessionState) -> io::Result<PathBuf> {
    let process = state.active_process();
    let transcript = match state.logs_for(process) {
        Some(logs) => render_log_transcript(logs),
        None => String::new(),
    };
    let export_dir = state.repo_root.join(".effigy/exports/managed-tui");
    effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root(&state.repo_root)?;
    fs::create_dir_all(&export_dir)?;
    let export_path = export_dir.join(format!("{process}.log"));
    fs::write(&export_path, transcript)?;
    Ok(export_path)
}

pub(super) fn render_log_transcript(logs: &VecDeque<LogEntry>) -> String {
    let mut rendered = String::new();
    let mut previous_typed = false;
    for entry in logs {
        let typed = entry.styles.is_some();
        if !rendered.is_empty() && !(previous_typed && typed) && !rendered.ends_with('\n') {
            rendered.push('\n');
        }
        if typed {
            rendered.push_str(&entry.line);
        } else {
            rendered.push_str(&format_log_entry_lines(entry).join("\n"));
        }
        previous_typed = typed;
    }
    rendered
}

fn format_log_entry_lines(entry: &LogEntry) -> Vec<String> {
    if entry.styles.is_some() {
        return vec![entry.line.clone()];
    }
    split_log_lines(&entry.line)
        .into_iter()
        .map(|line| match entry.kind {
            LogEntryKind::Stdout => line,
            LogEntryKind::Stderr => line,
            LogEntryKind::Exit => format!("[exit] {line}"),
        })
        .collect()
}

fn split_log_lines(line: &str) -> Vec<String> {
    line.split('\n')
        .map(|segment| segment.strip_suffix('\r').unwrap_or(segment).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::core::{LogEntry, LogEntryKind};

    use super::render_log_transcript;

    #[test]
    fn transcript_preserves_plain_stdout_and_stderr() {
        let logs = VecDeque::from([
            LogEntry {
                styles: None,
                kind: LogEntryKind::Stdout,
                line: "ready".to_owned(),
            },
            LogEntry {
                styles: None,
                kind: LogEntryKind::Stderr,
                line: "warning".to_owned(),
            },
            LogEntry {
                styles: None,
                kind: LogEntryKind::Exit,
                line: "code=1".to_owned(),
            },
        ]);

        assert_eq!(
            render_log_transcript(&logs),
            "ready\nwarning\n[exit] code=1"
        );
    }
}
