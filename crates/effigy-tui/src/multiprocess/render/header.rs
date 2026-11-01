use std::collections::HashMap;

use super::super::state::ProcessStartupState;
use crate::core::{effigy_panel_block, ProcessExitState, EFFIGY_ACCENT, EFFIGY_MUTED};
use crate::waits::EntryState;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Tabs};
use ratatui::Frame;

/// What the tab bar draws for one session: the processes, the selected tab,
/// and each entry's own state.
pub(super) struct TabBar<'a> {
    pub(super) process_names: &'a [String],
    pub(super) active_index: usize,
    pub(super) shell_capture_mode: bool,
    pub(super) exit_states: &'a HashMap<String, ProcessExitState>,
    pub(super) startup_states: &'a HashMap<String, ProcessStartupState>,
    pub(super) entry_waits: &'a HashMap<String, EntryState>,
}

pub(super) fn render_tabs(frame: &mut Frame<'_>, area: ratatui::layout::Rect, bar: TabBar<'_>) {
    let TabBar {
        process_names,
        active_index,
        shell_capture_mode,
        exit_states,
        startup_states,
        entry_waits,
    } = bar;
    let titles = process_names
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            // The session's own entry state is authoritative: a waiting entry
            // names what it waits for, and never shows a generic wait.
            let state_label = match entry_waits.get(name) {
                Some(entry) => entry.tab_label(),
                None => match startup_states.get(name) {
                    Some(ProcessStartupState::Waiting) => format!("{name} [waiting]"),
                    Some(ProcessStartupState::Starting) => format!("{name} [starting]"),
                    Some(ProcessStartupState::Failed) => format!("{name} [failed]"),
                    Some(ProcessStartupState::Running) if !exit_states.contains_key(name) => {
                        format!("{name} [running]")
                    }
                    Some(ProcessStartupState::Running) => name.clone(),
                    None => name.clone(),
                },
            };
            let label = if name == "shell" {
                if shell_capture_mode {
                    format!("{state_label} [live]")
                } else {
                    state_label
                }
            } else {
                state_label
            };
            let style = match exit_states.get(name) {
                Some(ProcessExitState::Success) => Style::default().fg(Color::Green),
                Some(ProcessExitState::Failure) => Style::default().fg(Color::Red),
                Some(ProcessExitState::Stopped) => Style::default().fg(EFFIGY_MUTED),
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
