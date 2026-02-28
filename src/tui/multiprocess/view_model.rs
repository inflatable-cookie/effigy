use std::time::{Duration, Instant};

use crate::tui::core::{LogEntry, LogEntryKind};
use vt100::Parser as VtParser;

use super::state::SessionState;
use super::terminal_text::vt_logs;

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
    let active = state.active_process().to_owned();
    let is_follow = *state.follow_mode.get(&active).unwrap_or(&true);

    let (active_logs, scroll_offset, max_offset, render_scroll_offset, scrollbar_total) =
        if vt_emulator_enabled && *state.vt_saw_chunk.get(&active).unwrap_or(&false) {
            let parser = state
                .vt_parsers
                .get_mut(&active)
                .expect("active vt parser missing unexpectedly");
            let stored = *state.scroll_offsets.get(&active).unwrap_or(&0usize);
            let (mut rendered, clamped, max_vt) = vt_logs(
                parser,
                output_height.saturating_sub(1).max(1),
                output_width.max(1),
                stored,
                is_follow,
            );
            if let Some(buffer) = state.logs.get(&active) {
                rendered.extend(buffer.iter().filter_map(|entry| {
                    if matches!(entry.kind, LogEntryKind::Exit) {
                        Some(entry.clone())
                    } else {
                        None
                    }
                }));
            }
            state.scroll_offsets.insert(active.clone(), clamped);
            (
                rendered,
                clamped,
                max_vt,
                0usize,
                max_vt.saturating_add(output_height.max(1)),
            )
        } else {
            let rendered = state
                .logs
                .get(&active)
                .map(|entries| entries.iter().cloned().collect::<Vec<LogEntry>>())
                .unwrap_or_default();
            let max = rendered.len().saturating_sub(output_height);
            let stored = *state.scroll_offsets.get(&active).unwrap_or(&0usize);
            let clamped = stored.min(max);
            state.scroll_offsets.insert(active.clone(), clamped);
            let render = if is_follow { max } else { clamped };
            (
                rendered,
                clamped,
                max,
                render,
                output_height.max(1).saturating_add(max),
            )
        };

    let shell_cursor = if active == "shell" && vt_emulator_enabled {
        state
            .vt_parsers
            .get(&active)
            .map(VtParser::screen)
            .map(|screen| screen.cursor_position())
    } else {
        None
    };

    let now = Instant::now();
    let active_elapsed = state
        .process_started_at
        .get(&active)
        .map(|started| now.saturating_duration_since(*started))
        .unwrap_or_default();
    let active_restart_count = *state.process_restart_count.get(&active).unwrap_or(&0);
    let active_output_seen = *state.output_seen.get(&active).unwrap_or(&false);

    ActiveViewModel {
        active_process: active,
        active_logs,
        scroll_offset,
        max_offset,
        render_scroll_offset,
        scrollbar_total,
        is_follow,
        shell_cursor,
        active_elapsed,
        active_restart_count,
        active_output_seen,
    }
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
