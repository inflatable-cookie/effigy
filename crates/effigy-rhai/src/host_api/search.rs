use std::sync::Arc;

use rhai::{Engine, EvalAltResult, ImmutableString, Map};

use crate::surface::MODULE_SEARCH;

use super::{resolve_runtime_path, search_files, ScriptContext};

pub(super) fn register_search_module(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_static_module(
        MODULE_SEARCH,
        std::rc::Rc::new(build_search_module(context)),
    );
}

fn build_search_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    let file_context = context.clone();
    module.set_native_fn(
        "files",
        move |root: ImmutableString,
              pattern: ImmutableString,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            let root = resolve_runtime_path(&file_context.cwd, root.as_str());
            search_files(&root, pattern.as_str(), options)
        },
    );
    module
}
