use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};

#[cfg(test)]
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Command,
    Insert,
}

#[derive(Debug, Clone)]
pub enum LogEntryKind {
    Stdout,
    Stderr,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExitState {
    Success,
    Failure,
}

pub const EFFIGY_ACCENT: Color = Color::Indexed(212);
pub const EFFIGY_ACCENT_SOFT: Color = Color::Indexed(218);
pub const EFFIGY_MUTED: Color = Color::Indexed(244);

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub kind: LogEntryKind,
    pub line: String,
}

pub fn next_index(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + 1) % len
    }
}

pub fn prev_index(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else if current == 0 {
        len - 1
    } else {
        current - 1
    }
}

pub fn effigy_panel_block<'a>(
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
                    .fg(EFFIGY_ACCENT)
                    .add_modifier(Modifier::BOLD),
            ))
            .left_aligned(),
        );
    }
    if show_version {
        let version = format!(" {} ", effigy_core::build_info::display_version());
        block = block.title_bottom(
            Line::from(Span::styled(
                version,
                Style::default().fg(EFFIGY_ACCENT_SOFT),
            ))
            .right_aligned(),
        );
    }
    block
}

#[cfg(test)]
pub fn toggle_follow_for_active(
    follow_mode: &mut HashMap<String, bool>,
    scroll_offsets: &mut HashMap<String, usize>,
    active: &str,
    max_offset: usize,
) {
    if let Some(follow) = follow_mode.get_mut(active) {
        *follow = !*follow;
        if *follow {
            if let Some(offset) = scroll_offsets.get_mut(active) {
                *offset = max_offset;
            }
        }
    }
}
