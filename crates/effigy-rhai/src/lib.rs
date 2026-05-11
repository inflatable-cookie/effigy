use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use anstyle::Style;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use effigy_context::EffigyRuntimeContext;
use effigy_core::path_error_text::failed_to_read_path;
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::OutputMode;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use rhai::module_resolvers::FileModuleResolver;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map, Position, Scope};
use ring::rand::SecureRandom;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
#[cfg(unix)]
use signal_hook::consts::signal::{SIGINT, SIGTERM};
#[cfg(unix)]
use signal_hook::flag as signal_flag;

pub const EFFIGY_RHAI_ARGS_JSON: &str = "EFFIGY_RHAI_ARGS_JSON";
pub const EFFIGY_RHAI_TASK_NAME: &str = "EFFIGY_RHAI_TASK_NAME";
pub const EFFIGY_RHAI_REPO_ROOT: &str = "EFFIGY_RHAI_REPO_ROOT";
pub const EFFIGY_RHAI_CATALOG_ROOT: &str = "EFFIGY_RHAI_CATALOG_ROOT";
pub const EFFIGY_RHAI_INVOCATION_CWD: &str = "EFFIGY_RHAI_INVOCATION_CWD";

static RHAI_TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static ACTIVE_RUNTIME_CONTEXT: RefCell<Option<EffigyRuntimeContext>> = const { RefCell::new(None) };
}

type TaskRunner = Arc<dyn Fn(&Path, &str, &[String]) -> Result<String, String> + Send + Sync>;
type EffigyRunner =
    Arc<dyn Fn(&Path, &[String], bool) -> Result<String, EffigyCommandError> + Send + Sync>;
type FeatureRunner =
    Arc<dyn Fn(&Path, &str, Value) -> Result<String, EffigyCommandError> + Send + Sync>;
type ContainerUpRunner = Arc<dyn Fn(&Path, &str, bool) -> Result<String, String> + Send + Sync>;
type ContainerDownRunner = Arc<dyn Fn(&Path, &str, bool) -> Result<String, String> + Send + Sync>;
type ContainerShellRunner =
    Arc<dyn Fn(&Path, &str, Option<&str>, &str) -> Result<String, String> + Send + Sync>;
type ContainerExecRunner = Arc<
    dyn Fn(&Path, &str, Option<&str>, &[String]) -> Result<HostCommandOutput, String> + Send + Sync,
>;
type ContainerExecWithOptionsRunner = Arc<
    dyn Fn(&Path, &str, Option<&str>, &[String], Value) -> Result<HostCommandOutput, String>
        + Send
        + Sync,
>;

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
    /// Working directory used by relative filesystem helpers.
    pub cwd: PathBuf,
    /// Repository root used by Effigy helper callbacks.
    pub repo_root: PathBuf,
    /// Logical task name exposed to the script.
    pub task_name: String,
    /// Shared cancellation flag checked by long-running script helpers.
    pub stop_requested: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Clone)]
pub struct HostCallbacks {
    pub run_task: TaskRunner,
    pub run_effigy: EffigyRunner,
    pub run_feature: FeatureRunner,
    pub container_up: ContainerUpRunner,
    pub container_down: ContainerDownRunner,
    pub container_shell: ContainerShellRunner,
    pub container_exec: ContainerExecRunner,
    pub container_exec_with_options: ContainerExecWithOptionsRunner,
}

#[derive(Debug, Clone)]
pub struct EffigyCommandError {
    pub message: String,
    pub rendered_output: String,
}

