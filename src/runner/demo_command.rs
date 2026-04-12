use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command as ProcessCommand, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::{setpgid, Pid};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::process_manager::{ProcessEvent, ProcessEventKind, ProcessSpec, ProcessSupervisor};
use crate::runner::catalog::select_catalog_and_task;
use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::execute::run_manifest_task_with_cwd;
use crate::runner::managed::command::resolve_managed_task_plan;
use crate::runner::manifest::{
    load_task_manifest_with_inspection, LoadedTaskManifest, ManifestDemoConfig, ManifestDemoMode,
    ManifestDemoStatus, ManifestTask,
};
use crate::runner::model::catalog::{LoadedCatalog, TaskRuntimeArgs, TaskSelection, TaskSelector};
use crate::runner::util::parse_task_selector;
use crate::runner::util::with_local_node_bin_path;
use crate::tui::run_demo_browser_tui;
use crate::ui::{KeyValue, NoticeLevel, PlainRenderer, Renderer, TableSpec};
use crate::{
    DemoArgs, DemoHistoryOutcome, DemoListGroupBy, DemoListQuery, DemoListStatus, DemoSubcommand,
    TaskInvocation,
};

use super::error::RunnerError;
use super::render::{encode_json, render_utf8, text_renderer};

const DEMO_RECEIPTS_DIR: &str = ".effigy/demo/receipts";
const DEMO_ACTIVE_DIR: &str = ".effigy/demo/active";
const DEMO_LOGS_DIR: &str = ".effigy/demo/logs";
const DEMO_HISTORY_DIR: &str = ".effigy/demo/history";
const DEMO_ATTEMPT_HISTORY_LIMIT: usize = 10;
const DEMO_ACTIVE_TERMINAL_RECENT_LINES: usize = 8;
const DEMO_INPUT_POLL_INTERVAL_MS: u64 = 40;
const DEMO_MANAGED_EVENT_POLL_INTERVAL_MS: u64 = 100;
const DEMO_STREAM_DRAIN_POLLS_AFTER_EXIT: usize = 3;
const DEMO_DEFAULT_TERMINAL_COLS: u16 = 80;
const DEMO_DEFAULT_TERMINAL_ROWS: u16 = 24;

pub(super) fn run_demo(args: DemoArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let manifest_path = repo_root.join("effigy.toml");
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;

    match args.subcommand {
        DemoSubcommand::Browser { group_by } => {
            if args.output_json {
                return demo_error(
                    true,
                    "effigy.demo.browser.v1",
                    "demo browser does not support json mode".to_owned(),
                    json!({ "repo_root": repo_root.display().to_string() }),
                );
            }
            run_demo_browser_tui(repo_root, group_by)?;
            Ok(String::new())
        }
        DemoSubcommand::List { query } => {
            render_demo_list(&repo_root, &loaded, &query, args.output_json)
        }
        DemoSubcommand::Inspect { demo_id } => {
            render_demo_inspect(&repo_root, &loaded, &demo_id, args.output_json)
        }
        DemoSubcommand::History {
            demo_id,
            limit,
            outcome,
            attempt_id,
            attempt_ordinal,
        } => render_demo_history(
            &repo_root,
            &loaded,
            &demo_id,
            limit,
            outcome,
            attempt_id.as_deref(),
            attempt_ordinal,
            args.output_json,
        ),
        DemoSubcommand::Run { demo_id } => render_demo_execute(
            &repo_root,
            &loaded,
            &demo_id,
            args.output_json,
            DemoInvocationKind::Run,
        ),
        DemoSubcommand::Rerun { demo_id } => render_demo_execute(
            &repo_root,
            &loaded,
            &demo_id,
            args.output_json,
            DemoInvocationKind::Rerun,
        ),
        DemoSubcommand::Stop { demo_id } => {
            render_demo_stop(&repo_root, &loaded, &demo_id, args.output_json)
        }
        DemoSubcommand::Input {
            demo_id,
            text,
            append_newline,
        } => render_demo_input(
            &repo_root,
            &loaded,
            &demo_id,
            &text,
            append_newline,
            args.output_json,
        ),
        DemoSubcommand::Resize {
            demo_id,
            cols,
            rows,
        } => render_demo_resize(&repo_root, &loaded, &demo_id, cols, rows, args.output_json),
    }
}

