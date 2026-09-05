//! Shared wall-clock bound for graph-backed commands.
//!
//! `effigy graph` queries and lazy-refresh consumers (today: `effigy docs
//! context`) share one parser, one bounded execution path, one typed timeout
//! detail, one health snapshot, and one recovery guidance block. A consumer
//! must not grow a second timeout model.

use std::path::Path;
use std::sync::mpsc;
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

/// Run a graph-backed operation on a worker thread and give up on it after
/// `budget`.
///
/// The worker is deliberately detached rather than cancelled: graph work runs
/// under a cross-process refresh lock that the OS releases when the process
/// exits, and a half-written index is worse than a slow one. The caller gets a
/// bounded failure carrying the health snapshot; the CLI exits right after.
///
/// The operation is cloned for the worker so a spawn failure can still run it
/// unbounded on the caller thread — an unbounded run beats refusing.
pub(super) fn run_bounded_graph_operation(
    repo_root: &Path,
    command: &'static str,
    budget: Duration,
    operation: impl Fn() -> Result<String, RunnerError> + Send + Clone + 'static,
) -> Result<String, RunnerError> {
    run_bounded_graph_value(repo_root, command, budget, operation)
}

/// Same bound, for an operation that yields a typed value instead of rendered
/// output. Cross-repository routing needs the per-repository payload back so a
/// timeout on one neighbour can be reported next to another's results; sharing
/// this function keeps that inside the one timeout model.
pub(super) fn run_bounded_graph_value<T: Send + 'static>(
    repo_root: &Path,
    command: &'static str,
    budget: Duration,
    operation: impl Fn() -> Result<T, RunnerError> + Send + Clone + 'static,
) -> Result<T, RunnerError> {
    // Clear any phase left by earlier graph work in this process, so a bound
    // that expires before the worker starts reports no phase rather than a
    // previous command's.
    effigy_codegraph::reset_graph_phase();
    let (sender, receiver) = mpsc::channel();
    let worker_operation = operation.clone();
    let spawned = std::thread::Builder::new()
        .name(command.to_owned())
        .spawn(move || {
            let _ = sender.send(worker_operation());
        });
    if spawned.is_err() {
        // No worker thread available: an unbounded run still beats refusing.
        return operation();
    }
    match receiver.recv_timeout(budget) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(graph_timeout_error(repo_root, command, budget))
        }
        // The worker died without sending; surface it as a bounded failure too.
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(graph_timeout_error(repo_root, command, budget))
        }
    }
}

fn graph_timeout_error(repo_root: &Path, command: &str, budget: Duration) -> RunnerError {
    let timeout_ms = budget.as_millis().min(u128::from(u64::MAX)) as u64;
    let health = effigy_codegraph::health(repo_root);
    // Additive diagnostics: the detached worker keeps running, so its current
    // phase is still readable here. Absent when nothing was recorded, so the
    // field never claims knowledge the runtime does not have.
    let phase = effigy_codegraph::graph_phase_snapshot();
    let mut next = vec![
        "run `effigy graph status --json` to inspect index freshness".to_owned(),
        format!("raise the budget with `{GRAPH_TIMEOUT_ENV}=<ms>` (0 disables it)"),
        "run `effigy graph index --json` once to pay the cold build separately".to_owned(),
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
