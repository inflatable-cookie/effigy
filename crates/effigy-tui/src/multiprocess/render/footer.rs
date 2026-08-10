use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::InputMode;

pub(super) struct FooterState<'a> {
    pub(super) input_mode: InputMode,
    pub(super) active_is_shell: bool,
    pub(super) shell_capture_mode: bool,
    pub(super) show_help: bool,
    pub(super) show_options: bool,
    pub(super) message: Option<&'a str>,
}

pub(super) fn render_footer(
    frame: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    state: FooterState<'_>,
) {
    let FooterState {
        input_mode,
        active_is_shell,
        shell_capture_mode,
        show_help,
        show_options,
        message: footer_message,
    } = state;
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
        Span::styled("  |  ", muted),
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
    if let Some(message) = footer_message {
        footer_spans.push(Span::styled("  |  ", muted));
        footer_spans.push(Span::styled(message.to_owned(), active));
    }
    let footer = Paragraph::new(Line::from(footer_spans));
    frame.render_widget(footer, area);
}
