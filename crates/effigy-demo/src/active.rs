use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::records::DemoEntrypoint;
use crate::runtime::{DemoActiveAttempt, DemoTerminalTransport};
use crate::{
    display_repo_path, effective_active_attempt_path, effective_input_handoff_path,
    effective_resize_handoff_path, now_epoch_ms, DemoStateError,
};
use effigy_core::path_error_text::{
    failed_to_parse_path, failed_to_read_path, failed_to_render_path, failed_to_write_path,
};
use effigy_manifest::ManifestDemoConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedDemoActiveAttempt {
    pub schema: String,
    pub schema_version: u8,
    pub attempt_id: String,
    pub demo_id: String,
    pub phase: PersistedDemoActivePhase,
    pub started_at_epoch_ms: u128,
    pub owner_pid: u32,
    pub target_pid: Option<u32>,
    pub stoppable: bool,
    pub entrypoint_kind: String,
    pub entrypoint_value: String,
    pub command: String,
    #[serde(default)]
    pub runtime_backend_kind: Option<String>,
    #[serde(default)]
    pub flattened_runtime_projection: bool,
    #[serde(default)]
    pub browser_live_attach_supported: bool,
    #[serde(default)]
    pub projection_shape_kind: Option<String>,
    #[serde(default)]
    pub managed_process_count: Option<usize>,
    #[serde(default)]
    pub managed_process_names: Vec<String>,
    #[serde(default)]
    pub projected_output_provenance_kind: Option<String>,
    #[serde(default)]
    pub terminal_transport: PersistedDemoTerminalTransport,
    #[serde(default)]
    pub supports_input_forwarding: bool,
    #[serde(default)]
    pub supports_resize: bool,
    #[serde(default)]
    pub nested_tui: bool,
    pub terminal_cols: Option<u16>,
    pub terminal_rows: Option<u16>,
    pub resize_handoff_path: Option<String>,
    pub stdin_input_path: Option<String>,
    pub stdout_log_path: Option<String>,
    pub stderr_log_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedDemoTerminalTransport {
    #[default]
    Stream,
    Pty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedDemoActivePhase {
    Running,
    StopRequested,
}

impl PersistedDemoActivePhase {
    pub fn rendered(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::StopRequested => "stop-requested",
        }
    }
}

impl PersistedDemoActiveAttempt {
    /// Build a task-backed active attempt record (non-managed tasks).
    ///
    /// Shapes the persistence payload for a demo whose entrypoint is a task
    /// that is not concurrent-runner backed. There is no target process,
    /// no terminal transport, and no input/resize forwarding.
    pub fn new_task_backed(attempt_id: String, demo_id: &str, task_name: &str) -> Self {
        Self {
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
        }
    }

    /// Build a concurrent-runner backed active attempt record.
    ///
    /// The caller supplies the projection shape kind, provenance kind,
    /// managed process summary, input/resize handoff rendered paths, log
    /// rendered paths, and initial terminal size — all derived from the
    /// runner's task plan. This keeps runtime-shape classification in
    /// `effigy-demo` while the runner produces the plan-level inputs.
    #[allow(clippy::too_many_arguments)]
    pub fn new_concurrent_runner_backed(
        attempt_id: String,
        demo_id: &str,
        task_name: &str,
        managed_command: String,
        browser_live_attach_supported: bool,
        projection_shape_kind: String,
        managed_process_count: usize,
        managed_process_names: Vec<String>,
        projected_output_provenance_kind: String,
        input_handoff_rendered: Option<String>,
        resize_handoff_rendered: Option<String>,
        stdout_log_rendered: Option<String>,
        stderr_log_rendered: Option<String>,
        terminal_size: Option<(u16, u16)>,
    ) -> Self {
        let supports_input_forwarding = input_handoff_rendered.is_some();
        let supports_resize = resize_handoff_rendered.is_some();
        Self {
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
            command: managed_command,
            runtime_backend_kind: Some("concurrent-runner".to_owned()),
            flattened_runtime_projection: true,
            browser_live_attach_supported,
            projection_shape_kind: Some(projection_shape_kind),
            managed_process_count: Some(managed_process_count),
            managed_process_names,
            projected_output_provenance_kind: Some(projected_output_provenance_kind),
            terminal_transport: PersistedDemoTerminalTransport::Stream,
            supports_input_forwarding,
            supports_resize,
            nested_tui: false,
            terminal_cols: terminal_size.map(|(cols, _)| cols),
            terminal_rows: terminal_size.map(|(_, rows)| rows),
            resize_handoff_path: resize_handoff_rendered,
            stdin_input_path: input_handoff_rendered,
            stdout_log_path: stdout_log_rendered,
            stderr_log_path: stderr_log_rendered,
        }
    }

    /// Build a run-backed active attempt record.
    ///
    /// Shapes the persistence payload for a demo whose entrypoint is a
    /// shell run command, with a known target pid, terminal transport, and
    /// optional input/resize handoff paths.
    #[allow(clippy::too_many_arguments)]
    pub fn new_run_backed(
        attempt_id: String,
        demo_id: &str,
        entrypoint_value: String,
        run_command: String,
        target_pid: u32,
        terminal_transport: PersistedDemoTerminalTransport,
        input_handoff_rendered: Option<String>,
        resize_handoff_rendered: Option<String>,
        stdout_log_rendered: Option<String>,
        stderr_log_rendered: Option<String>,
        terminal_size: Option<(u16, u16)>,
    ) -> Self {
        let supports_input_forwarding = input_handoff_rendered.is_some();
        let supports_resize = resize_handoff_rendered.is_some();
        Self {
            schema: "effigy.demo.active.v1".to_owned(),
            schema_version: 1,
            attempt_id,
            demo_id: demo_id.to_owned(),
            phase: PersistedDemoActivePhase::Running,
            started_at_epoch_ms: now_epoch_ms(),
            owner_pid: std::process::id(),
            target_pid: Some(target_pid),
            stoppable: true,
            entrypoint_kind: "run".to_owned(),
            entrypoint_value,
            command: run_command,
            runtime_backend_kind: Some("run".to_owned()),
            flattened_runtime_projection: false,
            browser_live_attach_supported: true,
            projection_shape_kind: Some("single-terminal".to_owned()),
            managed_process_count: None,
            managed_process_names: Vec::new(),
            projected_output_provenance_kind: Some("none".to_owned()),
            terminal_transport,
            supports_input_forwarding,
            supports_resize,
            nested_tui: false,
            terminal_cols: terminal_size.map(|(cols, _)| cols),
            terminal_rows: terminal_size.map(|(_, rows)| rows),
            resize_handoff_path: resize_handoff_rendered,
            stdin_input_path: input_handoff_rendered,
            stdout_log_path: stdout_log_rendered,
            stderr_log_path: stderr_log_rendered,
        }
    }
}

/// Classification of what a demo-stop invocation should do given the current
/// persisted demo state.
///
/// The runner is responsible for the IO side-effects (signalling the
/// process, writing the updated phase record, clearing active state, or
/// producing a user-facing error response). This classifier owns the
/// decision rules so those rules do not have to live in the runner shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemoStopDecision {
    /// The demo has no active attempt to stop.
    NoActiveAttempt,
    /// The demo uses a task entrypoint that is not concurrent-runner backed,
    /// so stop is not supported.
    TaskEntrypointNotStoppable { task_name: String },
    /// The active attempt is not stoppable through the current runtime.
    NotStoppable,
    /// Stop has already been requested; the runner should re-surface the
    /// current state without transitioning.
    AlreadyRequested,
    /// Concurrent-runner demo without a target pid: transition phase to
    /// `StopRequested` and report — no process signal needed because the
    /// runtime is owner-driven.
    TransitionOnly,
    /// Demo is active but has no stoppable process handle (non
    /// concurrent-runner backend missing `target_pid`).
    NoStoppableHandle,
    /// The target pid is no longer alive — runner should clear state and
    /// report "no longer running".
    ProcessNotRunning { target_pid: u32 },
    /// Transition phase to `StopRequested` and send `SIGTERM` to this pid.
    SignalPid { target_pid: u32 },
}

