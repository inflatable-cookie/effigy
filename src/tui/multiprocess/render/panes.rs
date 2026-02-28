use std::collections::HashMap;
use std::time::Duration;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;

use crate::tui::core::{InputMode, LogEntry, LogEntryKind, ProcessExitState};

use super::super::terminal_text::{ansi_line, runtime_meta_line};
use super::header::panel_block;

pub(super) fn render_output_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    active_logs: &[LogEntry],
    scroll_offset: usize,
    max_offset: usize,
    render_scroll_offset: usize,
    scrollbar_total: usize,
    active_process: &str,
    process_name: &str,
    shell_capture_mode: bool,
    active_output_seen: bool,
    spinner_tick: usize,
    active_elapsed: Duration,
    active_restart_count: usize,
    exit_states: &HashMap<String, ProcessExitState>,
    shell_cursor: Option<(u16, u16)>,
) {
    let active_is_shell = active_process == "shell";
    let output_height = area.height.saturating_sub(2) as usize;
    let mut lines = Vec::with_capacity(active_logs.len() + 1);
    if !active_is_shell {
        lines.push(runtime_meta_line(active_elapsed, active_restart_count));
    }
    lines.extend(active_logs.iter().map(|entry| match entry.kind {
        LogEntryKind::Stdout => ansi_line(&entry.line, Style::default()),
        LogEntryKind::Stderr => {
            let mut spans = vec![Span::styled("[stderr] ", Style::default().fg(Color::Red))];
            spans.extend(ansi_line(&entry.line, Style::default()).spans);
            Line::from(spans)
        }
        LogEntryKind::Exit => Line::from(vec![
            Span::styled("[exit] ", Style::default().fg(Color::Yellow)),
            Span::styled(entry.line.clone(), Style::default().fg(Color::Gray)),
        ]),
    }));

    let panel = panel_block(None, false, Color::DarkGray);
    let shell_inactive_style = if active_is_shell && !shell_capture_mode {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    } else {
        Style::default()
    };
    let logs = if !active_output_seen && !exit_states.contains_key(process_name) {
        let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner = spinner_frames[spinner_tick % spinner_frames.len()];
        Paragraph::new(vec![
            runtime_meta_line(active_elapsed, active_restart_count),
            Line::from(vec![
                Span::styled(spinner.to_owned(), Style::default().fg(Color::Yellow)),
                Span::styled(
                    " waiting for first output...",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
        ])
        .block(panel)
        .style(shell_inactive_style)
    } else {
        Paragraph::new(lines)
            .block(panel)
            .style(shell_inactive_style)
            .scroll((render_scroll_offset.min(u16::MAX as usize) as u16, 0))
    };
    frame.render_widget(logs, area);

    let show_shell_caret = (spinner_tick / 10).is_multiple_of(2);
    if active_is_shell && shell_capture_mode && show_shell_caret {
        if let Some((row, col)) = shell_cursor {
            let inner_x = area.x.saturating_add(1);
            let inner_y = area.y.saturating_add(1);
            let inner_w = area.width.saturating_sub(2);
            let inner_h = area.height.saturating_sub(2);
            if inner_w > 0 && inner_h > 0 {
                let cursor_x = inner_x.saturating_add(col.min(inner_w.saturating_sub(1)));
                let cursor_y = inner_y.saturating_add(row.min(inner_h.saturating_sub(1)));
                frame.set_cursor_position((cursor_x, cursor_y));
            }
        }
    }

    if active_output_seen || exit_states.contains_key(process_name) {
        let mut scrollbar_state = ScrollbarState::new(scrollbar_total.max(1))
            .viewport_content_length(output_height.max(1))
            .position(scroll_offset.min(max_offset));
        frame.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            area,
            &mut scrollbar_state,
        );
    }
}

pub(super) fn render_input_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    active_is_shell: bool,
    input_mode: InputMode,
    input_line: &str,
) {
    let input = if active_is_shell {
        Paragraph::new("")
    } else if input_mode == InputMode::Insert {
        let mut spans = vec![Span::styled("> ", Style::default().fg(Color::Yellow))];
        spans.push(Span::styled(
            input_line.to_owned(),
            Style::default().fg(Color::Gray),
        ));
        spans.push(Span::styled("▏", Style::default().fg(Color::Yellow)));
        Paragraph::new(Line::from(spans)).block(panel_block(
            Some("Input (Esc command, Enter send)"),
            false,
            Color::Magenta,
        ))
    } else {
        Paragraph::new("")
    };
    frame.render_widget(input, area);
}
