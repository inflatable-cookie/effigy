use std::time::{Duration, Instant};

use super::state::SessionState;

mod scroll;
mod snapshot;
#[cfg(test)]
mod tests;

use scroll::{build_scroll_view, shell_cursor};
use snapshot::active_snapshot;

pub(super) struct ActiveViewModel {
    pub(super) active_process: String,
    pub(super) active_logs: Vec<crate::tui::core::LogEntry>,
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
        .process_started_at_for(&snapshot.name)
        .map(|started| now.saturating_duration_since(started))
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
