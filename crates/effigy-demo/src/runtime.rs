use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::DemoStateError;

pub const DEMO_MANAGED_EVENT_POLL_INTERVAL_MS: u64 = 100;
pub const DEMO_STREAM_DRAIN_POLLS_AFTER_EXIT: usize = 3;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoRuntimeBackend {
    pub kind: String,
    pub label: String,
    pub flattened_projection: bool,
    pub projection_shape: DemoRuntimeProjectionShape,
    pub projected_process_summary: DemoRuntimeProjectedProcessSummary,
    pub projected_output_provenance: DemoRuntimeProjectedOutputProvenance,
    pub capabilities: Vec<String>,
}

impl DemoRuntimeBackend {
    pub fn none() -> Self {
        Self {
            kind: "none".to_owned(),
            label: "none".to_owned(),
            flattened_projection: false,
            projection_shape: DemoRuntimeProjectionShape::none(),
            projected_process_summary: DemoRuntimeProjectedProcessSummary::none(),
            projected_output_provenance: DemoRuntimeProjectedOutputProvenance::none(),
            capabilities: Vec::new(),
        }
    }

    pub fn task() -> Self {
        Self {
            kind: "task".to_owned(),
            label: "task-backed".to_owned(),
            flattened_projection: false,
            projection_shape: DemoRuntimeProjectionShape::none(),
            projected_process_summary: DemoRuntimeProjectedProcessSummary::none(),
            projected_output_provenance: DemoRuntimeProjectedOutputProvenance::none(),
            capabilities: Vec::new(),
        }
    }

    pub fn run() -> Self {
        Self {
            kind: "run".to_owned(),
            label: "run-backed".to_owned(),
            flattened_projection: false,
            projection_shape: DemoRuntimeProjectionShape::single_terminal(None),
            projected_process_summary: DemoRuntimeProjectedProcessSummary::none(),
            projected_output_provenance: DemoRuntimeProjectedOutputProvenance::none(),
            capabilities: vec![
                "active-terminal-session".to_owned(),
                "browser-live-attach".to_owned(),
                "live-terminal-output".to_owned(),
                "stop".to_owned(),
            ],
        }
    }

