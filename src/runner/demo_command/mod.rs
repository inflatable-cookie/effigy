use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, IsTerminal};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::Duration;

#[cfg(test)]
use effigy_demo::append_demo_terminal_resize;
use effigy_demo::browser::{
    build_error_payload as build_demo_error_payload,
    payload_from_json as demo_browser_payload_from_json,
    payload_to_json as demo_browser_payload_to_json, DemoHistoryAttemptHistoryPayload,
    DemoHistoryDemo, DemoHistoryPayload, DemoInspectPayload, DemoListPayload,
};
use effigy_demo::projection::{
    active_attempt_key_values, active_terminal_session_key_values, demo_action_key_values,
    demo_table_spec, recent_attempts_table_spec,
};
use effigy_demo::runtime::{DemoActiveAttempt, DemoConcurrentRuntimeState, DemoRuntimeBackend};
use effigy_demo::{
    active_attempt_is_stop_requested, append_demo_terminal_input, build_attempt_id,
    build_demo_groups as build_extracted_demo_groups, clear_active_attempt_state,
    clear_resize_handoff, concurrent_runner_input_target_process,
    concurrent_runner_projected_output_provenance, concurrent_runner_projection_shape,
    concurrent_runner_runtime_backend, current_terminal_size, demo_mode_prefers_attached_terminal,
    demo_run_preview as crate_demo_run_preview, derive_gap_class, display_repo_path,
    failed_demo_attempt, find_historical_attempt as find_extracted_historical_attempt,
    history_attempt_to_json as history_attempt_to_json_value,
    history_attempts_with_limit as history_attempts_with_limit_slice,
    history_attempts_with_outcome as history_attempts_with_outcome_filtered,
    initial_terminal_size_for_launch_mode, load_active_attempt as load_demo_active_attempt,
    load_active_terminal_session, load_attempt_history, load_latest_attempt,
    parse_task_backed_attempt_json as demo_parse_task_backed_attempt_json,
    prepare_demo_input_handoff, prepare_demo_resize_handoff, read_active_attempt_record,
    register_active_attempt, render_active_attempt_path, render_non_zero_exits,
    resolve_demo_launch_mode, run_attempt_from_output as demo_run_attempt_from_output,
    sanitize_pty_transcript, spawn_input_handoff_forward, spawn_output_capture,
    spawn_stdin_forward, spawn_stdin_handoff_capture, stop_input_handoff_forward,
    successful_demo_attempt, task_is_concurrent_runner_backed, terminated_demo_attempt,
    update_active_terminal_resize, write_active_attempt_record,
    write_latest_attempt_receipt as persist_latest_demo_attempt_receipt, DemoEntrypoint,
    DemoExecutionAttempt, DemoGroup, DemoHistoricalAttempt, DemoInvocationKind, DemoLaunchMode,
    DemoLogPaths, DemoRecord, DemoRecordGroupBy, OutputMirror, PersistedDemoActiveAttempt,
    PersistedDemoActivePhase, DEMO_DEFAULT_TERMINAL_COLS, DEMO_DEFAULT_TERMINAL_ROWS,
    DEMO_MANAGED_EVENT_POLL_INTERVAL_MS, DEMO_STREAM_DRAIN_POLLS_AFTER_EXIT,
};
#[cfg(test)]
use effigy_demo::wrap_pty_shell_command;
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

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::execute::run_manifest_task_with_cwd;
use crate::runner::manifest::{
    load_task_manifest_with_inspection, LoadedTaskManifest, ManifestDemoConfig, ManifestDemoMode,
    ManifestManagedRun, ManifestTask,
};
use crate::tui::run_demo_browser_tui;
use effigy_cli::{
    DemoArgs, DemoHistoryOutcome, DemoListGroupBy, DemoListQuery, DemoSubcommand, TaskInvocation,
};
use effigy_core::shell::with_local_node_bin_path;
use effigy_core::widgets::{KeyValue, NoticeLevel};
use effigy_managed::command::resolve_managed_task_plan;
use effigy_managed::run_spec::{render_task_run_spec, RunSpecContext};
use effigy_manifest::{LoadedCatalog, TaskSelection};
use effigy_process::{ProcessEventKind, ProcessSpec, ProcessSupervisor};
use effigy_routing::select_catalog_and_task;
use effigy_tasks::parse_task_selector;
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};
use effigy_ui::{encode_json, render_utf8, text_renderer, PlainRenderer, Renderer};

use super::error::RunnerError;
#[cfg(test)]
use execute::load_active_attempt;
#[cfg(test)]
use render::render_demo_execute_text;
use render::{
    render_demo_execute, render_demo_history, render_demo_input, render_demo_inspect,
    render_demo_list, render_demo_resize, render_demo_stop,
};

mod execute;
mod query;
mod render;

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

fn demo_error(
    output_json: bool,
    schema: &str,
    message: String,
    extra: JsonValue,
) -> Result<String, RunnerError> {
    if output_json {
        let payload = build_demo_error_payload(schema, &message, extra);
        let rendered = encode_json(&payload, true)?;
        return Err(RunnerError::CommandJsonFailure { rendered });
    }
    Err(RunnerError::task_invocation(message))
}

fn browser_payload_from_json<T: DeserializeOwned>(
    value: JsonValue,
    context: &str,
) -> Result<T, RunnerError> {
    demo_browser_payload_from_json(value, context).map_err(Into::into)
}

fn browser_payload_to_json<T: Serialize>(
    value: &T,
    context: &str,
) -> Result<JsonValue, RunnerError> {
    demo_browser_payload_to_json(value, context).map_err(Into::into)
}

#[cfg(test)]
mod tests;