fn render_demo_list(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    query: &DemoListQuery,
    output_json: bool,
) -> Result<String, RunnerError> {
    let all_demos = loaded
        .manifest
        .demos
        .iter()
        .map(|(demo_id, demo)| build_demo_record(repo_root, loaded, demo_id, demo))
        .collect::<Result<Vec<_>, _>>()?;
    let demos = all_demos
        .into_iter()
        .filter(|demo| demo.matches_query(query))
        .collect::<Vec<_>>();
    let groups = query
        .group_by
        .map(|group_by| build_demo_groups(&demos, group_by));

    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.list.v1",
                "schema_version": 1,
                "ok": true,
                "repo_root": repo_root.display().to_string(),
                "query": demo_list_query_to_json(query),
                "group_by": query.group_by.map(|value| value.as_str()),
                "count": demos.len(),
                "total_count": loaded.manifest.demos.len(),
                "groups": groups.as_ref().map(|groups| groups.iter().map(DemoGroup::to_json).collect::<Vec<_>>()),
                "demos": demos.iter().map(DemoRecord::to_json_summary).collect::<Vec<_>>(),
            }),
            true,
        );
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Registry")?;
    if demos.is_empty() {
        if query_is_empty(query) {
            renderer.notice(
                NoticeLevel::Info,
                "No demos are declared in the current effigy.toml manifest.",
            )?;
        } else {
            renderer.notice(
                NoticeLevel::Info,
                "No demos matched the current discovery query.",
            )?;
        }
        renderer.text("")?;
        return render_utf8(renderer.into_inner());
    }

    if !query_is_empty(query) {
        renderer.key_values(&demo_list_query_to_key_values(query))?;
        renderer.text("")?;
    }

    if let Some(groups) = groups {
        for group in groups {
            renderer.section(&format!("Group: {}", group.label))?;
            renderer.table(&demo_table_spec(&group.demos))?;
            renderer.text("")?;
        }
    } else {
        let demo_refs = demos.iter().collect::<Vec<_>>();
        renderer.table(&demo_table_spec(&demo_refs))?;
    }
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy demo inspect <DEMO_ID>` to inspect proof intent, coverage, action availability, active state, and latest attempt details.",
    )?;
    renderer.text("")?;
    render_utf8(renderer.into_inner())
}

fn render_demo_inspect(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.inspect.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.inspect.v1",
                "schema_version": 1,
                "ok": true,
                "repo_root": repo_root.display().to_string(),
                "demo": record.to_json_detail(),
            }),
            true,
        );
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Inspect")?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("summary", record.summary.clone()),
        KeyValue::new("proof", record.proof.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("mode", record.mode.as_str().to_owned()),
        KeyValue::new("base-status", record.status.as_str().to_owned()),
        KeyValue::new("effective-status", record.effective_status()),
        KeyValue::new("freshness", record.freshness_label().to_owned()),
        KeyValue::new("gap", record.gap_class.to_owned()),
        KeyValue::new("entrypoint", record.entrypoint.render_full()),
        KeyValue::new("defined-in", record.primary_source.clone()),
    ])?;
    renderer.text("")?;

    if !record.covers.is_empty() {
        renderer.bullet_list("covers", &record.covers)?;
        renderer.text("")?;
    }
    if !record.tags.is_empty() {
        renderer.bullet_list("tags", &record.tags)?;
        renderer.text("")?;
    }
    if record.sources.len() > 1 {
        renderer.bullet_list("sources", &record.sources)?;
        renderer.text("")?;
    }
    if !record.prerequisites.is_empty() {
        renderer.bullet_list("prerequisites", &record.prerequisites)?;
        renderer.text("")?;
    }
    if !record.dependencies.is_empty() {
        renderer.bullet_list("dependencies", &record.dependencies)?;
        renderer.text("")?;
    }

    renderer.section("Actions")?;
    renderer.key_values(&record.actions().to_key_values())?;
    renderer.text("")?;

    renderer.section("Active Attempt")?;
    renderer.key_values(&record.active_attempt.to_key_values())?;
    renderer.text("")?;

    renderer.section("Active Terminal Session")?;
    renderer.key_values(&record.active_terminal_session.to_key_values())?;
    if !record.active_terminal_session.recent_stdout.is_empty() {
        renderer.text("")?;
        renderer.bullet_list(
            "recent-stdout",
            &record.active_terminal_session.recent_stdout,
        )?;
    }
    if !record.active_terminal_session.recent_stderr.is_empty() {
        renderer.text("")?;
        renderer.bullet_list(
            "recent-stderr",
            &record.active_terminal_session.recent_stderr,
        )?;
    }
    renderer.text("")?;

    renderer.section("Latest Attempt")?;
    let mut latest_values = vec![
        KeyValue::new("state", record.latest_attempt.state_label()),
        KeyValue::new(
            "receipt",
            record
                .latest_attempt
                .receipt_path
                .clone()
                .unwrap_or_else(|| "<none>".to_owned()),
        ),
    ];
    if let Some(outcome) = &record.latest_attempt.outcome {
        latest_values.push(KeyValue::new("outcome", outcome.clone()));
    }
    if let Some(summary) = &record.latest_attempt.summary {
        latest_values.push(KeyValue::new("summary", summary.clone()));
    }
    if let Some(parse_error) = &record.latest_attempt.parse_error {
        latest_values.push(KeyValue::new("receipt-parse", parse_error.clone()));
    }
    if let Some(stdout_log_path) = &record.latest_attempt.stdout_log_path {
        latest_values.push(KeyValue::new("stdout-log", stdout_log_path.clone()));
    }
    if let Some(stderr_log_path) = &record.latest_attempt.stderr_log_path {
        latest_values.push(KeyValue::new("stderr-log", stderr_log_path.clone()));
    }
    renderer.key_values(&latest_values)?;
    if !record.latest_attempt.artifacts.is_empty() {
        renderer.text("")?;
        renderer.bullet_list("artifacts", &record.latest_attempt.artifacts)?;
    }
    if record.attempt_history.parse_error.is_some() || !record.attempt_history.attempts.is_empty() {
        renderer.text("")?;
        renderer.section("Recent Attempts")?;
        if let Some(parse_error) = &record.attempt_history.parse_error {
            renderer.key_values(&[KeyValue::new("history-parse", parse_error.clone())])?;
        }
        if !record.attempt_history.attempts.is_empty() {
            renderer.table(&recent_attempts_table_spec(
                &record.attempt_history.attempts,
            ))?;
        }
    }
    renderer.text("")?;
    render_utf8(renderer.into_inner())
}

fn render_demo_history(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    limit: Option<usize>,
    outcome: Option<DemoHistoryOutcome>,
    selected_attempt_id: Option<&str>,
    selected_attempt_ordinal: Option<usize>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.history.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    if selected_attempt_id.is_some() && selected_attempt_ordinal.is_some() {
        return demo_error(
            output_json,
            "effigy.demo.history.v1",
            "choose either `--attempt <ATTEMPT_ID>` or `--ordinal <N>`, not both".to_owned(),
            json!({
                "demo_id": demo_id,
                "attempt_id": selected_attempt_id,
                "ordinal": selected_attempt_ordinal,
            }),
        );
    }

    let filtered_attempts = history_attempts_with_outcome(&record.attempt_history, outcome);
    let displayed_attempts = history_attempts_with_limit(&filtered_attempts, limit);
    let filtered_count = filtered_attempts.len();
    let displayed_count = displayed_attempts.len();
    let stored_count = record.attempt_history.attempts.len();
    let selected_attempt = match (selected_attempt_id, selected_attempt_ordinal) {
        (Some(attempt_id), None) => {
            let Some(attempt) =
                find_historical_attempt(&record.attempt_history.attempts, attempt_id)
            else {
                return demo_error(
                    output_json,
                    "effigy.demo.history.v1",
                    format!("retained attempt `{attempt_id}` was not found for demo `{demo_id}`"),
                    json!({
                        "demo_id": demo_id,
                        "attempt_id": attempt_id,
                    }),
                );
            };
            Some(attempt)
        }
        (None, Some(ordinal)) => {
            let Some(attempt) = displayed_attempts.get(ordinal - 1).copied() else {
                return demo_error(
                    output_json,
                    "effigy.demo.history.v1",
                    format!(
                        "retained attempt ordinal `{ordinal}` was not found in the current history result for demo `{demo_id}`"
                    ),
                    json!({
                        "demo_id": demo_id,
                        "ordinal": ordinal,
                        "displayed_count": displayed_count,
                    }),
                );
            };
            Some(attempt)
        }
        (Some(_), Some(_)) => unreachable!("selection flags are mutually exclusive"),
        (None, None) => None,
    };

    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.history.v1",
                "schema_version": 1,
                "ok": true,
                "repo_root": repo_root.display().to_string(),
                "query": {
                    "demo_id": demo_id,
                    "limit": limit,
                    "outcome": outcome.map(DemoHistoryOutcome::as_str),
                    "attempt_id": selected_attempt_id,
                    "ordinal": selected_attempt_ordinal,
                },
                "demo": {
                    "id": record.id,
                    "title": record.title,
                    "owner": record.owner,
                    "entrypoint": record.entrypoint.to_json(),
                    "defined_in": record.primary_source,
                },
                "active_attempt": record.active_attempt.to_json(),
                "latest_attempt": record.latest_attempt.to_json(),
                "attempt_history": {
                    "path": record.attempt_history.path,
                    "stored_count": stored_count,
                    "filtered_count": filtered_count,
                    "displayed_count": displayed_count,
                    "count": displayed_count,
                    "limit": limit,
                    "outcome": outcome.map(DemoHistoryOutcome::as_str),
                    "parse_error": record.attempt_history.parse_error,
                    "attempts": displayed_attempts.iter().enumerate().map(|(index, attempt)| {
                        history_attempt_to_json(index + 1, attempt)
                    }).collect::<Vec<_>>(),
                },
                "selected_attempt": selected_attempt.map(DemoHistoricalAttempt::to_json),
            }),
            true,
        );
    }

    let mut renderer = text_renderer();
    renderer.section("Demo History")?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("entrypoint", record.entrypoint.render_full()),
        KeyValue::new(
            "history-path",
            record
                .attempt_history
                .path
                .clone()
                .unwrap_or_else(|| "<none>".to_owned()),
        ),
        KeyValue::new("stored-attempts", stored_count.to_string()),
        KeyValue::new("matching-attempts", filtered_count.to_string()),
        KeyValue::new("showing", displayed_count.to_string()),
        KeyValue::new(
            "limit",
            limit
                .map(|value| value.to_string())
                .unwrap_or_else(|| "all".to_owned()),
        ),
        KeyValue::new(
            "outcome",
            outcome
                .map(|value| value.as_str().to_owned())
                .unwrap_or_else(|| "all".to_owned()),
        ),
        KeyValue::new(
            "selected-attempt",
            selected_attempt_id.unwrap_or("none").to_owned(),
        ),
        KeyValue::new(
            "selected-ordinal",
            selected_attempt_ordinal
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_owned()),
        ),
    ])?;
    renderer.text("")?;

    renderer.section("Latest Result")?;
    let mut latest_values = vec![KeyValue::new(
        "state",
        record.latest_attempt.state_label().to_owned(),
    )];
    if let Some(outcome) = &record.latest_attempt.outcome {
        latest_values.push(KeyValue::new("outcome", outcome.clone()));
    }
    if let Some(summary) = &record.latest_attempt.summary {
        latest_values.push(KeyValue::new("summary", summary.clone()));
    }
    if let Some(receipt_path) = &record.latest_attempt.receipt_path {
        latest_values.push(KeyValue::new("receipt", receipt_path.clone()));
    }
    renderer.key_values(&latest_values)?;
    renderer.text("")?;

    renderer.section("Active Attempt")?;
    renderer.key_values(&record.active_attempt.to_key_values())?;
    renderer.text("")?;

    renderer.section("Recent Attempts")?;
    if let Some(parse_error) = &record.attempt_history.parse_error {
        renderer.key_values(&[KeyValue::new("history-parse", parse_error.clone())])?;
        renderer.text("")?;
    }
    if displayed_attempts.is_empty() {
        let message = if stored_count == 0 {
            "No retained terminal attempts are recorded for this demo yet."
        } else if outcome.is_some() {
            "No retained terminal attempts matched the current history query."
        } else {
            "No retained terminal attempts are available in the current history window."
        };
        renderer.notice(NoticeLevel::Info, message)?;
    } else {
        renderer.table(&recent_attempts_table_spec(displayed_attempts))?;
    }
    if let Some(attempt) = selected_attempt {
        renderer.text("")?;
        render_selected_historical_attempt(&mut renderer, attempt)?;
    }
    renderer.text("")?;
    render_utf8(renderer.into_inner())
}

fn render_selected_historical_attempt(
    renderer: &mut PlainRenderer<Vec<u8>>,
    attempt: &DemoHistoricalAttempt,
) -> Result<(), RunnerError> {
    renderer.section("Selected Attempt")?;
    let mut values = vec![
        KeyValue::new("attempt-id", attempt.attempt_id.clone()),
        KeyValue::new(
            "recorded-at-epoch-ms",
            attempt.recorded_at_epoch_ms.to_string(),
        ),
        KeyValue::new("outcome", attempt.outcome.clone()),
    ];
    if let Some(exit_code) = attempt.exit_code {
        values.push(KeyValue::new("exit-code", exit_code.to_string()));
    }
    if let Some(receipt_path) = &attempt.receipt_path {
        values.push(KeyValue::new("receipt", receipt_path.clone()));
    }
    if let Some(stdout_log_path) = &attempt.stdout_log_path {
        values.push(KeyValue::new("stdout-log", stdout_log_path.clone()));
    }
    if let Some(stderr_log_path) = &attempt.stderr_log_path {
        values.push(KeyValue::new("stderr-log", stderr_log_path.clone()));
    }
    renderer.key_values(&values)?;
    if let Some(summary) = &attempt.summary {
        renderer.text("")?;
        renderer.section("Result Summary")?;
        renderer.text(summary)?;
    }
    renderer.text("")?;
    renderer.section("Artifacts")?;
    if attempt.artifacts.is_empty() {
        renderer.notice(
            NoticeLevel::Info,
            "No artifacts were recorded for the selected historical attempt.",
        )?;
    } else {
        renderer.bullet_list("", &attempt.artifacts)?;
    }
    Ok(())
}

fn render_demo_execute(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
    invocation: DemoInvocationKind,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            invocation.schema(),
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let active_attempt = load_active_attempt(repo_root, demo_id)?;
    if active_attempt.active {
        return demo_error(
            output_json,
            invocation.schema(),
            format!(
                "demo `{demo_id}` already has an active attempt; stop it before starting a fresh run"
            ),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        );
    }

    let attempt = execute_demo_attempt(repo_root, demo_id, demo, output_json)?;
    write_latest_attempt_receipt(repo_root, demo_id, demo, &attempt)?;
    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;

    if output_json {
        let rendered = encode_json(
            &json!({
                "schema": invocation.schema(),
                "schema_version": 1,
                "ok": attempt.ok,
                "repo_root": repo_root.display().to_string(),
                "demo": {
                    "id": record.id,
                    "title": record.title,
                    "owner": record.owner,
                    "entrypoint": record.entrypoint.to_json(),
                    "defined_in": record.primary_source,
                },
                "execution": attempt.to_json(),
                "active_attempt": record.active_attempt.to_json(),
                "active_terminal_session": record.active_terminal_session.to_json(),
                "latest_attempt": record.latest_attempt.to_json(),
            }),
            true,
        )?;
        if attempt.ok {
            return Ok(rendered);
        }
        return Err(RunnerError::CommandJsonFailure { rendered });
    }

    if attempt.ok {
        return render_demo_execute_text(&record, &attempt, invocation.title());
    }

    Err(RunnerError::task_invocation(format!(
        "demo `{demo_id}` failed; latest attempt written to {}",
        record
            .latest_attempt
            .receipt_path
            .as_deref()
            .unwrap_or("<none>")
    )))
}

fn render_demo_stop(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let active_attempt = load_active_attempt(repo_root, demo_id)?;
    match demo_entrypoint(demo) {
        DemoEntrypoint::Task(task_name) => {
            if active_attempt.runtime_backend_kind != "concurrent-runner" {
                return demo_error(
                    output_json,
                    "effigy.demo.stop.v1",
                    format!(
                        "demo `{demo_id}` uses task entrypoint `{task_name}`; stop is not supported until task execution exposes cancellable handles"
                    ),
                    json!({
                        "demo_id": demo_id,
                        "entrypoint": { "kind": "task", "value": task_name },
                        "active_attempt": active_attempt.to_json(),
                    }),
                );
            }
        }
        DemoEntrypoint::Run(_) => {}
    }

    if !active_attempt.active {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` has no active attempt to stop"),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        );
    }
    if !active_attempt.stoppable {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` is active but not stoppable through the current runtime"),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        );
    }

    let mut persisted = read_active_attempt_record(repo_root, demo_id)?.ok_or_else(|| {
        RunnerError::task_invocation(format!("demo `{demo_id}` has no active attempt to stop"))
    })?;
    if persisted.phase == PersistedDemoActivePhase::StopRequested {
        return render_demo_stop_result(
            repo_root,
            loaded,
            demo_id,
            output_json,
            "stop already requested",
            demo_active_attempt_from_record(
                repo_root,
                demo_id,
                &persisted,
                render_active_attempt_path(repo_root, demo_id),
            ),
        );
    }

    let target_pid = persisted.target_pid;

    if persisted.runtime_backend_kind.as_deref() == Some("concurrent-runner") && target_pid.is_none()
    {
        persisted.phase = PersistedDemoActivePhase::StopRequested;
        write_active_attempt_record(repo_root, demo_id, &persisted)?;
        return render_demo_stop_result(
            repo_root,
            loaded,
            demo_id,
            output_json,
            "stop requested",
            demo_active_attempt_from_record(
                repo_root,
                demo_id,
                &persisted,
                render_active_attempt_path(repo_root, demo_id),
            ),
        );
    }

    let Some(target_pid) = target_pid else {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` is active but has no stoppable process handle"),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        );
    };

    if !pid_is_alive(target_pid) {
        clear_active_attempt_state(repo_root, demo_id);
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` is no longer running"),
            json!({
                "demo_id": demo_id,
                "active_attempt": DemoActiveAttempt::inactive(Some(render_active_attempt_path(repo_root, demo_id))).to_json(),
            }),
        );
    }

    persisted.phase = PersistedDemoActivePhase::StopRequested;
    write_active_attempt_record(repo_root, demo_id, &persisted)?;
    if let Err(error) = request_demo_termination(target_pid) {
        persisted.phase = PersistedDemoActivePhase::Running;
        write_active_attempt_record(repo_root, demo_id, &persisted)?;
        return Err(error);
    }
    render_demo_stop_result(
        repo_root,
        loaded,
        demo_id,
        output_json,
        "stop requested",
        demo_active_attempt_from_record(
            repo_root,
            demo_id,
            &persisted,
            render_active_attempt_path(repo_root, demo_id),
        ),
    )
}

fn render_demo_input(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    text: &str,
    append_newline: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.input.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    let forwarded_text = if append_newline {
        format!("{text}\n")
    } else {
        text.to_owned()
    };

    if !record.active_attempt.active {
        return demo_error(
            output_json,
            "effigy.demo.input.v1",
            format!("demo `{demo_id}` has no active terminal session to receive input"),
            json!({
                "demo_id": demo_id,
                "input": {
                    "text": text,
                    "append_newline": append_newline,
                    "forwarded_bytes": forwarded_text.len(),
                },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    }

    if !record.active_terminal_session.supports_input_forwarding {
        return demo_error(
            output_json,
            "effigy.demo.input.v1",
            format!(
                "demo `{demo_id}` does not expose terminal input forwarding in the current runtime"
            ),
            json!({
                "demo_id": demo_id,
                "input": {
                    "text": text,
                    "append_newline": append_newline,
                    "forwarded_bytes": forwarded_text.len(),
                },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    }

    let Some(input_path) = record.active_terminal_session.stdin_input_path.as_deref() else {
        return demo_error(
            output_json,
            "effigy.demo.input.v1",
            format!("demo `{demo_id}` does not expose a writable terminal input handoff"),
            json!({
                "demo_id": demo_id,
                "input": {
                    "text": text,
                    "append_newline": append_newline,
                    "forwarded_bytes": forwarded_text.len(),
                },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    };

    append_demo_terminal_input(repo_root, input_path, &forwarded_text)?;

    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.input.v1",
                "schema_version": 1,
                "ok": true,
                "demo_id": demo_id,
                "input": {
                    "text": text,
                    "append_newline": append_newline,
                    "forwarded_bytes": forwarded_text.len(),
                },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
            true,
        );
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Terminal Input")?;
    renderer.key_values(&[
        KeyValue::new("demo", demo_id.to_owned()),
        KeyValue::new("append-newline", if append_newline { "yes" } else { "no" }),
        KeyValue::new("forwarded-bytes", forwarded_text.len().to_string()),
    ])?;
    render_utf8(renderer.into_inner())
}

fn render_demo_resize(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    cols: u16,
    rows: u16,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.resize.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id, "terminal_size": { "cols": cols, "rows": rows } }),
        );
    };

    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    if !record.active_attempt.active {
        return demo_error(
            output_json,
            "effigy.demo.resize.v1",
            format!("demo `{demo_id}` has no active terminal session to resize"),
            json!({
                "demo_id": demo_id,
                "terminal_size": { "cols": cols, "rows": rows },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    }
    if !record.active_terminal_session.resize.available {
        return demo_error(
            output_json,
            "effigy.demo.resize.v1",
            format!(
                "demo `{demo_id}` does not expose terminal resize handoff in the current runtime"
            ),
            json!({
                "demo_id": demo_id,
                "terminal_size": { "cols": cols, "rows": rows },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    }

    let Some(resize_path) = record
        .active_terminal_session
        .resize_handoff_path
        .as_deref()
    else {
        return demo_error(
            output_json,
            "effigy.demo.resize.v1",
            format!("demo `{demo_id}` does not expose a writable terminal resize handoff"),
            json!({
                "demo_id": demo_id,
                "terminal_size": { "cols": cols, "rows": rows },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    };

    update_active_terminal_resize(repo_root, demo_id, cols, rows, resize_path)?;
    let refreshed = build_demo_record(repo_root, loaded, demo_id, demo)?;

    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.resize.v1",
                "schema_version": 1,
                "ok": true,
                "demo_id": demo_id,
                "terminal_size": {
                    "cols": cols,
                    "rows": rows,
                },
                "active_terminal_session": refreshed.active_terminal_session.to_json(),
            }),
            true,
        );
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Terminal Resize")?;
    renderer.key_values(&[
        KeyValue::new("demo", demo_id.to_owned()),
        KeyValue::new("cols", cols.to_string()),
        KeyValue::new("rows", rows.to_string()),
    ])?;
    render_utf8(renderer.into_inner())
}

fn render_demo_stop_result(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
    summary: &str,
    reported_active_attempt: DemoActiveAttempt,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };
    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.stop.v1",
                "schema_version": 1,
                "ok": true,
                "repo_root": repo_root.display().to_string(),
                "message": format!("demo `{demo_id}` {summary}"),
                "demo": {
                    "id": record.id,
                    "title": record.title,
                    "owner": record.owner,
                    "entrypoint": record.entrypoint.to_json(),
                    "defined_in": record.primary_source,
                },
                "active_attempt": reported_active_attempt.to_json(),
                "active_terminal_session": record.active_terminal_session.to_json(),
                "latest_attempt": record.latest_attempt.to_json(),
            }),
            true,
        );
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Stop")?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("state", reported_active_attempt.state_label().to_owned()),
        KeyValue::new(
            "stoppable",
            if reported_active_attempt.stoppable {
                "yes".to_owned()
            } else {
                "no".to_owned()
            },
        ),
    ])?;
    renderer.text("")?;
    let message = format!("demo `{demo_id}` {summary}");
    renderer.notice(NoticeLevel::Info, &message)?;
    renderer.text("")?;
    render_utf8(renderer.into_inner())
}

fn render_demo_execute_text(
    record: &DemoRecord,
    attempt: &DemoExecutionAttempt,
    section_title: &str,
) -> Result<String, RunnerError> {
    let mut renderer = text_renderer();
    renderer.section(section_title)?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("entrypoint", record.entrypoint.render_full()),
        KeyValue::new("outcome", attempt.outcome.clone()),
        KeyValue::new(
            "receipt",
            record
                .latest_attempt
                .receipt_path
                .clone()
                .unwrap_or_else(|| "<none>".to_owned()),
        ),
    ])?;
    if let Some(summary) = &attempt.summary {
        renderer.text("")?;
        renderer.notice(NoticeLevel::Info, summary)?;
    }
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy demo inspect <DEMO_ID>` to review the recorded latest attempt, recent attempt history, and any active state.",
    )?;
    renderer.text("")?;
    render_utf8(renderer.into_inner())
}

fn query_is_empty(query: &DemoListQuery) -> bool {
    query.search.is_none()
        && query.owner.is_none()
        && query.tag.is_none()
        && query.mode.is_none()
        && query.cover.is_none()
        && query.status.is_none()
        && query.gap.is_none()
        && !query.stale_only
}

fn demo_list_query_to_json(query: &DemoListQuery) -> JsonValue {
    json!({
        "search": query.search,
        "owner": query.owner,
        "tag": query.tag,
        "mode": query.mode.map(|value| value.as_str()),
        "cover": query.cover,
        "status": query.status.map(|value| value.as_str()),
        "gap": query.gap.map(|value| value.as_str()),
        "stale_only": query.stale_only,
        "group_by": query.group_by.map(|value| value.as_str()),
    })
}

fn demo_list_query_to_key_values(query: &DemoListQuery) -> Vec<KeyValue> {
    let mut values = Vec::new();
    if let Some(search) = &query.search {
        values.push(KeyValue::new("search", search.clone()));
    }
    if let Some(owner) = &query.owner {
        values.push(KeyValue::new("owner", owner.clone()));
    }
    if let Some(tag) = &query.tag {
        values.push(KeyValue::new("tag", tag.clone()));
    }
    if let Some(mode) = query.mode {
        values.push(KeyValue::new("mode", mode.as_str().to_owned()));
    }
    if let Some(cover) = &query.cover {
        values.push(KeyValue::new("cover", cover.clone()));
    }
    if let Some(status) = query.status {
        values.push(KeyValue::new("status", status.as_str().to_owned()));
    }
    if let Some(gap) = query.gap {
        values.push(KeyValue::new("gap", gap.as_str().to_owned()));
    }
    if query.stale_only {
        values.push(KeyValue::new("stale-only", "yes".to_owned()));
    }
    if let Some(group_by) = query.group_by {
        values.push(KeyValue::new("group-by", group_by.as_str().to_owned()));
    }
    values
}

fn demo_table_spec(demos: &[&DemoRecord]) -> TableSpec {
    TableSpec::new(
        vec![
            "ID".to_owned(),
            "Title".to_owned(),
            "Owner".to_owned(),
            "Mode".to_owned(),
            "Status".to_owned(),
            "Gap".to_owned(),
            "Actions".to_owned(),
            "Entrypoint".to_owned(),
        ],
        demos
            .iter()
            .map(|demo| {
                vec![
                    demo.id.clone(),
                    demo.title.clone(),
                    demo.owner.clone(),
                    demo.mode.as_str().to_owned(),
                    demo.effective_status(),
                    demo.gap_class.to_owned(),
                    demo.actions().summary_label(),
                    demo.entrypoint.render_compact(),
                ]
            })
            .collect(),
    )
}

fn recent_attempts_table_spec<T>(attempts: &[T]) -> TableSpec
where
    T: Borrow<DemoHistoricalAttempt>,
{
    TableSpec::new(
        vec![
            "#".to_owned(),
            "Attempt ID".to_owned(),
            "Recorded".to_owned(),
            "Status".to_owned(),
            "Summary".to_owned(),
            "Receipt".to_owned(),
        ],
        attempts
            .iter()
            .enumerate()
            .map(|attempt| {
                let (index, attempt) = attempt;
                let attempt = attempt.borrow();
                vec![
                    (index + 1).to_string(),
                    attempt.attempt_id.clone(),
                    attempt.recorded_at_epoch_ms.to_string(),
                    attempt.outcome.clone(),
                    attempt
                        .summary
                        .clone()
                        .unwrap_or_else(|| "<none>".to_owned()),
                    attempt
                        .receipt_path
                        .clone()
                        .unwrap_or_else(|| "<none>".to_owned()),
                ]
            })
            .collect(),
    )
}

fn history_attempt_to_json(ordinal: usize, attempt: &DemoHistoricalAttempt) -> JsonValue {
    let mut value = attempt.to_json();
    if let Some(object) = value.as_object_mut() {
        object.insert("ordinal".to_owned(), json!(ordinal));
    }
    value
}