    pub fn rendered_capabilities(&self) -> String {
        if self.capabilities.is_empty() {
            "none".to_owned()
        } else {
            self.capabilities.join(", ")
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "kind": self.kind,
            "label": self.label,
            "flattened_projection": self.flattened_projection,
            "projection_shape": self.projection_shape.to_json(),
            "projected_process_summary": self.projected_process_summary.to_json(),
            "projected_output_provenance": self.projected_output_provenance.to_json(),
            "capabilities": self.capabilities,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoRuntimeProjectionShape {
    pub kind: String,
    pub live_terminal_eligible: bool,
    pub projected_multi_process: bool,
    pub managed_process_count: Option<usize>,
}

impl DemoRuntimeProjectionShape {
    pub fn none() -> Self {
        Self {
            kind: "none".to_owned(),
            live_terminal_eligible: false,
            projected_multi_process: false,
            managed_process_count: None,
        }
    }

    pub fn single_terminal(managed_process_count: Option<usize>) -> Self {
        Self {
            kind: "single-terminal".to_owned(),
            live_terminal_eligible: true,
            projected_multi_process: false,
            managed_process_count,
        }
    }

    pub fn projected_multi_process(managed_process_count: Option<usize>) -> Self {
        Self {
            kind: "projected-multi-process".to_owned(),
            live_terminal_eligible: false,
            projected_multi_process: true,
            managed_process_count,
        }
    }

    pub fn rendered_label(&self) -> &str {
        &self.kind
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "kind": self.kind,
            "live_terminal_eligible": self.live_terminal_eligible,
            "projected_multi_process": self.projected_multi_process,
            "managed_process_count": self.managed_process_count,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoRuntimeProjectedProcessSummary {
    pub present: bool,
    pub managed_process_names: Vec<String>,
    pub merged_output_from_multiple_processes: bool,
}

impl DemoRuntimeProjectedProcessSummary {
    pub fn none() -> Self {
        Self {
            present: false,
            managed_process_names: Vec::new(),
            merged_output_from_multiple_processes: false,
        }
    }

    pub fn from_names(managed_process_names: Vec<String>) -> Self {
        Self {
            present: !managed_process_names.is_empty(),
            merged_output_from_multiple_processes: managed_process_names.len() > 1,
            managed_process_names,
        }
    }

    pub fn rendered_names(&self) -> String {
        if self.managed_process_names.is_empty() {
            "none".to_owned()
        } else {
            self.managed_process_names.join(", ")
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "present": self.present,
            "managed_process_names": self.managed_process_names,
            "merged_output_from_multiple_processes": self.merged_output_from_multiple_processes,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoRuntimeProjectedOutputProvenance {
    pub present: bool,
    pub kind: String,
    pub label: String,
    pub source_attributed: bool,
}

impl DemoRuntimeProjectedOutputProvenance {
    pub fn none() -> Self {
        Self {
            present: false,
            kind: "none".to_owned(),
            label: "none".to_owned(),
            source_attributed: false,
        }
    }

    pub fn single_source() -> Self {
        Self {
            present: true,
            kind: "single-source".to_owned(),
            label: "single-source".to_owned(),
            source_attributed: false,
        }
    }

    pub fn flattened_unlabeled() -> Self {
        Self {
            present: true,
            kind: "flattened-unlabeled".to_owned(),
            label: "flattened-unlabeled".to_owned(),
            source_attributed: false,
        }
    }

    pub fn source_attributed() -> Self {
        Self {
            present: true,
            kind: "source-attributed".to_owned(),
            label: "source-attributed".to_owned(),
            source_attributed: true,
        }
    }

    pub fn rendered_label(&self) -> &str {
        &self.label
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "present": self.present,
            "kind": self.kind,
            "label": self.label,
            "source_attributed": self.source_attributed,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DemoTerminalTransport {
    #[default]
    None,
    Stream,
    Pty,
}

impl DemoTerminalTransport {
    pub fn rendered(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Stream => "stream",
            Self::Pty => "pty",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoTerminalSize {
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

impl DemoTerminalSize {
    pub fn rendered(&self) -> Option<String> {
        Some(format!("{}x{}", self.cols?, self.rows?))
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "cols": self.cols,
            "rows": self.rows,
        })
    }
}

#[derive(Debug)]
pub struct DemoConcurrentRuntimeState {
    pub stdout: String,
    pub stderr: String,
    stdout_log: Option<fs::File>,
    stderr_log: Option<fs::File>,
    pub exit_count: usize,
    pub drained_after_exit: usize,
    pub non_zero_exits: Vec<(String, String)>,
    shutdown_on_exit_processes: BTreeSet<String>,
    input_target_process: Option<String>,
    input_handoff_path: Option<PathBuf>,
    input_forwarded_bytes: usize,
    pub stop_requested: bool,
    mirror_output: bool,
}

impl DemoConcurrentRuntimeState {
    pub fn new(
        stdout_log_path: Option<&Path>,
        stderr_log_path: Option<&Path>,
        shutdown_on_exit_processes: BTreeSet<String>,
        input_target_process: Option<String>,
        input_handoff_path: Option<PathBuf>,
        mirror_output: bool,
    ) -> Result<Self, DemoStateError> {
        Ok(Self {
            stdout: String::new(),
            stderr: String::new(),
            stdout_log: open_append_file(stdout_log_path)?,
            stderr_log: open_append_file(stderr_log_path)?,
            exit_count: 0,
            drained_after_exit: 0,
            non_zero_exits: Vec::new(),
            shutdown_on_exit_processes,
            input_target_process,
            input_handoff_path,
            input_forwarded_bytes: 0,
            stop_requested: false,
            mirror_output,
        })
    }

    pub fn record_stdout(&mut self, process: &str, payload: &str) -> Result<(), DemoStateError> {
        let rendered = format!("[{process}] {payload}\n");
        self.stdout.push_str(&rendered);
        if self.mirror_output {
            print!("{rendered}");
            std::io::stdout()
                .flush()
                .map_err(|error| DemoStateError::new(error.to_string()))?;
        }
        if let Some(file) = self.stdout_log.as_mut() {
            file.write_all(rendered.as_bytes())
                .and_then(|_| file.flush())
                .map_err(|error| DemoStateError::new(error.to_string()))?;
        }
        Ok(())
    }

    pub fn record_stderr(&mut self, process: &str, payload: &str) -> Result<(), DemoStateError> {
        let rendered = format!("[{process} stderr] {payload}\n");
        self.stderr.push_str(&rendered);
        if self.mirror_output {
            eprint!("{rendered}");
            std::io::stderr()
                .flush()
                .map_err(|error| DemoStateError::new(error.to_string()))?;
        }
        if let Some(file) = self.stderr_log.as_mut() {
            file.write_all(rendered.as_bytes())
                .and_then(|_| file.flush())
                .map_err(|error| DemoStateError::new(error.to_string()))?;
        }
        Ok(())
    }

    pub fn record_exit(&mut self, process: &str, payload: &str) -> bool {
        self.exit_count += 1;
        if payload != "exit=0" {
            self.non_zero_exits
                .push((process.to_owned(), payload.to_owned()));
        }
        self.shutdown_on_exit_processes.contains(process)
    }

    pub fn record_idle_tick(&mut self, expected: usize) {
        if self.exit_count >= expected {
            self.drained_after_exit += 1;
        }
    }

    pub fn reset_drain_after_activity(&mut self) {
        if self.exit_count > 0 {
            self.drained_after_exit = 0;
        }
    }

    pub fn pending_input_chunk(
        &mut self,
    ) -> Result<Option<(String, String, usize)>, DemoStateError> {
        let Some(path) = self.input_handoff_path.as_deref() else {
            return Ok(None);
        };
        let Some(process) = self.input_target_process.as_deref() else {
            return Ok(None);
        };
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        };
        if bytes.len() < self.input_forwarded_bytes {
            self.input_forwarded_bytes = 0;
        }
        if bytes.len() == self.input_forwarded_bytes {
            return Ok(None);
        }
        let chunk = &bytes[self.input_forwarded_bytes..];
        Ok(Some((
            process.to_owned(),
            String::from_utf8_lossy(chunk).into_owned(),
            bytes.len(),
        )))
    }

    pub fn mark_input_forwarded(&mut self, new_len: usize) {
        self.input_forwarded_bytes = new_len;
    }
}

fn open_append_file(path: Option<&Path>) -> Result<Option<fs::File>, DemoStateError> {
    let Some(path) = path else {
        return Ok(None);
    };
    Ok(Some(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| DemoStateError::new(error.to_string()))?,
    ))
}

pub fn render_non_zero_exits(processes: &[(String, String)]) -> String {
    processes
        .iter()
        .map(|(process, payload)| format!("{process} {payload}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn concurrent_runner_projection_shape(process_count: usize) -> DemoRuntimeProjectionShape {
    if process_count == 1 {
        DemoRuntimeProjectionShape::single_terminal(Some(1))
    } else {
        DemoRuntimeProjectionShape::projected_multi_process(Some(process_count))
    }
}

pub fn concurrent_runner_projected_process_summary(
    managed_process_names: Vec<String>,
) -> DemoRuntimeProjectedProcessSummary {
    DemoRuntimeProjectedProcessSummary::from_names(managed_process_names)
}

pub fn concurrent_runner_projected_output_provenance(
    process_count: usize,
) -> DemoRuntimeProjectedOutputProvenance {
    match process_count {
        1 => DemoRuntimeProjectedOutputProvenance::single_source(),
        count if count > 1 => DemoRuntimeProjectedOutputProvenance::flattened_unlabeled(),
        _ => DemoRuntimeProjectedOutputProvenance::none(),
    }
}

pub fn concurrent_runner_input_target_process(process_names: &[String]) -> Option<String> {
    if process_names.len() == 1 {
        return process_names.first().cloned();
    }
    None
}

pub fn concurrent_runner_supports_browser_live_attach(process_names: &[String]) -> bool {
    concurrent_runner_input_target_process(process_names).is_some()
}

pub fn concurrent_runner_runtime_backend(managed_process_names: Vec<String>) -> DemoRuntimeBackend {
    let process_count = managed_process_names.len();
    let mut capabilities = vec![
        "active-terminal-session".to_owned(),
        "live-terminal-output".to_owned(),
    ];
    if concurrent_runner_supports_browser_live_attach(&managed_process_names) {
        capabilities.push("browser-live-attach".to_owned());
    }
    DemoRuntimeBackend {
        kind: "concurrent-runner".to_owned(),
        label: runtime_backend_label("concurrent-runner").to_owned(),
        flattened_projection: true,
        projection_shape: if process_count > 0 {
            concurrent_runner_projection_shape(process_count)
        } else {
            DemoRuntimeProjectionShape::projected_multi_process(None)
        },
        projected_process_summary: concurrent_runner_projected_process_summary(
            managed_process_names,
        ),
        projected_output_provenance: concurrent_runner_projected_output_provenance(process_count),
        capabilities,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoTerminalInputForwarding {
    pub available: bool,
    pub reason: Option<String>,
    pub mode: String,
    pub append_newline_supported: bool,
    pub command_template: String,
}

impl DemoTerminalInputForwarding {
    pub fn unavailable(reason: String) -> Self {
        Self {
            available: false,
            reason: Some(reason),
            mode: "text".to_owned(),
            append_newline_supported: true,
            command_template: "effigy demo input <DEMO_ID> --text <TEXT> [--append-newline]"
                .to_owned(),
        }
    }

    pub fn available() -> Self {
        Self {
            available: true,
            reason: None,
            mode: "text".to_owned(),
            append_newline_supported: true,
            command_template: "effigy demo input <DEMO_ID> --text <TEXT> [--append-newline]"
                .to_owned(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "available": self.available,
            "reason": self.reason,
            "mode": self.mode,
            "append_newline_supported": self.append_newline_supported,
            "command_template": self.command_template,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoTerminalResize {
    pub available: bool,
    pub reason: Option<String>,
    pub mode: String,
    pub command_template: String,
}

impl DemoTerminalResize {
    pub fn unavailable(reason: String) -> Self {
        Self {
            available: false,
            reason: Some(reason),
            mode: "cells".to_owned(),
            command_template: "effigy demo resize <DEMO_ID> --cols <COLS> --rows <ROWS>".to_owned(),
        }
    }

    pub fn available() -> Self {
        Self {
            available: true,
            reason: None,
            mode: "cells".to_owned(),
            command_template: "effigy demo resize <DEMO_ID> --cols <COLS> --rows <ROWS>".to_owned(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "available": self.available,
            "reason": self.reason,
            "mode": self.mode,
            "command_template": self.command_template,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoTerminalRecentOutput {
    pub stdout_lines: Vec<String>,
    pub stderr_lines: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoActiveAttempt {
    pub active: bool,
    pub state: String,
    pub attempt_id: Option<String>,
    pub state_path: Option<String>,
    pub owner_pid: Option<u32>,
    pub target_pid: Option<u32>,
    pub stoppable: bool,
    pub started_at_epoch_ms: Option<u128>,
    pub entrypoint_kind: Option<String>,
    pub entrypoint_value: Option<String>,
    pub command: Option<String>,
    pub runtime_backend_kind: String,
    pub flattened_runtime_projection: bool,
    pub browser_live_attach_supported: bool,
    pub projection_shape_kind: String,
    pub managed_process_count: Option<usize>,
    pub managed_process_names: Vec<String>,
    pub projected_output_provenance_kind: String,
    pub terminal_transport: DemoTerminalTransport,
    pub supports_input_forwarding: bool,
    pub supports_resize: bool,
    pub nested_tui: bool,
    pub terminal_cols: Option<u16>,
    pub terminal_rows: Option<u16>,
    pub resize_handoff_path: Option<String>,
    pub stdin_input_path: Option<String>,
    pub stdout_log_path: Option<String>,
    pub stderr_log_path: Option<String>,
    pub parse_error: Option<String>,
}

impl DemoActiveAttempt {
    pub fn inactive(state_path: Option<String>) -> Self {
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
            browser_live_attach_supported: false,
            projection_shape_kind: "none".to_owned(),
            managed_process_count: None,
            managed_process_names: Vec::new(),
            projected_output_provenance_kind: "none".to_owned(),
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

    pub fn state_label(&self) -> &str {
        &self.state
    }

    pub fn runtime_backend(&self) -> DemoRuntimeBackend {
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
            if self.browser_live_attach_supported {
                capabilities.push("browser-live-attach".to_owned());
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
            projection_shape: match self.projection_shape_kind.as_str() {
                "single-terminal" => {
                    DemoRuntimeProjectionShape::single_terminal(self.managed_process_count)
                }
                "projected-multi-process" => {
                    DemoRuntimeProjectionShape::projected_multi_process(self.managed_process_count)
                }
                _ => DemoRuntimeProjectionShape::none(),
            },
            projected_process_summary: if self.managed_process_names.is_empty() {
                DemoRuntimeProjectedProcessSummary::none()
            } else {
                DemoRuntimeProjectedProcessSummary::from_names(self.managed_process_names.clone())
            },
            projected_output_provenance: match self.projected_output_provenance_kind.as_str() {
                "single-source" => DemoRuntimeProjectedOutputProvenance::single_source(),
                "source-attributed" => DemoRuntimeProjectedOutputProvenance::source_attributed(),
                "flattened-unlabeled" => {
                    DemoRuntimeProjectedOutputProvenance::flattened_unlabeled()
                }
                _ => DemoRuntimeProjectedOutputProvenance::none(),
            },
            capabilities,
        }
    }

    pub fn to_json(&self) -> JsonValue {
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoActiveTerminalSession {
    pub available: bool,
    pub state: String,
    pub attempt_id: Option<String>,
    pub runtime_backend: DemoRuntimeBackend,
    pub transport: String,
    pub pty: bool,
    pub supports_input_forwarding: bool,
    pub input_forwarding_reason: Option<String>,
    pub input_forwarding: DemoTerminalInputForwarding,
    pub nested_tui: bool,
    pub terminal_size: DemoTerminalSize,
    pub resize: DemoTerminalResize,
    pub resize_handoff_path: Option<String>,
    pub stdin_input_path: Option<String>,
    pub stdout_log_path: Option<String>,
    pub stderr_log_path: Option<String>,
    pub output_available: bool,
    pub recent_output: DemoTerminalRecentOutput,
}

impl DemoActiveTerminalSession {
    pub fn inactive() -> Self {
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
            resize: DemoTerminalResize::unavailable(
                "no active demo terminal session is currently available".to_owned(),
            ),
            resize_handoff_path: None,
            stdin_input_path: None,
            stdout_log_path: None,
            stderr_log_path: None,
            output_available: false,
            recent_output: DemoTerminalRecentOutput::default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
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
                "stdout_lines": self.recent_output.stdout_lines,
                "stderr_lines": self.recent_output.stderr_lines,
            },
        })
    }
}

pub fn runtime_backend_label(kind: &str) -> &'static str {
    match kind {
        "task" => "task-backed",
        "run" => "run-backed",
        "concurrent-runner" => "concurrent-runner-backed",
        "none" => "none",
        _ => "custom-runtime",
    }
}