/// Classify the appropriate stop decision for a demo based on its manifest
/// entrypoint, its observed active-attempt projection, and its persisted
/// active-attempt record (if any).
///
/// Task-entrypoint rejection is evaluated first because it applies even to
/// demos that have never been started. When `persisted` is `None`, the
/// decision falls through to `NoActiveAttempt`.
///
/// `is_pid_alive` is invoked only when a target pid is present on the
/// persisted record.
pub fn classify_demo_stop<F>(
    demo: &ManifestDemoConfig,
    active_attempt: &DemoActiveAttempt,
    persisted: Option<&PersistedDemoActiveAttempt>,
    is_pid_alive: F,
) -> DemoStopDecision
where
    F: FnOnce(u32) -> bool,
{
    if let DemoEntrypoint::Task(task_name) = DemoEntrypoint::from_manifest(demo) {
        if active_attempt.runtime_backend_kind != "concurrent-runner" {
            return DemoStopDecision::TaskEntrypointNotStoppable { task_name };
        }
    }
    if !active_attempt.active {
        return DemoStopDecision::NoActiveAttempt;
    }
    if !active_attempt.stoppable {
        return DemoStopDecision::NotStoppable;
    }
    let Some(persisted) = persisted else {
        return DemoStopDecision::NoActiveAttempt;
    };
    if persisted.phase == PersistedDemoActivePhase::StopRequested {
        return DemoStopDecision::AlreadyRequested;
    }
    if persisted.runtime_backend_kind.as_deref() == Some("concurrent-runner")
        && persisted.target_pid.is_none()
    {
        return DemoStopDecision::TransitionOnly;
    }
    let Some(target_pid) = persisted.target_pid else {
        return DemoStopDecision::NoStoppableHandle;
    };
    if !is_pid_alive(target_pid) {
        return DemoStopDecision::ProcessNotRunning { target_pid };
    }
    DemoStopDecision::SignalPid { target_pid }
}