fn history_attempts_with_outcome(
    history: &DemoAttemptHistory,
    outcome: Option<DemoHistoryOutcome>,
) -> Vec<&DemoHistoricalAttempt> {
    history
        .attempts
        .iter()
        .filter(|attempt| {
            outcome
                .map(|value| attempt.outcome == value.as_str())
                .unwrap_or(true)
        })
        .collect()
}

fn history_attempts_with_limit<'a>(
    attempts: &'a [&'a DemoHistoricalAttempt],
    limit: Option<usize>,
) -> &'a [&'a DemoHistoricalAttempt] {
    let end = limit
        .map(|value| value.min(attempts.len()))
        .unwrap_or(attempts.len());
    &attempts[..end]
}

fn find_historical_attempt<'a>(
    attempts: &'a [DemoHistoricalAttempt],
    attempt_id: &str,
) -> Option<&'a DemoHistoricalAttempt> {
    attempts
        .iter()
        .find(|attempt| attempt.attempt_id == attempt_id)
}

fn build_demo_groups<'a>(demos: &'a [DemoRecord], group_by: DemoListGroupBy) -> Vec<DemoGroup<'a>> {
    let mut groups: BTreeMap<String, Vec<&DemoRecord>> = BTreeMap::new();
    for demo in demos {
        match group_by {
            DemoListGroupBy::Owner => {
                groups.entry(demo.owner.clone()).or_default().push(demo);
            }
            DemoListGroupBy::Tag => {
                if demo.tags.is_empty() {
                    groups
                        .entry("(untagged)".to_owned())
                        .or_default()
                        .push(demo);
                } else {
                    for tag in &demo.tags {
                        groups.entry(tag.clone()).or_default().push(demo);
                    }
                }
            }
            DemoListGroupBy::Mode => {
                groups
                    .entry(demo.mode.as_str().to_owned())
                    .or_default()
                    .push(demo);
            }
            DemoListGroupBy::Cover => {
                if demo.covers.is_empty() {
                    groups
                        .entry("(unmapped)".to_owned())
                        .or_default()
                        .push(demo);
                } else {
                    for cover in &demo.covers {
                        groups.entry(cover.clone()).or_default().push(demo);
                    }
                }
            }
            DemoListGroupBy::Status => {
                groups
                    .entry(demo.effective_status())
                    .or_default()
                    .push(demo);
            }
            DemoListGroupBy::Gap => {
                groups
                    .entry(demo.gap_class.to_owned())
                    .or_default()
                    .push(demo);
            }
        }
    }

    groups
        .into_iter()
        .map(|(label, demos)| DemoGroup { label, demos })
        .collect()
}

fn availability_label(available: bool, reason: Option<&str>) -> String {
    if available {
        "yes".to_owned()
    } else if let Some(reason) = reason {
        format!("no ({reason})")
    } else {
        "no".to_owned()
    }
}

fn build_demo_record(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    demo: &ManifestDemoConfig,
) -> Result<DemoRecord, RunnerError> {
    let sources = demo_sources_for_id(repo_root, loaded, demo_id);
    let entrypoint = demo_entrypoint(demo);
    let primary_source = sources
        .first()
        .cloned()
        .unwrap_or_else(|| "effigy.toml".to_owned());
    let latest_attempt = load_latest_attempt(repo_root, demo_id, demo)?;
    let active_attempt = load_active_attempt(repo_root, demo_id)?;
    let attempt_history = load_attempt_history(repo_root, demo_id)?;
    let active_terminal_session = load_active_terminal_session(repo_root, &active_attempt);
    let gap_class = derive_gap_class(demo.status, latest_attempt.stale);

    Ok(DemoRecord {
        id: demo_id.to_owned(),
        title: demo.title.clone(),
        summary: demo.summary.clone(),
        proof: demo.proof.clone(),
        owner: demo.owner.clone(),
        mode: demo.mode,
        status: demo.status,
        covers: demo.covers.clone(),
        tags: demo.tags.clone(),
        prerequisites: demo.prerequisites.clone(),
        dependencies: demo.dependencies.clone(),
        entrypoint: entrypoint.clone(),
        sources,
        primary_source,
        gap_class,
        runtime_backend: demo_runtime_backend(repo_root, loaded, &entrypoint, &active_attempt),
        active_attempt,
        active_terminal_session,
        latest_attempt,
        attempt_history,
    })
}

fn demo_sources_for_id(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
) -> Vec<String> {
    let prefix = format!("demos.{demo_id}.");
    let mut seen = BTreeSet::new();
    loaded
        .value_sources
        .iter()
        .filter(|entry| entry.path == format!("demos.{demo_id}") || entry.path.starts_with(&prefix))
        .filter_map(|entry| {
            let rendered = display_repo_path(&entry.source, repo_root);
            seen.insert(rendered.clone()).then_some(rendered)
        })
        .collect::<Vec<_>>()
}

fn demo_entrypoint(demo: &ManifestDemoConfig) -> DemoEntrypoint {
    if let Some(task) = &demo.task {
        DemoEntrypoint::Task(task.clone())
    } else if let Some(run) = &demo.run {
        DemoEntrypoint::Run(run.clone())
    } else {
        DemoEntrypoint::Run("<invalid>".to_owned())
    }
}

fn load_latest_attempt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
) -> Result<DemoLatestAttempt, RunnerError> {
    let receipt_path = effective_receipt_path(repo_root, demo_id, demo);
    let mut artifacts = demo.artifacts.clone();
    let rendered_receipt_path = display_repo_path(&receipt_path, repo_root);
    if !receipt_path.exists() {
        return Ok(DemoLatestAttempt {
            recorded: false,
            receipt_path: Some(rendered_receipt_path),
            outcome: None,
            summary: None,
            stale: false,
            artifacts,
            stdout_log_path: None,
            stderr_log_path: None,
            parse_error: None,
        });
    }

    let content = fs::read_to_string(&receipt_path)
        .map_err(|error| RunnerError::task_invocation_failed_read(&receipt_path, error))?;
    let parsed = match serde_json::from_str::<JsonValue>(&content) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(DemoLatestAttempt {
                recorded: true,
                receipt_path: Some(rendered_receipt_path),
                outcome: None,
                summary: None,
                stale: false,
                artifacts,
                stdout_log_path: None,
                stderr_log_path: None,
                parse_error: Some(error.to_string()),
            });
        }
    };

    if let Some(receipt_artifacts) = parsed.get("artifacts").and_then(normalize_artifact_refs) {
        for artifact in receipt_artifacts {
            if !artifacts.contains(&artifact) {
                artifacts.push(artifact);
            }
        }
    }

    Ok(DemoLatestAttempt {
        recorded: true,
        receipt_path: Some(rendered_receipt_path),
        outcome: parsed
            .get("status")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        summary: parsed
            .get("summary")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        stale: parsed
            .get("stale")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
            || parsed
                .get("freshness")
                .and_then(JsonValue::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("stale")),
        artifacts,
        stdout_log_path: parsed
            .get("stdout_log_path")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        stderr_log_path: parsed
            .get("stderr_log_path")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        parse_error: None,
    })
}

fn load_attempt_history(
    repo_root: &Path,
    demo_id: &str,
) -> Result<DemoAttemptHistory, RunnerError> {
    let path = effective_attempt_history_path(repo_root, demo_id);
    let rendered_path = display_repo_path(&path, repo_root);
    if !path.exists() {
        return Ok(DemoAttemptHistory {
            path: Some(rendered_path),
            attempts: Vec::new(),
            parse_error: None,
        });
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| RunnerError::task_invocation_failed_read(&path, error))?;
    let parsed = match serde_json::from_str::<PersistedDemoAttemptHistory>(&content) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(DemoAttemptHistory {
                path: Some(rendered_path),
                attempts: Vec::new(),
                parse_error: Some(error.to_string()),
            });
        }
    };

    Ok(DemoAttemptHistory {
        path: Some(rendered_path),
        attempts: parsed
            .attempts
            .into_iter()
            .map(DemoHistoricalAttempt::from_persisted)
            .collect(),
        parse_error: None,
    })
}

fn load_active_attempt(repo_root: &Path, demo_id: &str) -> Result<DemoActiveAttempt, RunnerError> {
    let path = effective_active_attempt_path(repo_root, demo_id);
    let rendered_path = display_repo_path(&path, repo_root);
    let Some(record) = read_active_attempt_record(repo_root, demo_id)? else {
        return Ok(DemoActiveAttempt::inactive(Some(rendered_path)));
    };

    let target_alive = record.target_pid.is_none_or(pid_is_alive);
    let owner_alive = pid_is_alive(record.owner_pid);
    if record.phase == PersistedDemoActivePhase::StopRequested && (!owner_alive || !target_alive) {
        return Ok(demo_active_attempt_from_record(
            repo_root,
            demo_id,
            &record,
            rendered_path,
        ));
    }
    if !owner_alive || !target_alive {
        clear_active_attempt_state(repo_root, demo_id);
        return Ok(DemoActiveAttempt::inactive(Some(rendered_path)));
    }

    Ok(demo_active_attempt_from_record(
        repo_root,
        demo_id,
        &record,
        rendered_path,
    ))
}

fn load_active_terminal_session(
    repo_root: &Path,
    active_attempt: &DemoActiveAttempt,
) -> DemoActiveTerminalSession {
    if !active_attempt.active {
        return DemoActiveTerminalSession::inactive();
    }

    let stdout_log_path = active_attempt.stdout_log_path.clone();
    let stderr_log_path = active_attempt.stderr_log_path.clone();
    let runtime_backend = active_attempt.runtime_backend();
    let input_forwarding_reason = (!active_attempt.supports_input_forwarding)
        .then_some("input forwarding is not exposed through the current demo runtime".to_owned());
    DemoActiveTerminalSession {
        available: true,
        state: "live".to_owned(),
        attempt_id: active_attempt.attempt_id.clone(),
        runtime_backend,
        transport: active_attempt.terminal_transport.rendered().to_owned(),
        pty: matches!(
            active_attempt.terminal_transport,
            DemoTerminalTransport::Pty
        ),
        supports_input_forwarding: active_attempt.supports_input_forwarding,
        input_forwarding_reason: input_forwarding_reason.clone(),
        input_forwarding: if active_attempt.supports_input_forwarding {
            DemoTerminalInputForwarding::available()
        } else {
            DemoTerminalInputForwarding::unavailable(
                input_forwarding_reason
                    .expect("reason exists when input forwarding is unavailable"),
            )
        },
        nested_tui: active_attempt.nested_tui,
        terminal_size: DemoTerminalSize {
            cols: active_attempt.terminal_cols,
            rows: active_attempt.terminal_rows,
        },
        resize: if active_attempt.supports_resize {
            DemoTerminalResizeForwarding::available()
        } else {
            DemoTerminalResizeForwarding::unavailable(
                "terminal resize handoff is not exposed through the current demo runtime"
                    .to_owned(),
            )
        },
        resize_handoff_path: active_attempt.resize_handoff_path.clone(),
        stdin_input_path: active_attempt.stdin_input_path.clone(),
        stdout_log_path: stdout_log_path.clone(),
        stderr_log_path: stderr_log_path.clone(),
        output_available: stdout_log_path.is_some() || stderr_log_path.is_some(),
        recent_stdout: stdout_log_path
            .as_deref()
            .map(|path| {
                read_recent_output_lines(repo_root, path, DEMO_ACTIVE_TERMINAL_RECENT_LINES)
            })
            .unwrap_or_default(),
        recent_stderr: stderr_log_path
            .as_deref()
            .map(|path| {
                read_recent_output_lines(repo_root, path, DEMO_ACTIVE_TERMINAL_RECENT_LINES)
            })
            .unwrap_or_default(),
    }
}

fn update_active_terminal_resize(
    repo_root: &Path,
    demo_id: &str,
    cols: u16,
    rows: u16,
    rendered_resize_path: &str,
) -> Result<(), RunnerError> {
    let Some(mut record) = read_active_attempt_record(repo_root, demo_id)? else {
        return Err(RunnerError::task_invocation(format!(
            "demo `{demo_id}` no longer has an active terminal session"
        )));
    };
    record.terminal_cols = Some(cols);
    record.terminal_rows = Some(rows);
    write_active_attempt_record(repo_root, demo_id, &record)?;
    append_demo_terminal_resize(repo_root, rendered_resize_path, cols, rows)
}

fn append_demo_terminal_input(
    repo_root: &Path,
    rendered_path: &str,
    forwarded_text: &str,
) -> Result<(), RunnerError> {
    let absolute = resolve_repo_relative_path(repo_root, rendered_path);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&absolute)
        .map_err(|error| RunnerError::task_invocation_failed_write(&absolute, error))?;
    file.write_all(forwarded_text.as_bytes())
        .and_then(|_| file.flush())
        .map_err(|error| RunnerError::task_invocation_failed_write(&absolute, error))
}

fn append_demo_terminal_resize(
    repo_root: &Path,
    rendered_path: &str,
    cols: u16,
    rows: u16,
) -> Result<(), RunnerError> {
    let absolute = resolve_repo_relative_path(repo_root, rendered_path);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&absolute)
        .map_err(|error| RunnerError::task_invocation_failed_write(&absolute, error))?;
    let rendered = serde_json::to_string(&json!({
        "cols": cols,
        "rows": rows,
        "recorded_at_epoch_ms": now_epoch_ms(),
    }))
    .map_err(|error| RunnerError::task_invocation_failed_render(&absolute, error))?;
    file.write_all(rendered.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.flush())
        .map_err(|error| RunnerError::task_invocation_failed_write(&absolute, error))
}

fn demo_active_attempt_from_record(
    _repo_root: &Path,
    _demo_id: &str,
    record: &PersistedDemoActiveAttempt,
    rendered_path: String,
) -> DemoActiveAttempt {
    DemoActiveAttempt {
        active: true,
        state: record.phase.rendered().to_owned(),
        attempt_id: Some(record.attempt_id.clone()),
        state_path: Some(rendered_path),
        owner_pid: Some(record.owner_pid),
        target_pid: record.target_pid,
        stoppable: record.stoppable,
        started_at_epoch_ms: Some(record.started_at_epoch_ms),
        entrypoint_kind: Some(record.entrypoint_kind.clone()),
        entrypoint_value: Some(record.entrypoint_value.clone()),
        command: Some(record.command.clone()),
        runtime_backend_kind: infer_runtime_backend_kind(
            record.runtime_backend_kind.as_deref(),
            &record.entrypoint_kind,
        )
        .to_owned(),
        flattened_runtime_projection: record.flattened_runtime_projection,
        terminal_transport: match record.terminal_transport {
            PersistedDemoTerminalTransport::Stream => DemoTerminalTransport::Stream,
            PersistedDemoTerminalTransport::Pty => DemoTerminalTransport::Pty,
        },
        supports_input_forwarding: record.supports_input_forwarding,
        supports_resize: record.supports_resize,
        nested_tui: record.nested_tui,
        terminal_cols: record.terminal_cols,
        terminal_rows: record.terminal_rows,
        resize_handoff_path: record.resize_handoff_path.clone(),
        stdin_input_path: record.stdin_input_path.clone(),
        stdout_log_path: record.stdout_log_path.clone(),
        stderr_log_path: record.stderr_log_path.clone(),
        parse_error: None,
    }
}

fn infer_runtime_backend_kind<'a>(
    persisted_kind: Option<&'a str>,
    entrypoint_kind: &'a str,
) -> &'a str {
    persisted_kind.unwrap_or(match entrypoint_kind {
        "task" => "task",
        "run" => "run",
        _ => "none",
    })
}

fn runtime_backend_label(kind: &str) -> &'static str {
    match kind {
        "task" => "task-backed",
        "run" => "run-backed",
        "concurrent-runner" => "concurrent-runner-backed",
        "none" => "none",
        _ => "custom-runtime",
    }
}

fn read_active_attempt_record(
    repo_root: &Path,
    demo_id: &str,
) -> Result<Option<PersistedDemoActiveAttempt>, RunnerError> {
    let path = effective_active_attempt_path(repo_root, demo_id);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| RunnerError::task_invocation_failed_read(&path, error))?;
    let parsed = serde_json::from_str::<PersistedDemoActiveAttempt>(&content)
        .map_err(|error| RunnerError::task_invocation_failed_parse(&path, error))?;
    Ok(Some(parsed))
}

