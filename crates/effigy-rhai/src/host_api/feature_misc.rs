use std::sync::Arc;

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map};

use crate::surface::*;

use super::{
    dynamic_array_to_strings, effigy_result_map, module_feature_no_args, module_feature_options,
    module_feature_string, module_feature_string_options, module_feature_two_strings,
    rhai_runtime_error, HostCallbacks, ScriptContext,
};

pub(super) fn register_misc_feature_modules(
    engine: &mut Engine,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    engine.register_static_module(
        MODULE_DEPLOY,
        std::rc::Rc::new(build_deploy_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_SYSTEM,
        std::rc::Rc::new(build_system_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_DEMO,
        std::rc::Rc::new(build_demo_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_CHANGELOG,
        std::rc::Rc::new(build_changelog_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_CACHE,
        std::rc::Rc::new(build_cache_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_GATEWAY,
        std::rc::Rc::new(build_gateway_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_BUNDLE,
        std::rc::Rc::new(build_bundle_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_SERVICE,
        std::rc::Rc::new(build_service_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_CATALOG,
        std::rc::Rc::new(build_catalog_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_DOCTOR,
        std::rc::Rc::new(build_doctor_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_CONTRACTS,
        std::rc::Rc::new(build_contracts_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_UNLOCK,
        std::rc::Rc::new(build_unlock_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_TEST,
        std::rc::Rc::new(build_test_module(context.clone(), callbacks.clone())),
    );
    engine.register_static_module(
        MODULE_EFFIGY,
        std::rc::Rc::new(build_effigy_module(context, callbacks)),
    );
}

fn build_deploy_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(
        &mut module,
        "model",
        FEATURE_DEPLOY_MODEL,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "emit",
        FEATURE_DEPLOY_EMIT,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_system_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(
        &mut module,
        "status",
        FEATURE_SYSTEM_STATUS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "logs",
        FEATURE_SYSTEM_LOGS,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_demo_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(
        &mut module,
        "list",
        FEATURE_DEMO_LIST,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "inspect",
        FEATURE_DEMO_INSPECT,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "history",
        FEATURE_DEMO_HISTORY,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_changelog_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(
        &mut module,
        "validate",
        FEATURE_CHANGELOG_VALIDATE,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "extract",
        FEATURE_CHANGELOG_EXTRACT,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_cache_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(
        &mut module,
        "inspect",
        FEATURE_CACHE_INSPECT,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "inspect",
        FEATURE_CACHE_INSPECT,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "invalidate",
        FEATURE_CACHE_INVALIDATE,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_gateway_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(
        &mut module,
        "status",
        FEATURE_GATEWAY_STATUS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_no_args(
        &mut module,
        "setup_tls",
        FEATURE_GATEWAY_SETUP_TLS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "setup_tls",
        FEATURE_GATEWAY_SETUP_TLS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_no_args(
        &mut module,
        "up",
        FEATURE_GATEWAY_UP,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "up",
        FEATURE_GATEWAY_UP,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_no_args(
        &mut module,
        "down",
        FEATURE_GATEWAY_DOWN,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "down",
        FEATURE_GATEWAY_DOWN,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_bundle_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(
        &mut module,
        "list",
        FEATURE_BUNDLE_LIST,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string(
        &mut module,
        "inspect",
        FEATURE_BUNDLE_INSPECT,
        "bundle",
        context.clone(),
        callbacks.clone(),
    );
    module_feature_two_strings(
        &mut module,
        "emit",
        FEATURE_BUNDLE_EMIT,
        ["bundle", "path"],
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_service_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(
        &mut module,
        "list",
        FEATURE_SERVICE_LIST,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string_options(
        &mut module,
        "extract",
        FEATURE_SERVICE_EXTRACT,
        "service",
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_catalog_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(
        &mut module,
        "tasks",
        FEATURE_CATALOG_TASKS,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "tasks",
        FEATURE_CATALOG_TASKS,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_doctor_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(
        &mut module,
        "run",
        FEATURE_DOCTOR_RUN,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "run",
        FEATURE_DOCTOR_RUN,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_contracts_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(
        &mut module,
        "check_json",
        FEATURE_CONTRACTS_CHECK_JSON,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "validate_selection",
        FEATURE_CONTRACTS_VALIDATE_SELECTION,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_unlock_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(
        &mut module,
        "scopes",
        FEATURE_UNLOCK_SCOPES,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_test_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(
        &mut module,
        "plan",
        FEATURE_TEST_PLAN,
        context.clone(),
        callbacks.clone(),
    );
    module
}

fn build_effigy_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    let effigy_context = context.clone();
    let effigy_callbacks = callbacks.clone();
    module.set_native_fn(
        "run",
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
    module.set_native_fn(
        "run_json",
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
    module
}
