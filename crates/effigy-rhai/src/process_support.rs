use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use rhai::{Dynamic, EvalAltResult, Map, Position};
use serde_json::Value;

use crate::{HostCommandOutput, ProcessExecutionOptions};

pub(crate) fn process_result_map(output: std::process::Output) -> Map {
    process_status_and_streams_map(
        output.status.code().unwrap_or(-1).into(),
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn process_status_and_streams_map(
    status: i64,
    success: bool,
    stdout: String,
    stderr: String,
) -> Map {
    let mut map = Map::new();
    map.insert("status".into(), Dynamic::from_int(status));
    map.insert("success".into(), Dynamic::from_bool(success));
    map.insert(
        "stdout".into(),
        crate::redact_active_rhai_secrets(&stdout).into(),
    );
    map.insert(
        "stderr".into(),
        crate::redact_active_rhai_secrets(&stderr).into(),
    );
    map
}

pub(crate) fn reject_recursive_effigy_process(program: &str) -> Result<(), Box<EvalAltResult>> {
    if program == "effigy" || program == "effigy.exe" {
        return Err(rhai_runtime_error(
            "Rhai scripts must not call `run_process(\"effigy\", ...)`; use a typed host helper or add a new Rhai host surface",
        ));
    }
    Ok(())
}

pub(crate) fn resolve_process_execution_options(
    base_cwd: &Path,
    options: Map,
) -> Result<ProcessExecutionOptions, Box<EvalAltResult>> {
    let options = crate::map_to_json_object(options)?;
    let cwd = options
        .get("cwd")
        .map(|value| match value {
            Value::String(value) => Ok(crate::resolve_runtime_path(base_cwd, value)),
            _ => Err(rhai_runtime_error("`cwd` must be a string")),
        })
        .transpose()?
        .unwrap_or_else(|| base_cwd.to_path_buf());
    let env = options
        .get("env")
        .map(|value| match value {
            Value::Object(map) => map
                .iter()
                .map(|(key, value)| match value {
                    Value::String(value) => Ok((key.clone(), value.clone())),
                    _ => Err(rhai_runtime_error("`env` values must be strings")),
                })
                .collect::<Result<Vec<_>, _>>(),
            _ => Err(rhai_runtime_error("`env` must be a map of string values")),
        })
        .transpose()?
        .unwrap_or_default();
    let stdin_file = options
        .get("stdin_file")
        .map(|value| match value {
            Value::String(value) => Ok(Some(crate::resolve_runtime_path(&cwd, value))),
            Value::Null => Ok(None),
            _ => Err(rhai_runtime_error("`stdin_file` must be a string")),
        })
        .transpose()?
        .flatten();
    Ok(ProcessExecutionOptions {
        cwd,
        env,
        stdin_file,
    })
}

pub(crate) fn configure_process_command(
    process: &mut ProcessCommand,
    base_cwd: &Path,
    options: Option<Map>,
) -> Result<PathBuf, Box<EvalAltResult>> {
    let resolved = if let Some(options) = options {
        resolve_process_execution_options(base_cwd, options)?
    } else {
        ProcessExecutionOptions {
            cwd: base_cwd.to_path_buf(),
            env: Vec::new(),
            stdin_file: None,
        }
    };
    process.current_dir(&resolved.cwd);
    for (key, value) in &resolved.env {
        process.env(key, value);
    }
    if let Some(stdin_file) = &resolved.stdin_file {
        let file = std::fs::File::open(stdin_file)
            .map_err(|error| rhai_runtime_error(crate::failed_to_read_path(stdin_file, error)))?;
        process.stdin(Stdio::from(file));
    }
    Ok(resolved.cwd)
}

fn process_status_map(status: std::process::ExitStatus) -> Map {
    let mut map = Map::new();
    map.insert(
        "status".into(),
        Dynamic::from_int(status.code().unwrap_or(-1).into()),
    );
    map.insert("success".into(), Dynamic::from_bool(status.success()));
    map.insert("stdout".into(), String::new().into());
    map.insert("stderr".into(), String::new().into());
    map
}

pub(crate) fn host_command_output_map(output: HostCommandOutput) -> Map {
    let mut map = Map::new();
    map.insert("status".into(), Dynamic::from_int(output.status));
    map.insert("success".into(), Dynamic::from_bool(output.success));
    map.insert(
        "stdout".into(),
        crate::redact_active_rhai_secrets(&output.stdout).into(),
    );
    map.insert(
        "stderr".into(),
        crate::redact_active_rhai_secrets(&output.stderr).into(),
    );
    map
}

pub(crate) fn with_local_node_bin_path(process: &mut ProcessCommand, cwd: &Path) {
    let Some(merged) = local_node_bin_path_env(cwd) else {
        return;
    };
    process.env("PATH", merged);
}

fn local_node_bin_path_env(cwd: &Path) -> Option<String> {
    let local_bin = cwd.join("node_modules/.bin");
    if !local_bin.is_dir() {
        return None;
    }
    let local_rendered = local_bin.display().to_string();
    Some(match std::env::var("PATH") {
        Ok(path) if !path.is_empty() => format!("{local_rendered}:{path}"),
        _ => local_rendered,
    })
}

pub(crate) fn run_process_streaming(
    program: &str,
    args: &[String],
    cwd: &Path,
) -> Result<Map, Box<EvalAltResult>> {
    run_process_streaming_with_options(program, args, cwd, None)
}

pub(crate) fn run_process_streaming_with_options(
    program: &str,
    args: &[String],
    base_cwd: &Path,
    options: Option<Map>,
) -> Result<Map, Box<EvalAltResult>> {
    let resolved = if let Some(options) = options {
        resolve_process_execution_options(base_cwd, options)?
    } else {
        ProcessExecutionOptions {
            cwd: base_cwd.to_path_buf(),
            env: Vec::new(),
            stdin_file: None,
        }
    };

    if resolved.stdin_file.is_none() {
        match run_process_streaming_with_pty(program, args, &resolved) {
            Ok(result) => return Ok(result),
            Err(error) => {
                debug_assert!(
                    !error.to_string().is_empty(),
                    "pty streaming fallback should preserve the underlying error"
                );
            }
        }
    }

    let mut process = ProcessCommand::new(program);
    process.args(args);
    if resolved.stdin_file.is_none() {
        process.stdin(Stdio::null());
    }
    process.current_dir(&resolved.cwd);
    if let Some(stdin_file) = &resolved.stdin_file {
        let file = std::fs::File::open(stdin_file)
            .map_err(|error| rhai_runtime_error(crate::failed_to_read_path(stdin_file, error)))?;
        process.stdin(Stdio::from(file));
    }
    process.stdout(Stdio::inherit());
    process.stderr(Stdio::inherit());
    for (key, value) in &resolved.env {
        process.env(key, value);
    }
    with_local_node_bin_path(&mut process, &resolved.cwd);
    let status = process
        .status()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    Ok(process_status_map(status))
}

fn run_process_streaming_with_pty(
    program: &str,
    args: &[String],
    options: &ProcessExecutionOptions,
) -> Result<Map, Box<EvalAltResult>> {
    use std::io::{self, Read, Write};

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize::default())
        .map_err(|error| rhai_runtime_error(error.to_string()))?;

    let mut command = CommandBuilder::new(program);
    command.args(args);
    command.cwd(&options.cwd);
    for (key, value) in &options.env {
        command.env(key, value);
    }
    if let Some(path) = local_node_bin_path_env(&options.cwd) {
        command.env("PATH", path);
    }

    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let reader_thread = std::thread::spawn(move || -> std::io::Result<()> {
        let mut stdout = io::stdout().lock();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            stdout.write_all(&buffer[..read])?;
            stdout.flush()?;
        }
        Ok(())
    });

    let status = child
        .wait()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    reader_thread
        .join()
        .map_err(|_| rhai_runtime_error("pty reader thread panicked"))?
        .map_err(|error| rhai_runtime_error(error.to_string()))?;

    Ok(process_status_and_streams_map(
        status.exit_code().into(),
        status.success(),
        String::new(),
        String::new(),
    ))
}

