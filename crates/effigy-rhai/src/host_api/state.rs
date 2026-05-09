use std::sync::Arc;

use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, Map};
use serde_json::Value;

use crate::surface::MODULE_STATE;

use super::{
    module_feature_no_args, module_feature_options, module_feature_string, rhai_runtime_error,
    run_feature_dynamic, HostCallbacks, ScriptContext,
};

pub(super) fn register_state_module(
    engine: &mut Engine,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    engine.register_static_module(
        MODULE_STATE,
        std::rc::Rc::new(build_state_module(context, callbacks)),
    );
}

fn build_state_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(
        &mut module,
        "plan",
        crate::surface::FEATURE_STATE_PLAN,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string(
        &mut module,
        "plan",
        crate::surface::FEATURE_STATE_PLAN,
        "stack",
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "plan",
        crate::surface::FEATURE_STATE_PLAN,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_no_args(
        &mut module,
        "apply",
        crate::surface::FEATURE_STATE_APPLY,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string(
        &mut module,
        "apply",
        crate::surface::FEATURE_STATE_APPLY,
        "stack",
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "apply",
        crate::surface::FEATURE_STATE_APPLY,
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string(
        &mut module,
        "history",
        crate::surface::FEATURE_STATE_HISTORY,
        "stack",
        context.clone(),
        callbacks.clone(),
    );
    module_feature_options(
        &mut module,
        "history",
        crate::surface::FEATURE_STATE_HISTORY,
        context.clone(),
        callbacks.clone(),
    );
    let capture_context = context.clone();
    let capture_callbacks = callbacks.clone();
    module.set_native_fn(
        "capture",
        move |stack: ImmutableString,
              profile: ImmutableString|
              -> Result<Dynamic, Box<EvalAltResult>> {
            run_feature_dynamic(
                &capture_context,
                &capture_callbacks,
                crate::surface::FEATURE_STATE_CAPTURE,
                serde_json::json!({
                    "stack": stack.as_str(),
                    "profile": profile.as_str(),
                }),
            )
        },
    );
    module_feature_options(
        &mut module,
        "capture",
        crate::surface::FEATURE_STATE_CAPTURE,
        context.clone(),
        callbacks.clone(),
    );
    module.set_native_fn(
        "capture_context",
        || -> Result<Dynamic, Box<EvalAltResult>> {
            let path = required_state_env("EFFIGY_STATE_CAPTURE_CONTEXT")?;
            let contents = std::fs::read_to_string(&path).map_err(|error| {
                rhai_runtime_error(format!(
                    "failed to read state capture context `{path}`: {error}"
                ))
            })?;
            let value = serde_json::from_str::<Value>(&contents).map_err(|error| {
                rhai_runtime_error(format!(
                    "failed to parse state capture context `{path}`: {error}"
                ))
            })?;
            Ok(json_to_dynamic(value))
        },
    );
    module.set_native_fn(
        "capture_context_path",
        || -> Result<String, Box<EvalAltResult>> {
            required_state_env("EFFIGY_STATE_CAPTURE_CONTEXT")
        },
    );
    module.set_native_fn(
        "capture_source",
        || -> Result<String, Box<EvalAltResult>> {
            required_state_env("EFFIGY_STATE_CAPTURE_SOURCE")
        },
    );
    module.set_native_fn(
        "capture_destination_ref",
        || -> Result<String, Box<EvalAltResult>> {
            Ok(std::env::var("EFFIGY_STATE_CAPTURE_DESTINATION_REF").unwrap_or_default())
        },
    );
    module
}

fn required_state_env(name: &str) -> Result<String, Box<EvalAltResult>> {
    std::env::var(name).map_err(|_| rhai_runtime_error(format!("missing {name}")))
}

fn json_to_dynamic(value: Value) -> Dynamic {
    match value {
        Value::Null => ().into(),
        Value::Bool(value) => value.into(),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                value.into()
            } else if let Some(value) = value.as_u64() {
                if value <= i64::MAX as u64 {
                    (value as i64).into()
                } else {
                    value.to_string().into()
                }
            } else if let Some(value) = value.as_f64() {
                value.into()
            } else {
                ().into()
            }
        }
        Value::String(value) => ImmutableString::from(value).into(),
        Value::Array(values) => values
            .into_iter()
            .map(json_to_dynamic)
            .collect::<rhai::Array>()
            .into(),
        Value::Object(values) => {
            let mut map = Map::new();
            for (key, value) in values {
                map.insert(key.into(), json_to_dynamic(value));
            }
            map.into()
        }
    }
}
