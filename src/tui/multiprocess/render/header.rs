use std::collections::HashMap;

use crate::tui::core::{effigy_panel_block, ProcessExitState, EFFIGY_ACCENT, EFFIGY_MUTED};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Tabs};
use ratatui::Frame;

pub(super) fn render_tabs(
    frame: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    process_names: &[String],
    active_index: usize,
    shell_capture_mode: bool,
    exit_states: &HashMap<String, ProcessExitState>,
) {
    let titles = process_names
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let label = if name == "shell" {
                if shell_capture_mode {
                    "shell [live]".to_owned()
                } else {
                    "shell".to_owned()
                }
            } else {
                name.clone()
            };
            let style = match exit_states.get(name) {
                Some(ProcessExitState::Success) => Style::default().fg(Color::Green),
                Some(ProcessExitState::Failure) => Style::default().fg(Color::Red),
                None => {
                    if name == "shell" && shell_capture_mode && idx == active_index {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else if idx == active_index {
                        Style::default().fg(EFFIGY_ACCENT)
                    } else {
                        Style::default().fg(EFFIGY_MUTED)
                    }
                }
            };
            Line::from(Span::styled(label, style))
        })
        .collect::<Vec<Line>>();

    let tabs = Tabs::new(titles)
        .select(active_index)
        .block(panel_block(Some(" EFFIGY "), true, EFFIGY_ACCENT))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, area);
}

pub(super) fn panel_block<'a>(
    title: Option<&'a str>,
    show_version: bool,
    border_color: Color,
) -> Block<'a> {
    effigy_panel_block(title, show_version, border_color)
}
