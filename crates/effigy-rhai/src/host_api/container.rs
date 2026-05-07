use rhai::{Array, Dynamic, EvalAltResult, ImmutableString, Map};
use serde_json::json;
use std::sync::Arc;

use crate::surface::*;

use super::{
    dynamic_array_to_strings, host_command_output_map, map_to_json, module_feature_no_args,
    module_feature_options, module_feature_string, module_feature_string_options,
    rhai_runtime_error, run_feature_dynamic, HostCallbacks, ScriptContext,
};

pub(super) fn register_container_module(
    engine: &mut rhai::Engine,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    engine.register_static_module(
        MODULE_CONTAINER,
        std::rc::Rc::new(build_container_module(context, callbacks)),
    );
}

fn build_container_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    let container_context = context.clone();
    let container_callbacks = callbacks.clone();
    module.set_native_fn(
        "up",
        move |name: ImmutableString, detach: bool| -> Result<String, Box<EvalAltResult>> {
            (container_callbacks.container_up)(&container_context.repo_root, name.as_str(), detach)
                .map_err(rhai_runtime_error)
        },
    );
    let container_context = context.clone();
    let container_callbacks = callbacks.clone();
    module.set_native_fn(
        "down",
        move |name: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            (container_callbacks.container_down)(&container_context.repo_root, name.as_str(), false)
                .map_err(rhai_runtime_error)
        },
    );
    let container_context = context.clone();
    let container_callbacks = callbacks.clone();
    module.set_native_fn("down_all", move || -> Result<String, Box<EvalAltResult>> {
        (container_callbacks.container_down)(&container_context.repo_root, "", true)
            .map_err(rhai_runtime_error)
    });
    let container_shell_context = context.clone();
    let container_shell_callbacks = callbacks.clone();
    module.set_native_fn(
        "shell",
        move |name: ImmutableString,
              command: ImmutableString|
              -> Result<String, Box<EvalAltResult>> {
            (container_shell_callbacks.container_shell)(
                &container_shell_context.repo_root,
                name.as_str(),
                None,
                command.as_str(),
            )
            .map_err(rhai_runtime_error)
        },
    );
    let container_shell_context = context.clone();
    let container_shell_callbacks = callbacks.clone();
    module.set_native_fn(
        "shell",
        move |name: ImmutableString,
              service: ImmutableString,
              command: ImmutableString|
              -> Result<String, Box<EvalAltResult>> {
            (container_shell_callbacks.container_shell)(
                &container_shell_context.repo_root,
                name.as_str(),
                Some(service.as_str()),
                command.as_str(),
            )
            .map_err(rhai_runtime_error)
        },
    );
    let container_exec_context = context.clone();
    let container_exec_callbacks = callbacks.clone();
    module.set_native_fn(
        "exec",
        move |name: ImmutableString, command: Array| -> Result<Map, Box<EvalAltResult>> {
            Ok(host_command_output_map(
                (container_exec_callbacks.container_exec)(
                    &container_exec_context.repo_root,
                    name.as_str(),
                    None,
                    &dynamic_array_to_strings(&command)?,
                )
                .map_err(rhai_runtime_error)?,
            ))
        },
    );
    let container_exec_context = context.clone();
    let container_exec_callbacks = callbacks.clone();
    module.set_native_fn(
        "exec",
        move |name: ImmutableString,
              command: Array,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            Ok(host_command_output_map(
                (container_exec_callbacks.container_exec_with_options)(
                    &container_exec_context.repo_root,
                    name.as_str(),
                    None,
                    &dynamic_array_to_strings(&command)?,
                    map_to_json(options)?,
                )
                .map_err(rhai_runtime_error)?,
            ))
        },
    );
    let container_exec_context = context.clone();
    let container_exec_callbacks = callbacks.clone();
    module.set_native_fn(
        "exec",
        move |name: ImmutableString,
              service: ImmutableString,
              command: Array|
              -> Result<Map, Box<EvalAltResult>> {
            Ok(host_command_output_map(
                (container_exec_callbacks.container_exec)(
                    &container_exec_context.repo_root,
                    name.as_str(),
                    Some(service.as_str()),
                    &dynamic_array_to_strings(&command)?,
                )
                .map_err(rhai_runtime_error)?,
            ))
        },
    );
    let container_exec_context = context.clone();
    let container_exec_callbacks = callbacks.clone();
    module.set_native_fn(
        "exec",
        move |name: ImmutableString,
              service: ImmutableString,
              command: Array,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            Ok(host_command_output_map(
                (container_exec_callbacks.container_exec_with_options)(
                    &container_exec_context.repo_root,
                    name.as_str(),
                    Some(service.as_str()),
                    &dynamic_array_to_strings(&command)?,
                    map_to_json(options)?,
                )
                .map_err(rhai_runtime_error)?,
            ))
        },
    );
    module_feature_string(
        &mut module,
        "status",
        FEATURE_CONTAINER_STATUS,
        "name",
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "status",
        FEATURE_CONTAINER_STATUS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string_options(
        &mut module,
        "logs",
        FEATURE_CONTAINER_LOGS,
        "name",
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string_options(
        &mut module,
        "reset",
        FEATURE_CONTAINER_RESET,
        "name",
        context.clone(),
        callbacks.clone(),
    );
    let data_context = context.clone();
    let data_callbacks = callbacks.clone();
    module.set_native_fn(
        "data",
        move |operation: ImmutableString,
              name: ImmutableString|
              -> Result<Dynamic, Box<EvalAltResult>> {
            run_feature_dynamic(
                &data_context,
                &data_callbacks,
                FEATURE_CONTAINER_DATA,
                json!({
                    "operation": operation.as_str(),
                    "name": name.as_str(),
                }),
            )
        },
    );
    let data_context = context.clone();
    let data_callbacks = callbacks.clone();
    module.set_native_fn(
        "data",
        move |operation: ImmutableString,
              name: ImmutableString,
              volume: ImmutableString,
              path: ImmutableString|
              -> Result<Dynamic, Box<EvalAltResult>> {
            run_feature_dynamic(
                &data_context,
                &data_callbacks,
                FEATURE_CONTAINER_DATA,
                json!({
                    "operation": operation.as_str(),
                    "name": name.as_str(),
                    "volume": volume.as_str(),
                    "path": path.as_str(),
                }),
            )
        },
    );
    module_feature_string(
        &mut module,
        "eject",
        FEATURE_CONTAINER_EJECT,
        "name",
        context.clone(),
        callbacks.clone(),
    );
    module_feature_no_args(
        &mut module,
        "stats",
        FEATURE_CONTAINER_STATS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "stats",
        FEATURE_CONTAINER_STATS,
        context.clone(),
        callbacks.clone(),
    );
    module
}
