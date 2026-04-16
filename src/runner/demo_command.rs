use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, IsTerminal};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::Duration;

use effigy_demo::browser::{
    DemoHistoryAttemptHistoryPayload, DemoHistoryDemo, DemoHistoryPayload, DemoInspectPayload,
    DemoListPayload,
};
use effigy_demo::projection::{
    active_attempt_key_values as crate_active_attempt_key_values,
    active_terminal_session_key_values as crate_active_terminal_session_key_values,
    availability_label as crate_availability_label,
    demo_action_key_values as crate_demo_action_key_values,
    demo_table_spec as crate_demo_table_spec,
    recent_attempts_table_spec as crate_recent_attempts_table_spec,
};
use effigy_demo::runtime::{
    DemoActiveAttempt, DemoActiveTerminalSession, DemoConcurrentRuntimeState, DemoRuntimeBackend,
    DemoTerminalInputForwarding, DemoTerminalRecentOutput, DemoTerminalResize, DemoTerminalSize,
    DemoTerminalTransport,
};
use effigy_demo::{
    active_attempt_is_stop_requested as demo_active_attempt_is_stop_requested, build_attempt_id,
    build_demo_groups as build_extracted_demo_groups,
    clear_active_attempt_state as demo_clear_active_attempt_state,
    clear_resize_handoff as demo_clear_resize_handoff, concurrent_runner_input_target_process,
    concurrent_runner_projected_output_provenance, concurrent_runner_projection_shape,
    concurrent_runner_runtime_backend, current_terminal_size, demo_mode_prefers_attached_terminal,
    demo_run_preview as crate_demo_run_preview, derive_gap_class, display_repo_path,
    failed_demo_attempt as build_failed_demo_attempt,
    find_historical_attempt as find_extracted_historical_attempt,
    history_attempt_to_json as history_attempt_to_json_value,
    history_attempts_with_limit as history_attempts_with_limit_slice,
    history_attempts_with_outcome as history_attempts_with_outcome_filtered,
    initial_terminal_size_for_launch_mode, load_active_attempt as load_demo_active_attempt,
    load_attempt_history as load_demo_attempt_history,
    load_latest_attempt as load_demo_latest_attempt, now_epoch_ms,
    persist_demo_attempt_logs as persist_executed_demo_attempt_logs,
    prepare_demo_input_handoff as demo_prepare_demo_input_handoff,
    prepare_demo_resize_handoff as demo_prepare_demo_resize_handoff,
    read_active_attempt_record as demo_read_active_attempt_record,
    read_recent_output_lines as demo_read_recent_output_lines,
    register_active_attempt as demo_register_active_attempt, render_active_attempt_path,
    render_non_zero_exits, resolve_demo_launch_mode, sanitize_pty_transcript,
    spawn_input_handoff_forward, spawn_output_capture, spawn_stdin_forward,
    spawn_stdin_handoff_capture, stop_input_handoff_forward,
    successful_demo_attempt as build_successful_demo_attempt,
    terminated_demo_attempt as build_terminated_demo_attempt, wrap_pty_shell_command,
    write_active_attempt_record as demo_write_active_attempt_record,
    write_latest_attempt_receipt as persist_latest_demo_attempt_receipt, DemoActionAvailability,
    DemoAttemptHistory, DemoEntrypoint, DemoExecutionAttempt, DemoGroup, DemoHistoricalAttempt,
    DemoLatestAttempt, DemoLaunchMode, DemoLogPaths, DemoRecord, DemoRecordGroupBy, OutputMirror,
    PersistedDemoActiveAttempt, PersistedDemoActivePhase, PersistedDemoTerminalTransport,
    DEMO_DEFAULT_TERMINAL_COLS, DEMO_DEFAULT_TERMINAL_ROWS, DEMO_MANAGED_EVENT_POLL_INTERVAL_MS,
    DEMO_STREAM_DRAIN_POLLS_AFTER_EXIT,
};
#[cfg(test)]
use effigy_demo::{
    browser_terminal_size_override, DEMO_BROWSER_TERMINAL_COLS_ENV, DEMO_BROWSER_TERMINAL_ROWS_ENV,
};
#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::{setpgid, Pid};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value as JsonValue};

