use super::super::state::SessionState;

#[derive(Debug, Clone)]
pub(super) struct ActiveSnapshot {
    pub(super) name: String,
    pub(super) is_follow: bool,
    pub(super) stored_offset: usize,
    pub(super) vt_has_chunks: bool,
    pub(super) output_seen: bool,
    pub(super) restart_count: usize,
}

pub(super) fn active_snapshot(state: &SessionState) -> ActiveSnapshot {
    let name = state.active_process().to_owned();
    ActiveSnapshot {
        is_follow: state.follow_for(&name),
        stored_offset: state.scroll_offset_for(&name),
        vt_has_chunks: state.vt_saw_chunk_for(&name),
        output_seen: state.output_seen_for(&name),
        restart_count: state.restart_count_for(&name),
        name,
    }
}
