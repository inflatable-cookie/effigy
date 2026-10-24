//! User-owned cutover. Every lookup and the entire busy report precede staging.
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{self, discovery};
use base64::Engine as _;
use effigy_kernel::ctx::{Deadline, OpenReason};
use effigy_kernel::migration::{self, MigrationError};
use effigy_kernel::observe::Observed;
use effigy_kernel::root::StateRoot;
use effigy_kernel::store::StoreError;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(super) fn run(
    repo_override: Option<PathBuf>,
    plan_only: bool,
    launchers_stopped: bool,
    extra_checkouts: Vec<PathBuf>,
    stop: Option<String>,
    json: bool,
) -> Result<String, RunnerError> {
    let home = task_engine::state_home_path().ok_or_else(|| error("HOME is unset or relative"))?;
    let observe =
        task_engine::reading_context(Deadline::At(Instant::now() + Duration::from_secs(30)))
            .observe()
            .map_err(|_| error("migration observation context"))?;
    let source = match StateRoot::legacy_observe(&observe, &home) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => {
            return Ok(render(
                serde_json::json!({"schema":SCHEMA,"state":"no-legacy-state"}),
                "No legacy runtime state exists.",
                json,
            ))
        }
        Observed::Unknown(reason) => {
            return Err(error(format!(
                "legacy runtime state is unknown: {reason:?}"
            )))
        }
    };
    if let Some(run) = stop {
        if let Some(encoded) = run.strip_prefix("writer:") {
            if encoded.len() > 4096 {
                return Err(error("legacy writer receipt is oversized"));
            }
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(encoded)
                .map_err(error)?;
            let target: migration::WriterTarget = serde_json::from_slice(&bytes).map_err(error)?;
            let cx = task_engine::effect_context(OpenReason::Shell).child(Duration::from_secs(30));
            let signalled = migration::stop_legacy_writer(&cx, &source, &target).map_err(error)?;
            let accepted = matches!(signalled, Observed::Verified(..));
            let rendered=render(serde_json::json!({"schema":SCHEMA,"state":if accepted{"stop-requested"}else{"stop-refused"},"target":target,"signalled":signalled}),
                &format!("Legacy writer stop: {signalled:?}. Retry `effigy run migrate --plan` after settlement."),json);
            return if accepted {
                Ok(rendered)
            } else {
                refuse(rendered, json)
            };
        }
        let run = run.parse().map_err(|_| {
            error("`--stop` requires the exact legacy run id from the blocking report")
        })?;
        let cx = task_engine::effect_context(OpenReason::Shell).child(Duration::from_secs(120));
        let report =
            migration::request_legacy_stop(&cx, &source, run, task_engine::fresh_request_id()?)
                .map_err(error)?;
        return Ok(render(serde_json::json!({"schema":SCHEMA,"state":"stop-requested","report":report}),
            &format!("Legacy stop requested for {run}. Let its owner settle, then run `effigy run migrate --plan`."),json));
    }
    let committed = migration::committed(&observe, &home).map_err(error)?;
    if committed.is_none() && !migration::legacy_present(&observe, &source).map_err(error)? {
        return Ok(render(
            serde_json::json!({"schema":SCHEMA,"state":"no-legacy-state"}),
            "No legacy runtime database exists.",
            json,
        ));
    }
    let preflight = if committed.is_none() {
        Some(migration::inspect(&observe, &source).map_err(error)?)
    } else {
        None
    };
    if let Some(preflight) = preflight.as_ref().filter(|report| !report.ready()) {
        return blocked(preflight, json);
    }
    let cwd = crate::runner::command_context::current_working_dir()?;
    let current =
        match crate::runner::command_context::resolve_active_command_context(repo_override.clone())
        {
            Ok(context) => context.resolved.resolved_root,
            Err(failure) if repo_override.is_some() => return Err(failure),
            Err(_) => cwd.clone(),
        };
    let config = effigy_manifest::load_user_config().map_err(error)?;
    let mut roots = config.runtime.checkout_roots;
    roots.extend(extra_checkouts.into_iter().map(|path| {
        if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        }
    }));
    let discovery_cx =
        task_engine::reading_context(Deadline::At(Instant::now() + Duration::from_secs(5)))
            .observe()
            .map_err(|_| error("discovery observation context"))?;
    let found = discovery::collect(&discovery_cx, &current, &roots);
    let import_plan = if committed.is_some() {
        None
    } else {
        Some(match migration::plan(&observe, &source) {
            Ok(plan) => plan,
            Err(MigrationError::Blocked(report)) => return blocked(&report, json),
            Err(failure) => return Err(error(failure)),
        })
    };
    // Resolve every retained checkout before import can create any state. An
    // unlocatable checkout that owns only completed history is dropped (Q-010);
    // every unlocatable live partition is reported together.
    let mut destinations = BTreeMap::new();
    let mut dropped = migration::DroppedHistory::default();
    let mut unlocated_live = Vec::new();
    for partition in import_plan
        .as_ref()
        .into_iter()
        .flat_map(|plan| &plan.partitions)
    {
        if let Some(checkout) = partition.checkout {
            let path = match found.resolve(
                &observe,
                checkout,
                partition.path_hint.as_deref().map(Path::new),
            ) {
                Ok(path) => path,
                Err(StoreError::CheckoutUnlocated(_)) if !partition.live => {
                    dropped.record(partition);
                    continue;
                }
                Err(StoreError::CheckoutUnlocated(_)) => {
                    unlocated_live.push(partition);
                    continue;
                }
                Err(failure) => return Err(error(failure)),
            };
            add_destination(
                &mut destinations,
                checkout,
                path.root,
                &[],
                partition.unresolved_runs,
            );
        }
    }
    if !unlocated_live.is_empty() {
        return Err(error(migration::unlocated_live_report(&unlocated_live)));
    }
    if let Some(receipt) = committed.as_ref() {
        for destination in &receipt.destinations {
            if let Some(checkout) = destination.checkout {
                let path = found
                    .resolve(&observe, checkout, Some(&destination.checkout_root))
                    .map_err(error)?;
                for alias in &destination.checkout_aliases {
                    found
                        .resolve(&observe, *alias, Some(&destination.checkout_root))
                        .map_err(error)?;
                }
                add_destination(
                    &mut destinations,
                    checkout,
                    path.root,
                    &destination.checkout_aliases,
                    destination.unresolved_runs,
                );
            }
        }
    }
    if committed.is_none() {
        let mut busy = Vec::new();
        for row in destinations.values() {
            let location = migration::CheckoutLocation {
                root: row.root.clone(),
                git_directory: effigy_core::worktree_scope::git_directory(&row.root)
                    .map_err(error)?,
            };
            if let Some(blocker) =
                migration::destination_blocker(&observe, &location).map_err(error)?
            {
                busy.push(blocker);
            }
        }
        if !busy.is_empty() {
            return blocked_destinations(&busy, json);
        }
    }
    if plan_only || (!launchers_stopped && committed.is_none()) {
        let state = if committed.is_some() {
            "committed"
        } else if plan_only {
            "planned"
        } else {
            "needs-quiescence"
        };
        let mut text=format!("Runtime migration: {state}\n{} retained checkout journal(s); {} legacy resources; {} unresolved runs.\n",destinations.len(),import_plan.as_ref().map_or(0,|plan|plan.preflight.resources),import_plan.as_ref().map_or(0,|plan|plan.preflight.unresolved_runs));
        for row in destinations.values() {
            text.push_str(&format!("  {}\n", row.root.display()));
        }
        if dropped.checkouts > 0 {
            text.push_str(&format!(
                "{} recorded checkout(s) with no verified location hold only completed history and are dropped ({} resources, {} runs).\n",
                dropped.checkouts, dropped.resources, dropped.runs
            ));
        }
        if !plan_only {
            text.push_str("Withdraw old Effigy launchers, let the reported writers settle, then run `effigy run migrate --legacy-launchers-stopped`. No destination state has been created.");
        }
        let payload = serde_json::json!({"schema":SCHEMA,"state":state,"source":source.path(),"plan":import_plan,"committed":committed,"destinations":destinations.into_values().collect::<Vec<_>>(),"dropped_history":dropped,"discovery":found});
        let rendered = render(payload, &text, json);
        return if plan_only {
            Ok(rendered)
        } else {
            refuse(rendered, json)
        };
    }
    let cx = task_engine::effect_context(OpenReason::Shell).child(Duration::from_secs(1800));
    let lookup = cx
        .operation()
        .observe()
        .map_err(|_| error("migration lookup context"))?;
    let result = migration::import(
        &cx,
        &source,
        &home,
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |checkout, hint, _git| found.resolve(&lookup, checkout, hint),
    );
    let report = match result {
        Ok(report) => report,
        Err(MigrationError::Blocked(report)) => return blocked(&report, json),
        Err(MigrationError::DestinationBlocked(report)) => {
            return blocked_destinations(&[report], json)
        }
        Err(failure) => return Err(error(failure)),
    };
    let mut text = format!("Runtime migration committed in {} ms.\n", report.elapsed_ms);
    let dropped = &report.receipt.dropped_history;
    if dropped.checkouts > 0 {
        text.push_str(&format!(
            "  dropped completed history of {} unlocatable checkout(s): {} resources, {} runs\n",
            dropped.checkouts, dropped.resources, dropped.runs
        ));
    }
    for destination in &report.receipt.destinations {
        text.push_str(&format!(
            "  {}: {} rows, {} unresolved runs\n",
            destination.checkout_root.display(),
            destination.rows.values().sum::<u64>(),
            destination.unresolved_runs
        ));
    }
    Ok(render(
        serde_json::json!({"schema":SCHEMA,"state":"committed","report":report,"discovery":found}),
        text.trim_end(),
        json,
    ))
}

