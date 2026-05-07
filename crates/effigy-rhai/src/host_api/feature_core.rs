use std::sync::Arc;

use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString};
use serde_json::json;

use crate::surface::*;

use super::{
    dynamic_array_to_strings, module_feature_get_value, module_feature_no_args,
    module_feature_options, module_feature_string, rhai_runtime_error, HostCallbacks,
    ScriptContext,
};

pub(super) fn register_core_feature_modules(
    engine: &mut Engine,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    engine.register_static_module(
        MODULE_CONFIG,
        std::rc::Rc::new(build_config_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_TASK,
        std::rc::Rc::new(build_task_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_SCAN,
        std::rc::Rc::new(build_scan_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_DOCS,
        std::rc::Rc::new(build_docs_module(context, callbacks)),
    );
}

fn build_config_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(
        &mut module,
        "effective",
        FEATURE_CONFIG_EFFECTIVE,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_no_args(
        &mut module,
        "raw",
        FEATURE_CONFIG_RAW,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_get_value(
        &mut module,
        "get",
        FEATURE_CONFIG_GET,
        "path",
        context.clone(),
        callbacks.clone(),
    );
    let config_or_context = context.clone();
    let config_or_callbacks = callbacks.clone();
    module.set_native_fn(
        "get_or",
        move |path: ImmutableString, default: Dynamic| -> Result<Dynamic, Box<EvalAltResult>> {
            let output = (config_or_callbacks.run_feature)(
                &config_or_context.repo_root,
                FEATURE_CONFIG_GET,
                json!({ "path": path.as_str() }),
            )
            .map_err(|error| rhai_runtime_error(error.message))?;
            let value: serde_json::Value = serde_json::from_str(&output)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            let Some(found_value) = value.get("value") else {
                return Ok(default);
            };
            if found_value.is_null() {
                return Ok(default);
            }
            rhai::serde::to_dynamic(found_value.clone())
                .map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module
}

fn build_task_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    let task_context = context.clone();
    let task_callbacks = callbacks.clone();
    module.set_native_fn(
        "run",
        move |task: ImmutableString, args: Array| -> Result<String, Box<EvalAltResult>> {
            (task_callbacks.run_task)(
                &task_context.cwd,
                task.as_str(),
                &dynamic_array_to_strings(&args)?,
            )
            .map_err(rhai_runtime_error)
        },
    );
    let task_json_context = context.clone();
    let task_json_callbacks = callbacks.clone();
    module.set_native_fn(
        "run_json",
        move |task: ImmutableString, args: Array| -> Result<Dynamic, Box<EvalAltResult>> {
            let output = (task_json_callbacks.run_task)(
                &task_json_context.cwd,
                task.as_str(),
                &dynamic_array_to_strings(&args)?,
            )
            .map_err(rhai_runtime_error)?;
            let value: serde_json::Value = serde_json::from_str(&output)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module_feature_no_args(
        &mut module,
        "list",
        FEATURE_TASKS_LIST,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "list",
        FEATURE_TASKS_LIST,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string(
        &mut module,
        "resolve",
        FEATURE_TASKS_RESOLVE,
        "selector",
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string(
        &mut module,
        "info",
        FEATURE_TASKS_INFO,
        "selector",
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_scan_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(
        &mut module,
        "god_files",
        FEATURE_SCAN_GOD_FILES,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "generated_assets",
        FEATURE_SCAN_GENERATED_ASSETS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "generated_in_src",
        FEATURE_SCAN_GENERATED_IN_SRC,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "duplicate_blocks",
        FEATURE_SCAN_DUPLICATE_BLOCKS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "comment_ratio",
        FEATURE_SCAN_COMMENT_RATIO,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "attention_markers",
        FEATURE_SCAN_ATTENTION_MARKERS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "stale_suppressions",
        FEATURE_SCAN_STALE_SUPPRESSIONS,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_docs_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(
        &mut module,
        "check_links",
        FEATURE_DOCS_CHECK_LINKS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "check_json_examples",
        FEATURE_DOCS_CHECK_JSON_EXAMPLES,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "check_headings",
        FEATURE_DOCS_CHECK_HEADINGS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "check_paths",
        FEATURE_DOCS_CHECK_PATHS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "check_contains",
        FEATURE_DOCS_CHECK_CONTAINS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "check_forbidden",
        FEATURE_DOCS_CHECK_FORBIDDEN,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "check_index",
        FEATURE_DOCS_CHECK_INDEX,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "check_next_action",
        FEATURE_DOCS_CHECK_NEXT_ACTION,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "check_workflow_paths",
        FEATURE_DOCS_CHECK_WORKFLOW_PATHS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "add_log_index",
        FEATURE_DOCS_ADD_LOG_INDEX,
        context.clone(),
        callbacks.clone(),
    );
    module
}
