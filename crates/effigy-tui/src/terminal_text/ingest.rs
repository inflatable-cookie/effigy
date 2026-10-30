use std::collections::VecDeque;

use crate::core::{LogEntry, LogEntryKind};

use super::ansi::normalize_terminal_payload;
use super::config::MAX_LOG_LINES;
use super::sanitize::sanitize_log_text;

pub fn push_entry(buffer: &mut VecDeque<LogEntry>, entry: LogEntry) {
    buffer.push_back(entry);
    while buffer.len() > MAX_LOG_LINES {
        buffer.pop_front();
    }
}

pub fn ingest_log_payload(buffer: &mut VecDeque<LogEntry>, kind: LogEntryKind, payload: &str) {
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
                    styles: None,
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
                        styles: None,
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
            last.styles = None;
            return;
        }
    }
    push_entry(
        buffer,
        LogEntry {
            styles: None,
            kind,
            line,
        },
    );
}

/// Append typed writer output without terminal parsing or another normalizer.
/// Span/line text is validated, never repaired or joined after elision.
pub fn ingest_styled_chunk(
    buffer: &mut VecDeque<LogEntry>,
    chunk: effigy_core::terminal_text::StyledChunk,
) {
    use effigy_core::terminal_text::StyledSpan;
    let size: usize = chunk.spans.iter().map(|span| span.text.len()).sum();
    if size > 64 * 1024
        || chunk
            .spans
            .iter()
            .any(|span| span.text.chars().any(|ch| ch.is_control() && ch != '\n'))
    {
        return;
    }
    for span in chunk.spans {
        for text in span.text.split_inclusive('\n') {
            if !buffer.back().is_some_and(|entry| {
                entry.styles.is_some()
                    && !entry.line.ends_with('\n')
                    && entry.line.len() + text.len() <= 64 * 1024
            }) {
                push_entry(
                    buffer,
                    LogEntry {
                        kind: LogEntryKind::Stdout,
                        line: String::new(),
                        styles: Some(Vec::new()),
                    },
                );
            }
            if let Some(entry) = buffer.back_mut() {
                entry.line.push_str(text);
                let styles = entry.styles.as_mut().expect("typed output entry");
                if let Some(last) = styles.last_mut().filter(|last| last.style == span.style) {
                    last.text.push_str(text);
                } else {
                    styles.push(StyledSpan {
                        text: text.to_owned(),
                        style: span.style,
                    });
                }
            }
        }
    }
    // Bound presentation allocation as well as line count. Canonical bytes
    // remain in the log after older pane history expires.
    while buffer.len() > 1
        && buffer
            .iter()
            .map(|entry| {
                entry.line.len()
                    + entry
                        .styles
                        .as_ref()
                        .map_or(0, |spans| spans.len() * std::mem::size_of::<StyledSpan>())
            })
            .sum::<usize>()
            > 256 * 1024
    {
        buffer.pop_front();
    }
}