const SCHEMA: &str = "effigy.runtime-migration.v1";
fn blocked_destinations<T>(
    reports: &[migration::DestinationBlocker],
    json: bool,
) -> Result<T, RunnerError> {
    let mut blockers = Vec::new();
    let mut text = String::from("Runtime migration blocked before staging by checkout custody:\n");
    for report in reports {
        text.push_str(&format!(
            "  {}: {}\n",
            report.checkout_root.display(),
            report.reason
        ));
        if report.writers.is_empty() {
            blockers.push(serde_json::json!({"writer": {"checkout_root": report.checkout_root, "role": "unverified checkout scope holder", "reason": report.reason},
                "stop_command": null, "drain_check": ["effigy", "run", "migrate", "--plan"]}));
        }
        for writer in &report.writers {
            let live = matches!(
                writer.state,
                Observed::Verified(
                    effigy_kernel::ident::ProcessState::Active
                        | effigy_kernel::ident::ProcessState::Stopped,
                    _
                )
            );
            let stop = live
                .then(|| {
                    writer
                        .run
                        .as_deref()?
                        .parse::<effigy_kernel::id::RunId>()
                        .ok()
                })
                .flatten()
                .map(|id| format!("effigy run stop {id}"));
            text.push_str(&format!(
                "    run {:?}; pid {}; start {:?}; boot {:?}; state {:?}\n",
                writer.run, writer.pid, writer.start, writer.boot, writer.state
            ));
            if let Some(stop) = &stop {
                text.push_str(&format!(
                    "    stop in {}: {stop}\n",
                    report.checkout_root.display()
                ));
            }
            blockers.push(serde_json::json!({"writer": {"checkout_root": report.checkout_root, "role": "foreground checkout owner", "holder": writer},
                "stop_command": stop, "stop_cwd": report.checkout_root, "drain_check": ["effigy", "run", "migrate", "--plan"]}));
        }
        text.push_str(
            "    drain: let this checkout's work finish; retry `effigy run migrate --plan`\n",
        );
    }
    refuse(
        render(
            serde_json::json!({"schema":SCHEMA,"state":"blocked","staging_started":false,"blockers":blockers}),
            &text,
            json,
        ),
        json,
    )
}
fn error(reason: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(reason.to_string())
}
fn render(mut payload: serde_json::Value, text: &str, json: bool) -> String {
    payload["schema_version"] = serde_json::json!(1);
    if json {
        payload.to_string()
    } else {
        text.to_owned()
    }
}
fn refuse<T>(rendered: String, json: bool) -> Result<T, RunnerError> {
    Err(if json {
        RunnerError::CommandJsonFailure { rendered }
    } else {
        error(rendered)
    })
}
fn blocked<T>(report: &migration::Preflight, json: bool) -> Result<T, RunnerError> {
    let mut text=String::from("Runtime migration blocked before staging. These exact legacy writers must stop or drain:\n");
    let blockers=report.blockers.iter().map(|blocker|{
        let stop=migration::WriterTarget::from_blocker(blocker)
            .and_then(|target|serde_json::to_vec(&target).ok())
            .map(|bytes|format!("effigy run migrate --stop writer:{}",base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)))
            .or_else(||blocker.run.as_ref().map(|run|format!("effigy run migrate --stop {run}")));
        text.push_str(&format!("  {}/{}; {}; run {}; process {:?}; state {:?}\n",blocker.scope,blocker.resource,blocker.role,blocker.run.as_deref().unwrap_or("unverified"),blocker.process,blocker.state));
        if let Some(command)=&stop{text.push_str(&format!("    stop: {command}\n"));}
        text.push_str("    drain: let this operation settle; retry `effigy run migrate --plan`\n");
        serde_json::json!({"writer":blocker,"stop_command":stop,"drain_check":["effigy","run","migrate","--plan"]})
    }).collect::<Vec<_>>();
    refuse(
        render(
            serde_json::json!({"schema":SCHEMA,"state":"blocked","staging_started":false,"blockers":blockers}),
            &text,
            json,
        ),
        json,
    )
}

#[derive(Debug, Serialize)]
struct PlannedDestination {
    checkout: effigy_kernel::id::CheckoutId,
    checkout_aliases: Vec<effigy_kernel::id::CheckoutId>,
    root: PathBuf,
    unresolved_runs: usize,
}
fn add_destination(
    destinations: &mut BTreeMap<PathBuf, PlannedDestination>,
    checkout: effigy_kernel::id::CheckoutId,
    root: PathBuf,
    aliases: &[effigy_kernel::id::CheckoutId],
    unresolved: usize,
) {
    let entry = destinations
        .entry(root.clone())
        .or_insert_with(|| PlannedDestination {
            checkout,
            checkout_aliases: Vec::new(),
            root,
            unresolved_runs: 0,
        });
    for id in std::iter::once(checkout).chain(aliases.iter().copied()) {
        if id != entry.checkout && !entry.checkout_aliases.contains(&id) {
            entry.checkout_aliases.push(id);
        }
    }
    entry.unresolved_runs += unresolved;
}
