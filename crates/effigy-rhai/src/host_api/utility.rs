use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::Utc;
use effigy_core::shell::shell_quote;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString};

use crate::surface::{
    MODULE_JSON, MODULE_PATH, MODULE_RANDOM, MODULE_STR, MODULE_TIME, MODULE_TOML,
};

use super::{
    generate_jwt_env_keys_dynamic, generate_random_base64, rhai_runtime_error, ScriptContext,
};

pub(super) fn register_utility_modules(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_static_module(
        MODULE_TIME,
        std::rc::Rc::new(build_time_module(context.clone())),
    );
    engine.register_static_module(MODULE_PATH, std::rc::Rc::new(build_path_module()));
    engine.register_static_module(MODULE_JSON, std::rc::Rc::new(build_json_module()));
    engine.register_static_module(MODULE_TOML, std::rc::Rc::new(build_toml_module()));
    engine.register_static_module(MODULE_STR, std::rc::Rc::new(build_str_module()));
    engine.register_static_module(MODULE_RANDOM, std::rc::Rc::new(build_random_module()));
}

fn build_time_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn("now_utc", || -> Result<String, Box<EvalAltResult>> {
        Ok(Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string())
    });
    module.set_native_fn("process_id", || -> Result<i64, Box<EvalAltResult>> {
        Ok(i64::from(std::process::id()))
    });
    module.set_native_fn("sleep_ms", |millis: i64| {
        if millis > 0 {
            thread::sleep(Duration::from_millis(millis as u64));
        }
        Ok(())
    });
    let stop_context = context.clone();
    module.set_native_fn(
        "stop_requested",
        move || -> Result<bool, Box<EvalAltResult>> {
            Ok(stop_context.stop_requested.load(Ordering::Relaxed))
        },
    );
    module
}

fn build_path_module() -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "join",
        |base: ImmutableString, child: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            Ok(PathBuf::from(base.as_str())
                .join(child.as_str())
                .display()
                .to_string())
        },
    );
    module.set_native_fn(
        "file_name",
        |path: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            Ok(Path::new(path.as_str())
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default())
        },
    );
    module
}

fn build_json_module() -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "parse",
        |raw: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            let value: serde_json::Value = serde_json::from_str(raw.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module.set_native_fn(
        "stringify",
        |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
            let decoded: serde_json::Value = rhai::serde::from_dynamic(&value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            serde_json::to_string_pretty(&decoded)
                .map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module
}

fn build_toml_module() -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "parse",
        |raw: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            let value: toml::Value = toml::from_str(raw.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module.set_native_fn(
        "stringify",
        |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
            let decoded: toml::Value = rhai::serde::from_dynamic(&value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            toml::to_string_pretty(&decoded).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module
}

fn build_str_module() -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "trim",
        |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
            if value.is_unit() {
                Ok(String::new())
            } else {
                Ok(value.to_string().trim().to_owned())
            }
        },
    );
    module.set_native_fn(
        "contains",
        |value: Dynamic, needle: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok((!value.is_unit()) && value.to_string().contains(needle.as_str()))
        },
    );
    module.set_native_fn(
        "starts_with",
        |value: Dynamic, prefix: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok((!value.is_unit()) && value.to_string().starts_with(prefix.as_str()))
        },
    );
    module.set_native_fn(
        "ends_with",
        |value: Dynamic, suffix: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok((!value.is_unit()) && value.to_string().ends_with(suffix.as_str()))
        },
    );
    module.set_native_fn(
        "replace",
        |value: Dynamic,
         from: ImmutableString,
         to: ImmutableString|
         -> Result<String, Box<EvalAltResult>> {
            if value.is_unit() {
                Ok(String::new())
            } else {
                Ok(value.to_string().replace(from.as_str(), to.as_str()))
            }
        },
    );
    module.set_native_fn(
        "split_lines",
        |value: Dynamic| -> Result<Array, Box<EvalAltResult>> {
            if value.is_unit() {
                Ok(Array::new())
            } else {
                Ok(value
                    .to_string()
                    .lines()
                    .map(|line| line.to_owned().into())
                    .collect())
            }
        },
    );
    module.set_native_fn(
        "shell_quote",
        |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
            if value.is_unit() {
                Ok(shell_quote(""))
            } else {
                Ok(shell_quote(&value.to_string()))
            }
        },
    );
    module
}

fn build_random_module() -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn("jwt_env_keys", || -> Result<Dynamic, Box<EvalAltResult>> {
        generate_jwt_env_keys_dynamic()
    });
    module.set_native_fn(
        "base64",
        |size: i64| -> Result<String, Box<EvalAltResult>> { generate_random_base64(size) },
    );
    module
}
