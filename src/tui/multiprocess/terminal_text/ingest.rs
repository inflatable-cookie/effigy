use std::collections::VecDeque;

use crate::tui::core::{LogEntry, LogEntryKind};

use super::ansi::normalize_terminal_payload;
use super::sanitize::sanitize_log_text;
use super::MAX_LOG_LINES;

pub(crate) fn push_entry(buffer: &mut VecDeque<LogEntry>, entry: LogEntry) {
    buffer.push_back(entry);
    while buffer.len() > MAX_LOG_LINES {
        buffer.pop_front();
    }
}

pub(crate) fn ingest_log_payload(
    buffer: &mut VecDeque<LogEntry>,
    kind: LogEntryKind,
    payload: &str,
) {
    let normalized = normalize_terminal_payload(payload);
    let fragments = normalized
        .text
        .split('\r')
        .map(sanitize_log_text)
        .filter(|line| !line.is_empty())
        .collect::<Vec<String>>();
    if fragments.is_empty() {
        return;
    }

    if fragments.len() == 1 && !normalized.text.contains('\r') {
        if normalized.cursor_up > 0 {
            replace_last_renderable_line(buffer, kind, fragments[0].clone());
        } else {
            push_entry(
                buffer,
                LogEntry {
                    kind,
                    line: fragments[0].clone(),
                },
            );
        }
        return;
    }

    for (index, fragment) in fragments.into_iter().enumerate() {
        if index == 0 {
            if normalized.cursor_up > 0 {
                replace_last_renderable_line(buffer, kind.clone(), fragment);
            } else {
                push_entry(
                    buffer,
                    LogEntry {
                        kind: kind.clone(),
                        line: fragment,
                    },
                );
            }
            continue;
        }
        replace_last_renderable_line(buffer, kind.clone(), fragment);
    }
}

fn replace_last_renderable_line(buffer: &mut VecDeque<LogEntry>, kind: LogEntryKind, line: String) {
    if let Some(last) = buffer.back_mut() {
        if matches!(last.kind, LogEntryKind::Stdout | LogEntryKind::Stderr) {
            last.kind = kind;
            last.line = line;
            return;
        }
    }
    push_entry(buffer, LogEntry { kind, line });
}
