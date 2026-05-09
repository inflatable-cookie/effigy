use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, Map};
use std::sync::Arc;

use crate::surface::*;

use super::{
    allocate_temp_dir, configure_process_command, dynamic_array_to_strings, effigy_result_map,
    emit_host_log, generate_jwt_env_keys_dynamic, generate_random_base64, host_command_output_map,
    map_to_json, module_feature_get_value, module_feature_no_args, module_feature_options,
    module_feature_string, module_feature_string_options, module_feature_two_strings,
    process_result_map, reject_recursive_effigy_process, resolve_runtime_path, rhai_runtime_error,
    run_feature_dynamic, run_http_request, run_process_streaming,
    run_process_streaming_with_options, run_process_teeing, run_process_teeing_with_options,
    search_files, with_local_node_bin_path, HostCallbacks, ScriptContext,
};

#[path = "host_api/container.rs"]
mod container;
#[path = "host_api/exec.rs"]
mod exec;
#[path = "host_api/feature_core.rs"]
mod feature_core;
#[path = "host_api/feature_misc.rs"]
mod feature_misc;
#[path = "host_api/fs.rs"]
mod fs;
#[path = "host_api/http.rs"]
mod http;
#[path = "host_api/process.rs"]
mod process;
#[path = "host_api/search.rs"]
mod search;
#[path = "host_api/state.rs"]
mod state;
#[path = "host_api/utility.rs"]
mod utility;

pub(super) fn register_host_api(
    engine: &mut Engine,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    // Flat registrations (most commonly used)
    engine.register_fn("log", |message: ImmutableString| {
        let _ = emit_host_log(message.as_str(), false);
    });
    engine.register_fn("log_warn", |message: ImmutableString| {
        let _ = emit_host_log(message.as_str(), true);
    });
    engine.register_fn("env", |name: ImmutableString| -> String {
        std::env::var(name.as_str()).unwrap_or_default()
    });

    // Register all modules
    utility::register_utility_modules(engine, context.clone());
    engine.register_static_module(
        MODULE_RUNTIME,
        std::rc::Rc::new(build_runtime_module(context.clone())),
    );
    fs::register_fs_module(engine, context.clone());
    process::register_process_module(engine, context.clone());
    exec::register_exec_module(engine, context.clone(), callbacks.clone());
    http::register_http_module(engine, context.clone());
    search::register_search_module(engine, context.clone());
    state::register_state_module(engine, context.clone(), callbacks.clone());
    feature_core::register_core_feature_modules(engine, context.clone(), callbacks.clone());
    container::register_container_module(engine, context.clone(), callbacks.clone());
    feature_misc::register_misc_feature_modules(engine, context.clone(), callbacks.clone());
}

// Module builders

fn build_runtime_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn("context", move || -> Result<Dynamic, Box<EvalAltResult>> {
        let runtime_context = super::active_runtime_context_for_script(&context)
            .map_err(|error| rhai_runtime_error(error.to_string()))?;
        let mut host = Map::new();
        host.insert("os".into(), runtime_context.host().os.clone().into());
        host.insert("arch".into(), runtime_context.host().arch.clone().into());
        host.insert("no_color".into(), runtime_context.host().no_color.into());
        host.insert("ci".into(), runtime_context.host().ci.into());

        let mut value = Map::new();
        value.insert(
            "invocation_cwd".into(),
            runtime_context
                .invocation_cwd()
                .display()
                .to_string()
                .into(),
        );
        value.insert(
            "command_root".into(),
            runtime_context.command_root().display().to_string().into(),
        );
        value.insert(
            "repo_override".into(),
            runtime_context
                .repo_override()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
                .into(),
        );
        value.insert(
            "invocation_mode".into(),
            match runtime_context.invocation_mode() {
                effigy_context::RuntimeInvocationMode::Host => "host",
                effigy_context::RuntimeInvocationMode::ContainerHandoff => "container_handoff",
            }
            .into(),
        );
        value.insert(
            "inside_container_handoff".into(),
            runtime_context.container().inside_container_handoff.into(),
        );
        value.insert("host".into(), host.into());
        Ok(value.into())
    });
    module
}
