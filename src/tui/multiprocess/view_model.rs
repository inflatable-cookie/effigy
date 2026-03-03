use std::time::{Duration, Instant};

use crate::tui::core::{LogEntry, LogEntryKind};
use vt100::Parser as VtParser;

use super::state::SessionState;
use super::terminal_text::vt_logs;

#[derive(Debug, Clone)]
struct ActiveSnapshot {
    name: String,
    is_follow: bool,
    stored_offset: usize,
    vt_has_chunks: bool,
    output_seen: bool,
    restart_count: usize,
}

struct ViewScroll {
    logs: Vec<LogEntry>,
    scroll_offset: usize,
    max_offset: usize,
    render_scroll_offset: usize,
    scrollbar_total: usize,
}

pub(super) struct ActiveViewModel {
    pub(super) active_process: String,
    pub(super) active_logs: Vec<LogEntry>,
    pub(super) scroll_offset: usize,
    pub(super) max_offset: usize,
    pub(super) render_scroll_offset: usize,
    pub(super) scrollbar_total: usize,
    pub(super) is_follow: bool,
    pub(super) shell_cursor: Option<(u16, u16)>,
    pub(super) active_elapsed: Duration,
    pub(super) active_restart_count: usize,
    pub(super) active_output_seen: bool,
}

pub(super) fn build_active_view_model(
    state: &mut SessionState,
    output_height: usize,
    output_width: usize,
    vt_emulator_enabled: bool,
) -> ActiveViewModel {
    let snapshot = active_snapshot(state);
    let scroll = build_scroll_view(
        state,
        &snapshot,
        output_height,
        output_width,
        vt_emulator_enabled,
    );
    let shell_cursor = shell_cursor(state, &snapshot.name, vt_emulator_enabled);

    let now = Instant::now();
    let active_elapsed = state
        .process_started_at
        .get(&snapshot.name)
        .map(|started| now.saturating_duration_since(*started))
        .unwrap_or_default();

    ActiveViewModel {
        active_process: snapshot.name,
        active_logs: scroll.logs,
        scroll_offset: scroll.scroll_offset,
        max_offset: scroll.max_offset,
        render_scroll_offset: scroll.render_scroll_offset,
        scrollbar_total: scroll.scrollbar_total,
        is_follow: snapshot.is_follow,
        shell_cursor,
        active_elapsed,
        active_restart_count: snapshot.restart_count,
        active_output_seen: snapshot.output_seen,
    }
}

fn active_snapshot(state: &SessionState) -> ActiveSnapshot {
    let name = state.active_process().to_owned();
    ActiveSnapshot {
        is_follow: *state.follow_mode.get(&name).unwrap_or(&true),
        stored_offset: *state.scroll_offsets.get(&name).unwrap_or(&0usize),
        vt_has_chunks: *state.vt_saw_chunk.get(&name).unwrap_or(&false),
        output_seen: *state.output_seen.get(&name).unwrap_or(&false),
        restart_count: *state.process_restart_count.get(&name).unwrap_or(&0),
        name,
    }
}

fn build_scroll_view(
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
    if let Some(buffer) = state.logs.get(&snapshot.name) {
        rendered.extend(
            buffer
                .iter()
                .filter(|entry| matches!(entry.kind, LogEntryKind::Exit))
                .cloned(),
        );
    }
    state.scroll_offsets.insert(snapshot.name.clone(), clamped);
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
        .logs
        .get(&snapshot.name)
        .map(|entries| entries.iter().cloned().collect::<Vec<LogEntry>>())
        .unwrap_or_default();
    let max_offset = rendered.len().saturating_sub(output_height);
    let clamped = snapshot.stored_offset.min(max_offset);
    state.scroll_offsets.insert(snapshot.name.clone(), clamped);
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

fn shell_cursor(
    state: &SessionState,
    active_process: &str,
    vt_emulator_enabled: bool,
) -> Option<(u16, u16)> {
    if active_process != "shell" || !vt_emulator_enabled {
        return None;
    }
    state
        .vt_parsers
        .get(active_process)
        .map(VtParser::screen)
        .map(|screen| screen.cursor_position())
}

#[cfg(test)]
mod tests {
    use super::build_active_view_model;
    use crate::tui::core::{LogEntry, LogEntryKind};
    use crate::tui::multiprocess::state::SessionState;
    use std::time::Duration;

    #[test]
    fn non_vt_view_clamps_scroll_offset() {
        let mut state = SessionState::new(vec!["api".to_owned()], 2000, 240, 8000);
        let buffer = state.logs.get_mut("api").expect("api log buffer");
        for idx in 0..20usize {
            buffer.push_back(LogEntry {
                kind: LogEntryKind::Stdout,
                line: format!("line-{idx}"),
            });
        }
        state.scroll_offsets.insert("api".to_owned(), 999);
        state.follow_mode.insert("api".to_owned(), false);

        let view = build_active_view_model(&mut state, 5, 80, false);
        assert_eq!(view.max_offset, 15);
        assert_eq!(view.scroll_offset, 15);
        assert_eq!(view.render_scroll_offset, 15);
        assert!(!view.is_follow);
    }

    #[test]
    fn non_vt_follow_mode_tracks_end() {
        let mut state = SessionState::new(vec!["api".to_owned()], 2000, 240, 8000);
        let buffer = state.logs.get_mut("api").expect("api log buffer");
        for idx in 0..12usize {
            buffer.push_back(LogEntry {
                kind: LogEntryKind::Stdout,
                line: format!("line-{idx}"),
            });
        }
        state.scroll_offsets.insert("api".to_owned(), 0);
        state.follow_mode.insert("api".to_owned(), true);

        let view = build_active_view_model(&mut state, 5, 80, false);
        assert_eq!(view.max_offset, 7);
        assert_eq!(view.render_scroll_offset, 7);
        assert_eq!(view.scroll_offset, 0);
        assert!(view.is_follow);
    }

    #[test]
    fn vt_mode_clamps_scroll_offset_safely() {
        let mut state = SessionState::new(vec!["api".to_owned()], 2000, 240, 8000);
        let parser = state.vt_parsers.get_mut("api").expect("api vt parser");
        parser.process(b"test-one\r\n");
        parser.process(b"test-two\r\n");
        state.vt_saw_chunk.insert("api".to_owned(), true);
        state.scroll_offsets.insert("api".to_owned(), usize::MAX);
        state.follow_mode.insert("api".to_owned(), false);

        let view = build_active_view_model(&mut state, 3, 80, true);
        assert!(view.scroll_offset <= view.max_offset);
        assert!(view.scrollbar_total >= view.max_offset);
        assert!(view.active_elapsed >= Duration::from_millis(0));
    }
}
