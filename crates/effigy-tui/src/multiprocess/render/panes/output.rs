use std::time::Duration;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::core::{LogEntry, LogEntryKind};
use crate::multiprocess::state::ProcessStartupState;
use crate::waits::{EntryPhase, EntryState};

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
    startup_state: ProcessStartupState,
    entry_wait: Option<&EntryState>,
) -> Vec<Line<'static>> {
    // A managed entry has its own authoritative state: the wait names exactly
    // what is happening, so the pane shows it without a generic spinner and
    // never borrows another entry's label (effigy#458, Q-017).
    if let Some(entry) = entry_wait.filter(|entry| names_its_wait(entry)) {
        return vec![
            entry_meta_line(entry),
            Line::from(Span::styled(
                entry.status_line(),
                Style::default().fg(Color::DarkGray),
            )),
        ];
    }
    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner = spinner_frames[spinner_tick % spinner_frames.len()];
    vec![
        startup_meta_line(startup_state, active_elapsed, active_restart_count),
        Line::from(vec![
            Span::styled(spinner.to_owned(), Style::default().fg(Color::Yellow)),
            Span::styled(
                waiting_label(startup_state),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ]
}

/// Whether this entry's own state says more than the generic spinner would:
/// it has not started, or it is starting against a named readiness probe.
fn names_its_wait(entry: &EntryState) -> bool {
    matches!(
        entry.phase,
        EntryPhase::Waiting | EntryPhase::NotStarted | EntryPhase::Failed
    ) || (entry.phase == EntryPhase::Starting && entry.waiting_on.is_some())
}

fn entry_meta_line(entry: &EntryState) -> Line<'static> {
    Line::from(vec![
        Span::styled("startup: ", Style::default().fg(Color::LightBlue)),
        Span::styled(
            entry.phase.as_str().replace('_', " "),
            match entry.phase {
                EntryPhase::NotStarted | EntryPhase::Failed => Style::default().fg(Color::Red),
                _ => Style::default().fg(Color::DarkGray),
            },
        ),
    ])
}

fn startup_meta_line(
    startup_state: ProcessStartupState,
    active_elapsed: Duration,
    active_restart_count: usize,
) -> Line<'static> {
    match startup_state {
        ProcessStartupState::Waiting => status_meta_line("pending"),
        ProcessStartupState::Starting => status_meta_line("starting"),
        ProcessStartupState::Running => runtime_meta_line(active_elapsed, active_restart_count),
        ProcessStartupState::Failed => status_meta_line("failed"),
    }
}

fn status_meta_line(status: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled("startup: ", Style::default().fg(Color::LightBlue)),
        Span::styled(status, Style::default().fg(Color::DarkGray)),
    ])
}

fn waiting_label(startup_state: ProcessStartupState) -> &'static str {
    match startup_state {
        ProcessStartupState::Waiting => " waiting for startup delay...",
        ProcessStartupState::Starting => " starting process...",
        ProcessStartupState::Running => " waiting for first output...",
        ProcessStartupState::Failed => " startup failed",
    }
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