fn normalize_artifact_refs(value: &JsonValue) -> Option<Vec<String>> {
    let entries = value.as_array()?;
    let mut rendered = Vec::new();
    for entry in entries {
        match entry {
            JsonValue::String(path) if !path.trim().is_empty() => rendered.push(path.clone()),
            JsonValue::Object(map) => {
                if let Some(path) = map.get("path").and_then(JsonValue::as_str) {
                    if !path.trim().is_empty() {
                        rendered.push(path.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    Some(rendered)
}

fn read_recent_output_lines(repo_root: &Path, rendered_path: &str, limit: usize) -> Vec<String> {
    let absolute = resolve_repo_relative_path(repo_root, rendered_path);
    let Ok(content) = fs::read_to_string(&absolute) else {
        return Vec::new();
    };
    let mut lines = content
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if lines.len() > limit {
        let keep_from = lines.len() - limit;
        lines.drain(..keep_from);
    }
    lines
}

fn resolve_repo_relative_path(repo_root: &Path, path: &str) -> PathBuf {
    let rendered = Path::new(path);
    if rendered.is_absolute() {
        rendered.to_path_buf()
    } else {
        repo_root.join(rendered)
    }
}

fn execute_demo_attempt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    match demo_entrypoint(demo) {
        DemoEntrypoint::Task(task_name) => execute_task_backed_demo(
            repo_root,
            demo_id,
            &task_name,
            demo.mode,
            output_json,
        ),
        DemoEntrypoint::Run(run_command) => {
            execute_run_backed_demo(repo_root, demo_id, demo.mode, &run_command, output_json)
        }
    }
}

fn execute_task_backed_demo(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    demo_mode: ManifestDemoMode,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    if let Some(selection) = demo_task_selection(repo_root, task_name)? {
        if task_is_concurrent_runner_backed(selection.task()?) {
            return execute_concurrent_runner_backed_demo(
                repo_root,
                demo_id,
                task_name,
                demo_mode,
                selection,
                output_json,
            );
        }
    }

    let attempt_id = build_attempt_id(demo_id);
    let _active_guard = register_active_attempt(
        repo_root,
        demo_id,
        PersistedDemoActiveAttempt {
            schema: "effigy.demo.active.v1".to_owned(),
            schema_version: 1,
            attempt_id,
            demo_id: demo_id.to_owned(),
            phase: PersistedDemoActivePhase::Running,
            started_at_epoch_ms: now_epoch_ms(),
            owner_pid: std::process::id(),
            target_pid: None,
            stoppable: false,
            entrypoint_kind: "task".to_owned(),
            entrypoint_value: task_name.to_owned(),
            command: task_name.to_owned(),
            runtime_backend_kind: Some("task".to_owned()),
            flattened_runtime_projection: false,
            terminal_transport: PersistedDemoTerminalTransport::Stream,
            supports_input_forwarding: false,
            supports_resize: false,
            nested_tui: false,
            terminal_cols: None,
            terminal_rows: None,
            resize_handoff_path: None,
            stdin_input_path: None,
            stdout_log_path: None,
            stderr_log_path: None,
        },
    )?;

    if output_json {
        let task = TaskInvocation {
            name: task_name.to_owned(),
            args: vec!["--json".to_owned()],
        };
        return match run_manifest_task_with_cwd(&task, repo_root.to_path_buf()) {
            Ok(rendered) => {
                parse_task_backed_attempt_json(repo_root, demo_id, task_name, &rendered)
            }
            Err(RunnerError::CommandJsonFailure { rendered }) => {
                parse_task_backed_attempt_json(repo_root, demo_id, task_name, &rendered)
            }
            Err(error) => Ok(failed_demo_attempt(
                "task",
                task_name,
                task_name,
                None,
                format!("Demo `{demo_id}` failed to run task `{task_name}`: {error}"),
                String::new(),
                String::new(),
                DemoLogPaths::none(),
            )),
        };
    }

    let task = TaskInvocation {
        name: task_name.to_owned(),
        args: Vec::new(),
    };
    match run_manifest_task_with_cwd(&task, repo_root.to_path_buf()) {
        Ok(_) => Ok(successful_demo_attempt(
            "task",
            task_name,
            task_name,
            None,
            Some(format!(
                "Demo `{demo_id}` completed via task `{task_name}`."
            )),
            String::new(),
            String::new(),
            DemoLogPaths::none(),
        )),
        Err(RunnerError::TaskCommandFailure { code, .. }) => Ok(failed_demo_attempt(
            "task",
            task_name,
            task_name,
            code,
            format!("Demo `{demo_id}` failed via task `{task_name}`."),
            String::new(),
            String::new(),
            DemoLogPaths::none(),
        )),
        Err(error) => Ok(failed_demo_attempt(
            "task",
            task_name,
            task_name,
            None,
            format!("Demo `{demo_id}` failed to run task `{task_name}`: {error}"),
            String::new(),
            String::new(),
            DemoLogPaths::none(),
        )),
    }
}

struct DemoTaskSelectionResolved {
    selector: TaskSelector,
    catalogs: Vec<LoadedCatalog>,
    selected_catalog_index: usize,
}

impl DemoTaskSelectionResolved {
    fn selection(&self) -> Result<TaskSelection<'_>, RunnerError> {
        select_catalog_and_task(
            &self.selector,
            &self.catalogs,
            &self.catalogs[self.selected_catalog_index].catalog_root,
        )
    }

    fn task(&self) -> Result<&ManifestTask, RunnerError> {
        self.selection().map(|selection| selection.task)
    }
}

fn demo_task_selection(
    repo_root: &Path,
    task_name: &str,
) -> Result<Option<DemoTaskSelectionResolved>, RunnerError> {
    let catalogs = crate::runner::catalog::discover_catalogs_allow_missing(repo_root)?;
    if catalogs.is_empty() {
        return Ok(None);
    }
    let selector = parse_task_selector(task_name)?;
    let selection = select_catalog_and_task(&selector, &catalogs, repo_root)?;
    let selected_catalog_index = catalogs
        .iter()
        .position(|catalog| {
            catalog.alias == selection.catalog.alias
                && catalog.catalog_root == selection.catalog.catalog_root
                && catalog.manifest_path == selection.catalog.manifest_path
        })
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "failed to re-identify selected task catalog for demo task `{task_name}`"
            ))
        })?;
    Ok(Some(DemoTaskSelectionResolved {
        selector,
        catalogs,
        selected_catalog_index,
    }))
}

fn task_is_concurrent_runner_backed(task: &ManifestTask) -> bool {
    task.mode.as_deref() == Some("tui") && (!task.concurrent.is_empty() || !task.profiles.is_empty())
}

fn execute_concurrent_runner_backed_demo(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    demo_mode: ManifestDemoMode,
    resolved: DemoTaskSelectionResolved,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let selection = resolved.selection()?;
    let runtime_args = TaskRuntimeArgs {
        repo_override: None,
        verbose_root: false,
        env_schema_override: None,
        passthrough: Vec::new(),
    };
    let plan = resolve_managed_task_plan(
        &resolved.selector,
        selection.catalog,
        selection.task,
        &runtime_args,
        &resolved.catalogs,
        &selection.catalog.catalog_root,
    )?
    .ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "demo `{demo_id}` task `{task_name}` does not resolve to a managed concurrent runtime"
        ))
    })?;
    let log_paths = DemoLogPaths::prepare_split(repo_root, demo_id)?;
    let initial_terminal_size = if demo_mode_prefers_attached_terminal(demo_mode) {
        current_terminal_size()
    } else {
        Some((DEMO_DEFAULT_TERMINAL_COLS, DEMO_DEFAULT_TERMINAL_ROWS))
    };
    let attempt_id = build_attempt_id(demo_id);
    let _active_guard = register_active_attempt(
        repo_root,
        demo_id,
        PersistedDemoActiveAttempt {
            schema: "effigy.demo.active.v1".to_owned(),
            schema_version: 1,
            attempt_id,
            demo_id: demo_id.to_owned(),
            phase: PersistedDemoActivePhase::Running,
            started_at_epoch_ms: now_epoch_ms(),
            owner_pid: std::process::id(),
            target_pid: None,
            stoppable: true,
            entrypoint_kind: "task".to_owned(),
            entrypoint_value: task_name.to_owned(),
            command: format!("<managed:{task_name} profile:{}>", plan.profile),
            runtime_backend_kind: Some("concurrent-runner".to_owned()),
            flattened_runtime_projection: true,
            terminal_transport: PersistedDemoTerminalTransport::Stream,
            supports_input_forwarding: false,
            supports_resize: false,
            nested_tui: false,
            terminal_cols: initial_terminal_size.map(|(cols, _)| cols),
            terminal_rows: initial_terminal_size.map(|(_, rows)| rows),
            resize_handoff_path: None,
            stdin_input_path: None,
            stdout_log_path: log_paths.stdout.clone(),
            stderr_log_path: log_paths.stderr.clone(),
        },
    )?;

    run_concurrent_runner_demo_runtime(repo_root, demo_id, task_name, plan, log_paths, output_json)
}

fn run_concurrent_runner_demo_runtime(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    plan: crate::runner::model::managed::ManagedTaskPlan,
    log_paths: DemoLogPaths,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let shutdown_on_exit_processes = plan
        .processes
        .iter()
        .filter(|process| process.shutdown_on_exit)
        .map(|process| process.name.clone())
        .collect::<BTreeSet<String>>();
    let specs = plan
        .processes
        .iter()
        .cloned()
        .map(|process| ProcessSpec {
            name: process.name,
            run: process.run,
            cwd: process.cwd,
            start_after_ms: process.start_after_ms,
            shutdown_on_exit: process.shutdown_on_exit,
            pty: true,
            env: BTreeMap::new(),
        })
        .collect::<Vec<_>>();
    let expected = specs.len();
    let supervisor = ProcessSupervisor::spawn(repo_root.to_path_buf(), specs)?;
    let mut state = DemoConcurrentRuntimeState::new(
        log_paths.stdout_absolute.as_deref(),
        log_paths.stderr_absolute.as_deref(),
        shutdown_on_exit_processes,
        !output_json,
    )?;

    while state.exit_count < expected || state.drained_after_exit < DEMO_STREAM_DRAIN_POLLS_AFTER_EXIT
    {
        if !state.stop_requested && active_attempt_is_stop_requested(repo_root, demo_id) {
            state.stop_requested = true;
            supervisor.terminate_all();
        }
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(
            DEMO_MANAGED_EVENT_POLL_INTERVAL_MS,
        )) {
            state.record_event(event, &supervisor)?;
        } else {
            state.record_idle_tick(expected);
        }
    }

    supervisor.terminate_all();

    let command = format!("<managed:{task_name} profile:{}>", plan.profile);
    let summary = if state.stop_requested {
        format!(
            "Demo `{demo_id}` terminated after stop request while projecting managed task `{task_name}`."
        )
    } else if plan.fail_on_non_zero && !state.non_zero_exits.is_empty() {
        format!(
            "Demo `{demo_id}` failed via managed task `{task_name}`: {}",
            render_non_zero_exits(&state.non_zero_exits)
        )
    } else {
        format!(
            "Demo `{demo_id}` completed via managed task `{task_name}` profile `{}`.",
            plan.profile
        )
    };

    if state.stop_requested {
        Ok(terminated_demo_attempt(
            "task",
            task_name,
            &command,
            None,
            summary,
            state.stdout,
            state.stderr,
            log_paths,
        ))
    } else if plan.fail_on_non_zero && !state.non_zero_exits.is_empty() {
        Ok(failed_demo_attempt(
            "task",
            task_name,
            &command,
            None,
            summary,
            state.stdout,
            state.stderr,
            log_paths,
        ))
    } else {
        Ok(successful_demo_attempt(
            "task",
            task_name,
            &command,
            None,
            Some(summary),
            state.stdout,
            state.stderr,
            log_paths,
        ))
    }
}

struct DemoConcurrentRuntimeState {
    stdout: String,
    stderr: String,
    stdout_log: Option<fs::File>,
    stderr_log: Option<fs::File>,
    exit_count: usize,
    drained_after_exit: usize,
    non_zero_exits: Vec<(String, String)>,
    shutdown_on_exit_processes: BTreeSet<String>,
    stop_requested: bool,
    mirror_output: bool,
}

impl DemoConcurrentRuntimeState {
    fn new(
        stdout_log_path: Option<&Path>,
        stderr_log_path: Option<&Path>,
        shutdown_on_exit_processes: BTreeSet<String>,
        mirror_output: bool,
    ) -> Result<Self, RunnerError> {
        Ok(Self {
            stdout: String::new(),
            stderr: String::new(),
            stdout_log: open_append_file(stdout_log_path)?,
            stderr_log: open_append_file(stderr_log_path)?,
            exit_count: 0,
            drained_after_exit: 0,
            non_zero_exits: Vec::new(),
            shutdown_on_exit_processes,
            stop_requested: false,
            mirror_output,
        })
    }

    fn record_event(
        &mut self,
        event: ProcessEvent,
        supervisor: &ProcessSupervisor,
    ) -> Result<(), RunnerError> {
        if self.exit_count > 0 {
            self.drained_after_exit = 0;
        }
        match event.kind {
            ProcessEventKind::Stdout => self.record_stdout(&event.process, &event.payload)?,
            ProcessEventKind::Stderr => self.record_stderr(&event.process, &event.payload)?,
            ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {}
            ProcessEventKind::Exit => self.record_exit(&event.process, &event.payload, supervisor),
        }
        Ok(())
    }

    fn record_stdout(&mut self, process: &str, payload: &str) -> Result<(), RunnerError> {
        let rendered = format!("[{process}] {payload}\n");
        self.stdout.push_str(&rendered);
        if self.mirror_output {
            print!("{rendered}");
            io::stdout()
                .flush()
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        }
        if let Some(file) = self.stdout_log.as_mut() {
            file.write_all(rendered.as_bytes())
                .and_then(|_| file.flush())
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        }
        Ok(())
    }

    fn record_stderr(&mut self, process: &str, payload: &str) -> Result<(), RunnerError> {
        let rendered = format!("[{process} stderr] {payload}\n");
        self.stderr.push_str(&rendered);
        if self.mirror_output {
            eprint!("{rendered}");
            io::stderr()
                .flush()
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        }
        if let Some(file) = self.stderr_log.as_mut() {
            file.write_all(rendered.as_bytes())
                .and_then(|_| file.flush())
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        }
        Ok(())
    }

    fn record_exit(
        &mut self,
        process: &str,
        payload: &str,
        supervisor: &ProcessSupervisor,
    ) {
        self.exit_count += 1;
        if payload != "exit=0" {
            self.non_zero_exits
                .push((process.to_owned(), payload.to_owned()));
        }
        if self.shutdown_on_exit_processes.contains(process) {
            supervisor.terminate_all();
        }
    }

    fn record_idle_tick(&mut self, expected: usize) {
        if self.exit_count >= expected {
            self.drained_after_exit += 1;
        }
    }
}

fn open_append_file(path: Option<&Path>) -> Result<Option<fs::File>, RunnerError> {
    let Some(path) = path else {
        return Ok(None);
    };
    Ok(Some(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| RunnerError::task_invocation_failed_write(path, error))?,
    ))
}

fn render_non_zero_exits(processes: &[(String, String)]) -> String {
    processes
        .iter()
        .map(|(process, payload)| format!("{process} {payload}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_task_backed_attempt_json(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    rendered: &str,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let parsed: JsonValue = serde_json::from_str(rendered).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse json task payload for demo `{demo_id}` task `{task_name}`: {error}"
        ))
    })?;
    let ok = parsed
        .get("ok")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let exit_code = parsed
        .get("exit_code")
        .and_then(JsonValue::as_i64)
        .and_then(|value| i32::try_from(value).ok());
    let stdout = parsed
        .get("stdout")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let stderr = parsed
        .get("stderr")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let command = parsed
        .get("command")
        .and_then(JsonValue::as_str)
        .unwrap_or(task_name)
        .to_owned();

    let log_paths = persist_demo_attempt_logs(repo_root, demo_id, &stdout, &stderr)?;

    Ok(DemoExecutionAttempt {
        ok,
        outcome: if ok {
            "passed".to_owned()
        } else {
            "failed".to_owned()
        },
        entrypoint_kind: "task".to_owned(),
        entrypoint_value: task_name.to_owned(),
        command,
        exit_code,
        summary: Some(if ok {
            format!("Demo `{demo_id}` completed via task `{task_name}`.")
        } else {
            format!("Demo `{demo_id}` failed via task `{task_name}`.")
        }),
        stdout,
        stderr,
        stdout_log_path: log_paths.stdout,
        stderr_log_path: log_paths.stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    })
}