pub struct DemoActiveAttemptGuard {
    path: PathBuf,
}

impl Drop for DemoActiveAttemptGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn load_active_attempt<F>(
    repo_root: &Path,
    demo_id: &str,
    is_pid_alive: F,
) -> Result<DemoActiveAttempt, DemoStateError>
where
    F: Fn(u32) -> bool,
{
    let path = effective_active_attempt_path(repo_root, demo_id);
    let rendered_path = display_repo_path(&path, repo_root);
    let Some(record) = read_active_attempt_record(repo_root, demo_id)? else {
        return Ok(DemoActiveAttempt::inactive(Some(rendered_path)));
    };

    let target_alive = record.target_pid.is_none_or(&is_pid_alive);
    let owner_alive = is_pid_alive(record.owner_pid);
    if record.phase == PersistedDemoActivePhase::StopRequested && (!owner_alive || !target_alive) {
        return Ok(demo_active_attempt_from_record(&record, rendered_path));
    }
    if !owner_alive || !target_alive {
        clear_active_attempt_state(repo_root, demo_id);
        return Ok(DemoActiveAttempt::inactive(Some(rendered_path)));
    }

    Ok(demo_active_attempt_from_record(&record, rendered_path))
}

pub fn read_active_attempt_record(
    repo_root: &Path,
    demo_id: &str,
) -> Result<Option<PersistedDemoActiveAttempt>, DemoStateError> {
    let path = effective_active_attempt_path(repo_root, demo_id);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| DemoStateError::new(failed_to_read_path(&path, error)))?;
    let parsed = serde_json::from_str::<PersistedDemoActiveAttempt>(&content)
        .map_err(|error| DemoStateError::new(failed_to_parse_path(&path, error)))?;
    Ok(Some(parsed))
}