pub(crate) fn run_process_teeing(
    program: &str,
    args: &[String],
    cwd: &Path,
) -> Result<Map, Box<EvalAltResult>> {
    run_process_teeing_with_options(program, args, cwd, None)
}

pub(crate) fn run_process_teeing_with_options(
    program: &str,
    args: &[String],
    base_cwd: &Path,
    options: Option<Map>,
) -> Result<Map, Box<EvalAltResult>> {
    use std::io::{Read, Write};

    let mut process = ProcessCommand::new(program);
    process.args(args);
    if options.is_none() {
        process.stdin(Stdio::null());
    }
    let resolved_cwd = configure_process_command(&mut process, base_cwd, options)?;
    process.stdout(Stdio::piped());
    process.stderr(Stdio::piped());
    with_local_node_bin_path(&mut process, &resolved_cwd);

    let mut child = process
        .spawn()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;

    let mut stdout_reader = child
        .stdout
        .take()
        .ok_or_else(|| rhai_runtime_error("failed to capture stdout pipe"))?;
    let stdout_thread = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut stdout = std::io::stdout().lock();
        let mut captured = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = stdout_reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            stdout.write_all(&buffer[..read])?;
            stdout.flush()?;
            captured.extend_from_slice(&buffer[..read]);
        }
        Ok(captured)
    });

    let mut stderr_reader = child
        .stderr
        .take()
        .ok_or_else(|| rhai_runtime_error("failed to capture stderr pipe"))?;
    let stderr_thread = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut stderr = std::io::stderr().lock();
        let mut captured = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = stderr_reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            stderr.write_all(&buffer[..read])?;
            stderr.flush()?;
            captured.extend_from_slice(&buffer[..read]);
        }
        Ok(captured)
    });

    let status = child
        .wait()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let stdout = stdout_thread
        .join()
        .map_err(|_| rhai_runtime_error("stdout tee thread panicked"))?
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| rhai_runtime_error("stderr tee thread panicked"))?
        .map_err(|error| rhai_runtime_error(error.to_string()))?;

    Ok(process_status_and_streams_map(
        status.code().unwrap_or(-1).into(),
        status.success(),
        String::from_utf8_lossy(&stdout).to_string(),
        String::from_utf8_lossy(&stderr).to_string(),
    ))
}

pub(crate) fn rhai_runtime_error(message: impl Into<String>) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(message.into().into(), Position::NONE).into()
}
