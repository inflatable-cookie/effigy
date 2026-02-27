use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::super::InputMode;

pub(super) fn render_footer(
    frame: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    input_mode: InputMode,
    active_is_shell: bool,
    shell_capture_mode: bool,
    show_help: bool,
    show_options: bool,
) {
    let mode_label = if input_mode == InputMode::Insert {
        "insert"
    } else {
        "command"
    };
    let muted = Style::default().fg(Color::DarkGray);
    let active = Style::default().fg(Color::Yellow);
    let mut footer_spans = vec![
        Span::styled(
            if active_is_shell {
                format!(
                    "mode:{} (ctrl+g)",
                    if shell_capture_mode {
                        "shell"
                    } else {
                        "command"
                    }
                )
            } else {
                format!("mode:{mode_label} (tab)")
            },
            if (active_is_shell && shell_capture_mode) || input_mode == InputMode::Insert {
                active
            } else {
                muted
            },
        ),
        Span::styled("  |  ", muted),
        Span::styled("help (h)", if show_help { active } else { muted }),
        Span::styled("  |  ", muted),
        Span::styled("options (o)", if show_options { active } else { muted }),
    ];
    if active_is_shell {
        footer_spans.push(Span::styled("  |  ", muted));
        footer_spans.push(Span::styled(
            if shell_capture_mode {
                "shell: live (ctrl+g to exit)"
            } else {
                "shell: command (tab/ctrl+g to enter)"
            },
            active,
        ));
    }
    let footer = Paragraph::new(Line::from(footer_spans));
    frame.render_widget(footer, area);
}