pub fn active_attempt_is_stop_requested(repo_root: &Path, demo_id: &str) -> bool {
    read_active_attempt_record(repo_root, demo_id)
        .ok()
        .flatten()
        .is_some_and(|record| record.phase == PersistedDemoActivePhase::StopRequested)
}

pub fn register_active_attempt(
    repo_root: &Path,
    demo_id: &str,
    record: &PersistedDemoActiveAttempt,
) -> Result<DemoActiveAttemptGuard, DemoStateError> {
    write_active_attempt_record(repo_root, demo_id, record)?;
    Ok(DemoActiveAttemptGuard {
        path: effective_active_attempt_path(repo_root, demo_id),
    })
}

pub fn write_active_attempt_record(
    repo_root: &Path,
    demo_id: &str,
    record: &PersistedDemoActiveAttempt,
) -> Result<(), DemoStateError> {
    let path = effective_active_attempt_path(repo_root, demo_id);
    if let Some(parent) = path.parent() {
        effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root(repo_root).map_err(|error| {
            DemoStateError::new(failed_to_write_path(&repo_root.join(".gitignore"), error))
        })?;
        fs::create_dir_all(parent)
            .map_err(|error| DemoStateError::new(failed_to_write_path(parent, error)))?;
    }
    let rendered = serde_json::to_string_pretty(record)
        .map_err(|error| DemoStateError::new(failed_to_render_path(&path, error)))?;
    write_atomic_text_file(&path, &rendered)
}

pub fn clear_active_attempt_state(repo_root: &Path, demo_id: &str) {
    let path = effective_active_attempt_path(repo_root, demo_id);
    let _ = fs::remove_file(path);
}

pub fn update_active_terminal_resize(
    repo_root: &Path,
    demo_id: &str,
    cols: u16,
    rows: u16,
    rendered_resize_path: &str,
) -> Result<(), DemoStateError> {
    let Some(mut record) = read_active_attempt_record(repo_root, demo_id)? else {
        return Err(DemoStateError::new(format!(
            "demo `{demo_id}` no longer has an active terminal session"
        )));
    };
    record.terminal_cols = Some(cols);
    record.terminal_rows = Some(rows);
    write_active_attempt_record(repo_root, demo_id, &record)?;
    append_demo_terminal_resize(repo_root, rendered_resize_path, cols, rows)
}

pub fn append_demo_terminal_input(
    repo_root: &Path,
    rendered_path: &str,
    forwarded_text: &str,
) -> Result<(), DemoStateError> {
    let absolute = resolve_repo_relative_path(repo_root, rendered_path);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| DemoStateError::new(failed_to_write_path(parent, error)))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&absolute)
        .map_err(|error| DemoStateError::new(failed_to_write_path(&absolute, error)))?;
    file.write_all(forwarded_text.as_bytes())
        .and_then(|_| file.flush())
        .map_err(|error| DemoStateError::new(failed_to_write_path(&absolute, error)))
}

pub fn append_demo_terminal_resize(
    repo_root: &Path,
    rendered_path: &str,
    cols: u16,
    rows: u16,
) -> Result<(), DemoStateError> {
    let absolute = resolve_repo_relative_path(repo_root, rendered_path);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| DemoStateError::new(failed_to_write_path(parent, error)))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&absolute)
        .map_err(|error| DemoStateError::new(failed_to_write_path(&absolute, error)))?;
    let rendered = serde_json::to_string(&json!({
        "cols": cols,
        "rows": rows,
        "recorded_at_epoch_ms": now_epoch_ms(),
    }))
    .map_err(|error| DemoStateError::new(failed_to_render_path(&absolute, error)))?;
    file.write_all(rendered.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.flush())
        .map_err(|error| DemoStateError::new(failed_to_write_path(&absolute, error)))
}

pub fn prepare_demo_input_handoff(
    repo_root: &Path,
    demo_id: &str,
) -> Result<PathBuf, DemoStateError> {
    let path = effective_input_handoff_path(repo_root, demo_id);
    prepare_handoff_file(&path)?;
    Ok(path)
}