#[derive(Debug, Clone)]
pub struct HostCommandOutput {
    pub status: i64,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

struct ProcessExecutionOptions {
    cwd: PathBuf,
    env: Vec<(String, String)>,
    stdin_file: Option<PathBuf>,
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
    execute_rhai_script_with_runtime_context(context, None, script, script_args, callbacks)
}

pub fn execute_rhai_script_with_runtime_context(
    context: &ScriptContext,
    runtime_context: Option<&EffigyRuntimeContext>,
    script: &str,
    script_args: &[String],
    callbacks: &HostCallbacks,
) -> Result<(), RhaiHostError> {
    with_rhai_runtime_context(runtime_context, || {
        execute_rhai_script_inner(context, script, script_args, callbacks)
    })
}

fn execute_rhai_script_inner(
    context: &ScriptContext,
    script: &str,
    script_args: &[String],
    callbacks: &HostCallbacks,
) -> Result<(), RhaiHostError> {
    let context = Arc::new(context.clone());
    let callbacks = callbacks.clone();
    let mut engine = Engine::new();
    let catalog_root = resolve_context_path(EFFIGY_RHAI_CATALOG_ROOT, &context.cwd);
    let invocation_cwd = resolve_invocation_cwd(&context);
    engine.set_module_resolver(FileModuleResolver::new_with_path(&catalog_root));
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
    scope.push_constant("catalog_root", catalog_root.display().to_string());
    scope.push_constant("invocation_cwd", invocation_cwd.display().to_string());
    scope.push_constant("task_name", context.task_name.clone());

    engine
        .run_with_scope(&mut scope, script)
        .map_err(|error| RhaiHostError::new(error.to_string()))
}

fn resolve_context_path(key: &str, fallback: &Path) -> PathBuf {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.to_path_buf())
}

fn resolve_invocation_cwd(script_context: &ScriptContext) -> PathBuf {
    if let Some(path) = std::env::var(EFFIGY_RHAI_INVOCATION_CWD)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
    {
        return path;
    }
    if let Some(context) = ACTIVE_RUNTIME_CONTEXT.with(|active| active.borrow().clone()) {
        return context.invocation_cwd().to_path_buf();
    }
    script_context.cwd.clone()
}

fn with_rhai_runtime_context<T>(
    context: Option<&EffigyRuntimeContext>,
    run: impl FnOnce() -> T,
) -> T {
    ACTIVE_RUNTIME_CONTEXT.with(|active| {
        let previous = active.replace(context.cloned());
        let output = run();
        active.replace(previous);
        output
    })
}

pub(crate) fn active_runtime_context_for_script(
    script_context: &ScriptContext,
) -> Result<EffigyRuntimeContext, RhaiHostError> {
    if let Some(context) = ACTIVE_RUNTIME_CONTEXT.with(|active| active.borrow().clone()) {
        return Ok(context);
    }
    EffigyRuntimeContext::capture_lossy(
        Some(script_context.cwd.clone()),
        Some(script_context.repo_root.clone()),
    )
    .map_err(|error| RhaiHostError::new(error.to_string()))
}

mod host_api;
pub mod surface;
use host_api::register_host_api;

fn generate_jwt_env_keys_dynamic() -> Result<Dynamic, Box<EvalAltResult>> {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| rhai_runtime_error("failed to generate Ed25519 PKCS#8 keypair"))?;
    let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| rhai_runtime_error("failed to parse generated Ed25519 PKCS#8 keypair"))?;

    let mut map = Map::new();
    map.insert("private_key".into(), STANDARD.encode(pkcs8.as_ref()).into());
    map.insert(
        "public_key".into(),
        URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref()).into(),
    );
    Ok(Dynamic::from_map(map))
}

fn generate_random_base64(size: i64) -> Result<String, Box<EvalAltResult>> {
    if size <= 0 {
        return Err(rhai_runtime_error(
            "generate_random_base64 size must be greater than zero",
        ));
    }
    let mut bytes = vec![0_u8; size as usize];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| rhai_runtime_error("failed to generate secure random bytes"))?;
    Ok(STANDARD.encode(bytes))
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
    const STATUS_PREFIXES: [(&str, fn(&Theme) -> Style); 10] = [
        ("[ok]", |theme| theme.success),
        ("[check]", |theme| theme.warning),
        ("[error]", |theme| theme.error),
        ("[warning]", |theme| theme.warning),
        ("[warn]", |theme| theme.warning),
        ("[info]", |theme| theme.label),
        ("[gateway]", |theme| theme.label),
        ("[bootstrap]", |theme| theme.label),
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
    map.insert("stdout".into(), stdout.into());
    map.insert("stderr".into(), stderr.into());
    map
}

