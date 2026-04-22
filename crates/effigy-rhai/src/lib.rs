use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread, time::Duration};

use anstyle::Style;
use chrono::Utc;
use effigy_core::path_error_text::{failed_to_read_path, failed_to_write_path};
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::OutputMode;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map, Position, Scope};
#[cfg(unix)]
use signal_hook::consts::signal::{SIGINT, SIGTERM};
#[cfg(unix)]
use signal_hook::flag as signal_flag;

pub const EFFIGY_RHAI_ARGS_JSON: &str = "EFFIGY_RHAI_ARGS_JSON";
pub const EFFIGY_RHAI_TASK_NAME: &str = "EFFIGY_RHAI_TASK_NAME";
pub const EFFIGY_RHAI_REPO_ROOT: &str = "EFFIGY_RHAI_REPO_ROOT";

static RHAI_TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

type TaskRunner = Arc<dyn Fn(&Path, &str, &[String]) -> Result<String, String> + Send + Sync>;
type EffigyRunner =
    Arc<dyn Fn(&Path, &[String], bool) -> Result<String, EffigyCommandError> + Send + Sync>;
type ContainerUpRunner = Arc<dyn Fn(&Path, &str, bool) -> Result<String, String> + Send + Sync>;
type ContainerDownRunner = Arc<dyn Fn(&Path, &str) -> Result<String, String> + Send + Sync>;
type ContainerShellRunner = Arc<dyn Fn(&Path, &str, &str) -> Result<String, String> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct RhaiHostError {
    message: String,
}

impl RhaiHostError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for RhaiHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RhaiHostError {}

#[derive(Clone)]
pub struct ScriptContext {
    pub cwd: PathBuf,
    pub repo_root: PathBuf,
    pub task_name: String,
    pub stop_requested: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Clone)]
pub struct HostCallbacks {
    pub run_task: TaskRunner,
    pub run_effigy: EffigyRunner,
    pub container_up: ContainerUpRunner,
    pub container_down: ContainerDownRunner,
    pub container_shell: ContainerShellRunner,
}

#[derive(Debug, Clone)]
pub struct EffigyCommandError {
    pub message: String,
    pub rendered_output: String,
}

pub fn load_script(path: &Path, cwd: &Path) -> Result<String, RhaiHostError> {
    let resolved = resolve_script_path(cwd, path);
    std::fs::read_to_string(&resolved)
        .map_err(|error| RhaiHostError::new(failed_to_read_path(&resolved, error)))
}

pub fn load_script_args_from_env() -> Result<Vec<String>, RhaiHostError> {
    let raw = required_env(EFFIGY_RHAI_ARGS_JSON)?;
    serde_json::from_str(&raw).map_err(|error| RhaiHostError::new(error.to_string()))
}

pub fn required_env(key: &str) -> Result<String, RhaiHostError> {
    std::env::var(key).map_err(|_| {
        RhaiHostError::new(format!(
            "missing internal Rhai environment variable `{key}`"
        ))
    })
}

pub fn resolve_script_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

pub fn install_stop_requested_flag() -> Result<Arc<std::sync::atomic::AtomicBool>, RhaiHostError> {
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(unix)]
    {
        signal_flag::register(SIGTERM, Arc::clone(&flag))
            .map_err(|error| RhaiHostError::new(error.to_string()))?;
        signal_flag::register(SIGINT, Arc::clone(&flag))
            .map_err(|error| RhaiHostError::new(error.to_string()))?;
    }
    Ok(flag)
}

pub fn execute_rhai_script(
    context: &ScriptContext,
    script: &str,
    script_args: &[String],
    callbacks: &HostCallbacks,
) -> Result<(), RhaiHostError> {
    let context = Arc::new(context.clone());
    let callbacks = callbacks.clone();
    let mut engine = Engine::new();
    register_host_api(&mut engine, context.clone(), callbacks);

    let mut scope = Scope::new();
    scope.push_constant(
        "args",
        script_args
            .iter()
            .cloned()
            .map(Into::into)
            .collect::<Array>(),
    );
    scope.push_constant("cwd", context.cwd.display().to_string());
    scope.push_constant("repo_root", context.repo_root.display().to_string());
    scope.push_constant("task_name", context.task_name.clone());

    engine
        .run_with_scope(&mut scope, script)
        .map_err(|error| RhaiHostError::new(error.to_string()))
}

