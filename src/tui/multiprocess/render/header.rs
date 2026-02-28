use std::collections::HashMap;

use crate::tui::core::ProcessExitState;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};
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
                        Style::default().fg(Color::Magenta)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }
                }
            };
            Line::from(Span::styled(label, style))
        })
        .collect::<Vec<Line>>();

    let tabs = Tabs::new(titles)
        .select(active_index)
        .block(panel_block(Some(" EFFIGY "), true, Color::Magenta))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, area);
}

pub(super) fn panel_block<'a>(
    title: Option<&'a str>,
    show_version: bool,
    border_color: Color,
) -> Block<'a> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(border_color));
    if let Some(title) = title {
        block = block.title_top(
            Line::from(Span::styled(
                title.to_owned(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ))
            .left_aligned(),
        );
    }
    if show_version {
        let version = format!(" v{} ", env!("CARGO_PKG_VERSION"));
        block = block.title_bottom(
            Line::from(Span::styled(
                version,
                Style::default().fg(Color::LightMagenta),
            ))
            .right_aligned(),
        );
    }
    block
}
