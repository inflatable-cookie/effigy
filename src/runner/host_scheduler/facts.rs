//! Fact reporting for the host-run scheduler: nested runs, operator overrides
//! and owned-container started/removed. Facts persist in the client's pending
//! journal before any send and replay until acknowledged, so a down scheduler
//! never loses one and a missing acknowledgement never becomes a success.

use effigy_host_run::{
    container_removed_fact, container_started_fact, journal_facts_offline, nested_fact,
    override_fact, HostRunRoot, SystemClock,
};
use serde_json::Value;

use super::{active_nested, open_client, refuse, root_path, Nested, REPORTED_NESTED};
use crate::runner::error::RunnerError;

/// Record the override durably before any heavy work runs. Failing to journal
/// it refuses the run: an override is never an unrecorded bypass.
pub(super) fn record_override(reason: &str, selector: &str) -> Result<(), RunnerError> {
    let fact = override_fact(reason, selector, &SystemClock)
        .map_err(|error| refuse(75, format!("scheduler override was not recorded: {error}")))?;
    let path = root_path()?;
    let root = HostRunRoot::open_journal(&path, true)
        .map_err(|error| refuse(75, format!("scheduler override was not recorded: {error}")))?;
    journal_facts_offline(&root, std::slice::from_ref(&fact))
        .map_err(|error| refuse(75, format!("scheduler override was not recorded: {error}")))?;
    eprintln!(
        "warning: {} override recorded for `{selector}`: {reason}",
        super::OVERRIDE_ENV
    );
    flush_best_effort();
    Ok(())
}

pub(super) fn report_nested_once(nested: &Nested, selector: &str) {
    let first = REPORTED_NESTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(selector.to_owned());
    if !first {
        return;
    }
    match nested_fact(&nested.run_id, selector, &SystemClock) {
        Ok(fact) => report_best_effort(fact),
        Err(error) => eprintln!("warning: nested host-run fact was not built: {error}"),
    }
}

/// Report an owned container started under the active scheduler run. No-op
/// outside a validated scheduler run: there is no run to attribute it to.
pub(in crate::runner) fn report_container_started(runtime: &str, container_id: &str) {
    let Some(nested) = active_nested() else {
        return;
    };
    match container_started_fact(
        &nested.run_id,
        nested.epoch,
        runtime,
        container_id,
        &SystemClock,
    ) {
        Ok(fact) => report_best_effort(fact),
        Err(error) => eprintln!("warning: container started fact was not built: {error}"),
    }
}

/// Report an owned container's removal result. `Some(true)` only when removal
/// was confirmed; a failed removal is `Some(false)` and an unobserved one is
/// `None` (reported as `"unknown"`). Never inferred from the host process group.
pub(in crate::runner) fn report_container_removed(
    runtime: &str,
    container_id: &str,
    removed: Option<bool>,
) {
    let Some(nested) = active_nested() else {
        return;
    };
    match container_removed_fact(
        &nested.run_id,
        nested.epoch,
        runtime,
        container_id,
        removed,
        &SystemClock,
    ) {
        Ok(fact) => report_best_effort(fact),
        Err(error) => eprintln!("warning: container removed fact was not built: {error}"),
    }
}

fn flush_best_effort() {
    if let Ok((mut client, _)) = open_client() {
        let _ = client.flush_pending_facts();
    }
}

/// Send one fact, keeping it in the pending journal on any failure.
fn report_best_effort(fact: Value) {
    let facts = [fact];
    let reported = open_client()
        .ok()
        .map(|(mut client, _)| client.report_facts(&facts));
    if matches!(reported, Some(Ok(_))) {
        return;
    }
    // report_facts journals before sending; this covers a client that could not
    // be opened or failed before journaling. Identical copies are a no-op.
    let journaled = root_path()
        .ok()
        .and_then(|path| HostRunRoot::open_journal(&path, false).ok())
        .map(|root| journal_facts_offline(&root, &facts));
    match (reported, journaled) {
        (_, Some(Ok(()))) => {
            eprintln!("warning: host-run fact is pending acknowledgement; it will replay on the next scheduler call");
        }
        _ => eprintln!("warning: host-run fact could not be journaled and was not reported"),
    }
}
