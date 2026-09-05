use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde_json::{json, Map, Value};

use crate::GateResult;

pub(crate) const RELEASE_GATE_REPORTS_DIR: &str = ".effigy/reports/release/gates";
pub(crate) const ENVIRONMENT_FILE_NAME: &str = "environment.json";
pub(crate) const FAILED_GATE_TAIL_LINES: usize = 20;
const REDACTED_VALUE: &str = "<redacted>";

pub(crate) fn persist_gate_run_environment(root: &Path) -> Option<PathBuf> {
    write_gate_environment(root).ok()
}

pub(crate) fn persist_gate_result_log(
    root: &Path,
    result: &GateResult,
    started_at: &str,
) -> Option<PathBuf> {
    write_gate_log(root, result, started_at).ok()
}

pub(crate) fn capture_started_at() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub(crate) fn resolved_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned())
}

pub(crate) fn env_key_requires_redaction(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.contains("TOKEN")
        || upper.contains("SECRET")
        || upper.contains("KEY")
        || upper.contains("PASSWORD")
        || upper.contains("CREDENTIAL")
}

pub(crate) fn should_capture_env_key(key: &str) -> bool {
    key == "PATH"
        || key == "HOME"
        || key == "RUSTFLAGS"
        || key.starts_with("CARGO_")
        || key.starts_with("RUSTUP_")
}

pub(crate) fn redacted_environment_record(
    shell: &str,
    cwd: &Path,
    vars: impl IntoIterator<Item = (String, String)>,
) -> Value {
    let mut captured = BTreeMap::new();
    for (key, value) in vars {
        if !should_capture_env_key(&key) {
            continue;
        }
        let recorded = if env_key_requires_redaction(&key) {
            REDACTED_VALUE.to_owned()
        } else {
            value
        };
        captured.insert(key, recorded);
    }

    let mut map = Map::new();
    map.insert("shell".to_owned(), json!(shell));
    map.insert("cwd".to_owned(), json!(cwd.display().to_string()));
    for (key, value) in captured {
        map.insert(key, json!(value));
    }
    Value::Object(map)
}

pub(crate) fn failed_gate_tail_lines(gate: &GateResult) -> Vec<String> {
    let mut chunks = Vec::new();
    if !gate.stdout.is_empty() {
        chunks.push(gate.stdout.as_str());
    }
    if !gate.stderr.is_empty() {
        chunks.push(gate.stderr.as_str());
    }
    let combined = chunks.join("\n");
    let mut lines: Vec<String> = combined.lines().map(ToOwned::to_owned).collect();
    if lines.is_empty() {
        if let Some(error) = &gate.launch_error {
            lines.push(error.clone());
        }
    }
    let skip = lines.len().saturating_sub(FAILED_GATE_TAIL_LINES);
    lines.into_iter().skip(skip).collect()
}

fn reports_dir(root: &Path) -> PathBuf {
    root.join(RELEASE_GATE_REPORTS_DIR)
}

fn write_gate_environment(root: &Path) -> Result<PathBuf, std::io::Error> {
    let dir = reports_dir(root);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(ENVIRONMENT_FILE_NAME);
    let record = redacted_environment_record(&resolved_shell(), root, std::env::vars());
    let rendered = serde_json::to_string_pretty(&record).unwrap_or_else(|_| "{}".to_owned());
    std::fs::write(&path, format!("{rendered}\n"))?;
    Ok(path)
}

fn write_gate_log(
    root: &Path,
    result: &GateResult,
    started_at: &str,
) -> Result<PathBuf, std::io::Error> {
    let dir = reports_dir(root);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.log", sanitize_gate_file_stem(&result.name)));
    let exit_code = result
        .exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "none".to_owned());
    let launch_error = result.launch_error.as_deref().unwrap_or("none");
    let contents = format!(
        "command: {}\ncwd: {}\nstarted-at: {}\nexit_code: {exit_code}\nduration_ms: {}\nlaunch_error: {launch_error}\n\nstdout:\n{}\n\nstderr:\n{}\n",
        result.command,
        root.display(),
        started_at,
        result.duration_ms,
        result.stdout,
        result.stderr
    );
    std::fs::write(&path, contents)?;
    Ok(path)
}

fn sanitize_gate_file_stem(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | '\0' => '_',
            _ => ch,
        })
        .collect();
    if sanitized.is_empty() {
        "unnamed".to_owned()
    } else {
        sanitized
    }
}
