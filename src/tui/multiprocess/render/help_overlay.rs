use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use super::super::OptionsAction;
use super::header::panel_block;

const OPTIONS_ACTIONS: [OptionsAction; 5] = [
    OptionsAction::ToggleFollow,
    OptionsAction::Restart,
    OptionsAction::Stop,
    OptionsAction::Cancel,
    OptionsAction::Quit,
];

pub(super) fn options_actions(_follow_enabled: bool) -> Vec<OptionsAction> {
    OPTIONS_ACTIONS.to_vec()
}

pub(super) fn render_help_overlay(frame: &mut Frame<'_>, area: Rect) {
    let help_lines = vec![
        Line::from(vec![Span::styled(
            "Command Mode",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("tab             toggle insert/command mode"),
        Line::from("tab             enter shell capture mode (shell tab)"),
        Line::from("ctrl+g          toggle shell capture mode (shell tab)"),
        Line::from("left/right       switch process tabs"),
        Line::from("up/down          scroll output line-by-line"),
        Line::from("pgup/pgdn        scroll output by page"),
        Line::from("home/end         jump to top/bottom (end re-enables follow)"),
        Line::from("h               toggle this help"),
        Line::from("o               open per-process options menu"),
        Line::from("ctrl+c          quit and shut down managed processes"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Insert Mode",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("type            send input text to active process"),
        Line::from("enter           submit input"),
        Line::from("esc             return to command mode"),
    ];
    let help = Paragraph::new(help_lines).block(panel_block(Some("Help"), false, Color::Magenta));
    frame.render_widget(help, area);
}

fn options_action_label(action: OptionsAction, follow_enabled: bool) -> &'static str {
    match action {
        OptionsAction::ToggleFollow => {
            if follow_enabled {
                "Disable follow (f)"
            } else {
                "Enable follow (f)"
            }
        }
        OptionsAction::Restart => "Restart process (r)",
        OptionsAction::Stop => "Stop process (s)",
        OptionsAction::Cancel => "Cancel (o)",
        OptionsAction::Quit => "Quit (q)",
    }
}

pub(super) fn render_options_overlay(
    frame: &mut Frame<'_>,
    process: &str,
    selected: usize,
    follow_enabled: bool,
) {
    let area = centered_rect(54, 44, frame.area());
    frame.render_widget(Clear, area);
    let rows = options_actions(follow_enabled)
        .iter()
        .enumerate()
        .map(|(idx, action)| {
            let marker = if idx == selected { "› " } else { "  " };
            let style = if idx == selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(Span::styled(
                format!("{marker}{}", options_action_label(*action, follow_enabled)),
                style,
            ))
        })
        .collect::<Vec<Line>>();
    let block = panel_block(Some(" Options "), false, Color::Magenta);
    let paragraph = Paragraph::new({
        let mut lines = vec![Line::from(Span::styled(
            format!("process: {process}"),
            Style::default().fg(Color::DarkGray),
        ))];
        lines.push(Line::from(""));
        lines.extend(rows);
        lines
    })
    .block(block);
    frame.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
