use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ChildStdin;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use effigy_manifest::ManifestDemoMode;

use crate::{DemoStateError, PersistedDemoTerminalTransport};

pub const DEMO_DEFAULT_TERMINAL_COLS: u16 = 80;
pub const DEMO_DEFAULT_TERMINAL_ROWS: u16 = 24;
const DEMO_INPUT_POLL_INTERVAL_MS: u64 = 40;
pub const DEMO_BROWSER_TERMINAL_COLS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_COLS";
pub const DEMO_BROWSER_TERMINAL_ROWS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_ROWS";

pub fn demo_mode_prefers_attached_terminal(mode: ManifestDemoMode) -> bool {
    matches!(
        mode,
        ManifestDemoMode::Interactive | ManifestDemoMode::Hybrid
    )
}

#[derive(Clone, Copy)]
pub enum DemoLaunchMode {
    DetachedJson,
    AttachedStream,
    AttachedPty,
}

impl DemoLaunchMode {
    pub fn attached_terminal(self) -> bool {
        matches!(self, Self::AttachedStream | Self::AttachedPty)
    }

    pub fn capture_output(self) -> bool {
        matches!(
            self,
            Self::DetachedJson | Self::AttachedStream | Self::AttachedPty
        )
    }

    pub fn forward_stdin(self) -> bool {
        matches!(self, Self::AttachedPty)
    }

    pub fn supports_input_forwarding(self) -> bool {
        matches!(self, Self::DetachedJson)
    }

    pub fn supports_resize(self) -> bool {
        matches!(self, Self::DetachedJson)
    }

    pub fn transport(self) -> PersistedDemoTerminalTransport {
        match self {
            Self::AttachedPty => PersistedDemoTerminalTransport::Pty,
            Self::DetachedJson | Self::AttachedStream => PersistedDemoTerminalTransport::Stream,
        }
    }
}

pub fn initial_terminal_size_for_launch_mode(launch_mode: DemoLaunchMode) -> Option<(u16, u16)> {
    match launch_mode {
        DemoLaunchMode::AttachedStream | DemoLaunchMode::AttachedPty => current_terminal_size(),
        DemoLaunchMode::DetachedJson => {
            Some((DEMO_DEFAULT_TERMINAL_COLS, DEMO_DEFAULT_TERMINAL_ROWS))
        }
    }
}

pub fn current_terminal_size() -> Option<(u16, u16)> {
    if let Some(size) = browser_terminal_size_override() {
        return Some(size);
    }
    crossterm::terminal::size().ok()
}

pub fn browser_terminal_size_override() -> Option<(u16, u16)> {
    let cols = std::env::var(DEMO_BROWSER_TERMINAL_COLS_ENV)
        .ok()?
        .parse::<u16>()
        .ok()?;
    let rows = std::env::var(DEMO_BROWSER_TERMINAL_ROWS_ENV)
        .ok()?
        .parse::<u16>()
        .ok()?;
    if cols == 0 || rows == 0 {
        return None;
    }
    Some((cols, rows))
}

pub fn resolve_demo_launch_mode(
    mode: ManifestDemoMode,
    output_json: bool,
    run_command: &str,
) -> DemoLaunchMode {
    if output_json {
        return DemoLaunchMode::DetachedJson;
    }
    if !demo_mode_prefers_attached_terminal(mode) {
        return DemoLaunchMode::DetachedJson;
    }
    if run_command_prefers_stream_transport(run_command) {
        return DemoLaunchMode::AttachedStream;
    }
    if demo_runtime_supports_pty() {
        DemoLaunchMode::AttachedPty
    } else {
        DemoLaunchMode::AttachedStream
    }
}

pub fn run_command_prefers_stream_transport(run_command: &str) -> bool {
    run_command.contains(" script run --file ") || run_command.starts_with("script run --file ")
}

#[cfg(target_os = "macos")]
pub fn demo_runtime_supports_pty() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
pub fn demo_runtime_supports_pty() -> bool {
    false
}

