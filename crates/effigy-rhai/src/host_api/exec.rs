use effigy_core::path_error_text::failed_to_read_path;
use effigy_exec::CwdMapper;
use effigy_execution::{
    ExecutionEnvironmentPlan, ExecutionIntent, ExecutionOutputMode, ExecutionRoute,
    ExecutionRunTarget, ExecutionRuntimePolicy, ExecutionSurface, TaskExecutionRequestBuilder,
};
use rhai::{Array, EvalAltResult, Map};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::Arc;

use crate::surface::MODULE_EXEC;

use super::{
    dynamic_array_to_strings, host_command_output_map, map_to_json,
    reject_recursive_effigy_process, rhai_runtime_error, with_local_node_bin_path, HostCallbacks,
    ScriptContext,
};

pub(super) fn register_exec_module(
    engine: &mut rhai::Engine,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    engine.register_static_module(
        MODULE_EXEC,
        std::rc::Rc::new(build_exec_module(context, callbacks)),
    );
}

fn build_exec_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "run",
        move |command: Array, options: Map| -> Result<Map, Box<EvalAltResult>> {
            let command = dynamic_array_to_strings(&command)?;
            run_execution_request(&context, &callbacks, command, options)
        },
    );
    module
}

fn run_execution_request(
    context: &ScriptContext,
    callbacks: &HostCallbacks,
    command: Vec<String>,
    options: Map,
) -> Result<Map, Box<EvalAltResult>> {
    let program = command
        .first()
        .ok_or_else(|| rhai_runtime_error("exec::run command must not be empty"))?;
    reject_recursive_effigy_process(program)?;

    let options_json = match map_to_json(options.clone())? {
        Value::Object(map) => map,
        _ => unreachable!("map_to_json returns an object for Rhai maps"),
    };
    let run_in = execution_run_target(&options_json)?;
    let container = execution_string_option(&options_json, "container")?;
    let service = execution_string_option(&options_json, "service")?;
    if run_in == ExecutionRunTarget::Container && container.is_none() {
        return Err(rhai_runtime_error(
            "`container` is required when `run_in` is \"container\"",
        ));
    }

    let environment = execution_environment_plan(&options_json)?;
    let runtime_policy = match run_in {
        ExecutionRunTarget::Host => ExecutionRuntimePolicy::host(),
        ExecutionRunTarget::Container => ExecutionRuntimePolicy::container(
            container.clone().unwrap_or_default(),
            service.clone(),
        ),
        ExecutionRunTarget::Either => ExecutionRuntimePolicy {
            run_in,
            container: container.clone(),
            service: service.clone(),
        },
    };
    let runtime_context = super::super::active_runtime_context_for_script(context)
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let plan = TaskExecutionRequestBuilder::new()
        .runtime_context(runtime_context)
        .invocation(ExecutionIntent::Command {
            command: command.clone(),
        })
        .surface(ExecutionSurface::Rhai)
        .output_mode(ExecutionOutputMode::Capture)
        .runtime_policy(runtime_policy)
        .environment(environment)
        .resolve()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;

    let output = match &plan.route {
        ExecutionRoute::Host | ExecutionRoute::LocalContainerHandoff { .. } => {
            run_exec_host_capture_with_environment(
                context,
                &plan.request.runtime_context,
                &command,
                &plan.request.environment,
            )?
        }
        ExecutionRoute::Container { container, service } => {
            let container = container.as_deref().unwrap_or_default();
            (callbacks.container_exec_with_options)(
                &context.repo_root,
                container,
                service.as_deref(),
                &command,
                resolved_execution_options_json(context, &plan.request.environment),
            )
            .map_err(rhai_runtime_error)?
        }
    };

    let mut result = host_command_output_map(output);
    result.insert("route".into(), execution_route_map(&plan.route).into());
    Ok(result)
}

