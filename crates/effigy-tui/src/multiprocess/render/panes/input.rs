use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::InputMode;

use super::super::header::panel_block;

pub(super) fn render_input_pane_impl(
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
