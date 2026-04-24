use std::collections::BTreeMap;
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
use effigy_demo::browser::build_error_payload as build_demo_error_payload;
use effigy_demo::runtime::{DemoActiveAttempt, DemoConcurrentRuntimeState, DemoRuntimeBackend};
#[cfg(test)]
use effigy_demo::wrap_pty_shell_command;
use effigy_demo::{
    active_attempt_is_stop_requested, append_demo_terminal_input, build_attempt_id,
    clear_active_attempt_state, clear_resize_handoff, concurrent_runner_input_target_process,
    concurrent_runner_projected_output_provenance, concurrent_runner_projection_shape,
    concurrent_runner_runtime_backend, current_terminal_size, demo_mode_prefers_attached_terminal,
    demo_run_preview as crate_demo_run_preview, display_repo_path, failed_demo_attempt,
    initial_terminal_size_for_launch_mode, load_active_attempt as load_demo_active_attempt,
    parse_task_backed_attempt_json as demo_parse_task_backed_attempt_json,
    prepare_demo_input_handoff, prepare_demo_resize_handoff, read_active_attempt_record,
    register_active_attempt, render_active_attempt_path, render_non_zero_exits,
    resolve_demo_launch_mode, run_attempt_from_output as demo_run_attempt_from_output,
    sanitize_pty_transcript, spawn_input_handoff_forward, spawn_output_capture,
    spawn_stdin_forward, spawn_stdin_handoff_capture, stop_input_handoff_forward,
    successful_demo_attempt, task_is_concurrent_runner_backed, terminated_demo_attempt,
    update_active_terminal_resize, write_active_attempt_record,
    write_latest_attempt_receipt as persist_latest_demo_attempt_receipt, DemoEntrypoint,
    DemoExecutionAttempt, DemoInvocationKind, DemoLaunchMode, DemoLogPaths, DemoRecord,
    DemoRecordGroupBy, OutputMirror, PersistedDemoActiveAttempt, PersistedDemoActivePhase,
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
use serde_json::{json, Value as JsonValue};

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::execute::api::run_manifest_task_with_cwd;
use crate::runner::manifest::{
    load_task_manifest_with_inspection, LoadedTaskManifest, ManifestDemoConfig, ManifestDemoMode,
    ManifestManagedRun, ManifestTask,
};
use crate::tui::run_demo_browser_tui;
use effigy_cli::{
    DemoArgs, DemoHistoryOutcome, DemoListGroupBy, DemoListQuery, DemoSubcommand, TaskInvocation,
};
use effigy_core::shell::with_local_node_bin_path;
use effigy_managed::command::resolve_managed_task_plan;
use effigy_managed::run_spec::{render_task_run_spec, RunSpecContext};
use effigy_manifest::{LoadedCatalog, TaskSelection};
use effigy_process::{ProcessEventKind, ProcessSpec, ProcessSupervisor};
use effigy_routing::select_catalog_and_task;
use effigy_tasks::parse_task_selector;
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};
use effigy_ui::encode_json;

use super::error::RunnerError;
pub(in crate::runner) use entry::{demo_error, run_demo};
#[cfg(test)]
use execute::load_active_attempt;
use render::{
    render_demo_execute, render_demo_history, render_demo_input, render_demo_inspect,
    render_demo_list, render_demo_resize, render_demo_stop,
};

mod entry;
mod execute;
mod query;
mod render;

#[cfg(test)]
mod tests;