fn run_exec_host_capture_with_environment(
    context: &ScriptContext,
    runtime_context: &effigy_context::EffigyRuntimeContext,
    command: &[String],
    environment: &ExecutionEnvironmentPlan,
) -> Result<super::super::HostCommandOutput, Box<EvalAltResult>> {
    let program = command
        .first()
        .ok_or_else(|| rhai_runtime_error("exec::run command must not be empty"))?;
    let mut process = ProcessCommand::new(program);
    process.args(&command[1..]);
    let desired_cwd = resolved_execution_cwd(runtime_context.invocation_cwd(), environment);
    let desired_stdin = resolved_execution_stdin_file(&desired_cwd, environment);
    let (resolved_cwd, resolved_stdin) = if runtime_context.container().inside_container_handoff {
        remap_execution_paths_for_local_handoff(
            runtime_context,
            &context.cwd,
            desired_cwd,
            desired_stdin,
        )?
    } else {
        (desired_cwd, desired_stdin)
    };
    process.current_dir(&resolved_cwd);
    for (key, value) in &environment.env {
        process.env(key, value);
    }
    if let Some(stdin_file) = resolved_stdin {
        let file = std::fs::File::open(&stdin_file)
            .map_err(|error| rhai_runtime_error(failed_to_read_path(&stdin_file, error)))?;
        process.stdin(std::process::Stdio::from(file));
    }
    with_local_node_bin_path(&mut process, &resolved_cwd);
    let output = process
        .output()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    Ok(super::super::HostCommandOutput {
        status: output.status.code().unwrap_or(-1).into(),
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn remap_execution_paths_for_local_handoff(
    runtime_context: &effigy_context::EffigyRuntimeContext,
    local_repo_root_hint: &Path,
    desired_cwd: PathBuf,
    desired_stdin: Option<PathBuf>,
) -> Result<(PathBuf, Option<PathBuf>), Box<EvalAltResult>> {
    let local_repo_root = local_repo_root_hint
        .canonicalize()
        .ok()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| local_repo_root_hint.to_path_buf());
    let mapper = CwdMapper::new(
        runtime_context.command_root().to_path_buf(),
        local_repo_root,
    );
    let remapped_cwd = mapper
        .host_to_container(&desired_cwd)
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let remapped_stdin = desired_stdin
        .map(|stdin_file| {
            mapper
                .host_to_container(&stdin_file)
                .map_err(|error| rhai_runtime_error(error.to_string()))
        })
        .transpose()?;
    Ok((remapped_cwd, remapped_stdin))
}

fn resolved_execution_options_json(
    context: &ScriptContext,
    environment: &ExecutionEnvironmentPlan,
) -> Value {
    let cwd = resolved_execution_cwd(&context.cwd, environment);
    let mut options = serde_json::Map::new();
    options.insert("cwd".to_owned(), json!(cwd.display().to_string()));
    if let Some(stdin_file) = resolved_execution_stdin_file(&cwd, environment) {
        options.insert(
            "stdin_file".to_owned(),
            json!(stdin_file.display().to_string()),
        );
    }
    if !environment.env.is_empty() {
        options.insert(
            "env".to_owned(),
            Value::Object(
                environment
                    .env
                    .iter()
                    .map(|(key, value)| (key.clone(), json!(value.to_string_lossy().to_string())))
                    .collect(),
            ),
        );
    }
    Value::Object(options)
}

fn resolved_execution_cwd(base_cwd: &Path, environment: &ExecutionEnvironmentPlan) -> PathBuf {
    environment
        .cwd
        .as_ref()
        .map(|cwd| resolve_execution_path(base_cwd, cwd))
        .unwrap_or_else(|| {
            base_cwd
                .canonicalize()
                .unwrap_or_else(|_| base_cwd.to_path_buf())
        })
}

fn resolved_execution_stdin_file(
    resolved_cwd: &Path,
    environment: &ExecutionEnvironmentPlan,
) -> Option<PathBuf> {
    environment
        .stdin_file
        .as_ref()
        .map(|stdin_file| resolve_execution_path(resolved_cwd, stdin_file))
}

fn resolve_execution_path(base: &Path, path: &Path) -> PathBuf {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    resolved.canonicalize().unwrap_or(resolved)
}

fn execution_run_target(
    options: &serde_json::Map<String, Value>,
) -> Result<ExecutionRunTarget, Box<EvalAltResult>> {
    match execution_string_option(options, "run_in")?.as_deref() {
        Some("host") => Ok(ExecutionRunTarget::Host),
        Some("container") => Ok(ExecutionRunTarget::Container),
        Some("either") | None => Ok(ExecutionRunTarget::Either),
        Some(value) => Err(rhai_runtime_error(format!(
            "`run_in` must be \"host\", \"container\", or \"either\", got `{value}`"
        ))),
    }
}

fn execution_string_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be a string"))),
    }
}

fn execution_environment_plan(
    options: &serde_json::Map<String, Value>,
) -> Result<ExecutionEnvironmentPlan, Box<EvalAltResult>> {
    let mut plan = ExecutionEnvironmentPlan::default();
    if let Some(cwd) = execution_string_option(options, "cwd")? {
        plan = plan.cwd(cwd);
    }
    if let Some(stdin_file) = execution_string_option(options, "stdin_file")? {
        plan = plan.stdin_file(stdin_file);
    }
    if let Some(value) = options.get("env") {
        let Value::Object(env) = value else {
            return Err(rhai_runtime_error("`env` must be a map of string values"));
        };
        for (key, value) in env {
            let Value::String(value) = value else {
                return Err(rhai_runtime_error("`env` values must be strings"));
            };
            plan = plan.env(key.clone(), value.clone());
        }
    }
    Ok(plan)
}

fn execution_route_map(route: &ExecutionRoute) -> Map {
    let mut map = Map::new();
    match route {
        ExecutionRoute::Host => {
            map.insert("run_in".into(), "host".into());
        }
        ExecutionRoute::Container { container, service } => {
            map.insert("run_in".into(), "container".into());
            map.insert(
                "container".into(),
                container.clone().unwrap_or_default().into(),
            );
            map.insert("service".into(), service.clone().unwrap_or_default().into());
        }
        ExecutionRoute::LocalContainerHandoff { service } => {
            map.insert("run_in".into(), "container_handoff".into());
            map.insert("service".into(), service.clone().unwrap_or_default().into());
        }
    }
    map
}
