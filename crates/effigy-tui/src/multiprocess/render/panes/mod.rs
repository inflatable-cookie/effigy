use std::collections::HashMap;
use std::time::Duration;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use ratatui::Frame;

use crate::core::{InputMode, LogEntry, ProcessExitState};

use super::header::panel_block;

mod input;
mod output;
mod shell_cursor;
#[cfg(test)]
mod tests;

use input::render_input_pane_impl;
use output::{output_lines, waiting_for_output_lines};
use shell_cursor::{shell_cursor_position, should_show_output_scrollbar, should_show_shell_caret};

pub(super) struct OutputPaneRenderArgs<'a> {
    pub(super) active_logs: &'a [LogEntry],
    pub(super) scroll_offset: usize,
    pub(super) max_offset: usize,
    pub(super) render_scroll_offset: usize,
    pub(super) scrollbar_total: usize,
    pub(super) active_process: &'a str,
    pub(super) active_vt: bool,
    pub(super) process_name: &'a str,
    pub(super) shell_capture_mode: bool,
    pub(super) active_output_seen: bool,
    pub(super) spinner_tick: usize,
    pub(super) active_elapsed: Duration,
    pub(super) active_restart_count: usize,
    pub(super) exit_states: &'a HashMap<String, ProcessExitState>,
    pub(super) shell_cursor: Option<(u16, u16)>,
}

pub(super) fn render_output_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    args: OutputPaneRenderArgs<'_>,
) {
    let OutputPaneRenderArgs {
        active_logs,
        scroll_offset,
        max_offset,
        render_scroll_offset,
        scrollbar_total,
        active_process,
        active_vt,
        process_name,
        shell_capture_mode,
        active_output_seen,
        spinner_tick,
        active_elapsed,
        active_restart_count,
        exit_states,
        shell_cursor,
    } = args;
    let active_is_shell = active_process == "shell";
    let output_height = area.height.saturating_sub(2) as usize;
    let lines = output_lines(
        active_logs,
        active_is_shell,
        active_elapsed,
        active_restart_count,
    );

    let panel = panel_block(None, false, Color::DarkGray);
    let shell_inactive_style = if active_is_shell && !shell_capture_mode {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    } else {
        Style::default()
    };
    let logs = if !active_output_seen && !exit_states.contains_key(process_name) {
        Paragraph::new(waiting_for_output_lines(
            spinner_tick,
            active_elapsed,
            active_restart_count,
        ))
        .block(panel)
        .style(shell_inactive_style)
    } else if active_vt {
        Paragraph::new(lines)
            .block(panel)
            .style(shell_inactive_style)
            .scroll((render_scroll_offset.min(u16::MAX as usize) as u16, 0))
            .wrap(Wrap { trim: false })
    } else {
        Paragraph::new(lines)
            .block(panel)
            .style(shell_inactive_style)
            .scroll((render_scroll_offset.min(u16::MAX as usize) as u16, 0))
            .wrap(Wrap { trim: false })
    };
    frame.render_widget(logs, area);

    if active_is_shell && shell_capture_mode && should_show_shell_caret(spinner_tick) {
        if let Some((cursor_x, cursor_y)) = shell_cursor_position(area, shell_cursor) {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    if should_show_output_scrollbar(active_output_seen, exit_states, process_name) {
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
    render_input_pane_impl(frame, area, active_is_shell, input_mode, input_line);
}