fn reject_recursive_effigy_process(program: &str) -> Result<(), Box<EvalAltResult>> {
    if program == "effigy" || program == "effigy.exe" {
        return Err(rhai_runtime_error(
            "Rhai scripts must not call `run_process(\"effigy\", ...)`; use a typed host helper or add a new Rhai host surface",
        ));
    }
    Ok(())
}

fn resolve_process_execution_options(
    base_cwd: &Path,
    options: Map,
) -> Result<ProcessExecutionOptions, Box<EvalAltResult>> {
    let options = map_to_json_object(options)?;
    let cwd = options
        .get("cwd")
        .map(|value| match value {
            Value::String(value) => Ok(resolve_runtime_path(base_cwd, value)),
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
            Value::String(value) => Ok(Some(resolve_runtime_path(&cwd, value))),
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

fn configure_process_command(
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
            .map_err(|error| rhai_runtime_error(failed_to_read_path(stdin_file, error)))?;
        process.stdin(Stdio::from(file));
    }
    Ok(resolved.cwd)
}

fn search_files(root: &Path, pattern: &str, options: Map) -> Result<Map, Box<EvalAltResult>> {
    let options = map_to_json_object(options)?;
    let glob = json_object_string_option(&options, "glob")?;
    let literal = json_object_bool_option(&options, "literal")?.unwrap_or(false);
    let matcher = if literal {
        None
    } else {
        Some(regex::Regex::new(pattern).map_err(|error| rhai_runtime_error(error.to_string()))?)
    };
    let mut matches = Vec::<Value>::new();
    for path in search_candidate_files(root, glob.as_deref())? {
        let contents = std::fs::read_to_string(&path)
            .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
        for (index, line) in contents.lines().enumerate() {
            let matched = if let Some(matcher) = &matcher {
                matcher.is_match(line)
            } else {
                line.contains(pattern)
            };
            if matched {
                matches.push(json!({
                    "path": path.display().to_string(),
                    "line": index + 1,
                    "text": line,
                }));
            }
        }
    }

    let stdout = matches
        .iter()
        .filter_map(|entry| {
            Some(format!(
                "{}:{}:{}",
                entry.get("path")?.as_str()?,
                entry.get("line")?.as_u64()?,
                entry.get("text")?.as_str()?
            ))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut map = Map::new();
    map.insert(
        "status".into(),
        Dynamic::from_int(if matches.is_empty() { 1 } else { 0 }),
    );
    map.insert("success".into(), Dynamic::from_bool(!matches.is_empty()));
    map.insert(
        "count".into(),
        Dynamic::from_int(i64::try_from(matches.len()).unwrap_or(i64::MAX)),
    );
    map.insert("stdout".into(), stdout.into());
    map.insert("stderr".into(), String::new().into());
    map.insert(
        "matches".into(),
        rhai::serde::to_dynamic(Value::Array(matches))
            .map_err(|error| rhai_runtime_error(error.to_string()))?,
    );
    Ok(map)
}

fn search_candidate_files(
    root: &Path,
    glob: Option<&str>,
) -> Result<Vec<PathBuf>, Box<EvalAltResult>> {
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    if !root.is_dir() {
        return Err(rhai_runtime_error(format!(
            "search root not found: {}",
            root.display()
        )));
    }
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.map_err(|error| rhai_runtime_error(error.to_string()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(glob) = glob {
            if !path_matches_simple_glob(path, glob) {
                continue;
            }
        }
        files.push(path.to_path_buf());
    }
    files.sort();
    Ok(files)
}

fn path_matches_simple_glob(path: &Path, glob: &str) -> bool {
    let rendered = path.display().to_string();
    if let Some(suffix) = glob.strip_prefix('*') {
        return rendered.ends_with(suffix);
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == glob)
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

fn run_http_request(method: &str, url: &str, options: Map) -> Result<Map, Box<EvalAltResult>> {
    let options = map_to_json_object(options)?;
    let timeout_ms = json_object_usize_option(&options, "timeout_ms")?.unwrap_or(30_000);
    let mut builder =
        reqwest::blocking::Client::builder().timeout(Duration::from_millis(timeout_ms as u64));
    if json_object_bool_option(&options, "danger_accept_invalid_certs")?.unwrap_or(false) {
        builder = builder.danger_accept_invalid_certs(true);
    }
    let client = builder
        .build()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let mut request = client.request(method, url);
    if let Some(headers) = options.get("headers") {
        let headers = headers.as_object().ok_or_else(|| {
            rhai_runtime_error("`headers` must be a map of string names to string values")
        })?;
        for (name, value) in headers {
            let value = value
                .as_str()
                .ok_or_else(|| rhai_runtime_error("`headers` values must be strings"))?;
            request = request.header(name, value);
        }
    }
    if let Some(body) = options.get("body") {
        let body = body
            .as_str()
            .ok_or_else(|| rhai_runtime_error("`body` must be a string"))?;
        request = request.body(body.to_owned());
    }
    if let Some(json_body) = options.get("json") {
        let body = serde_json::to_string(json_body)
            .map_err(|error| rhai_runtime_error(error.to_string()))?;
        request = request
            .header("content-type", "application/json")
            .body(body);
    }
    let response = request
        .send()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                Value::String(value.to_str().unwrap_or_default().to_owned()),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    let body = response
        .text()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let mut map = Map::new();
    map.insert(
        "status".into(),
        Dynamic::from_int(i64::from(status.as_u16())),
    );
    map.insert("success".into(), Dynamic::from_bool(status.is_success()));
    map.insert("body".into(), body.into());
    map.insert(
        "headers".into(),
        rhai::serde::to_dynamic(Value::Object(headers))
            .map_err(|error| rhai_runtime_error(error.to_string()))?,
    );
    Ok(map)
}

fn host_command_output_map(output: HostCommandOutput) -> Map {
    let mut map = Map::new();
    map.insert("status".into(), Dynamic::from_int(output.status));
    map.insert("success".into(), Dynamic::from_bool(output.success));
    map.insert("stdout".into(), output.stdout.into());
    map.insert("stderr".into(), output.stderr.into());
    map
}

// Module-based registration helpers

fn module_feature_no_args(
    module: &mut rhai::Module,
    function: &'static str,
    feature: &'static str,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    module.set_native_fn(function, move || -> Result<Dynamic, Box<EvalAltResult>> {
        run_feature_dynamic(&context, &callbacks, feature, json!({}))
    });
}

fn module_feature_options(
    module: &mut rhai::Module,
    function: &'static str,
    feature: &'static str,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    module.set_native_fn(
        function,
        move |options: Map| -> Result<Dynamic, Box<EvalAltResult>> {
            run_feature_dynamic(&context, &callbacks, feature, map_to_json(options)?)
        },
    );
}

fn module_feature_string(
    module: &mut rhai::Module,
    function: &'static str,
    feature: &'static str,
    key: &'static str,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    module.set_native_fn(
        function,
        move |value: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            run_feature_dynamic(
                &context,
                &callbacks,
                feature,
                json!({ key: value.as_str() }),
            )
        },
    );
}

fn module_feature_get_value(
    module: &mut rhai::Module,
    function: &'static str,
    feature: &'static str,
    key: &'static str,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    module.set_native_fn(
        function,
        move |value: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            let output = (callbacks.run_feature)(
                &context.repo_root,
                feature,
                json!({ key: value.as_str() }),
            )
            .map_err(|error| rhai_runtime_error(error.message))?;
            let value: serde_json::Value = serde_json::from_str(&output)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            let Some(value) = value.get("value") else {
                return Ok(Dynamic::UNIT);
            };
            if value.is_null() {
                return Ok(Dynamic::UNIT);
            }
            rhai::serde::to_dynamic(value.clone())
                .map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
}

fn module_feature_string_options(
    module: &mut rhai::Module,
    function: &'static str,
    feature: &'static str,
    key: &'static str,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    let no_options_context = context.clone();
    let no_options_callbacks = callbacks.clone();
    module.set_native_fn(
        function,
        move |value: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            run_feature_dynamic(
                &no_options_context,
                &no_options_callbacks,
                feature,
                json!({ key: value.as_str() }),
            )
        },
    );
    module.set_native_fn(
        function,
        move |value: ImmutableString, options: Map| -> Result<Dynamic, Box<EvalAltResult>> {
            let mut options = map_to_json_object(options)?;
            options.insert(key.to_owned(), json!(value.as_str()));
            run_feature_dynamic(&context, &callbacks, feature, Value::Object(options))
        },
    );
}