pub fn prepare_demo_resize_handoff(
    repo_root: &Path,
    demo_id: &str,
) -> Result<PathBuf, DemoStateError> {
    let path = effective_resize_handoff_path(repo_root, demo_id);
    prepare_handoff_file(&path)?;
    Ok(path)
}

pub fn clear_resize_handoff(path: Option<&Path>) {
    if let Some(path) = path {
        let _ = fs::remove_file(path);
    }
}

pub fn read_recent_output_lines(
    repo_root: &Path,
    rendered_path: &str,
    limit: usize,
) -> Vec<String> {
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

pub fn resolve_repo_relative_path(repo_root: &Path, path: &str) -> PathBuf {
    let rendered = Path::new(path);
    if rendered.is_absolute() {
        rendered.to_path_buf()
    } else {
        repo_root.join(rendered)
    }
}

fn prepare_handoff_file(path: &Path) -> Result<(), DemoStateError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| DemoStateError::new(failed_to_write_path(parent, error)))?;
    }
    fs::write(path, "").map_err(|error| DemoStateError::new(failed_to_write_path(path, error)))
}

fn write_atomic_text_file(path: &Path, contents: &str) -> Result<(), DemoStateError> {
    let Some(parent) = path.parent() else {
        return fs::write(path, contents)
            .map_err(|error| DemoStateError::new(failed_to_write_path(path, error)));
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
        .map_err(|error| DemoStateError::new(failed_to_write_path(&temp_path, error)))?;
    fs::rename(&temp_path, path)
        .map_err(|error| DemoStateError::new(failed_to_write_path(path, error)))
}

fn demo_active_attempt_from_record(
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
        browser_live_attach_supported: infer_browser_live_attach_supported(record),
        projection_shape_kind: infer_projection_shape_kind(record).to_owned(),
        managed_process_count: infer_managed_process_count(record),
        managed_process_names: record.managed_process_names.clone(),
        projected_output_provenance_kind: infer_projected_output_provenance_kind(record).to_owned(),
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

fn infer_browser_live_attach_supported(record: &PersistedDemoActiveAttempt) -> bool {
    if record.browser_live_attach_supported {
        return true;
    }
    if record.runtime_backend_kind.as_deref() == Some("run") {
        return true;
    }
    record.runtime_backend_kind.as_deref() == Some("concurrent-runner")
        && !record.nested_tui
        && record.supports_input_forwarding
}

fn infer_projection_shape_kind(record: &PersistedDemoActiveAttempt) -> &str {
    if let Some(kind) = record.projection_shape_kind.as_deref() {
        return kind;
    }
    let runtime_backend_kind = infer_runtime_backend_kind(
        record.runtime_backend_kind.as_deref(),
        &record.entrypoint_kind,
    );
    if runtime_backend_kind == "run" {
        return "single-terminal";
    }
    if runtime_backend_kind == "concurrent-runner" {
        if infer_browser_live_attach_supported(record) {
            return "single-terminal";
        }
        return "projected-multi-process";
    }
    "none"
}

fn infer_managed_process_count(record: &PersistedDemoActiveAttempt) -> Option<usize> {
    if record.managed_process_count.is_some() {
        return record.managed_process_count;
    }
    let runtime_backend_kind = infer_runtime_backend_kind(
        record.runtime_backend_kind.as_deref(),
        &record.entrypoint_kind,
    );
    if runtime_backend_kind == "concurrent-runner" && infer_browser_live_attach_supported(record) {
        return Some(1);
    }
    None
}

fn infer_projected_output_provenance_kind(record: &PersistedDemoActiveAttempt) -> &str {
    if let Some(kind) = record.projected_output_provenance_kind.as_deref() {
        return kind;
    }
    let runtime_backend_kind = infer_runtime_backend_kind(
        record.runtime_backend_kind.as_deref(),
        &record.entrypoint_kind,
    );
    if runtime_backend_kind != "concurrent-runner" {
        return "none";
    }
    if infer_managed_process_count(record).unwrap_or(0) <= 1 {
        return "single-source";
    }
    "flattened-unlabeled"
}
