use std::sync::Arc;

use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, Map};

use crate::surface::MODULE_STATE;

use super::{
    map_to_json, module_feature_string_options, HostCallbacks, ScriptContext,
};

/// The `state::` module keeps only the plain data-artifact verbs. v0.16 retired
/// the layered stack helpers (`plan`, `apply`, `capture`, `capture_set`,
/// `history`, the capture/apply context readers and `capture_source`); a
/// repository now declares those as tasks.
/// See `docs/knowledge/deployment-data-workflows.md`.
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
    module_feature_string_options(
        &mut module,
        "artifact_inspect",
        crate::surface::FEATURE_STATE_ARTIFACT_INSPECT,
        "source",
        context.clone(),
        callbacks.clone(),
    );
    module_feature_string_options(
        &mut module,
        "artifact_stage",
        crate::surface::FEATURE_STATE_ARTIFACT_STAGE,
        "source",
        context.clone(),
        callbacks.clone(),
    );
    let capture_context = context.clone();
    let capture_callbacks = callbacks.clone();
    module.set_native_fn(
        "artifact_capture",
        move |source: ImmutableString,
              destination: ImmutableString|
              -> Result<Dynamic, Box<EvalAltResult>> {
            super::run_feature_dynamic(
                &capture_context,
                &capture_callbacks,
                crate::surface::FEATURE_STATE_ARTIFACT_CAPTURE,
                serde_json::json!({
                    "source": source.as_str(),
                    "destination": destination.as_str(),
                }),
            )
        },
    );
    module.set_native_fn(
        "artifact_capture",
        move |source: ImmutableString,
              destination: ImmutableString,
              options: Map|
              -> Result<Dynamic, Box<EvalAltResult>> {
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
                crate::surface::FEATURE_STATE_ARTIFACT_CAPTURE,
                serde_json::Value::Object(options),
            )
        },
    );
    module
}