fn module_feature_two_strings(
    module: &mut rhai::Module,
    function: &'static str,
    feature: &'static str,
    keys: [&'static str; 2],
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    module.set_native_fn(
        function,
        move |first: ImmutableString,
              second: ImmutableString|
              -> Result<Dynamic, Box<EvalAltResult>> {
            run_feature_dynamic(
                &context,
                &callbacks,
                feature,
                json!({ keys[0]: first.as_str(), keys[1]: second.as_str() }),
            )
        },
    );
}

fn run_feature_dynamic(
    context: &ScriptContext,
    callbacks: &HostCallbacks,
    feature: &str,
    options: Value,
) -> Result<Dynamic, Box<EvalAltResult>> {
    let output = (callbacks.run_feature)(&context.repo_root, feature, options)
        .map_err(|error| rhai_runtime_error(error.message))?;
    if output.trim().is_empty() {
        return rhai::serde::to_dynamic(json!({ "ok": true }))
            .map_err(|error| rhai_runtime_error(error.to_string()));
    }
    let value: serde_json::Value =
        serde_json::from_str(&output).map_err(|error| rhai_runtime_error(error.to_string()))?;
    rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
}

fn map_to_json(options: Map) -> Result<Value, Box<EvalAltResult>> {
    Ok(Value::Object(map_to_json_object(options)?))
}

fn map_to_json_object(options: Map) -> Result<serde_json::Map<String, Value>, Box<EvalAltResult>> {
    let dynamic = Dynamic::from_map(options);
    let value: serde_json::Value = rhai::serde::from_dynamic(&dynamic)
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(rhai_runtime_error("feature options must be a map")),
    }
}

fn json_object_bool_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be a bool"))),
    }
}

fn json_object_string_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be a string"))),
    }
}

fn json_object_usize_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<usize>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::Number(value)) => value
            .as_u64()
            .map(|value| Some(value as usize))
            .ok_or_else(|| rhai_runtime_error(format!("`{key}` must be a usize"))),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be a usize"))),
    }
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

fn run_process_streaming(
    program: &str,
    args: &[String],
    cwd: &Path,
) -> Result<Map, Box<EvalAltResult>> {
    run_process_streaming_with_options(program, args, cwd, None)
}

fn run_process_streaming_with_options(
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
            .map_err(|error| rhai_runtime_error(failed_to_read_path(stdin_file, error)))?;
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

fn run_process_teeing(
    program: &str,
    args: &[String],
    cwd: &Path,
) -> Result<Map, Box<EvalAltResult>> {
    run_process_teeing_with_options(program, args, cwd, None)
}

fn run_process_teeing_with_options(
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

fn rhai_runtime_error(message: impl Into<String>) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(message.into().into(), Position::NONE).into()
}

#[cfg(test)]
mod tests;