fn execute_run_backed_demo(
    repo_root: &Path,
    demo_id: &str,
    mode: ManifestDemoMode,
    run_command: &str,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let launch_mode = resolve_demo_launch_mode(mode, output_json);
    let attached_terminal = launch_mode.attached_terminal();
    let initial_terminal_size = initial_terminal_size_for_launch_mode(launch_mode);
    let input_handoff_path = launch_mode
        .supports_input_forwarding()
        .then(|| prepare_demo_input_handoff(repo_root, demo_id))
        .transpose()?;
    let resize_handoff_path = launch_mode
        .supports_resize()
        .then(|| prepare_demo_resize_handoff(repo_root, demo_id))
        .transpose()?;
    let log_paths = if output_json || attached_terminal {
        DemoLogPaths::prepare_for_launch_mode(repo_root, demo_id, launch_mode)?
    } else {
        DemoLogPaths::none()
    };
    let mut child = build_run_backed_process(repo_root, run_command, launch_mode)?
        .spawn()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "Demo `{demo_id}` failed to launch run entrypoint: {error}"
            ))
        })?;

    let attempt_id = build_attempt_id(demo_id);
    let _active_guard = register_active_attempt(
        repo_root,
        demo_id,
        PersistedDemoActiveAttempt {
            schema: "effigy.demo.active.v1".to_owned(),
            schema_version: 1,
            attempt_id,
            demo_id: demo_id.to_owned(),
            phase: PersistedDemoActivePhase::Running,
            started_at_epoch_ms: now_epoch_ms(),
            owner_pid: std::process::id(),
            target_pid: Some(child.id()),
            stoppable: true,
            entrypoint_kind: "run".to_owned(),
            entrypoint_value: run_command.to_owned(),
            command: run_command.to_owned(),
            runtime_backend_kind: Some("run".to_owned()),
            flattened_runtime_projection: false,
            terminal_transport: launch_mode.transport(),
            supports_input_forwarding: input_handoff_path.is_some(),
            supports_resize: resize_handoff_path.is_some(),
            nested_tui: false,
            terminal_cols: initial_terminal_size.map(|(cols, _)| cols),
            terminal_rows: initial_terminal_size.map(|(_, rows)| rows),
            resize_handoff_path: resize_handoff_path
                .as_ref()
                .map(|path| display_repo_path(path, repo_root)),
            stdin_input_path: input_handoff_path
                .as_ref()
                .map(|path| display_repo_path(path, repo_root)),
            stdout_log_path: log_paths.stdout.clone(),
            stderr_log_path: log_paths.stderr.clone(),
        },
    )?;

    if output_json || attached_terminal {
        let _stdin_forward = if launch_mode.forward_stdin() && io::stdin().is_terminal() {
            child.stdin.take().map(spawn_stdin_forward)
        } else {
            None
        };
        let input_forward = child
            .stdin
            .take()
            .zip(input_handoff_path.as_ref())
            .map(|(stdin, path)| spawn_input_handoff_forward(path.clone(), stdin));
        let stdout_reader = child.stdout.take().ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "Demo `{demo_id}` launched without a stdout capture pipe."
            ))
        })?;
        let stderr_reader = child.stderr.take().ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "Demo `{demo_id}` launched without a stderr capture pipe."
            ))
        })?;
        let stdout_handle = spawn_output_capture(
            stdout_reader,
            log_paths.stdout_absolute.clone(),
            attached_terminal.then_some(OutputMirror::Stdout),
        );
        let stderr_handle = spawn_output_capture(
            stderr_reader,
            log_paths.stderr_absolute.clone(),
            attached_terminal.then_some(OutputMirror::Stderr),
        );
        let status = child.wait().map_err(|error| {
            RunnerError::task_invocation(format!(
                "Demo `{demo_id}` failed to wait for run entrypoint: {error}"
            ))
        })?;
        if let Some(forward) = input_forward {
            stop_input_handoff_forward(forward, input_handoff_path.as_deref());
        }
        clear_resize_handoff(resize_handoff_path.as_deref());
        let mut stdout = join_output_capture(stdout_handle, "stdout", demo_id)?;
        let mut stderr = join_output_capture(stderr_handle, "stderr", demo_id)?;
        if matches!(launch_mode, DemoLaunchMode::AttachedPty) {
            stdout = sanitize_pty_transcript(&stdout);
            stderr = sanitize_pty_transcript(&stderr);
            if let Some(path) = &log_paths.stdout_absolute {
                fs::write(path, &stdout)
                    .map_err(|error| RunnerError::task_invocation_failed_write(path, error))?;
            }
            if let Some(path) = &log_paths.stderr_absolute {
                fs::write(path, &stderr)
                    .map_err(|error| RunnerError::task_invocation_failed_write(path, error))?;
            }
        }
        let stop_requested = active_attempt_is_stop_requested(repo_root, demo_id);
        return Ok(run_attempt_from_output(
            demo_id,
            run_command,
            status.code(),
            status.success(),
            stop_requested,
            stdout,
            stderr,
            log_paths,
        ));
    }

    let status = child.wait().map_err(|error| {
        RunnerError::task_invocation(format!(
            "Demo `{demo_id}` failed to wait for run entrypoint: {error}"
        ))
    })?;
    clear_resize_handoff(resize_handoff_path.as_deref());
    let stop_requested = active_attempt_is_stop_requested(repo_root, demo_id);
    Ok(run_attempt_from_output(
        demo_id,
        run_command,
        status.code(),
        status.success(),
        stop_requested,
        String::new(),
        String::new(),
        DemoLogPaths::none(),
    ))
}

fn demo_mode_prefers_attached_terminal(mode: ManifestDemoMode) -> bool {
    matches!(
        mode,
        ManifestDemoMode::Interactive | ManifestDemoMode::Hybrid
    )
}

#[derive(Clone, Copy)]
enum DemoLaunchMode {
    DetachedJson,
    AttachedStream,
    AttachedPty,
}

impl DemoLaunchMode {
    fn attached_terminal(self) -> bool {
        matches!(self, Self::AttachedStream | Self::AttachedPty)
    }

    fn capture_output(self) -> bool {
        matches!(self, Self::DetachedJson | Self::AttachedPty)
    }

    fn forward_stdin(self) -> bool {
        matches!(self, Self::AttachedPty)
    }

    fn supports_input_forwarding(self) -> bool {
        matches!(self, Self::DetachedJson)
    }

    fn supports_resize(self) -> bool {
        matches!(self, Self::DetachedJson)
    }

    fn transport(self) -> PersistedDemoTerminalTransport {
        match self {
            Self::AttachedPty => PersistedDemoTerminalTransport::Pty,
            Self::DetachedJson | Self::AttachedStream => PersistedDemoTerminalTransport::Stream,
        }
    }
}

fn initial_terminal_size_for_launch_mode(launch_mode: DemoLaunchMode) -> Option<(u16, u16)> {
    match launch_mode {
        DemoLaunchMode::AttachedStream | DemoLaunchMode::AttachedPty => current_terminal_size(),
        DemoLaunchMode::DetachedJson => {
            Some((DEMO_DEFAULT_TERMINAL_COLS, DEMO_DEFAULT_TERMINAL_ROWS))
        }
    }
}

fn current_terminal_size() -> Option<(u16, u16)> {
    crossterm::terminal::size().ok()
}

fn resolve_demo_launch_mode(mode: ManifestDemoMode, output_json: bool) -> DemoLaunchMode {
    if output_json {
        return DemoLaunchMode::DetachedJson;
    }
    if !demo_mode_prefers_attached_terminal(mode) {
        return DemoLaunchMode::DetachedJson;
    }
    if demo_runtime_supports_pty() {
        DemoLaunchMode::AttachedPty
    } else {
        DemoLaunchMode::AttachedStream
    }
}

#[cfg(target_os = "macos")]
fn demo_runtime_supports_pty() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
fn demo_runtime_supports_pty() -> bool {
    false
}

fn build_run_backed_process(
    repo_root: &Path,
    run_command: &str,
    launch_mode: DemoLaunchMode,
) -> Result<ProcessCommand, RunnerError> {
    let mut process = match launch_mode {
        DemoLaunchMode::AttachedPty => build_run_backed_pty_process(repo_root, run_command),
        DemoLaunchMode::DetachedJson | DemoLaunchMode::AttachedStream => {
            let mut process = ProcessCommand::new("sh");
            process.arg("-c").arg(run_command).current_dir(repo_root);
            process
        }
    };
    if launch_mode.capture_output() {
        process
            .stdin(
                if launch_mode.forward_stdin() || launch_mode.supports_input_forwarding() {
                    Stdio::piped()
                } else {
                    Stdio::inherit()
                },
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    } else {
        process
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    }
    #[cfg(unix)]
    unsafe {
        process.pre_exec(|| {
            setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|error| std::io::Error::other(error.to_string()))
        });
    }
    with_local_node_bin_path(&mut process, repo_root);
    Ok(process)
}

#[cfg(target_os = "macos")]
fn build_run_backed_pty_process(repo_root: &Path, run_command: &str) -> ProcessCommand {
    let mut process = ProcessCommand::new("script");
    process
        .arg("-q")
        .arg("/dev/null")
        .arg("sh")
        .arg("-c")
        .arg(run_command)
        .current_dir(repo_root);
    process
}

#[cfg(not(target_os = "macos"))]
fn build_run_backed_pty_process(repo_root: &Path, run_command: &str) -> ProcessCommand {
    let mut process = ProcessCommand::new("sh");
    process.arg("-c").arg(run_command).current_dir(repo_root);
    process
}

fn run_attempt_from_output(
    demo_id: &str,
    run_command: &str,
    exit_code: Option<i32>,
    success: bool,
    stop_requested: bool,
    stdout: String,
    stderr: String,
    log_paths: DemoLogPaths,
) -> DemoExecutionAttempt {
    if stop_requested {
        return terminated_demo_attempt(
            "run",
            run_command,
            run_command,
            exit_code,
            format!("Demo `{demo_id}` was terminated after stop was requested."),
            stdout,
            stderr,
            log_paths,
        );
    }
    if success {
        return successful_demo_attempt(
            "run",
            run_command,
            run_command,
            exit_code,
            Some(format!("Demo `{demo_id}` completed via run entrypoint.")),
            stdout,
            stderr,
            log_paths,
        );
    }
    failed_demo_attempt(
        "run",
        run_command,
        run_command,
        exit_code,
        format!("Demo `{demo_id}` failed via run entrypoint."),
        stdout,
        stderr,
        log_paths,
    )
}

#[derive(Clone, Copy)]
enum OutputMirror {
    Stdout,
    Stderr,
}

fn spawn_output_capture<R>(
    mut reader: R,
    log_path: Option<PathBuf>,
    mirror: Option<OutputMirror>,
) -> thread::JoinHandle<Result<String, RunnerError>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        let mut sink = match log_path {
            Some(path) => Some(
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|error| RunnerError::task_invocation_failed_write(&path, error))?,
            ),
            None => None,
        };
        let mut buffer = [0u8; 4096];
        loop {
            let read = reader.read(&mut buffer).map_err(|error| {
                RunnerError::task_invocation(format!("failed to read demo output: {error}"))
            })?;
            if read == 0 {
                break;
            }
            output.extend_from_slice(&buffer[..read]);
            if let Some(file) = sink.as_mut() {
                file.write_all(&buffer[..read]).map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "failed to write demo output log: {error}"
                    ))
                })?;
            }
            if let Some(mirror) = mirror {
                match mirror {
                    OutputMirror::Stdout => {
                        let mut stream = io::stdout().lock();
                        stream.write_all(&buffer[..read]).map_err(|error| {
                            RunnerError::task_invocation(format!(
                                "failed to mirror demo stdout: {error}"
                            ))
                        })?;
                        stream.flush().map_err(|error| {
                            RunnerError::task_invocation(format!(
                                "failed to flush demo stdout: {error}"
                            ))
                        })?;
                    }
                    OutputMirror::Stderr => {
                        let mut stream = io::stderr().lock();
                        stream.write_all(&buffer[..read]).map_err(|error| {
                            RunnerError::task_invocation(format!(
                                "failed to mirror demo stderr: {error}"
                            ))
                        })?;
                        stream.flush().map_err(|error| {
                            RunnerError::task_invocation(format!(
                                "failed to flush demo stderr: {error}"
                            ))
                        })?;
                    }
                }
            }
        }
        Ok(String::from_utf8_lossy(&output).to_string())
    })
}

fn spawn_stdin_forward(mut child_stdin: ChildStdin) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut input = io::stdin().lock();
        let _ = io::copy(&mut input, &mut child_stdin);
        let _ = child_stdin.flush();
    })
}

fn sanitize_pty_transcript(output: &str) -> String {
    output
        .chars()
        .filter(|ch| matches!(ch, '\n' | '\r' | '\t') || !ch.is_control())
        .collect::<String>()
        .replace("^D", "")
}

struct DemoInputHandoffForward {
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<()>,
}

fn prepare_demo_input_handoff(repo_root: &Path, demo_id: &str) -> Result<PathBuf, RunnerError> {
    let path = effective_input_handoff_path(repo_root, demo_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    fs::write(&path, "")
        .map_err(|error| RunnerError::task_invocation_failed_write(&path, error))?;
    Ok(path)
}

fn prepare_demo_resize_handoff(repo_root: &Path, demo_id: &str) -> Result<PathBuf, RunnerError> {
    let path = effective_resize_handoff_path(repo_root, demo_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    fs::write(&path, "")
        .map_err(|error| RunnerError::task_invocation_failed_write(&path, error))?;
    Ok(path)
}

fn spawn_input_handoff_forward(
    path: PathBuf,
    mut child_stdin: ChildStdin,
) -> DemoInputHandoffForward {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        let mut forwarded_bytes = 0usize;
        while !stop_flag.load(Ordering::Relaxed) {
            if let Ok(bytes) = fs::read(&path) {
                if bytes.len() < forwarded_bytes {
                    forwarded_bytes = 0;
                }
                if bytes.len() > forwarded_bytes {
                    let chunk = &bytes[forwarded_bytes..];
                    if child_stdin
                        .write_all(chunk)
                        .and_then(|_| child_stdin.flush())
                        .is_err()
                    {
                        break;
                    }
                    forwarded_bytes = bytes.len();
                }
            }
            thread::sleep(Duration::from_millis(DEMO_INPUT_POLL_INTERVAL_MS));
        }
        let _ = child_stdin.flush();
    });
    DemoInputHandoffForward { stop, handle }
}

fn stop_input_handoff_forward(forward: DemoInputHandoffForward, path: Option<&Path>) {
    forward.stop.store(true, Ordering::Relaxed);
    let _ = forward.handle.join();
    if let Some(path) = path {
        let _ = fs::remove_file(path);
    }
}

fn clear_resize_handoff(path: Option<&Path>) {
    if let Some(path) = path {
        let _ = fs::remove_file(path);
    }
}

fn join_output_capture(
    handle: thread::JoinHandle<Result<String, RunnerError>>,
    stream_name: &str,
    demo_id: &str,
) -> Result<String, RunnerError> {
    handle.join().map_err(|_| {
        RunnerError::task_invocation(format!(
            "demo `{demo_id}` {stream_name} capture thread panicked"
        ))
    })?
}

fn active_attempt_is_stop_requested(repo_root: &Path, demo_id: &str) -> bool {
    read_active_attempt_record(repo_root, demo_id)
        .ok()
        .flatten()
        .is_some_and(|record| record.phase == PersistedDemoActivePhase::StopRequested)
}

fn register_active_attempt(
    repo_root: &Path,
    demo_id: &str,
    record: PersistedDemoActiveAttempt,
) -> Result<DemoActiveAttemptGuard, RunnerError> {
    write_active_attempt_record(repo_root, demo_id, &record)?;
    Ok(DemoActiveAttemptGuard {
        path: effective_active_attempt_path(repo_root, demo_id),
    })
}

fn write_active_attempt_record(
    repo_root: &Path,
    demo_id: &str,
    record: &PersistedDemoActiveAttempt,
) -> Result<(), RunnerError> {
    let path = effective_active_attempt_path(repo_root, demo_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    let rendered = serde_json::to_string_pretty(record)
        .map_err(|error| RunnerError::task_invocation_failed_render(&path, error))?;
    write_atomic_text_file(&path, &rendered)
}

fn clear_active_attempt_state(repo_root: &Path, demo_id: &str) {
    let path = effective_active_attempt_path(repo_root, demo_id);
    let _ = fs::remove_file(path);
}

fn write_atomic_text_file(path: &Path, contents: &str) -> Result<(), RunnerError> {
    let Some(parent) = path.parent() else {
        return fs::write(path, contents)
            .map_err(|error| RunnerError::task_invocation_failed_write(path, error));
    };
    let temp_path = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("effigy-temp"),
        std::process::id(),
        now_epoch_ms()
    ));
    fs::write(&temp_path, contents)
        .map_err(|error| RunnerError::task_invocation_failed_write(&temp_path, error))?;
    fs::rename(&temp_path, path)
        .map_err(|error| RunnerError::task_invocation_failed_write(path, error))
}

