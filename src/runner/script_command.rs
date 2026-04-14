use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::Arc;

use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map, Position, Scope};

use crate::{InternalRhaiArgs, InternalRhaiSource, TaskInvocation};

use super::error::RunnerError;
use super::execute::run_manifest_task_with_cwd;
use super::util::with_local_node_bin_path;

const EFFIGY_RHAI_INLINE: &str = "EFFIGY_RHAI_INLINE";
const EFFIGY_RHAI_ARGS_JSON: &str = "EFFIGY_RHAI_ARGS_JSON";
const EFFIGY_RHAI_TASK_NAME: &str = "EFFIGY_RHAI_TASK_NAME";
const EFFIGY_RHAI_REPO_ROOT: &str = "EFFIGY_RHAI_REPO_ROOT";

#[derive(Clone)]
struct ScriptContext {
    cwd: PathBuf,
    repo_root: PathBuf,
    task_name: String,
}

pub(in crate::runner) fn run_internal_rhai(args: InternalRhaiArgs) -> Result<String, RunnerError> {
    let context = ScriptContext {
        cwd: std::env::current_dir().map_err(RunnerError::Cwd)?,
        repo_root: PathBuf::from(required_env(EFFIGY_RHAI_REPO_ROOT)?),
        task_name: required_env(EFFIGY_RHAI_TASK_NAME)?,
    };
    let script = load_script(&args, &context)?;
    let script_args = load_script_args()?;
    execute_rhai_script(&context, &script, &script_args)?;
    Ok(String::new())
}

fn load_script(args: &InternalRhaiArgs, context: &ScriptContext) -> Result<String, RunnerError> {
    match &args.source {
        InternalRhaiSource::Inline => required_env(EFFIGY_RHAI_INLINE),
        InternalRhaiSource::File(path) => {
            let resolved = resolve_script_path(&context.cwd, path);
            std::fs::read_to_string(&resolved)
                .map_err(|error| RunnerError::task_invocation_failed_read(&resolved, error))
        }
    }
}

fn load_script_args() -> Result<Vec<String>, RunnerError> {
    let raw = required_env(EFFIGY_RHAI_ARGS_JSON)?;
    serde_json::from_str(&raw).map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn execute_rhai_script(
    context: &ScriptContext,
    script: &str,
    script_args: &[String],
) -> Result<(), RunnerError> {
    let context = Arc::new(context.clone());
    let mut engine = Engine::new();
    register_host_api(&mut engine, context.clone());

    let mut scope = Scope::new();
    scope.push_constant("args", script_args.iter().cloned().map(Into::into).collect::<Array>());
    scope.push_constant("cwd", context.cwd.display().to_string());
    scope.push_constant("repo_root", context.repo_root.display().to_string());
    scope.push_constant("task_name", context.task_name.clone());

    engine
        .run_with_scope(&mut scope, script)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn register_host_api(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_fn("log", |message: ImmutableString| {
        println!("{message}");
    });
    engine.register_fn("log_warn", |message: ImmutableString| {
        eprintln!("{message}");
    });

    engine.register_fn("env", |name: ImmutableString| -> String {
        std::env::var(name.as_str()).unwrap_or_default()
    });
    engine.register_fn("path_join", |base: ImmutableString, child: ImmutableString| -> String {
        PathBuf::from(base.as_str())
            .join(child.as_str())
            .display()
            .to_string()
    });

    let file_context = context.clone();
    engine.register_fn(
        "read_file",
        move |path: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::read_to_string(&path).map_err(|error| rhai_runtime_error(format!(
                "{}",
                RunnerError::task_invocation_failed_read(&path, error)
            )))
        },
    );
    let file_context = context.clone();
    engine.register_fn(
        "write_file",
        move |path: ImmutableString, contents: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    rhai_runtime_error(format!(
                        "{}",
                        RunnerError::task_invocation_failed_write(parent, error)
                    ))
                })?;
            }
            std::fs::write(&path, contents.as_str()).map_err(|error| {
                rhai_runtime_error(format!(
                    "{}",
                    RunnerError::task_invocation_failed_write(&path, error)
                ))
            })
        },
    );
    let file_context = context.clone();
    engine.register_fn("path_exists", move |path: ImmutableString| -> bool {
        resolve_runtime_path(&file_context.cwd, path.as_str()).exists()
    });
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
                .map_err(|error| rhai_runtime_error(format!(
                    "{}",
                    RunnerError::task_invocation_failed_read(&path, error)
                )))
        },
    );
    let file_context = context.clone();
    engine.register_fn(
        "create_dir",
        move |path: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::create_dir_all(&path).map_err(|error| {
                rhai_runtime_error(format!(
                    "{}",
                    RunnerError::task_invocation_failed_write(&path, error)
                ))
            })
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
            .map_err(|error| {
                rhai_runtime_error(format!(
                    "{}",
                    RunnerError::task_invocation_failed_write(&path, error)
                ))
            })
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
                std::os::unix::fs::symlink(&target, &link).map_err(|error| {
                    rhai_runtime_error(format!(
                        "{}",
                        RunnerError::task_invocation_failed_write(&link, error)
                    ))
                })
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

    let task_context = context;
    engine.register_fn(
        "run_task",
        move |task: ImmutableString, args: Array| -> Result<String, Box<EvalAltResult>> {
            let invocation = TaskInvocation {
                name: task.to_string(),
                args: dynamic_array_to_strings(&args)?,
            };
            run_manifest_task_with_cwd(&invocation, task_context.cwd.clone())
                .map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
}

fn process_result_map(output: std::process::Output) -> Map {
    let mut map = Map::new();
    map.insert("status".into(), Dynamic::from_int(output.status.code().unwrap_or(-1).into()));
    map.insert("success".into(), Dynamic::from_bool(output.status.success()));
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

fn required_env(key: &str) -> Result<String, RunnerError> {
    std::env::var(key).map_err(|_| {
        RunnerError::task_invocation(format!("missing internal Rhai environment variable `{key}`"))
    })
}

fn resolve_script_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn resolve_runtime_path(cwd: &Path, raw: &str) -> PathBuf {
    let path = Path::new(raw);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn rhai_runtime_error(message: String) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(message.into(), Position::NONE).into()
}