#[cfg(any(target_os = "macos", test))]
pub fn wrap_pty_shell_command(run_command: &str, terminal_size: Option<(u16, u16)>) -> String {
    let Some((cols, rows)) = terminal_size else {
        return run_command.to_owned();
    };
    format!("stty cols {cols} rows {rows} >/dev/null 2>&1; {run_command}")
}

#[cfg(all(not(target_os = "macos"), not(test)))]
pub fn wrap_pty_shell_command(run_command: &str, _terminal_size: Option<(u16, u16)>) -> String {
    run_command.to_owned()
}

#[derive(Clone, Copy)]
pub enum OutputMirror {
    Stdout,
    Stderr,
}

pub fn spawn_output_capture<R>(
    mut reader: R,
    log_path: Option<PathBuf>,
    mirror: Option<OutputMirror>,
) -> thread::JoinHandle<Result<String, DemoStateError>>
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
                    .map_err(|error| {
                        DemoStateError::new(format!(
                            "failed to open demo output log `{}`: {error}",
                            path.display()
                        ))
                    })?,
            ),
            None => None,
        };
        let mut buffer = [0u8; 4096];
        loop {
            let read = reader.read(&mut buffer).map_err(|error| {
                DemoStateError::new(format!("failed to read demo output: {error}"))
            })?;
            if read == 0 {
                break;
            }
            output.extend_from_slice(&buffer[..read]);
            if let Some(file) = sink.as_mut() {
                file.write_all(&buffer[..read]).map_err(|error| {
                    DemoStateError::new(format!("failed to write demo output log: {error}"))
                })?;
            }
            if let Some(mirror) = mirror {
                match mirror {
                    OutputMirror::Stdout => {
                        let mut stream = io::stdout().lock();
                        stream.write_all(&buffer[..read]).map_err(|error| {
                            DemoStateError::new(format!("failed to mirror demo stdout: {error}"))
                        })?;
                        stream.flush().map_err(|error| {
                            DemoStateError::new(format!("failed to flush demo stdout: {error}"))
                        })?;
                    }
                    OutputMirror::Stderr => {
                        let mut stream = io::stderr().lock();
                        stream.write_all(&buffer[..read]).map_err(|error| {
                            DemoStateError::new(format!("failed to mirror demo stderr: {error}"))
                        })?;
                        stream.flush().map_err(|error| {
                            DemoStateError::new(format!("failed to flush demo stderr: {error}"))
                        })?;
                    }
                }
            }
        }
        Ok(String::from_utf8_lossy(&output).to_string())
    })
}

pub fn spawn_stdin_forward(mut child_stdin: ChildStdin) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut input = io::stdin().lock();
        let _ = io::copy(&mut input, &mut child_stdin);
        let _ = child_stdin.flush();
    })
}

pub fn spawn_stdin_handoff_capture(path: PathBuf) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut input = io::stdin().lock();
        let mut buffer = [0u8; 1024];
        while let Ok(read) = input.read(&mut buffer) {
            if read == 0 {
                break;
            }
            let Ok(mut handoff) = fs::OpenOptions::new().append(true).open(&path) else {
                break;
            };
            if handoff
                .write_all(&buffer[..read])
                .and_then(|_| handoff.flush())
                .is_err()
            {
                break;
            }
        }
    })
}

pub fn sanitize_pty_transcript(output: &str) -> String {
    output
        .chars()
        .filter(|ch| matches!(ch, '\n' | '\r' | '\t') || !ch.is_control())
        .collect::<String>()
        .replace("^D", "")
}

pub struct DemoInputHandoffForward {
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<()>,
}

pub fn spawn_input_handoff_forward(
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

pub fn stop_input_handoff_forward(forward: DemoInputHandoffForward, path: Option<&Path>) {
    forward.stop.store(true, Ordering::Relaxed);
    let _ = forward.handle.join();
    if let Some(path) = path {
        let _ = fs::remove_file(path);
    }
}