fn request_demo_termination(target_pid: u32) -> Result<(), RunnerError> {
    #[cfg(unix)]
    {
        let raw = target_pid as i32;
        match signal::kill(Pid::from_raw(-raw), Signal::SIGTERM) {
            Ok(()) => Ok(()),
            Err(error) => Err(RunnerError::task_invocation(format!(
                "failed to send stop signal to demo process group `{target_pid}`: {error}"
            ))),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = target_pid;
        Err(RunnerError::task_invocation(
            "demo stop is not supported on this platform in the current runtime".to_owned(),
        ))
    }
}

fn successful_demo_attempt(
    entrypoint_kind: &str,
    entrypoint_value: &str,
    command: &str,
    exit_code: Option<i32>,
    summary: Option<String>,
    stdout: String,
    stderr: String,
    log_paths: DemoLogPaths,
) -> DemoExecutionAttempt {
    DemoExecutionAttempt {
        ok: true,
        outcome: "passed".to_owned(),
        entrypoint_kind: entrypoint_kind.to_owned(),
        entrypoint_value: entrypoint_value.to_owned(),
        command: command.to_owned(),
        exit_code,
        summary,
        stdout,
        stderr,
        stdout_log_path: log_paths.stdout,
        stderr_log_path: log_paths.stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    }
}

fn failed_demo_attempt(
    entrypoint_kind: &str,
    entrypoint_value: &str,
    command: &str,
    exit_code: Option<i32>,
    summary: String,
    stdout: String,
    stderr: String,
    log_paths: DemoLogPaths,
) -> DemoExecutionAttempt {
    DemoExecutionAttempt {
        ok: false,
        outcome: "failed".to_owned(),
        entrypoint_kind: entrypoint_kind.to_owned(),
        entrypoint_value: entrypoint_value.to_owned(),
        command: command.to_owned(),
        exit_code,
        summary: Some(summary),
        stdout,
        stderr,
        stdout_log_path: log_paths.stdout,
        stderr_log_path: log_paths.stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    }
}

fn terminated_demo_attempt(
    entrypoint_kind: &str,
    entrypoint_value: &str,
    command: &str,
    exit_code: Option<i32>,
    summary: String,
    stdout: String,
    stderr: String,
    log_paths: DemoLogPaths,
) -> DemoExecutionAttempt {
    DemoExecutionAttempt {
        ok: false,
        outcome: "terminated".to_owned(),
        entrypoint_kind: entrypoint_kind.to_owned(),
        entrypoint_value: entrypoint_value.to_owned(),
        command: command.to_owned(),
        exit_code,
        summary: Some(summary),
        stdout,
        stderr,
        stdout_log_path: log_paths.stdout,
        stderr_log_path: log_paths.stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    }
}

fn write_latest_attempt_receipt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    attempt: &DemoExecutionAttempt,
) -> Result<(), RunnerError> {
    let receipt_path = effective_receipt_path(repo_root, demo_id, demo);
    if let Some(parent) = receipt_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }

    let rendered = serde_json::to_string_pretty(&json!({
        "schema": "effigy.demo.receipt.v1",
        "schema_version": 1,
        "demo_id": demo_id,
        "ok": attempt.ok,
        "status": attempt.outcome,
        "summary": attempt.summary,
        "stale": false,
        "recorded_at_epoch_ms": attempt.recorded_at_epoch_ms,
        "entrypoint": {
            "kind": attempt.entrypoint_kind,
            "value": attempt.entrypoint_value,
        },
        "command": attempt.command,
        "exit_code": attempt.exit_code,
        "stdout_log_path": attempt.stdout_log_path,
        "stderr_log_path": attempt.stderr_log_path,
        "artifacts": demo.artifacts,
    }))
    .map_err(|error| RunnerError::task_invocation_failed_render(&receipt_path, error))?;

    fs::write(&receipt_path, rendered)
        .map_err(|error| RunnerError::task_invocation_failed_write(&receipt_path, error))?;
    append_attempt_history(repo_root, demo_id, demo, attempt)?;
    Ok(())
}

fn effective_receipt_path(repo_root: &Path, demo_id: &str, demo: &ManifestDemoConfig) -> PathBuf {
    if let Some(path) = &demo.receipt {
        return repo_root.join(path);
    }
    repo_root
        .join(DEMO_RECEIPTS_DIR)
        .join(format!("{}.json", sanitize_demo_id_for_filename(demo_id)))
}

fn effective_active_attempt_path(repo_root: &Path, demo_id: &str) -> PathBuf {
    repo_root
        .join(DEMO_ACTIVE_DIR)
        .join(format!("{}.json", sanitize_demo_id_for_filename(demo_id)))
}

fn effective_attempt_history_path(repo_root: &Path, demo_id: &str) -> PathBuf {
    repo_root
        .join(DEMO_HISTORY_DIR)
        .join(format!("{}.json", sanitize_demo_id_for_filename(demo_id)))
}

fn effective_output_log_path(repo_root: &Path, demo_id: &str, stream: &str) -> PathBuf {
    repo_root.join(DEMO_LOGS_DIR).join(format!(
        "{}.{}.log",
        sanitize_demo_id_for_filename(demo_id),
        stream
    ))
}

fn effective_input_handoff_path(repo_root: &Path, demo_id: &str) -> PathBuf {
    repo_root.join(DEMO_ACTIVE_DIR).join(format!(
        "{}.stdin.log",
        sanitize_demo_id_for_filename(demo_id)
    ))
}

fn effective_resize_handoff_path(repo_root: &Path, demo_id: &str) -> PathBuf {
    repo_root.join(DEMO_ACTIVE_DIR).join(format!(
        "{}.resize.jsonl",
        sanitize_demo_id_for_filename(demo_id)
    ))
}

fn render_active_attempt_path(repo_root: &Path, demo_id: &str) -> String {
    display_repo_path(
        &effective_active_attempt_path(repo_root, demo_id),
        repo_root,
    )
}

fn append_attempt_history(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    attempt: &DemoExecutionAttempt,
) -> Result<(), RunnerError> {
    let path = effective_attempt_history_path(repo_root, demo_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }

    let mut history = if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|error| RunnerError::task_invocation_failed_read(&path, error))?;
        serde_json::from_str::<PersistedDemoAttemptHistory>(&content)
            .map_err(|error| RunnerError::task_invocation_failed_parse(&path, error))?
    } else {
        PersistedDemoAttemptHistory::new(demo_id)
    };

    history.push(PersistedDemoHistoricalAttempt::from_execution(
        demo_id,
        demo,
        attempt,
        display_repo_path(&effective_receipt_path(repo_root, demo_id, demo), repo_root),
    ));

    let rendered = serde_json::to_string_pretty(&history)
        .map_err(|error| RunnerError::task_invocation_failed_render(&path, error))?;
    fs::write(&path, rendered)
        .map_err(|error| RunnerError::task_invocation_failed_write(&path, error))
}

fn build_attempt_id(demo_id: &str) -> String {
    format!(
        "{}-{}",
        sanitize_demo_id_for_filename(demo_id),
        now_epoch_ms()
    )
}

fn sanitize_demo_id_for_filename(demo_id: &str) -> String {
    demo_id
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '_',
            _ => ch,
        })
        .collect()
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn persist_demo_attempt_logs(
    repo_root: &Path,
    demo_id: &str,
    stdout: &str,
    stderr: &str,
) -> Result<DemoLogPaths, RunnerError> {
    if stdout.is_empty() && stderr.is_empty() {
        return Ok(DemoLogPaths::none());
    }
    let log_paths = DemoLogPaths::prepare_split(repo_root, demo_id)?;
    if let Some(path) = &log_paths.stdout_absolute {
        fs::write(path, stdout)
            .map_err(|error| RunnerError::task_invocation_failed_write(path, error))?;
    }
    if let Some(path) = &log_paths.stderr_absolute {
        fs::write(path, stderr)
            .map_err(|error| RunnerError::task_invocation_failed_write(path, error))?;
    }
    Ok(log_paths)
}

#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let raw = pid as i32;
    match signal::kill(Pid::from_raw(raw), None) {
        Ok(()) => true,
        Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

#[cfg(not(unix))]
fn pid_is_alive(pid: u32) -> bool {
    pid != 0
}

fn derive_gap_class(status: ManifestDemoStatus, stale: bool) -> &'static str {
    if stale {
        return "stale";
    }
    match status {
        ManifestDemoStatus::Planned => "planned",
        ManifestDemoStatus::Missing => "missing",
        ManifestDemoStatus::Broken => "broken",
        ManifestDemoStatus::Ready
        | ManifestDemoStatus::Running
        | ManifestDemoStatus::Passed
        | ManifestDemoStatus::Failed => "existing",
    }
}

fn display_status(
    status: ManifestDemoStatus,
    stale: bool,
    active_attempt: &DemoActiveAttempt,
) -> String {
    if active_attempt.active {
        return match active_attempt.state.as_str() {
            "stop-requested" => "running (stop-requested)".to_owned(),
            _ => "running".to_owned(),
        };
    }
    if stale {
        format!("{} (stale)", status.as_str())
    } else {
        status.as_str().to_owned()
    }
}

fn display_repo_path(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn demo_error(
    output_json: bool,
    schema: &str,
    message: String,
    extra: JsonValue,
) -> Result<String, RunnerError> {
    if output_json {
        let mut payload = serde_json::Map::new();
        payload.insert("schema".to_owned(), JsonValue::String(schema.to_owned()));
        payload.insert("schema_version".to_owned(), JsonValue::from(1));
        payload.insert("ok".to_owned(), JsonValue::Bool(false));
        payload.insert("message".to_owned(), JsonValue::String(message.clone()));
        if let JsonValue::Object(extra_map) = extra {
            payload.extend(extra_map);
        }
        let rendered = encode_json(&JsonValue::Object(payload), true)?;
        return Err(RunnerError::CommandJsonFailure { rendered });
    }
    Err(RunnerError::task_invocation(message))
}

#[derive(Debug, Clone, Copy)]
enum DemoInvocationKind {
    Run,
    Rerun,
}

impl DemoInvocationKind {
    fn schema(&self) -> &'static str {
        match self {
            Self::Run => "effigy.demo.run.v1",
            Self::Rerun => "effigy.demo.rerun.v1",
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::Run => "Demo Run",
            Self::Rerun => "Demo Rerun",
        }
    }
}

#[derive(Debug, Clone)]
struct DemoRecord {
    id: String,
    title: String,
    summary: String,
    proof: String,
    owner: String,
    mode: ManifestDemoMode,
    status: ManifestDemoStatus,
    covers: Vec<String>,
    tags: Vec<String>,
    prerequisites: Vec<String>,
    dependencies: Vec<String>,
    entrypoint: DemoEntrypoint,
    sources: Vec<String>,
    primary_source: String,
    gap_class: &'static str,
    runtime_backend: DemoRuntimeBackend,
    active_attempt: DemoActiveAttempt,
    active_terminal_session: DemoActiveTerminalSession,
    latest_attempt: DemoLatestAttempt,
    attempt_history: DemoAttemptHistory,
}

impl DemoRecord {
    fn effective_status(&self) -> String {
        display_status(self.status, self.latest_attempt.stale, &self.active_attempt)
    }

    fn freshness_label(&self) -> &'static str {
        if self.latest_attempt.stale {
            "stale"
        } else {
            "current"
        }
    }

    fn actions(&self) -> DemoActionAvailability {
        let can_run = !self.active_attempt.active;
        let can_rerun = !self.active_attempt.active;
        let can_stop = self.active_attempt.active && self.active_attempt.stoppable;
        DemoActionAvailability {
            run_available: can_run,
            run_reason: (!can_run).then(|| {
                "an active attempt already exists; stop it before starting a fresh run".to_owned()
            }),
            stop_available: can_stop,
            stop_reason: if can_stop {
                None
            } else if self.active_attempt.active {
                Some("the active attempt is not stoppable through the current runtime".to_owned())
            } else {
                Some("no active attempt is currently running".to_owned())
            },
            rerun_available: can_rerun,
            rerun_reason: (!can_rerun)
                .then(|| "an active attempt already exists; stop it before rerunning".to_owned()),
        }
    }

    fn to_json_summary(&self) -> JsonValue {
        json!({
            "id": self.id,
            "title": self.title,
            "summary": self.summary,
            "owner": self.owner,
            "mode": self.mode.as_str(),
            "status": self.status.as_str(),
            "effective_status": self.effective_status(),
            "freshness": self.freshness_label(),
            "stale": self.latest_attempt.stale,
            "gap_class": self.gap_class,
            "covers": self.covers,
            "tags": self.tags,
            "entrypoint": self.entrypoint.to_json(),
            "defined_in": self.primary_source,
            "runtime_backend": self.runtime_backend.to_json(),
            "actions": self.actions().to_json(),
            "active_attempt": self.active_attempt.to_json(),
            "active_terminal_session": self.active_terminal_session.to_json(),
            "latest_attempt": self.latest_attempt.to_json(),
        })
    }

    fn to_json_detail(&self) -> JsonValue {
        json!({
            "id": self.id,
            "title": self.title,
            "summary": self.summary,
            "proof": self.proof,
            "owner": self.owner,
            "mode": self.mode.as_str(),
            "status": self.status.as_str(),
            "effective_status": self.effective_status(),
            "freshness": self.freshness_label(),
            "stale": self.latest_attempt.stale,
            "gap_class": self.gap_class,
            "covers": self.covers,
            "tags": self.tags,
            "prerequisites": self.prerequisites,
            "dependencies": self.dependencies,
            "entrypoint": self.entrypoint.to_json(),
            "defined_in": self.primary_source,
            "sources": self.sources,
            "runtime_backend": self.runtime_backend.to_json(),
            "actions": self.actions().to_json(),
            "active_attempt": self.active_attempt.to_json(),
            "active_terminal_session": self.active_terminal_session.to_json(),
            "latest_attempt": self.latest_attempt.to_json(),
            "attempt_history": self.attempt_history.to_json(),
        })
    }

    fn matches_query(&self, query: &DemoListQuery) -> bool {
        if let Some(search) = &query.search {
            let needle = search.to_ascii_lowercase();
            let haystacks = [&self.id, &self.title, &self.summary];
            if !haystacks
                .iter()
                .any(|value| value.to_ascii_lowercase().contains(&needle))
            {
                return false;
            }
        }
        if let Some(owner) = &query.owner {
            if &self.owner != owner {
                return false;
            }
        }
        if let Some(tag) = &query.tag {
            if !self.tags.iter().any(|value| value == tag) {
                return false;
            }
        }
        if let Some(mode) = query.mode {
            if self.mode.as_str() != mode.as_str() {
                return false;
            }
        }
        if let Some(cover) = &query.cover {
            if !self.covers.iter().any(|value| value == cover) {
                return false;
            }
        }
        if let Some(status) = query.status {
            if self.browser_status() != status {
                return false;
            }
        }
        if let Some(gap) = query.gap {
            if self.gap_class != gap.as_str() {
                return false;
            }
        }
        if query.stale_only && !self.latest_attempt.stale {
            return false;
        }
        true
    }

    fn browser_status(&self) -> DemoListStatus {
        if self.active_attempt.active {
            return DemoListStatus::Running;
        }
        match self.status {
            ManifestDemoStatus::Planned => DemoListStatus::Planned,
            ManifestDemoStatus::Ready => DemoListStatus::Ready,
            ManifestDemoStatus::Running => DemoListStatus::Running,
            ManifestDemoStatus::Passed => DemoListStatus::Passed,
            ManifestDemoStatus::Failed => DemoListStatus::Failed,
            ManifestDemoStatus::Broken => DemoListStatus::Broken,
            ManifestDemoStatus::Missing => DemoListStatus::Missing,
        }
    }
}

#[derive(Debug, Clone)]
struct DemoActionAvailability {
    run_available: bool,
    run_reason: Option<String>,
    stop_available: bool,
    stop_reason: Option<String>,
    rerun_available: bool,
    rerun_reason: Option<String>,
}

impl DemoActionAvailability {
    fn summary_label(&self) -> String {
        let mut actions = Vec::new();
        if self.run_available {
            actions.push("run");
        }
        if self.stop_available {
            actions.push("stop");
        }
        if self.rerun_available {
            actions.push("rerun");
        }
        if actions.is_empty() {
            "none".to_owned()
        } else {
            actions.join(", ")
        }
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "run": {
                "available": self.run_available,
                "reason": self.run_reason,
            },
            "stop": {
                "available": self.stop_available,
                "reason": self.stop_reason,
            },
            "rerun": {
                "available": self.rerun_available,
                "reason": self.rerun_reason,
            },
        })
    }

    fn to_key_values(&self) -> Vec<KeyValue> {
        vec![
            KeyValue::new(
                "run",
                availability_label(self.run_available, self.run_reason.as_deref()),
            ),
            KeyValue::new(
                "stop",
                availability_label(self.stop_available, self.stop_reason.as_deref()),
            ),
            KeyValue::new(
                "rerun",
                availability_label(self.rerun_available, self.rerun_reason.as_deref()),
            ),
        ]
    }
}

