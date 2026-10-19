//! Graph index freshness check.
//!
//! Pure observation of an optional derived index. A missing index is harmless;
//! stale, refreshing or degraded data names explicit indexing as the next action.

use super::definitions::DoctorCheckContext;
use crate::contracts::{check_id, remediation};
use crate::DoctorState;

pub(super) fn run_graph_index_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    let status = match effigy_codegraph::status(context.resolved_root) {
        Ok(status) => status,
        Err(error) => {
            state.add_check_warning(
                check_id::GRAPH_INDEX,
                format!("graph index probe failed: {error}"),
                remediation::GRAPH_INDEX_REFRESH,
            );
            return;
        }
    };
    let freshness = &status.freshness;
    if freshness.stale && freshness.state != "missing-index" {
        state.add_check_warning(
            check_id::GRAPH_INDEX,
            format!("graph index is not current: {}", freshness.summary),
            remediation::GRAPH_INDEX_REFRESH,
        );
    }
}