fn register_host_api(engine: &mut Engine, context: Arc<ScriptContext>, callbacks: HostCallbacks) {
    engine.register_fn("log", |message: ImmutableString| {
        let _ = emit_host_log(message.as_str(), false);
    });
    engine.register_fn("log_warn", |message: ImmutableString| {
        let _ = emit_host_log(message.as_str(), true);
    });

    engine.register_fn("env", |name: ImmutableString| -> String {
        std::env::var(name.as_str()).unwrap_or_default()
    });
    let stop_context = context.clone();
    engine.register_fn("stop_requested", move || -> bool {
        stop_context.stop_requested.load(Ordering::Relaxed)
    });
    engine.register_fn("now_utc", || -> String {
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
    });
    engine.register_fn("process_id", || -> i64 { i64::from(std::process::id()) });
    engine.register_fn("sleep_ms", |millis: i64| {
        if millis > 0 {
            thread::sleep(Duration::from_millis(millis as u64));
        }
    });
    engine.register_fn(
        "path_join",
        |base: ImmutableString, child: ImmutableString| -> String {
            PathBuf::from(base.as_str())
                .join(child.as_str())
                .display()
                .to_string()
        },
    );
    engine.register_fn("path_file_name", |path: ImmutableString| -> String {
        Path::new(path.as_str())
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    });

    engine.register_fn(
        "make_temp_dir",
        move |prefix: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = allocate_temp_dir(prefix.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            std::fs::create_dir_all(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
            Ok(path.display().to_string())
        },
    );
    let file_context = context.clone();
    engine.register_fn(
        "read_file",
        move |path: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
        },
    );
    let file_context = context.clone();
    engine.register_fn(
        "write_file",
        move |path: ImmutableString, contents: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            std::fs::write(&path, contents.as_str())
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    engine.register_fn(
        "append_file",
        move |path: ImmutableString, contents: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
            use std::io::Write;
            file.write_all(contents.as_bytes())
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    engine.register_fn(
        "write_lines",
        move |path: ImmutableString, lines: Array| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            let rendered = dynamic_array_to_strings(&lines)
                .map_err(|error| rhai_runtime_error(error.to_string()))?
                .join("\n");
            let output = if rendered.is_empty() {
                String::new()
            } else {
                format!("{rendered}\n")
            };
            std::fs::write(&path, output)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    engine.register_fn("path_exists", move |path: ImmutableString| -> bool {
        resolve_runtime_path(&file_context.cwd, path.as_str()).exists()
    });
    let file_context = context.clone();
    engine.register_fn(
        "list_dir",
        move |path: ImmutableString| -> Result<Array, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let mut entries = std::fs::read_dir(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?
                .map(|entry| {
                    entry
                        .map(|entry| entry.path().display().to_string())
                        .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
                })
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort();
            Ok(entries.into_iter().map(Into::into).collect())
        },
    );
    let file_context = context.clone();
    engine.register_fn("is_file", move |path: ImmutableString| -> bool {
        resolve_runtime_path(&file_context.cwd, path.as_str()).is_file()
    });
    let file_context = context.clone();
    engine.register_fn(
        "is_symlink",
        move |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::symlink_metadata(&path)
                .map(|metadata| metadata.file_type().is_symlink())
                .or_else(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        Ok(false)
                    } else {
                        Err(error)
                    }
                })
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
        },
    );
    let file_context = context.clone();
    engine.register_fn(
        "create_dir",
        move |path: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::create_dir_all(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    engine.register_fn(
        "remove_path",
        move |path: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
                        std::fs::remove_dir_all(&path)
                    } else {
                        std::fs::remove_file(&path)
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
            .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    engine.register_fn(
        "create_symlink",
        move |target: ImmutableString, link: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let target = resolve_runtime_path(&file_context.cwd, target.as_str());
            let link = resolve_runtime_path(&file_context.cwd, link.as_str());
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&target, &link)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(&link, error)))
            }
            #[cfg(not(unix))]
            {
                let _ = target;
                let _ = link;
                Err(rhai_runtime_error(
                    "Rhai symlink helpers are only supported on unix hosts".to_owned(),
                ))
            }
        },
    );

    engine.register_fn(
        "json_parse",
        |raw: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            let value: serde_json::Value = serde_json::from_str(raw.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    engine.register_fn(
        "json_stringify",
        |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
            let decoded: serde_json::Value = rhai::serde::from_dynamic(&value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            serde_json::to_string_pretty(&decoded)
                .map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    engine.register_fn(
        "toml_parse",
        |raw: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            let value: toml::Value = toml::from_str(raw.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    engine.register_fn(
        "toml_stringify",
        |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
            let decoded: toml::Value = rhai::serde::from_dynamic(&value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            toml::to_string_pretty(&decoded).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );

    let process_context = context.clone();
    engine.register_fn(
        "run_process",
        move |program: ImmutableString, args: Array| -> Result<Map, Box<EvalAltResult>> {
            let mut process = ProcessCommand::new(program.as_str());
            process.args(dynamic_array_to_strings(&args)?);
            process.current_dir(&process_context.cwd);
            with_local_node_bin_path(&mut process, &process_context.cwd);
            let output = process
                .output()
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            Ok(process_result_map(output))
        },
    );
    let process_context = context.clone();
    engine.register_fn(
        "run_process_stream",
        move |program: ImmutableString, args: Array| -> Result<Map, Box<EvalAltResult>> {
            let mut process = ProcessCommand::new(program.as_str());
            process.args(dynamic_array_to_strings(&args)?);
            process.current_dir(&process_context.cwd);
            process.stdin(Stdio::null());
            process.stdout(Stdio::inherit());
            process.stderr(Stdio::inherit());
            with_local_node_bin_path(&mut process, &process_context.cwd);
            let status = process
                .status()
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            Ok(process_status_map(status))
        },
    );

    let task_context = context.clone();
    let task_callbacks = callbacks.clone();
    engine.register_fn(
        "run_task",
        move |task: ImmutableString, args: Array| -> Result<String, Box<EvalAltResult>> {
            (task_callbacks.run_task)(
                &task_context.cwd,
                task.as_str(),
                &dynamic_array_to_strings(&args)?,
            )
            .map_err(rhai_runtime_error)
        },
    );

    let effigy_context = context.clone();
    let effigy_callbacks = callbacks.clone();
    engine.register_fn(
        "run_effigy",
        move |args: Array| -> Result<Map, Box<EvalAltResult>> {
            let args = dynamic_array_to_strings(&args)?;
            Ok(effigy_result_map((effigy_callbacks.run_effigy)(
                &effigy_context.repo_root,
                &args,
                false,
            )))
        },
    );
    let effigy_json_context = context.clone();
    let effigy_json_callbacks = callbacks.clone();
    engine.register_fn(
        "run_effigy_json",
        move |args: Array| -> Result<Dynamic, Box<EvalAltResult>> {
            let args = dynamic_array_to_strings(&args)?;
            let output =
                (effigy_json_callbacks.run_effigy)(&effigy_json_context.repo_root, &args, true)
                    .map_err(|error| rhai_runtime_error(error.message))?;
            let value: serde_json::Value = serde_json::from_str(&output)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );

    let container_context = context.clone();
    let container_callbacks = callbacks.clone();
    engine.register_fn(
        "container_up",
        move |name: ImmutableString, detach: bool| -> Result<String, Box<EvalAltResult>> {
            (container_callbacks.container_up)(&container_context.repo_root, name.as_str(), detach)
                .map_err(rhai_runtime_error)
        },
    );
    let container_context = context.clone();
    let container_callbacks = callbacks.clone();
    engine.register_fn(
        "container_down",
        move |name: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            (container_callbacks.container_down)(&container_context.repo_root, name.as_str())
                .map_err(rhai_runtime_error)
        },
    );
    let container_context = context;
    engine.register_fn(
        "container_shell",
        move |name: ImmutableString,
              command: ImmutableString|
              -> Result<String, Box<EvalAltResult>> {
            (callbacks.container_shell)(
                &container_context.repo_root,
                name.as_str(),
                command.as_str(),
            )
            .map_err(rhai_runtime_error)
        },
    );
}

fn emit_host_log(message: &str, stderr: bool) -> std::io::Result<()> {
    use std::io::{self, IsTerminal, Write};

    let color_enabled = if stderr {
        resolve_color_enabled(OutputMode::from_env(), io::stderr().is_terminal())
    } else {
        resolve_color_enabled(OutputMode::from_env(), io::stdout().is_terminal())
    };
    let rendered = render_host_log_message(message, color_enabled);
    if stderr {
        let mut handle = io::stderr().lock();
        handle.write_all(rendered.as_bytes())?;
        if !message.ends_with('\n') {
            handle.write_all(b"\n")?;
        }
        handle.flush()
    } else {
        let mut handle = io::stdout().lock();
        handle.write_all(rendered.as_bytes())?;
        if !message.ends_with('\n') {
            handle.write_all(b"\n")?;
        }
        handle.flush()
    }
}

fn render_host_log_message(message: &str, color_enabled: bool) -> String {
    message
        .split_inclusive('\n')
        .map(|line| render_host_log_line(line, color_enabled))
        .collect()
}

fn render_host_log_line(line: &str, color_enabled: bool) -> String {
    const STATUS_PREFIXES: [(&str, fn(&Theme) -> Style); 8] = [
        ("[ok]", |theme| theme.success),
        ("[check]", |theme| theme.warning),
        ("[error]", |theme| theme.error),
        ("[warning]", |theme| theme.warning),
        ("[warn]", |theme| theme.warning),
        ("[info]", |theme| theme.label),
        ("[next]", |theme| theme.accent),
        ("[note]", |theme| theme.accent_soft),
    ];

    for (prefix, style) in STATUS_PREFIXES {
        if let Some(rest) = line.strip_prefix(prefix) {
            return format!(
                "{}{}",
                style_prefix(prefix, color_enabled, style(&Theme::default())),
                rest
            );
        }
    }
    line.to_owned()
}

fn style_prefix(prefix: &str, color_enabled: bool, style: Style) -> String {
    if !color_enabled {
        return prefix.to_owned();
    }
    format!("{}{}{}", style.render(), prefix, style.render_reset())
}

fn process_result_map(output: std::process::Output) -> Map {
    let mut map = Map::new();
    map.insert(
        "status".into(),
        Dynamic::from_int(output.status.code().unwrap_or(-1).into()),
    );
    map.insert(
        "success".into(),
        Dynamic::from_bool(output.status.success()),
    );
    map.insert(
        "stdout".into(),
        String::from_utf8_lossy(&output.stdout).to_string().into(),
    );
    map.insert(
        "stderr".into(),
        String::from_utf8_lossy(&output.stderr).to_string().into(),
    );
    map
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

fn dynamic_array_to_strings(args: &Array) -> Result<Vec<String>, Box<EvalAltResult>> {
    args.iter()
        .map(|value| {
            if value.is_string() {
                Ok(value.clone_cast::<String>())
            } else {
                Ok(value.to_string())
            }
        })
        .collect()
}

fn effigy_result_map(result: Result<String, EffigyCommandError>) -> Map {
    let mut map = Map::new();
    match result {
        Ok(output) => {
            map.insert("status".into(), Dynamic::from_int(0));
            map.insert("success".into(), Dynamic::from_bool(true));
            map.insert("output".into(), output.into());
            map.insert("error".into(), "".into());
            map.insert("rendered_output".into(), "".into());
        }
        Err(error) => {
            map.insert("status".into(), Dynamic::from_int(1));
            map.insert("success".into(), Dynamic::from_bool(false));
            map.insert("output".into(), String::new().into());
            map.insert("error".into(), error.message.into());
            map.insert("rendered_output".into(), error.rendered_output.into());
        }
    }
    map
}

fn resolve_runtime_path(cwd: &Path, raw: &str) -> PathBuf {
    let path = Path::new(raw);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn allocate_temp_dir(prefix: &str) -> Result<PathBuf, RhaiHostError> {
    let sanitized = if prefix.is_empty() {
        "effigy-rhai"
    } else {
        prefix
    };
    let base = std::env::temp_dir();
    let pid = std::process::id();
    for _ in 0..256 {
        let nonce = RHAI_TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| RhaiHostError::new(error.to_string()))?
            .as_millis();
        let candidate = base.join(format!("{sanitized}-{pid}-{millis}-{nonce}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(RhaiHostError::new(format!(
        "failed to allocate unique temp dir for {sanitized}"
    )))
}

fn with_local_node_bin_path(process: &mut ProcessCommand, cwd: &Path) {
    let local_bin = cwd.join("node_modules/.bin");
    if !local_bin.is_dir() {
        return;
    }
    let local_rendered = local_bin.display().to_string();
    let merged = match std::env::var("PATH") {
        Ok(path) if !path.is_empty() => format!("{local_rendered}:{path}"),
        _ => local_rendered,
    };
    process.env("PATH", merged);
}

fn rhai_runtime_error(message: impl Into<String>) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(message.into().into(), Position::NONE).into()
}

#[cfg(test)]
mod tests;