#[derive(Debug, Clone)]
struct DemoGroup<'a> {
    label: String,
    demos: Vec<&'a DemoRecord>,
}

impl DemoGroup<'_> {
    fn to_json(&self) -> JsonValue {
        json!({
            "label": self.label,
            "count": self.demos.len(),
            "demos": self.demos.iter().map(|demo| demo.to_json_summary()).collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone)]
enum DemoEntrypoint {
    Task(String),
    Run(String),
}

impl DemoEntrypoint {
    fn render_compact(&self) -> String {
        match self {
            Self::Task(task) => format!("task:{task}"),
            Self::Run(run) => format!("run:{run}"),
        }
    }

    fn render_full(&self) -> String {
        match self {
            Self::Task(task) => format!("task `{task}`"),
            Self::Run(run) => format!("run `{run}`"),
        }
    }

    fn to_json(&self) -> JsonValue {
        match self {
            Self::Task(task) => json!({ "kind": "task", "value": task }),
            Self::Run(run) => json!({ "kind": "run", "value": run }),
        }
    }
}

#[derive(Debug, Clone)]
struct DemoRuntimeBackend {
    kind: String,
    label: String,
    flattened_projection: bool,
    capabilities: Vec<String>,
}

impl DemoRuntimeBackend {
    fn none() -> Self {
        Self {
            kind: "none".to_owned(),
            label: "none".to_owned(),
            flattened_projection: false,
            capabilities: Vec::new(),
        }
    }

    fn from_entrypoint(entrypoint: &DemoEntrypoint) -> Self {
        match entrypoint {
            DemoEntrypoint::Task(_) => Self {
                kind: "task".to_owned(),
                label: "task-backed".to_owned(),
                flattened_projection: false,
                capabilities: Vec::new(),
            },
            DemoEntrypoint::Run(_) => Self {
                kind: "run".to_owned(),
                label: "run-backed".to_owned(),
                flattened_projection: false,
                capabilities: vec![
                    "active-terminal-session".to_owned(),
                    "live-terminal-output".to_owned(),
                    "stop".to_owned(),
                ],
            },
        }
    }

    fn rendered_capabilities(&self) -> String {
        if self.capabilities.is_empty() {
            "none".to_owned()
        } else {
            self.capabilities.join(", ")
        }
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "kind": self.kind,
            "label": self.label,
            "flattened_projection": self.flattened_projection,
            "capabilities": self.capabilities,
        })
    }
}

fn demo_runtime_backend(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    entrypoint: &DemoEntrypoint,
    active_attempt: &DemoActiveAttempt,
) -> DemoRuntimeBackend {
    if active_attempt.active {
        active_attempt.runtime_backend()
    } else {
        demo_runtime_backend_from_entrypoint(repo_root, loaded, entrypoint)
    }
}

fn demo_runtime_backend_from_entrypoint(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    entrypoint: &DemoEntrypoint,
) -> DemoRuntimeBackend {
    match entrypoint {
        DemoEntrypoint::Task(task_name) => {
            if loaded
                .manifest
                .tasks
                .get(task_name)
                .is_some_and(task_is_concurrent_runner_backed)
                || demo_task_selection(repo_root, task_name)
                    .ok()
                    .flatten()
                    .and_then(|selection| selection.task().ok().map(task_is_concurrent_runner_backed))
                    .unwrap_or(false)
            {
                DemoRuntimeBackend {
                    kind: "concurrent-runner".to_owned(),
                    label: runtime_backend_label("concurrent-runner").to_owned(),
                    flattened_projection: true,
                    capabilities: vec![
                        "active-terminal-session".to_owned(),
                        "live-terminal-output".to_owned(),
                    ],
                }
            } else {
                DemoRuntimeBackend::from_entrypoint(entrypoint)
            }
        }
        DemoEntrypoint::Run(_) => DemoRuntimeBackend::from_entrypoint(entrypoint),
    }
}

#[derive(Debug, Clone)]
struct DemoLogPaths {
    stdout: Option<String>,
    stderr: Option<String>,
    stdout_absolute: Option<PathBuf>,
    stderr_absolute: Option<PathBuf>,
}

impl DemoLogPaths {
    fn none() -> Self {
        Self {
            stdout: None,
            stderr: None,
            stdout_absolute: None,
            stderr_absolute: None,
        }
    }

    fn prepare_for_launch_mode(
        repo_root: &Path,
        demo_id: &str,
        launch_mode: DemoLaunchMode,
    ) -> Result<Self, RunnerError> {
        match launch_mode {
            DemoLaunchMode::AttachedPty => Self::prepare_pty(repo_root, demo_id),
            DemoLaunchMode::DetachedJson | DemoLaunchMode::AttachedStream => {
                Self::prepare_split(repo_root, demo_id)
            }
        }
    }

    fn prepare_split(repo_root: &Path, demo_id: &str) -> Result<Self, RunnerError> {
        let stdout_absolute = effective_output_log_path(repo_root, demo_id, "stdout");
        let stderr_absolute = effective_output_log_path(repo_root, demo_id, "stderr");
        if let Some(parent) = stdout_absolute.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
        }
        fs::write(&stdout_absolute, "")
            .map_err(|error| RunnerError::task_invocation_failed_write(&stdout_absolute, error))?;
        fs::write(&stderr_absolute, "")
            .map_err(|error| RunnerError::task_invocation_failed_write(&stderr_absolute, error))?;
        Ok(Self {
            stdout: Some(display_repo_path(&stdout_absolute, repo_root)),
            stderr: Some(display_repo_path(&stderr_absolute, repo_root)),
            stdout_absolute: Some(stdout_absolute),
            stderr_absolute: Some(stderr_absolute),
        })
    }

    fn prepare_pty(repo_root: &Path, demo_id: &str) -> Result<Self, RunnerError> {
        let stdout_absolute = effective_output_log_path(repo_root, demo_id, "stdout");
        if let Some(parent) = stdout_absolute.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
        }
        fs::write(&stdout_absolute, "")
            .map_err(|error| RunnerError::task_invocation_failed_write(&stdout_absolute, error))?;
        Ok(Self {
            stdout: Some(display_repo_path(&stdout_absolute, repo_root)),
            stderr: None,
            stdout_absolute: Some(stdout_absolute),
            stderr_absolute: None,
        })
    }
}

#[derive(Debug, Clone)]
struct DemoActiveAttempt {
    active: bool,
    state: String,
    attempt_id: Option<String>,
    state_path: Option<String>,
    owner_pid: Option<u32>,
    target_pid: Option<u32>,
    stoppable: bool,
    started_at_epoch_ms: Option<u128>,
    entrypoint_kind: Option<String>,
    entrypoint_value: Option<String>,
    command: Option<String>,
    runtime_backend_kind: String,
    flattened_runtime_projection: bool,
    terminal_transport: DemoTerminalTransport,
    supports_input_forwarding: bool,
    supports_resize: bool,
    nested_tui: bool,
    terminal_cols: Option<u16>,
    terminal_rows: Option<u16>,
    resize_handoff_path: Option<String>,
    stdin_input_path: Option<String>,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
    parse_error: Option<String>,
}

impl DemoActiveAttempt {
    fn inactive(state_path: Option<String>) -> Self {
        Self {
            active: false,
            state: "not-active".to_owned(),
            attempt_id: None,
            state_path,
            owner_pid: None,
            target_pid: None,
            stoppable: false,
            started_at_epoch_ms: None,
            entrypoint_kind: None,
            entrypoint_value: None,
            command: None,
            runtime_backend_kind: "none".to_owned(),
            flattened_runtime_projection: false,
            terminal_transport: DemoTerminalTransport::None,
            supports_input_forwarding: false,
            supports_resize: false,
            nested_tui: false,
            terminal_cols: None,
            terminal_rows: None,
            resize_handoff_path: None,
            stdin_input_path: None,
            stdout_log_path: None,
            stderr_log_path: None,
            parse_error: None,
        }
    }

    fn state_label(&self) -> &str {
        &self.state
    }

    fn runtime_backend(&self) -> DemoRuntimeBackend {
        if !self.active {
            return DemoRuntimeBackend::none();
        }
        let mut capabilities = Vec::new();
        if matches!(
            self.runtime_backend_kind.as_str(),
            "run" | "concurrent-runner"
        ) {
            capabilities.push("active-terminal-session".to_owned());
            capabilities.push("live-terminal-output".to_owned());
            if self.stoppable {
                capabilities.push("stop".to_owned());
            }
            if self.supports_input_forwarding {
                capabilities.push("input-forwarding".to_owned());
            }
            if self.supports_resize {
                capabilities.push("resize".to_owned());
            }
            if self.terminal_transport == DemoTerminalTransport::Pty {
                capabilities.push("pty".to_owned());
            }
        }
        DemoRuntimeBackend {
            kind: self.runtime_backend_kind.clone(),
            label: runtime_backend_label(&self.runtime_backend_kind).to_owned(),
            flattened_projection: self.flattened_runtime_projection,
            capabilities,
        }
    }

    fn to_key_values(&self) -> Vec<KeyValue> {
        let mut values = vec![
            KeyValue::new("state", self.state.clone()),
            KeyValue::new("runtime-backend", self.runtime_backend().label.clone()),
            KeyValue::new(
                "runtime-flattened",
                if self.flattened_runtime_projection {
                    "yes"
                } else {
                    "no"
                },
            ),
            KeyValue::new(
                "runtime-capabilities",
                self.runtime_backend().rendered_capabilities(),
            ),
            KeyValue::new(
                "stoppable",
                if self.stoppable {
                    "yes".to_owned()
                } else {
                    "no".to_owned()
                },
            ),
            KeyValue::new(
                "state-path",
                self.state_path
                    .clone()
                    .unwrap_or_else(|| "<none>".to_owned()),
            ),
        ];
        if let Some(attempt_id) = &self.attempt_id {
            values.push(KeyValue::new("attempt-id", attempt_id.clone()));
        }
        if let Some(owner_pid) = self.owner_pid {
            values.push(KeyValue::new("owner-pid", owner_pid.to_string()));
        }
        if let Some(target_pid) = self.target_pid {
            values.push(KeyValue::new("target-pid", target_pid.to_string()));
        }
        if let Some(started_at_epoch_ms) = self.started_at_epoch_ms {
            values.push(KeyValue::new(
                "started-at-epoch-ms",
                started_at_epoch_ms.to_string(),
            ));
        }
        if let (Some(kind), Some(value)) = (&self.entrypoint_kind, &self.entrypoint_value) {
            values.push(KeyValue::new("entrypoint", format!("{kind}:{value}")));
        }
        if let Some(command) = &self.command {
            values.push(KeyValue::new("command", command.clone()));
        }
        if let Some(stdout_log_path) = &self.stdout_log_path {
            values.push(KeyValue::new("stdout-log", stdout_log_path.clone()));
        }
        if let Some(stderr_log_path) = &self.stderr_log_path {
            values.push(KeyValue::new("stderr-log", stderr_log_path.clone()));
        }
        if let Some(parse_error) = &self.parse_error {
            values.push(KeyValue::new("parse-error", parse_error.clone()));
        }
        values
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "active": self.active,
            "state": self.state,
            "attempt_id": self.attempt_id,
            "state_path": self.state_path,
            "owner_pid": self.owner_pid,
            "target_pid": self.target_pid,
            "stoppable": self.stoppable,
            "started_at_epoch_ms": self.started_at_epoch_ms,
            "entrypoint": {
                "kind": self.entrypoint_kind,
                "value": self.entrypoint_value,
            },
            "command": self.command,
            "runtime_backend": self.runtime_backend().to_json(),
            "terminal_transport": self.terminal_transport.rendered(),
            "supports_input_forwarding": self.supports_input_forwarding,
            "supports_resize": self.supports_resize,
            "nested_tui": self.nested_tui,
            "terminal_size": {
                "cols": self.terminal_cols,
                "rows": self.terminal_rows,
            },
            "resize_handoff_path": self.resize_handoff_path,
            "stdin_input_path": self.stdin_input_path,
            "stdout_log_path": self.stdout_log_path,
            "stderr_log_path": self.stderr_log_path,
            "output_available": self.stdout_log_path.is_some() || self.stderr_log_path.is_some(),
            "parse_error": self.parse_error,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DemoTerminalTransport {
    None,
    Stream,
    Pty,
}

impl DemoTerminalTransport {
    fn rendered(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Stream => "stream",
            Self::Pty => "pty",
        }
    }
}

#[derive(Debug, Clone)]
struct DemoActiveTerminalSession {
    available: bool,
    state: String,
    attempt_id: Option<String>,
    runtime_backend: DemoRuntimeBackend,
    transport: String,
    pty: bool,
    supports_input_forwarding: bool,
    input_forwarding_reason: Option<String>,
    input_forwarding: DemoTerminalInputForwarding,
    nested_tui: bool,
    terminal_size: DemoTerminalSize,
    resize: DemoTerminalResizeForwarding,
    resize_handoff_path: Option<String>,
    stdin_input_path: Option<String>,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
    output_available: bool,
    recent_stdout: Vec<String>,
    recent_stderr: Vec<String>,
}

impl DemoActiveTerminalSession {
    fn inactive() -> Self {
        Self {
            available: false,
            state: "none".to_owned(),
            attempt_id: None,
            runtime_backend: DemoRuntimeBackend::none(),
            transport: "none".to_owned(),
            pty: false,
            supports_input_forwarding: false,
            input_forwarding_reason: Some(
                "no active demo terminal session is currently available".to_owned(),
            ),
            input_forwarding: DemoTerminalInputForwarding::unavailable(
                "no active demo terminal session is currently available".to_owned(),
            ),
            nested_tui: false,
            terminal_size: DemoTerminalSize {
                cols: None,
                rows: None,
            },
            resize: DemoTerminalResizeForwarding::unavailable(
                "no active demo terminal session is currently available".to_owned(),
            ),
            resize_handoff_path: None,
            stdin_input_path: None,
            stdout_log_path: None,
            stderr_log_path: None,
            output_available: false,
            recent_stdout: Vec::new(),
            recent_stderr: Vec::new(),
        }
    }

    fn to_key_values(&self) -> Vec<KeyValue> {
        let mut values = vec![
            KeyValue::new("state", self.state.clone()),
            KeyValue::new("runtime-backend", self.runtime_backend.label.clone()),
            KeyValue::new(
                "runtime-flattened",
                if self.runtime_backend.flattened_projection {
                    "yes"
                } else {
                    "no"
                },
            ),
            KeyValue::new(
                "runtime-capabilities",
                self.runtime_backend.rendered_capabilities(),
            ),
            KeyValue::new("transport", self.transport.clone()),
            KeyValue::new("pty", if self.pty { "yes" } else { "no" }),
            KeyValue::new(
                "input-forwarding",
                if self.input_forwarding.available {
                    "yes".to_owned()
                } else {
                    availability_label(false, self.input_forwarding_reason.as_deref())
                },
            ),
            KeyValue::new(
                "input-command",
                self.input_forwarding.command_template.clone(),
            ),
            KeyValue::new(
                "terminal-size",
                self.terminal_size
                    .rendered()
                    .unwrap_or_else(|| "<unknown>".to_owned()),
            ),
            KeyValue::new(
                "resize",
                if self.resize.available {
                    "yes".to_owned()
                } else {
                    availability_label(false, self.resize.reason.as_deref())
                },
            ),
            KeyValue::new("resize-command", self.resize.command_template.clone()),
            KeyValue::new("nested-tui", if self.nested_tui { "yes" } else { "no" }),
            KeyValue::new(
                "output-available",
                if self.output_available { "yes" } else { "no" },
            ),
        ];
        if let Some(attempt_id) = &self.attempt_id {
            values.push(KeyValue::new("attempt-id", attempt_id.clone()));
        }
        if let Some(stdin_input_path) = &self.stdin_input_path {
            values.push(KeyValue::new("stdin-input", stdin_input_path.clone()));
        }
        if let Some(resize_handoff_path) = &self.resize_handoff_path {
            values.push(KeyValue::new("resize-handoff", resize_handoff_path.clone()));
        }
        if let Some(stdout_log_path) = &self.stdout_log_path {
            values.push(KeyValue::new("stdout-log", stdout_log_path.clone()));
        }
        if let Some(stderr_log_path) = &self.stderr_log_path {
            values.push(KeyValue::new("stderr-log", stderr_log_path.clone()));
        }
        values
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "available": self.available,
            "state": self.state,
            "attempt_id": self.attempt_id,
            "runtime_backend": self.runtime_backend.to_json(),
            "transport": self.transport,
            "pty": self.pty,
            "supports_input_forwarding": self.supports_input_forwarding,
            "input_forwarding_reason": self.input_forwarding_reason,
            "input_forwarding": self.input_forwarding.to_json(),
            "nested_tui": self.nested_tui,
            "terminal_size": self.terminal_size.to_json(),
            "resize": self.resize.to_json(),
            "resize_handoff_path": self.resize_handoff_path,
            "stdin_input_path": self.stdin_input_path,
            "stdout_log_path": self.stdout_log_path,
            "stderr_log_path": self.stderr_log_path,
            "output_available": self.output_available,
            "recent_output": {
                "stdout_lines": self.recent_stdout,
                "stderr_lines": self.recent_stderr,
            },
        })
    }
}

