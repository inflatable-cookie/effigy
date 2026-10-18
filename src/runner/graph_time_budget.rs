//! Shared wall-clock bound for graph-backed commands.
//!
//! `effigy graph` queries and lazy-refresh consumers (today: `effigy docs
//! context`) share one parser, one bounded execution path, one typed timeout
//! detail, one health snapshot, and one recovery guidance block. A consumer
//! must not grow a second timeout model.

use std::path::Path;
use std::time::Duration;

use super::error::RunnerError;

/// Default wall-clock budget for a single graph command.
///
/// Graph reads refresh a stale index on demand, so any query can turn into a
/// full repo walk. Unbounded, that walk is indistinguishable from a hang: the
/// caller waits forever with no way to tell "slow first index" from "wedged".
/// Two minutes is well clear of a cold index on a pruned tree and still ends.
pub(super) const DEFAULT_GRAPH_TIMEOUT_MS: u64 = 120_000;

/// Env override for the budget. `0` disables the bound entirely.
pub(super) const GRAPH_TIMEOUT_ENV: &str = "EFFIGY_GRAPH_TIMEOUT_MS";

/// Resolve the configured budget, or `None` when the bound is switched off.
pub(super) fn graph_time_budget() -> Option<Duration> {
    let millis = match std::env::var(GRAPH_TIMEOUT_ENV) {
        Ok(raw) => raw
            .trim()
            .parse::<u64>()
            .unwrap_or(DEFAULT_GRAPH_TIMEOUT_MS),
        Err(_) => DEFAULT_GRAPH_TIMEOUT_MS,
    };
    (millis > 0).then(|| Duration::from_millis(millis))
}

/// Run graph work synchronously and stop refresh at its next safe boundary.
pub(super) fn run_bounded_graph_operation(
    repo_root: &Path,
    command: &'static str,
    budget: Duration,
    operation: impl FnOnce() -> Result<String, RunnerError>,
) -> Result<String, RunnerError> {
    run_bounded_graph_value(repo_root, command, budget, operation)
}

/// Same bound, for an operation that yields a typed value instead of rendered
/// output. Cross-repository routing needs the per-repository payload back so a
/// timeout on one neighbour can be reported next to another's results; sharing
/// this function keeps that inside the one timeout model.
pub(super) fn run_bounded_graph_value<T>(
    repo_root: &Path,
    command: &'static str,
    budget: Duration,
    operation: impl FnOnce() -> Result<T, RunnerError>,
) -> Result<T, RunnerError> {
    effigy_codegraph::reset_graph_phase();
    let started = std::time::Instant::now();
    let result = effigy_codegraph::deadline::with_deadline(budget, operation);
    if started.elapsed() >= budget {
        return Err(graph_timeout_error(repo_root, command, budget));
    }
    result
}

fn graph_timeout_error(repo_root: &Path, command: &str, budget: Duration) -> RunnerError {
    let timeout_ms = budget.as_millis().min(u128::from(u64::MAX)) as u64;
    let health = effigy_codegraph::health(repo_root);
    // The operation has stopped; the last phase records its safe boundary.
    let phase = effigy_codegraph::graph_phase_snapshot();
    let mut next = vec![
        "run `effigy graph status --json` to inspect index freshness".to_owned(),
        format!("raise the budget with `{GRAPH_TIMEOUT_ENV}=<ms>` (0 disables it)"),
        "run `effigy graph index --json` once to pay the cold build separately".to_owned(),
        "pass `--stale-index` to read committed files without waiting on a refresh lock".to_owned(),
    ];
    if let Some(phase) = phase.as_ref() {
        next.insert(0, describe_phase(phase));
    }
    let rendered = serde_json::json!({
        "schema": "effigy.graph.timeout.v1",
        "schema_version": 1,
        "command": command,
        "repo_root": repo_root.display().to_string(),
        "timeout_ms": timeout_ms,
        "timeout_env": GRAPH_TIMEOUT_ENV,
        "health": health,
        "phase": phase,
        "committed": true,
        "done": phase.as_ref().and_then(|phase| phase.items_done),
        "total": phase.as_ref().and_then(|phase| phase.items_total),
        "next": next,
    })
    .to_string();
    RunnerError::GraphOperationTimeout {
        command: command.to_owned(),
        timeout_ms,
        rendered,
    }
}

/// One human-readable line naming what the bounded run was doing, so a text
/// reader gets the same answer the additive `phase` field carries.
fn describe_phase(phase: &effigy_codegraph::GraphPhaseSnapshot) -> String {
    match (phase.items_done, phase.items_total) {
        (Some(done), Some(total)) => format!(
            "the bound expired during `{}` after {}ms ({done}/{total} files)",
            phase.name, phase.elapsed_ms
        ),
        _ => format!(
            "the bound expired during `{}` after {}ms",
            phase.name, phase.elapsed_ms
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::describe_phase;
    use effigy_codegraph::GraphPhaseSnapshot;

    #[test]
    fn phase_description_reports_progress_only_when_the_phase_counts_items() {
        let counted = GraphPhaseSnapshot {
            name: "index-files".to_owned(),
            elapsed_ms: 4820,
            items_done: Some(1804),
            items_total: Some(3940),
        };
        assert_eq!(
            describe_phase(&counted),
            "the bound expired during `index-files` after 4820ms (1804/3940 files)"
        );

        let uncounted = GraphPhaseSnapshot {
            name: "docs-rank".to_owned(),
            elapsed_ms: 12,
            items_done: None,
            items_total: None,
        };
        assert_eq!(
            describe_phase(&uncounted),
            "the bound expired during `docs-rank` after 12ms"
        );
    }
}