use crate::process_manager::{ProcessEventKind, ProcessSpec, ProcessSupervisor};
use crate::runner::catalog::select_catalog_and_task;
use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::execute::run_manifest_task_with_cwd;
use crate::runner::managed::command::resolve_managed_task_plan;
use crate::runner::managed::run_spec::{render_task_run_spec, RunSpecContext};
use crate::runner::manifest::{
    load_task_manifest_with_inspection, LoadedTaskManifest, ManifestDemoConfig, ManifestDemoMode,
    ManifestDemoStatus, ManifestManagedRun, ManifestTask,
};
use crate::runner::model::catalog::{LoadedCatalog, TaskRuntimeArgs, TaskSelection, TaskSelector};
use crate::runner::util::parse_task_selector;
use crate::runner::util::with_local_node_bin_path;
use crate::tui::run_demo_browser_tui;
use crate::ui::{KeyValue, NoticeLevel, PlainRenderer, Renderer, TableSpec};
use crate::{
    DemoArgs, DemoHistoryOutcome, DemoListGroupBy, DemoListQuery, DemoSubcommand, TaskInvocation,
};

use super::error::RunnerError;
use super::render::{encode_json, render_utf8, text_renderer};

const DEMO_ACTIVE_TERMINAL_RECENT_LINES: usize = 8;
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
        .filter(|demo| demo_matches_query(demo, query))
        .collect::<Vec<_>>();
    let groups = query
        .group_by
        .map(|group_by| build_demo_groups(&demos, group_by));

    if output_json {
        let payload = DemoListPayload {
            schema: "effigy.demo.list.v1".to_owned(),
            schema_version: 1,
            ok: true,
            repo_root: repo_root.display().to_string(),
            query: demo_list_query_to_json(query),
            group_by: query.group_by.map(|value| value.as_str().to_owned()),
            count: demos.len(),
            total_count: loaded.manifest.demos.len(),
            groups: groups
                .as_ref()
                .map(|groups| {
                    groups
                        .iter()
                        .map(|group| browser_payload_from_json(group.to_json(), "demo list group"))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
            demos: demos
                .iter()
                .map(|demo| browser_payload_from_json(demo.to_json_summary(), "demo list summary"))
                .collect::<Result<Vec<_>, _>>()?,
        };
        return encode_json(
            &browser_payload_to_json(&payload, "demo list payload")?,
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
        let payload = DemoInspectPayload {
            schema: "effigy.demo.inspect.v1".to_owned(),
            schema_version: 1,
            ok: true,
            repo_root: repo_root.display().to_string(),
            demo: browser_payload_from_json(record.to_json_detail(), "demo inspect detail")?,
        };
        return encode_json(
            &browser_payload_to_json(&payload, "demo inspect payload")?,
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
    renderer.key_values(&demo_action_key_values(&record.actions()))?;
    renderer.text("")?;

    renderer.section("Active Attempt")?;
    renderer.key_values(&active_attempt_key_values(&record.active_attempt))?;
    renderer.text("")?;

    renderer.section("Active Terminal Session")?;
    renderer.key_values(&active_terminal_session_key_values(
        &record.active_terminal_session,
    ))?;
    if !record
        .active_terminal_session
        .recent_output
        .stdout_lines
        .is_empty()
    {
        renderer.text("")?;
        renderer.bullet_list(
            "recent-stdout",
            &record.active_terminal_session.recent_output.stdout_lines,
        )?;
    }
    if !record
        .active_terminal_session
        .recent_output
        .stderr_lines
        .is_empty()
    {
        renderer.text("")?;
        renderer.bullet_list(
            "recent-stderr",
            &record.active_terminal_session.recent_output.stderr_lines,
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
        let payload = DemoHistoryPayload {
            schema: "effigy.demo.history.v1".to_owned(),
            schema_version: 1,
            ok: true,
            repo_root: repo_root.display().to_string(),
            query: json!({
                "demo_id": demo_id,
                "limit": limit,
                "outcome": outcome.map(DemoHistoryOutcome::as_str),
                "attempt_id": selected_attempt_id,
                "ordinal": selected_attempt_ordinal,
            }),
            demo: DemoHistoryDemo {
                id: record.id.clone(),
                title: record.title.clone(),
                owner: record.owner.clone(),
                entrypoint: browser_payload_from_json(
                    record.entrypoint.to_json(),
                    "demo history entrypoint",
                )?,
                defined_in: record.primary_source.clone(),
            },
            active_attempt: browser_payload_from_json(
                record.active_attempt.to_json(),
                "demo history active attempt",
            )?,
            latest_attempt: browser_payload_from_json(
                record.latest_attempt.to_json(),
                "demo history latest attempt",
            )?,
            attempt_history: DemoHistoryAttemptHistoryPayload {
                path: record.attempt_history.path.clone(),
                stored_count,
                filtered_count,
                displayed_count,
                count: displayed_count,
                limit,
                outcome: outcome.map(|value| value.as_str().to_owned()),
                parse_error: record.attempt_history.parse_error.clone(),
                attempts: displayed_attempts
                    .iter()
                    .enumerate()
                    .map(|(index, attempt)| {
                        browser_payload_from_json(
                            history_attempt_to_json(index + 1, attempt),
                            "demo history attempt",
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            selected_attempt: selected_attempt.cloned(),
        };
        return encode_json(
            &browser_payload_to_json(&payload, "demo history payload")?,
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
    renderer.key_values(&active_attempt_key_values(&record.active_attempt))?;
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

    let attempt = execute_demo_attempt(repo_root, loaded, demo_id, demo, output_json)?;
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

    if attempt.ok || attempt.outcome == "terminated" {
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
    match DemoEntrypoint::from_manifest(demo) {
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
        let active_attempt = load_active_attempt(repo_root, demo_id)?;
        return render_demo_stop_result(
            repo_root,
            loaded,
            demo_id,
            output_json,
            "stop already requested",
            active_attempt,
        );
    }

    let target_pid = persisted.target_pid;

    if persisted.runtime_backend_kind.as_deref() == Some("concurrent-runner")
        && target_pid.is_none()
    {
        persisted.phase = PersistedDemoActivePhase::StopRequested;
        write_active_attempt_record(repo_root, demo_id, &persisted)?;
        let active_attempt = load_active_attempt(repo_root, demo_id)?;
        return render_demo_stop_result(
            repo_root,
            loaded,
            demo_id,
            output_json,
            "stop requested",
            active_attempt,
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
    let active_attempt = load_active_attempt(repo_root, demo_id)?;
    render_demo_stop_result(
        repo_root,
        loaded,
        demo_id,
        output_json,
        "stop requested",
        active_attempt,
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
    crate_demo_table_spec(demos)
}

fn recent_attempts_table_spec<T>(attempts: &[T]) -> TableSpec
where
    T: Borrow<DemoHistoricalAttempt>,
{
    crate_recent_attempts_table_spec(attempts)
}

fn history_attempt_to_json(ordinal: usize, attempt: &DemoHistoricalAttempt) -> JsonValue {
    history_attempt_to_json_value(ordinal, attempt)
}

fn history_attempts_with_outcome(
    history: &DemoAttemptHistory,
    outcome: Option<DemoHistoryOutcome>,
) -> Vec<&DemoHistoricalAttempt> {
    history_attempts_with_outcome_filtered(history, outcome.map(|value| value.as_str()))
}

fn history_attempts_with_limit<'a>(
    attempts: &'a [&'a DemoHistoricalAttempt],
    limit: Option<usize>,
) -> &'a [&'a DemoHistoricalAttempt] {
    history_attempts_with_limit_slice(attempts, limit)
}

fn find_historical_attempt<'a>(
    attempts: &'a [DemoHistoricalAttempt],
    attempt_id: &str,
) -> Option<&'a DemoHistoricalAttempt> {
    find_extracted_historical_attempt(attempts, attempt_id)
}

fn build_demo_groups<'a>(demos: &'a [DemoRecord], group_by: DemoListGroupBy) -> Vec<DemoGroup<'a>> {
    build_extracted_demo_groups(demos, demo_record_group_by(group_by))
}

fn availability_label(available: bool, reason: Option<&str>) -> String {
    crate_availability_label(available, reason)
}

fn demo_action_key_values(actions: &DemoActionAvailability) -> Vec<KeyValue> {
    crate_demo_action_key_values(actions)
}

fn demo_record_group_by(group_by: DemoListGroupBy) -> DemoRecordGroupBy {
    match group_by {
        DemoListGroupBy::Owner => DemoRecordGroupBy::Owner,
        DemoListGroupBy::Tag => DemoRecordGroupBy::Tag,
        DemoListGroupBy::Mode => DemoRecordGroupBy::Mode,
        DemoListGroupBy::Cover => DemoRecordGroupBy::Cover,
        DemoListGroupBy::Status => DemoRecordGroupBy::Status,
        DemoListGroupBy::Gap => DemoRecordGroupBy::Gap,
    }
}

fn demo_matches_query(record: &DemoRecord, query: &DemoListQuery) -> bool {
    record.matches_filters(
        query.search.as_deref(),
        query.owner.as_deref(),
        query.tag.as_deref(),
        query.mode.map(|value| value.as_str()),
        query.cover.as_deref(),
        query.status.map(|value| value.as_str()),
        query.gap.map(|value| value.as_str()),
        query.stale_only,
    )
}

fn active_attempt_key_values(active_attempt: &DemoActiveAttempt) -> Vec<KeyValue> {
    crate_active_attempt_key_values(active_attempt)
}

fn active_terminal_session_key_values(session: &DemoActiveTerminalSession) -> Vec<KeyValue> {
    crate_active_terminal_session_key_values(session)
}

// active_attempt_key_values and active_terminal_session_key_values body
// moved to effigy_demo::projection; stubs above forward to crate.

fn build_demo_record(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    demo: &ManifestDemoConfig,
) -> Result<DemoRecord, RunnerError> {
    let sources = demo_sources_for_id(repo_root, loaded, demo_id);
    let entrypoint = DemoEntrypoint::from_manifest(demo);
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

// demo_entrypoint moved to DemoEntrypoint::from_manifest in effigy-demo
// demo_run_preview moved to effigy_demo::demo_run_preview

fn render_demo_run_command(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    run: &ManifestManagedRun,
) -> Result<String, RunnerError> {
    let catalogs = crate::runner::catalog::discover_catalogs_allow_missing(repo_root)?;
    let task_env = BTreeMap::new();
    render_task_run_spec(
        run,
        RunSpecContext {
            task_name: demo_id,
            task_env: &task_env,
            task_env_file: None,
            env_profiles: &loaded.manifest.env,
            args_rendered: "",
            args_raw: &[],
            repo_root,
            catalogs: &catalogs,
            task_scope_cwd: repo_root,
            runtime_env_schema_override: None,
            depth: 0,
        },
    )
}

fn load_latest_attempt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
) -> Result<DemoLatestAttempt, RunnerError> {
    load_demo_latest_attempt(repo_root, demo_id, demo)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn load_attempt_history(
    repo_root: &Path,
    demo_id: &str,
) -> Result<DemoAttemptHistory, RunnerError> {
    load_demo_attempt_history(repo_root, demo_id)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn read_active_attempt_record(
    repo_root: &Path,
    demo_id: &str,
) -> Result<Option<PersistedDemoActiveAttempt>, RunnerError> {
    demo_read_active_attempt_record(repo_root, demo_id)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn register_active_attempt(
    repo_root: &Path,
    demo_id: &str,
    record: &PersistedDemoActiveAttempt,
) -> Result<effigy_demo::DemoActiveAttemptGuard, RunnerError> {
    demo_register_active_attempt(repo_root, demo_id, record)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn write_active_attempt_record(
    repo_root: &Path,
    demo_id: &str,
    record: &PersistedDemoActiveAttempt,
) -> Result<(), RunnerError> {
    demo_write_active_attempt_record(repo_root, demo_id, record)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn clear_active_attempt_state(repo_root: &Path, demo_id: &str) {
    demo_clear_active_attempt_state(repo_root, demo_id);
}

fn prepare_demo_input_handoff(repo_root: &Path, demo_id: &str) -> Result<PathBuf, RunnerError> {
    demo_prepare_demo_input_handoff(repo_root, demo_id)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn prepare_demo_resize_handoff(repo_root: &Path, demo_id: &str) -> Result<PathBuf, RunnerError> {
    demo_prepare_demo_resize_handoff(repo_root, demo_id)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn clear_resize_handoff(path: Option<&Path>) {
    demo_clear_resize_handoff(path);
}

fn active_attempt_is_stop_requested(repo_root: &Path, demo_id: &str) -> bool {
    demo_active_attempt_is_stop_requested(repo_root, demo_id)
}

fn read_recent_output_lines(repo_root: &Path, rendered_path: &str, limit: usize) -> Vec<String> {
    demo_read_recent_output_lines(repo_root, rendered_path, limit)
}

fn load_active_attempt(repo_root: &Path, demo_id: &str) -> Result<DemoActiveAttempt, RunnerError> {
    load_demo_active_attempt(repo_root, demo_id, pid_is_alive)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn load_active_terminal_session(
    repo_root: &Path,
    active_attempt: &DemoActiveAttempt,
) -> DemoActiveTerminalSession {
    let stdout_lines = active_attempt
        .stdout_log_path
        .as_deref()
        .map(|path| read_recent_output_lines(repo_root, path, DEMO_ACTIVE_TERMINAL_RECENT_LINES))
        .unwrap_or_default();
    let stderr_lines = active_attempt
        .stderr_log_path
        .as_deref()
        .map(|path| read_recent_output_lines(repo_root, path, DEMO_ACTIVE_TERMINAL_RECENT_LINES))
        .unwrap_or_default();
    DemoActiveTerminalSession::from_active_attempt(active_attempt, stdout_lines, stderr_lines)
}

fn update_active_terminal_resize(
    repo_root: &Path,
    demo_id: &str,
    cols: u16,
    rows: u16,
    rendered_resize_path: &str,
) -> Result<(), RunnerError> {
    effigy_demo::update_active_terminal_resize(repo_root, demo_id, cols, rows, rendered_resize_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn append_demo_terminal_input(
    repo_root: &Path,
    rendered_path: &str,
    forwarded_text: &str,
) -> Result<(), RunnerError> {
    effigy_demo::append_demo_terminal_input(repo_root, rendered_path, forwarded_text)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

#[cfg(test)]
fn append_demo_terminal_resize(
    repo_root: &Path,
    rendered_path: &str,
    cols: u16,
    rows: u16,
) -> Result<(), RunnerError> {
    effigy_demo::append_demo_terminal_resize(repo_root, rendered_path, cols, rows)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn execute_demo_attempt(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    match DemoEntrypoint::from_manifest(demo) {
        DemoEntrypoint::Task(task_name) => {
            execute_task_backed_demo(repo_root, demo_id, &task_name, demo.mode, output_json)
        }
        DemoEntrypoint::Run(run_spec) => {
            let entrypoint_value = crate_demo_run_preview(&run_spec);
            let rendered_command = render_demo_run_command(repo_root, loaded, demo_id, &run_spec)?;
            execute_run_backed_demo(
                repo_root,
                demo_id,
                demo.mode,
                &entrypoint_value,
                &rendered_command,
                output_json,
            )
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
    let active_record = PersistedDemoActiveAttempt {
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
        browser_live_attach_supported: false,
        projection_shape_kind: Some("none".to_owned()),
        managed_process_count: None,
        managed_process_names: Vec::new(),
        projected_output_provenance_kind: Some("none".to_owned()),
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
    };
    let _active_guard = register_active_attempt(repo_root, demo_id, &active_record)?;

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
    task.mode.as_deref() == Some("tui")
        && (!task.concurrent.is_empty() || !task.profiles.is_empty())
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
    let managed_process_names = plan
        .processes
        .iter()
        .map(|process| process.name.clone())
        .collect::<Vec<_>>();
    let detached_interaction_projection = output_json;
    let browser_live_attach_supported =
        effigy_demo::concurrent_runner_supports_browser_live_attach(&managed_process_names);
    let input_target_process = if detached_interaction_projection || browser_live_attach_supported {
        concurrent_runner_input_target_process(&managed_process_names)
    } else {
        None
    };
    let input_handoff_path = input_target_process
        .as_ref()
        .map(|_| prepare_demo_input_handoff(repo_root, demo_id))
        .transpose()?;
    let resize_handoff_path = detached_interaction_projection
        .then(|| prepare_demo_resize_handoff(repo_root, demo_id))
        .transpose()?;
    let log_paths = DemoLogPaths::prepare_split(repo_root, demo_id)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let initial_terminal_size = if demo_mode_prefers_attached_terminal(demo_mode) {
        current_terminal_size()
    } else {
        Some((DEMO_DEFAULT_TERMINAL_COLS, DEMO_DEFAULT_TERMINAL_ROWS))
    };
    let attempt_id = build_attempt_id(demo_id);
    let active_record = PersistedDemoActiveAttempt {
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
        browser_live_attach_supported,
        projection_shape_kind: Some(
            concurrent_runner_projection_shape(managed_process_names.len())
                .kind
                .clone(),
        ),
        managed_process_count: Some(plan.processes.len()),
        managed_process_names: managed_process_names.clone(),
        projected_output_provenance_kind: Some(
            concurrent_runner_projected_output_provenance(managed_process_names.len())
                .kind
                .clone(),
        ),
        terminal_transport: PersistedDemoTerminalTransport::Stream,
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
    };
    let _active_guard = register_active_attempt(repo_root, demo_id, &active_record)?;

    run_concurrent_runner_demo_runtime(
        repo_root,
        demo_id,
        task_name,
        plan,
        log_paths,
        input_target_process,
        input_handoff_path,
        resize_handoff_path,
        output_json,
    )
}

fn run_concurrent_runner_demo_runtime(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    plan: crate::runner::model::managed::ManagedTaskPlan,
    log_paths: DemoLogPaths,
    input_target_process: Option<String>,
    input_handoff_path: Option<PathBuf>,
    resize_handoff_path: Option<PathBuf>,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let shutdown_on_exit_processes = plan
        .processes
        .iter()
        .filter(|process| process.shutdown_on_exit)
        .map(|process| process.name.clone())
        .collect();
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
    let input_forwarding_available = input_target_process.is_some();
    let mut state = DemoConcurrentRuntimeState::new(
        log_paths.stdout_absolute.as_deref(),
        log_paths.stderr_absolute.as_deref(),
        shutdown_on_exit_processes,
        input_target_process,
        input_handoff_path.clone(),
        !output_json,
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let _stdin_handoff = if !output_json {
        input_handoff_path
            .as_ref()
            .filter(|_| input_forwarding_available)
            .map(|path| spawn_stdin_handoff_capture(path.clone()))
    } else {
        None
    };

    while state.exit_count < expected
        || state.drained_after_exit < DEMO_STREAM_DRAIN_POLLS_AFTER_EXIT
    {
        if !state.stop_requested && active_attempt_is_stop_requested(repo_root, demo_id) {
            state.stop_requested = true;
            supervisor.terminate_all();
        }
        if let Some((process, rendered, new_len)) = state
            .pending_input_chunk()
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        {
            supervisor
                .send_input(&process, &rendered)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            state.mark_input_forwarded(new_len);
        }
        if let Some(event) = supervisor
            .next_event_timeout(Duration::from_millis(DEMO_MANAGED_EVENT_POLL_INTERVAL_MS))
        {
            state.reset_drain_after_activity();
            match event.kind {
                ProcessEventKind::Stdout => state
                    .record_stdout(&event.process, &event.payload)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
                ProcessEventKind::Stderr => state
                    .record_stderr(&event.process, &event.payload)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
                ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {}
                ProcessEventKind::Exit => {
                    if state.record_exit(&event.process, &event.payload) {
                        supervisor.terminate_all();
                    }
                }
            }
        } else {
            state.record_idle_tick(expected);
        }
    }

    supervisor.terminate_all();
    if let Some(path) = input_handoff_path.as_deref() {
        let _ = fs::remove_file(path);
    }
    clear_resize_handoff(resize_handoff_path.as_deref());

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

fn concurrent_runner_task_process_names(repo_root: &Path, task_name: &str) -> Option<Vec<String>> {
    let Ok(Some(resolved)) = demo_task_selection(repo_root, task_name) else {
        return None;
    };
    let Ok(selection) = resolved.selection() else {
        return None;
    };
    let runtime_args = TaskRuntimeArgs {
        repo_override: None,
        verbose_root: false,
        env_schema_override: None,
        passthrough: Vec::new(),
    };
    resolve_managed_task_plan(
        &resolved.selector,
        selection.catalog,
        selection.task,
        &runtime_args,
        &resolved.catalogs,
        &selection.catalog.catalog_root,
    )
    .ok()
    .flatten()
    .map(|plan| {
        plan.processes
            .iter()
            .map(|process| process.name.clone())
            .collect()
    })
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
    entrypoint_value: &str,
    run_command: &str,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let launch_mode = resolve_demo_launch_mode(mode, output_json, run_command);
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
        demo_log_paths_for_launch_mode(repo_root, demo_id, launch_mode)?
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
    let active_record = PersistedDemoActiveAttempt {
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
        entrypoint_value: entrypoint_value.to_owned(),
        command: run_command.to_owned(),
        runtime_backend_kind: Some("run".to_owned()),
        flattened_runtime_projection: false,
        browser_live_attach_supported: true,
        projection_shape_kind: Some("single-terminal".to_owned()),
        managed_process_count: None,
        managed_process_names: Vec::new(),
        projected_output_provenance_kind: Some("none".to_owned()),
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
    };
    let _active_guard = register_active_attempt(repo_root, demo_id, &active_record)?;

    if output_json || attached_terminal {
        let _stdin_forward = if launch_mode.forward_stdin() && io::stdin().is_terminal() {
            child.stdin.take().map(spawn_stdin_forward)
        } else {
            None
        };
        let input_forward = input_handoff_path.as_ref().and_then(|path| {
            child
                .stdin
                .take()
                .map(|stdin| spawn_input_handoff_forward(path.clone(), stdin))
        });
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
            entrypoint_value,
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
        entrypoint_value,
        run_command,
        status.code(),
        status.success(),
        stop_requested,
        String::new(),
        String::new(),
        DemoLogPaths::none(),
    ))
}

fn demo_log_paths_for_launch_mode(
    repo_root: &Path,
    demo_id: &str,
    launch_mode: DemoLaunchMode,
) -> Result<DemoLogPaths, RunnerError> {
    match launch_mode {
        DemoLaunchMode::AttachedPty => DemoLogPaths::prepare_pty(repo_root, demo_id),
        DemoLaunchMode::DetachedJson | DemoLaunchMode::AttachedStream => {
            DemoLogPaths::prepare_split(repo_root, demo_id)
        }
    }
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
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
    if let Some((cols, rows)) = current_terminal_size() {
        process
            .env("COLUMNS", cols.to_string())
            .env("LINES", rows.to_string());
    }
    with_local_node_bin_path(&mut process, repo_root);
    Ok(process)
}

#[cfg(target_os = "macos")]
fn build_run_backed_pty_process(repo_root: &Path, run_command: &str) -> ProcessCommand {
    let wrapped = wrap_pty_shell_command(run_command, current_terminal_size());
    let mut process = ProcessCommand::new("script");
    process
        .arg("-q")
        .arg("/dev/null")
        .arg("sh")
        .arg("-c")
        .arg(wrapped)
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
    entrypoint_value: &str,
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
            entrypoint_value,
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
            entrypoint_value,
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
        entrypoint_value,
        run_command,
        exit_code,
        format!("Demo `{demo_id}` failed via run entrypoint."),
        stdout,
        stderr,
        log_paths,
    )
}

fn join_output_capture(
    handle: thread::JoinHandle<Result<String, effigy_demo::DemoStateError>>,
    stream_name: &str,
    demo_id: &str,
) -> Result<String, RunnerError> {
    handle
        .join()
        .map_err(|_| {
            RunnerError::task_invocation(format!(
                "demo `{demo_id}` {stream_name} capture thread panicked"
            ))
        })?
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
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
    build_successful_demo_attempt(
        entrypoint_kind,
        entrypoint_value,
        command,
        exit_code,
        summary,
        stdout,
        stderr,
        log_paths,
    )
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
    build_failed_demo_attempt(
        entrypoint_kind,
        entrypoint_value,
        command,
        exit_code,
        summary,
        stdout,
        stderr,
        log_paths,
    )
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
    build_terminated_demo_attempt(
        entrypoint_kind,
        entrypoint_value,
        command,
        exit_code,
        summary,
        stdout,
        stderr,
        log_paths,
    )
}

fn write_latest_attempt_receipt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    attempt: &DemoExecutionAttempt,
) -> Result<(), RunnerError> {
    persist_latest_demo_attempt_receipt(repo_root, demo_id, demo, attempt)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn persist_demo_attempt_logs(
    repo_root: &Path,
    demo_id: &str,
    stdout: &str,
    stderr: &str,
) -> Result<DemoLogPaths, RunnerError> {
    persist_executed_demo_attempt_logs(repo_root, demo_id, stdout, stderr)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
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

// derive_gap_class moved to effigy_demo::records

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

fn browser_payload_from_json<T: DeserializeOwned>(
    value: JsonValue,
    context: &str,
) -> Result<T, RunnerError> {
    serde_json::from_value(value).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to build shared demo browser payload for {context}: {error}"
        ))
    })
}

fn browser_payload_to_json<T: Serialize>(
    value: &T,
    context: &str,
) -> Result<JsonValue, RunnerError> {
    serde_json::to_value(value).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render shared demo browser payload for {context}: {error}"
        ))
    })
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
                    .and_then(|selection| {
                        selection.task().ok().map(task_is_concurrent_runner_backed)
                    })
                    .unwrap_or(false)
            {
                let managed_process_names =
                    concurrent_runner_task_process_names(repo_root, task_name).unwrap_or_default();
                concurrent_runner_runtime_backend(managed_process_names)
            } else {
                match entrypoint {
                    DemoEntrypoint::Task(_) => DemoRuntimeBackend::task(),
                    DemoEntrypoint::Run(_) => DemoRuntimeBackend::run(),
                }
            }
        }
        DemoEntrypoint::Run(_) => DemoRuntimeBackend::run(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_demo_terminal_input, append_demo_terminal_resize, browser_terminal_size_override,
        load_active_attempt, read_recent_output_lines, render_demo_execute_text,
        terminated_demo_attempt, wrap_pty_shell_command, write_active_attempt_record,
        DemoActiveAttempt, DemoActiveTerminalSession, DemoEntrypoint, DemoLogPaths, DemoRecord,
        DemoRuntimeBackend, DemoTerminalTransport, PersistedDemoActiveAttempt,
        PersistedDemoActivePhase, PersistedDemoTerminalTransport, DEMO_BROWSER_TERMINAL_COLS_ENV,
        DEMO_BROWSER_TERMINAL_ROWS_ENV,
    };
    use crate::runner::manifest::{ManifestDemoMode, ManifestDemoStatus, ManifestManagedRun};
    use effigy_demo::{DemoAttemptHistory, DemoLatestAttempt};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

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
                browser_live_attach_supported: true,
                projection_shape_kind: Some("single-terminal".to_owned()),
                managed_process_count: None,
                managed_process_names: Vec::new(),
                projected_output_provenance_kind: Some("none".to_owned()),
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
        assert_eq!(
            active.runtime_backend().projection_shape.rendered_label(),
            "single-terminal"
        );
        assert!(
            active
                .runtime_backend()
                .projection_shape
                .live_terminal_eligible
        );
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

    #[test]
    fn browser_terminal_size_override_reads_valid_env_values() {
        unsafe {
            std::env::set_var(DEMO_BROWSER_TERMINAL_COLS_ENV, "96");
            std::env::set_var(DEMO_BROWSER_TERMINAL_ROWS_ENV, "28");
        }

        let size = browser_terminal_size_override();

        unsafe {
            std::env::remove_var(DEMO_BROWSER_TERMINAL_COLS_ENV);
            std::env::remove_var(DEMO_BROWSER_TERMINAL_ROWS_ENV);
        }

        assert_eq!(size, Some((96, 28)));
    }

    #[test]
    fn wrap_pty_shell_command_prefixes_stty_with_terminal_size() {
        let wrapped = wrap_pty_shell_command("printf demo", Some((96, 28)));

        assert_eq!(wrapped, "stty cols 96 rows 28 >/dev/null 2>&1; printf demo");
    }

    #[test]
    fn render_demo_execute_treats_terminated_attempt_as_non_error_text_result() {
        let record = DemoRecord {
            id: "demo".to_owned(),
            title: "Demo".to_owned(),
            summary: "summary".to_owned(),
            proof: "proof".to_owned(),
            owner: "owner".to_owned(),
            mode: ManifestDemoMode::Interactive,
            status: ManifestDemoStatus::Ready,
            covers: Vec::new(),
            tags: Vec::new(),
            prerequisites: Vec::new(),
            dependencies: Vec::new(),
            entrypoint: DemoEntrypoint::Run(ManifestManagedRun::Command("printf demo".to_owned())),
            sources: vec!["effigy.toml".to_owned()],
            primary_source: "effigy.toml".to_owned(),
            gap_class: "existing",
            runtime_backend: DemoRuntimeBackend::run(),
            active_attempt: DemoActiveAttempt::inactive(None),
            active_terminal_session: DemoActiveTerminalSession::inactive(),
            latest_attempt: DemoLatestAttempt {
                recorded: true,
                receipt_path: Some(".effigy/demo/receipts/demo.json".to_owned()),
                outcome: Some("terminated".to_owned()),
                summary: Some("terminated".to_owned()),
                stale: false,
                artifacts: Vec::new(),
                stdout_log_path: None,
                stderr_log_path: None,
                parse_error: None,
            },
            attempt_history: DemoAttemptHistory {
                path: None,
                attempts: Vec::new(),
                parse_error: None,
            },
        };
        let attempt = terminated_demo_attempt(
            "run",
            "printf demo",
            "printf demo",
            None,
            "Demo `demo` was terminated after stop was requested.".to_owned(),
            String::new(),
            String::new(),
            DemoLogPaths::none(),
        );

        let rendered =
            render_demo_execute_text(&record, &attempt, "Demo Run").expect("render terminated");

        assert!(rendered.contains("outcome: terminated"));
        assert!(!rendered.contains("[error] Task failed"));
    }
}
