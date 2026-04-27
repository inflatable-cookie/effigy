use std::time::Duration;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::core::{LogEntry, LogEntryKind};

use super::super::super::terminal_text::{ansi_line, runtime_meta_line};

pub(super) fn output_lines(
    active_logs: &[LogEntry],
    active_is_shell: bool,
    active_elapsed: Duration,
    active_restart_count: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(active_logs.len() + 1);
    if !active_is_shell {
        lines.push(runtime_meta_line(active_elapsed, active_restart_count));
    }
    lines.extend(active_logs.iter().map(format_log_entry_line));
    lines
}

pub(super) fn waiting_for_output_lines(
    spinner_tick: usize,
    active_elapsed: Duration,
    active_restart_count: usize,
) -> Vec<Line<'static>> {
    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner = spinner_frames[spinner_tick % spinner_frames.len()];
    vec![
        runtime_meta_line(active_elapsed, active_restart_count),
        Line::from(vec![
            Span::styled(spinner.to_owned(), Style::default().fg(Color::Yellow)),
            Span::styled(
                " waiting for first output...",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ]
}

fn format_log_entry_line(entry: &LogEntry) -> Line<'static> {
    match entry.kind {
        LogEntryKind::Stdout => ansi_line(&entry.line, Style::default()),
        LogEntryKind::Stderr => ansi_line(&entry.line, Style::default()),
        LogEntryKind::Exit => Line::from(vec![
            Span::styled("[exit] ", Style::default().fg(Color::Yellow)),
            Span::styled(entry.line.clone(), Style::default().fg(Color::Gray)),
        ]),
    }
}
