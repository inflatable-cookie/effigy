use std::process::Command as ProcessCommand;
use std::sync::Arc;

use rhai::{Array, Engine, EvalAltResult, ImmutableString, Map};

use crate::surface::MODULE_PROCESS;

use super::{
    configure_process_command, dynamic_array_to_strings, process_result_map,
    reject_recursive_effigy_process, rhai_runtime_error, run_process_streaming,
    run_process_streaming_with_options, run_process_teeing, run_process_teeing_with_options,
    with_local_node_bin_path, ScriptContext,
};

pub(super) fn register_process_module(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_static_module(
        MODULE_PROCESS,
        std::rc::Rc::new(build_process_module(context)),
    );
}

fn build_process_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    let process_context = context.clone();
    module.set_native_fn(
        "run",
        move |program: ImmutableString, args: Array| -> Result<Map, Box<EvalAltResult>> {
            reject_recursive_effigy_process(program.as_str())?;
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
    let process_context = context.clone();
    module.set_native_fn(
        "run",
        move |program: ImmutableString,
              args: Array,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            reject_recursive_effigy_process(program.as_str())?;
            let mut process = ProcessCommand::new(program.as_str());
            process.args(dynamic_array_to_strings(&args)?);
            let resolved_cwd =
                configure_process_command(&mut process, &process_context.cwd, Some(options))?;
            with_local_node_bin_path(&mut process, &resolved_cwd);
            let output = process
                .output()
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            Ok(process_result_map(output))
        },
    );
    let process_context = context.clone();
    module.set_native_fn(
        "stream",
        move |program: ImmutableString, args: Array| -> Result<Map, Box<EvalAltResult>> {
            reject_recursive_effigy_process(program.as_str())?;
            let args = dynamic_array_to_strings(&args)?;
            run_process_streaming(program.as_str(), &args, &process_context.cwd)
        },
    );
    let process_context = context.clone();
    module.set_native_fn(
        "stream",
        move |program: ImmutableString,
              args: Array,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            reject_recursive_effigy_process(program.as_str())?;
            let args = dynamic_array_to_strings(&args)?;
            run_process_streaming_with_options(
                program.as_str(),
                &args,
                &process_context.cwd,
                Some(options),
            )
        },
    );
    let process_context = context.clone();
    module.set_native_fn(
        "tee",
        move |program: ImmutableString, args: Array| -> Result<Map, Box<EvalAltResult>> {
            reject_recursive_effigy_process(program.as_str())?;
            let args = dynamic_array_to_strings(&args)?;
            run_process_teeing(program.as_str(), &args, &process_context.cwd)
        },
    );
    let process_context = context.clone();
    module.set_native_fn(
        "tee",
        move |program: ImmutableString,
              args: Array,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            reject_recursive_effigy_process(program.as_str())?;
            let args = dynamic_array_to_strings(&args)?;
            run_process_teeing_with_options(
                program.as_str(),
                &args,
                &process_context.cwd,
                Some(options),
            )
        },
    );
    module
}