#[derive(Debug, Clone)]
struct DemoTerminalSize {
    cols: Option<u16>,
    rows: Option<u16>,
}

impl DemoTerminalSize {
    fn rendered(&self) -> Option<String> {
        Some(format!("{}x{}", self.cols?, self.rows?))
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "cols": self.cols,
            "rows": self.rows,
        })
    }
}

#[derive(Debug, Clone)]
struct DemoTerminalInputForwarding {
    available: bool,
    reason: Option<String>,
    mode: String,
    append_newline_supported: bool,
    command_template: String,
}

impl DemoTerminalInputForwarding {
    fn unavailable(reason: String) -> Self {
        Self {
            available: false,
            reason: Some(reason),
            mode: "text".to_owned(),
            append_newline_supported: true,
            command_template: "effigy demo input <DEMO_ID> --text <TEXT> [--append-newline]"
                .to_owned(),
        }
    }

    fn available() -> Self {
        Self {
            available: true,
            reason: None,
            mode: "text".to_owned(),
            append_newline_supported: true,
            command_template: "effigy demo input <DEMO_ID> --text <TEXT> [--append-newline]"
                .to_owned(),
        }
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "available": self.available,
            "reason": self.reason,
            "mode": self.mode,
            "append_newline_supported": self.append_newline_supported,
            "command_template": self.command_template,
        })
    }
}

#[derive(Debug, Clone)]
struct DemoTerminalResizeForwarding {
    available: bool,
    reason: Option<String>,
    mode: String,
    command_template: String,
}

impl DemoTerminalResizeForwarding {
    fn unavailable(reason: String) -> Self {
        Self {
            available: false,
            reason: Some(reason),
            mode: "cells".to_owned(),
            command_template: "effigy demo resize <DEMO_ID> --cols <COLS> --rows <ROWS>".to_owned(),
        }
    }

    fn available() -> Self {
        Self {
            available: true,
            reason: None,
            mode: "cells".to_owned(),
            command_template: "effigy demo resize <DEMO_ID> --cols <COLS> --rows <ROWS>".to_owned(),
        }
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "available": self.available,
            "reason": self.reason,
            "mode": self.mode,
            "command_template": self.command_template,
        })
    }
}

#[derive(Debug, Clone)]
struct DemoLatestAttempt {
    recorded: bool,
    receipt_path: Option<String>,
    outcome: Option<String>,
    summary: Option<String>,
    stale: bool,
    artifacts: Vec<String>,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
    parse_error: Option<String>,
}

impl DemoLatestAttempt {
    fn state_label(&self) -> &'static str {
        if self.recorded {
            "recorded"
        } else {
            "no-recorded-attempt"
        }
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "recorded": self.recorded,
            "state": self.state_label(),
            "receipt_path": self.receipt_path,
            "receipt_present": self.receipt_path.is_some(),
            "outcome": self.outcome,
            "summary": self.summary,
            "freshness": if self.stale { "stale" } else { "current" },
            "stale": self.stale,
            "artifact_count": self.artifacts.len(),
            "artifacts": self.artifacts,
            "stdout_log_path": self.stdout_log_path,
            "stderr_log_path": self.stderr_log_path,
            "output_available": self.stdout_log_path.is_some() || self.stderr_log_path.is_some(),
            "parse_error": self.parse_error,
        })
    }
}

#[derive(Debug, Clone)]
struct DemoAttemptHistory {
    path: Option<String>,
    attempts: Vec<DemoHistoricalAttempt>,
    parse_error: Option<String>,
}

impl DemoAttemptHistory {
    fn to_json(&self) -> JsonValue {
        json!({
            "path": self.path,
            "count": self.attempts.len(),
            "attempts": self.attempts.iter().map(DemoHistoricalAttempt::to_json).collect::<Vec<_>>(),
            "parse_error": self.parse_error,
        })
    }
}

#[derive(Debug, Clone)]
struct DemoHistoricalAttempt {
    attempt_id: String,
    recorded_at_epoch_ms: u128,
    outcome: String,
    summary: Option<String>,
    receipt_path: Option<String>,
    artifacts: Vec<String>,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
    exit_code: Option<i32>,
}

impl DemoHistoricalAttempt {
    fn from_persisted(value: PersistedDemoHistoricalAttempt) -> Self {
        Self {
            attempt_id: value.attempt_id,
            recorded_at_epoch_ms: value.recorded_at_epoch_ms,
            outcome: value.outcome,
            summary: value.summary,
            receipt_path: value.receipt_path,
            artifacts: value.artifacts,
            stdout_log_path: value.stdout_log_path,
            stderr_log_path: value.stderr_log_path,
            exit_code: value.exit_code,
        }
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "attempt_id": self.attempt_id,
            "recorded_at_epoch_ms": self.recorded_at_epoch_ms,
            "outcome": self.outcome,
            "summary": self.summary,
            "receipt_path": self.receipt_path,
            "artifact_count": self.artifacts.len(),
            "artifacts": self.artifacts,
            "stdout_log_path": self.stdout_log_path,
            "stderr_log_path": self.stderr_log_path,
            "exit_code": self.exit_code,
        })
    }
}

#[derive(Debug, Clone)]
struct DemoExecutionAttempt {
    ok: bool,
    outcome: String,
    entrypoint_kind: String,
    entrypoint_value: String,
    command: String,
    exit_code: Option<i32>,
    summary: Option<String>,
    stdout: String,
    stderr: String,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
    recorded_at_epoch_ms: u128,
}

impl DemoExecutionAttempt {
    fn to_json(&self) -> JsonValue {
        json!({
            "ok": self.ok,
            "outcome": self.outcome,
            "entrypoint": {
                "kind": self.entrypoint_kind,
                "value": self.entrypoint_value,
            },
            "command": self.command,
            "exit_code": self.exit_code,
            "summary": self.summary,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "stdout_log_path": self.stdout_log_path,
            "stderr_log_path": self.stderr_log_path,
            "recorded_at_epoch_ms": self.recorded_at_epoch_ms,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedDemoAttemptHistory {
    schema: String,
    schema_version: u8,
    demo_id: String,
    attempts: Vec<PersistedDemoHistoricalAttempt>,
}

impl PersistedDemoAttemptHistory {
    fn new(demo_id: &str) -> Self {
        Self {
            schema: "effigy.demo.attempt-history.v1".to_owned(),
            schema_version: 1,
            demo_id: demo_id.to_owned(),
            attempts: Vec::new(),
        }
    }

    fn push(&mut self, attempt: PersistedDemoHistoricalAttempt) {
        self.attempts.insert(0, attempt);
        self.attempts.truncate(DEMO_ATTEMPT_HISTORY_LIMIT);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedDemoHistoricalAttempt {
    attempt_id: String,
    recorded_at_epoch_ms: u128,
    outcome: String,
    summary: Option<String>,
    receipt_path: Option<String>,
    artifacts: Vec<String>,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
    exit_code: Option<i32>,
}

impl PersistedDemoHistoricalAttempt {
    fn from_execution(
        demo_id: &str,
        demo: &ManifestDemoConfig,
        attempt: &DemoExecutionAttempt,
        receipt_path: String,
    ) -> Self {
        Self {
            attempt_id: format!(
                "{}-{}",
                sanitize_demo_id_for_filename(demo_id),
                attempt.recorded_at_epoch_ms
            ),
            recorded_at_epoch_ms: attempt.recorded_at_epoch_ms,
            outcome: attempt.outcome.clone(),
            summary: attempt.summary.clone(),
            receipt_path: Some(receipt_path),
            artifacts: demo.artifacts.clone(),
            stdout_log_path: attempt.stdout_log_path.clone(),
            stderr_log_path: attempt.stderr_log_path.clone(),
            exit_code: attempt.exit_code,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedDemoActiveAttempt {
    schema: String,
    schema_version: u8,
    attempt_id: String,
    demo_id: String,
    phase: PersistedDemoActivePhase,
    started_at_epoch_ms: u128,
    owner_pid: u32,
    target_pid: Option<u32>,
    stoppable: bool,
    entrypoint_kind: String,
    entrypoint_value: String,
    command: String,
    #[serde(default)]
    runtime_backend_kind: Option<String>,
    #[serde(default)]
    flattened_runtime_projection: bool,
    #[serde(default)]
    terminal_transport: PersistedDemoTerminalTransport,
    #[serde(default)]
    supports_input_forwarding: bool,
    #[serde(default)]
    supports_resize: bool,
    #[serde(default)]
    nested_tui: bool,
    terminal_cols: Option<u16>,
    terminal_rows: Option<u16>,
    resize_handoff_path: Option<String>,
    stdin_input_path: Option<String>,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
enum PersistedDemoTerminalTransport {
    #[default]
    Stream,
    Pty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum PersistedDemoActivePhase {
    Running,
    StopRequested,
}

impl PersistedDemoActivePhase {
    fn rendered(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::StopRequested => "stop-requested",
        }
    }
}

struct DemoActiveAttemptGuard {
    path: PathBuf,
}

impl Drop for DemoActiveAttemptGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_demo_terminal_input, append_demo_terminal_resize, load_active_attempt,
        read_recent_output_lines, write_active_attempt_record, DemoActiveAttempt,
        DemoTerminalTransport, PersistedDemoActiveAttempt, PersistedDemoActivePhase,
        PersistedDemoAttemptHistory, PersistedDemoHistoricalAttempt,
        PersistedDemoTerminalTransport, DEMO_ATTEMPT_HISTORY_LIMIT,
    };
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn demo_attempt_history_push_keeps_newest_entries_within_limit() {
        let mut history = PersistedDemoAttemptHistory::new("demo");
        for index in 0..(DEMO_ATTEMPT_HISTORY_LIMIT + 2) {
            history.push(PersistedDemoHistoricalAttempt {
                attempt_id: format!("attempt-{index}"),
                recorded_at_epoch_ms: index as u128,
                outcome: "passed".to_owned(),
                summary: None,
                receipt_path: None,
                artifacts: Vec::new(),
                stdout_log_path: None,
                stderr_log_path: None,
                exit_code: Some(0),
            });
        }

        assert_eq!(history.attempts.len(), DEMO_ATTEMPT_HISTORY_LIMIT);
        assert_eq!(history.attempts[0].attempt_id, "attempt-11");
        assert_eq!(
            history.attempts[DEMO_ATTEMPT_HISTORY_LIMIT - 1].attempt_id,
            "attempt-2"
        );
    }

    #[test]
    fn load_active_attempt_preserves_stop_requested_record_until_owner_clears_it() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-demo-active-stop-requested-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be monotonic enough for test ids")
                .as_nanos()
        ));
        fs::create_dir_all(&repo_root).expect("create temp repo root");
        let demo_id = "demo";
        write_active_attempt_record(
            &repo_root,
            demo_id,
            &PersistedDemoActiveAttempt {
                schema: "effigy.demo.active.v1".to_owned(),
                schema_version: 1,
                attempt_id: "attempt-1".to_owned(),
                demo_id: demo_id.to_owned(),
                phase: PersistedDemoActivePhase::StopRequested,
                started_at_epoch_ms: 1,
                owner_pid: u32::MAX,
                target_pid: Some(u32::MAX),
                stoppable: true,
                entrypoint_kind: "run".to_owned(),
                entrypoint_value: "sleep 1".to_owned(),
                command: "sleep 1".to_owned(),
                runtime_backend_kind: Some("run".to_owned()),
                flattened_runtime_projection: false,
                terminal_transport: PersistedDemoTerminalTransport::Stream,
                supports_input_forwarding: false,
                supports_resize: false,
                nested_tui: false,
                terminal_cols: None,
                terminal_rows: None,
                resize_handoff_path: None,
                stdin_input_path: None,
                stdout_log_path: None,
                stderr_log_path: None,
            },
        )
        .expect("write active attempt");

        let active = load_active_attempt(&repo_root, demo_id).expect("load active attempt");
        assert!(active.active);
        assert_eq!(active.state_label(), "stop-requested");
        assert!(matches!(
            active,
            DemoActiveAttempt {
                stoppable: true,
                ..
            }
        ));
        assert!(
            repo_root.join(".effigy/demo/active/demo.json").exists(),
            "stop-requested active record should survive until the owner process clears it"
        );
        let _ = fs::remove_dir_all(&repo_root);
    }

    #[test]
    fn load_active_attempt_defaults_terminal_fields_for_legacy_records() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-demo-active-legacy-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be monotonic enough for test ids")
                .as_nanos()
        ));
        fs::create_dir_all(repo_root.join(".effigy/demo/active")).expect("create active dir");
        let path = repo_root.join(".effigy/demo/active/demo.json");
        fs::write(
            &path,
            format!(
                r#"{{
  "schema": "effigy.demo.active.v1",
  "schema_version": 1,
  "attempt_id": "attempt-legacy",
  "demo_id": "demo",
  "phase": "running",
  "started_at_epoch_ms": 1,
  "owner_pid": {},
  "target_pid": null,
  "stoppable": true,
  "entrypoint_kind": "run",
  "entrypoint_value": "sleep 1",
  "command": "sleep 1",
  "stdout_log_path": ".effigy/demo/logs/demo.stdout.log",
  "stderr_log_path": ".effigy/demo/logs/demo.stderr.log"
}}"#,
                std::process::id()
            ),
        )
        .expect("write legacy active attempt");

        let active = load_active_attempt(&repo_root, "demo").expect("load active attempt");
        assert!(active.active);
        assert_eq!(active.terminal_transport, DemoTerminalTransport::Stream);
        assert_eq!(active.runtime_backend().kind, "run");
        assert!(!active.supports_input_forwarding);
        assert!(!active.supports_resize);
        assert!(!active.nested_tui);
        let _ = fs::remove_dir_all(&repo_root);
    }

    #[test]
    fn read_recent_output_lines_keeps_last_non_empty_lines() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-demo-terminal-tail-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be monotonic enough for test ids")
                .as_nanos()
        ));
        fs::create_dir_all(repo_root.join(".effigy/demo/logs")).expect("create logs dir");
        let path = repo_root.join(".effigy/demo/logs/demo.stdout.log");
        fs::write(&path, "one\n\ntwo\nthree\nfour\n").expect("write log");

        let lines = read_recent_output_lines(&repo_root, ".effigy/demo/logs/demo.stdout.log", 2);
        assert_eq!(lines, vec!["three".to_owned(), "four".to_owned()]);

        let _ = fs::remove_dir_all(&repo_root);
    }

    #[test]
    fn append_demo_terminal_input_appends_text_to_repo_relative_handoff_file() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-demo-input-handoff-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be monotonic enough for test ids")
                .as_nanos()
        ));
        fs::create_dir_all(repo_root.join(".effigy/demo/active")).expect("create input dir");
        let rendered_path = ".effigy/demo/active/demo.stdin.log";

        append_demo_terminal_input(&repo_root, rendered_path, "status")
            .expect("append first payload");
        append_demo_terminal_input(&repo_root, rendered_path, "\n").expect("append second payload");

        let written = fs::read_to_string(repo_root.join(rendered_path)).expect("read input file");
        assert_eq!(written, "status\n");

        let _ = fs::remove_dir_all(&repo_root);
    }

    #[test]
    fn append_demo_terminal_resize_appends_jsonl_events() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-demo-resize-handoff-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be monotonic enough for test ids")
                .as_nanos()
        ));
        fs::create_dir_all(repo_root.join(".effigy/demo/active")).expect("create resize dir");
        let rendered_path = ".effigy/demo/active/demo.resize.jsonl";

        append_demo_terminal_resize(&repo_root, rendered_path, 120, 32)
            .expect("append first resize payload");

        let written = fs::read_to_string(repo_root.join(rendered_path)).expect("read resize file");
        assert!(written.contains("\"cols\":120"));
        assert!(written.contains("\"rows\":32"));

        let _ = fs::remove_dir_all(&repo_root);
    }
}
