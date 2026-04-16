use crate::core::{LogEntry, LogEntryKind};
use vt100::Parser as VtParser;

use super::super::state::SessionState;
use super::snapshot::ActiveSnapshot;
use crate::terminal_text::vt_logs;

pub(super) struct ViewScroll {
    pub(super) logs: Vec<LogEntry>,
    pub(super) scroll_offset: usize,
    pub(super) max_offset: usize,
    pub(super) render_scroll_offset: usize,
    pub(super) scrollbar_total: usize,
}

pub(super) fn build_scroll_view(
    state: &mut SessionState,
    snapshot: &ActiveSnapshot,
    output_height: usize,
    output_width: usize,
    vt_emulator_enabled: bool,
) -> ViewScroll {
    if vt_emulator_enabled && snapshot.vt_has_chunks {
        return build_vt_scroll_view(state, snapshot, output_height, output_width);
    }
    build_plain_scroll_view(state, snapshot, output_height)
}

pub(super) fn shell_cursor(
    state: &SessionState,
    active_process: &str,
    vt_emulator_enabled: bool,
) -> Option<(u16, u16)> {
    if active_process != "shell" || !vt_emulator_enabled {
        return None;
    }
    state
        .vt_parser_for(active_process)
        .map(VtParser::screen)
        .map(|screen| screen.cursor_position())
}

fn build_vt_scroll_view(
    state: &mut SessionState,
    snapshot: &ActiveSnapshot,
    output_height: usize,
    output_width: usize,
) -> ViewScroll {
    let parser = state
        .vt_parsers
        .get_mut(&snapshot.name)
        .expect("active vt parser missing unexpectedly");
    let (mut rendered, clamped, max_vt) = vt_logs(
        parser,
        output_height.saturating_sub(1).max(1),
        output_width.max(1),
        snapshot.stored_offset,
        snapshot.is_follow,
    );
    if let Some(buffer) = state.logs_for(&snapshot.name) {
        rendered.extend(
            buffer
                .iter()
                .filter(|entry| matches!(entry.kind, LogEntryKind::Exit))
                .cloned(),
        );
    }
    state.set_scroll_offset_for(&snapshot.name, clamped);
    ViewScroll {
        logs: rendered,
        scroll_offset: clamped,
        max_offset: max_vt,
        render_scroll_offset: 0usize,
        scrollbar_total: max_vt.saturating_add(output_height.max(1)),
    }
}

fn build_plain_scroll_view(
    state: &mut SessionState,
    snapshot: &ActiveSnapshot,
    output_height: usize,
) -> ViewScroll {
    let rendered = state
        .logs_for(&snapshot.name)
        .map(|entries| entries.iter().cloned().collect::<Vec<LogEntry>>())
        .unwrap_or_default();
    let max_offset = rendered.len().saturating_sub(output_height);
    let clamped = snapshot.stored_offset.min(max_offset);
    state.set_scroll_offset_for(&snapshot.name, clamped);
    ViewScroll {
        logs: rendered,
        scroll_offset: clamped,
        max_offset,
        render_scroll_offset: if snapshot.is_follow {
            max_offset
        } else {
            clamped
        },
        scrollbar_total: output_height.max(1).saturating_add(max_offset),
    }
}
