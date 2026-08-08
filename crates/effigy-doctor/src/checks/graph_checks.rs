//! Graph index freshness check.
//!
//! Report-only, like `effigy graph status`: a missing index is self-healing
//! (queries and gated scans rebuild on demand), so nothing is reported until
//! an index exists but is stale or degraded — the remediation then points at
//! the one-command self-heal path (`effigy graph status --refresh`).

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
    // `missing-index` is self-healing (queries and gated scans build on
    // demand), so only a present-but-troubled index is worth flagging.
    if matches!(freshness.state.as_str(), "refresh-recommended" | "degraded") {
        state.add_check_warning(
            check_id::GRAPH_INDEX,
            format!("graph index is not current: {}", freshness.summary),
            remediation::GRAPH_INDEX_REFRESH,
        );
    }
}
