use std::collections::HashMap;
use std::time::Duration;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs,
};
use ratatui::Frame;

use super::terminal_text::{ansi_line, runtime_meta_line};
use super::{InputMode, LogEntry, LogEntryKind, OptionsAction, ProcessExitState};

pub(super) fn render_ui(
    frame: &mut Frame<'_>,
    process_names: &[String],
    active_index: usize,
    active_logs: &[LogEntry],
    scroll_offset: usize,
    max_offset: usize,
    render_scroll_offset: usize,
    scrollbar_total: usize,
    follow: bool,
    active_process: &str,
    input_line: &str,
    input_mode: InputMode,
    shell_capture_mode: bool,
    exit_states: &HashMap<String, ProcessExitState>,
    show_help: bool,
    show_options: bool,
    options_index: usize,
    active_output_seen: bool,
    spinner_tick: usize,
    active_elapsed: Duration,
    active_restart_count: usize,
    shell_cursor: Option<(u16, u16)>,
) {
    let active_is_shell = active_process == "shell";
    let input_height = if active_is_shell {
        0
    } else if input_mode == InputMode::Insert {
        3
    } else {
        0
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(input_height),
            Constraint::Length(1),
        ])
        .split(frame.area());

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
    frame.render_widget(tabs, chunks[0]);

    let output_height = chunks[1].height.saturating_sub(2) as usize;
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
    if show_help {
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
        let help =
            Paragraph::new(help_lines).block(panel_block(Some("Help"), false, Color::Magenta));
        frame.render_widget(help, chunks[1]);
    } else {
        let panel = panel_block(None, false, Color::DarkGray);
        let shell_inactive_style = if active_is_shell && !shell_capture_mode {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM)
        } else {
            Style::default()
        };
        let logs = if !active_output_seen && !exit_states.contains_key(&process_names[active_index])
        {
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
        frame.render_widget(logs, chunks[1]);
        // Blink shell caret at ~500ms cadence (event loop ticks every ~50ms).
        let show_shell_caret = (spinner_tick / 10).is_multiple_of(2);
        if active_is_shell && shell_capture_mode && show_shell_caret {
            if let Some((row, col)) = shell_cursor {
                let inner_x = chunks[1].x.saturating_add(1);
                let inner_y = chunks[1].y.saturating_add(1);
                let inner_w = chunks[1].width.saturating_sub(2);
                let inner_h = chunks[1].height.saturating_sub(2);
                if inner_w > 0 && inner_h > 0 {
                    let cursor_x = inner_x.saturating_add(col.min(inner_w.saturating_sub(1)));
                    let cursor_y = inner_y.saturating_add(row.min(inner_h.saturating_sub(1)));
                    frame.set_cursor_position((cursor_x, cursor_y));
                }
            }
        }
        if active_output_seen || exit_states.contains_key(&process_names[active_index]) {
            let mut scrollbar_state = ScrollbarState::new(scrollbar_total.max(1))
                .viewport_content_length(output_height.max(1))
                .position(scroll_offset.min(max_offset));
            frame.render_stateful_widget(
                Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
                chunks[1],
                &mut scrollbar_state,
            );
        }
    }

    if show_options {
        render_options_overlay(
            frame,
            process_names[active_index].as_str(),
            options_index,
            follow,
        );
    }

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
    frame.render_widget(input, chunks[2]);

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
    frame.render_widget(footer, chunks[3]);
}

fn panel_block<'a>(title: Option<&'a str>, show_version: bool, border_color: Color) -> Block<'a> {
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

fn render_options_overlay(
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
