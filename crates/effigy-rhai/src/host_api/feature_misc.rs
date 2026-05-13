use std::sync::Arc;

use effigy_core::build_info;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map};

use crate::surface::*;

use super::{
    dynamic_array_to_strings, effigy_result_map, map_to_json, module_feature_no_args,
    module_feature_options, module_feature_string_options, rhai_runtime_error, HostCallbacks,
    ScriptContext,
};

pub(super) fn register_misc_feature_modules(
    engine: &mut Engine,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    engine.register_static_module(
        MODULE_ARTIFACT,
        std::rc::Rc::new(build_artifact_module(context.clone(), callbacks.clone())),
    );
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
        std::rc::Rc::new(build_effigy_module(context.clone(), callbacks)),
    );
    engine.register_static_module(
        MODULE_SECRETS,
        std::rc::Rc::new(build_secrets_module(context)),
    );
}

fn build_artifact_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_string_options(
        &mut module,
        "inspect",
        FEATURE_ARTIFACT_INSPECT,
        "source",
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string_options(
        &mut module,
        "stage",
        FEATURE_ARTIFACT_STAGE,
        "source",
        context.clone(),
        callbacks.clone(),
    );
    let capture_context = context.clone();
    let capture_callbacks = callbacks.clone();
    module.set_native_fn(
        "capture",
        move |source: rhai::ImmutableString,
              destination: rhai::ImmutableString|
              -> Result<Dynamic, Box<rhai::EvalAltResult>> {
            super::run_feature_dynamic(
                &capture_context,
                &capture_callbacks,
                FEATURE_ARTIFACT_CAPTURE,
                serde_json::json!({
                    "source": source.as_str(),
                    "destination": destination.as_str(),
                }),
            )
        },
    );
    module.set_native_fn(
        "capture",
        move |source: rhai::ImmutableString,
              destination: rhai::ImmutableString,
              options: Map|
              -> Result<Dynamic, Box<rhai::EvalAltResult>> {
            let mut options = match map_to_json(options)? {
                serde_json::Value::Object(options) => options,
                _ => unreachable!("Rhai map_to_json must produce a JSON object"),
            };
            options.insert("source".to_owned(), serde_json::json!(source.as_str()));
            options.insert(
                "destination".to_owned(),
                serde_json::json!(destination.as_str()),
            );
            super::run_feature_dynamic(
                &context,
                &callbacks,
                FEATURE_ARTIFACT_CAPTURE,
                serde_json::Value::Object(options),
            )
        },
    );
    module
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
    module.set_native_fn(
        "provider_context",
        || -> Result<Dynamic, Box<EvalAltResult>> {
            let path = required_deploy_env("EFFIGY_DEPLOY_PROVIDER_CONTEXT")?;
            let contents = std::fs::read_to_string(&path).map_err(|error| {
                rhai_runtime_error(format!(
                    "failed to read deploy provider context `{path}`: {error}"
                ))
            })?;
            let value = serde_json::from_str::<serde_json::Value>(&contents).map_err(|error| {
                rhai_runtime_error(format!(
                    "failed to parse deploy provider context `{path}`: {error}"
                ))
            })?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module.set_native_fn(
        "provider_context_path",
        || -> Result<String, Box<EvalAltResult>> {
            required_deploy_env("EFFIGY_DEPLOY_PROVIDER_CONTEXT")
        },
    );
    module.set_native_fn(
        "provider_report_path",
        || -> Result<String, Box<EvalAltResult>> {
            required_deploy_env("EFFIGY_DEPLOY_PROVIDER_REPORT")
        },
    );
    module.set_native_fn(
        "provider_report",
        |report: Map| -> Result<Dynamic, Box<EvalAltResult>> {
            let path = required_deploy_env("EFFIGY_DEPLOY_PROVIDER_REPORT")?;
            let value = map_to_json(report)?;
            let pretty = serde_json::to_string_pretty(&value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            std::fs::write(&path, pretty).map_err(|error| {
                rhai_runtime_error(format!(
                    "failed to write deploy provider report `{path}`: {error}"
                ))
            })?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module
}

fn required_deploy_env(name: &str) -> Result<String, Box<EvalAltResult>> {
    std::env::var(name).map_err(|_| rhai_runtime_error(format!("missing {name}")))
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
        "inspect",
        FEATURE_BUNDLE_INSPECT,
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
    module.set_native_fn(
        "active_version",
        || -> Result<String, Box<EvalAltResult>> { Ok(build_info::active_version()) },
    );
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

fn build_secrets_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    let get_context = context.clone();
    module.set_native_fn(
        "get",
        move |name: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            super::active_rhai_secret(&get_context.repo_root, name.as_str())
        },
    );
    let has_context = context.clone();
    module.set_native_fn(
        "has",
        move |name: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            super::active_rhai_has_secret(&has_context.repo_root, name.as_str())
        },
    );
    let set_context = context.clone();
    module.set_native_fn(
        "set",
        move |name: ImmutableString, value: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            super::active_rhai_set_secret(&set_context.repo_root, name.as_str(), value.as_str())
        },
    );
    module.set_native_fn(
        "set_many",
        move |values: Map| -> Result<(), Box<EvalAltResult>> {
            super::active_rhai_set_secrets(&context.repo_root, values)
        },
    );
    module
}
