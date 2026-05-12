use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::Utc;
use effigy_core::shell::shell_quote;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map};
use url::Url;

use crate::surface::{
    MODULE_JSON, MODULE_PATH, MODULE_RANDOM, MODULE_REGEX, MODULE_STR, MODULE_TIME, MODULE_TOML,
    MODULE_URL,
};

use super::{
    generate_jwt_env_keys_dynamic, generate_random_base64, resolve_runtime_path,
    rhai_runtime_error, ScriptContext,
};

pub(super) fn register_utility_modules(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_static_module(
        MODULE_TIME,
        std::rc::Rc::new(build_time_module(context.clone())),
    );
    engine.register_static_module(MODULE_PATH, std::rc::Rc::new(build_path_module()));
    engine.register_static_module(MODULE_URL, std::rc::Rc::new(build_url_module()));
    engine.register_static_module(
        MODULE_JSON,
        std::rc::Rc::new(build_json_module(context.clone())),
    );
    engine.register_static_module(
        MODULE_TOML,
        std::rc::Rc::new(build_toml_module(context.clone())),
    );
    engine.register_static_module(MODULE_STR, std::rc::Rc::new(build_str_module()));
    engine.register_static_module(MODULE_REGEX, std::rc::Rc::new(build_regex_module()));
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

fn build_url_module() -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "parse",
        |raw: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            let url =
                Url::parse(raw.as_str()).map_err(|error| rhai_runtime_error(error.to_string()))?;
            Ok(url_to_map(&url))
        },
    );
    module.set_native_fn(
        "query_get",
        |raw: ImmutableString, key: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let url =
                Url::parse(raw.as_str()).map_err(|error| rhai_runtime_error(error.to_string()))?;
            Ok(url
                .query_pairs()
                .find(|(candidate, _)| candidate == key.as_str())
                .map(|(_, value)| value.to_string())
                .unwrap_or_default())
        },
    );
    module.set_native_fn(
        "parse_mysql_dsn",
        |raw: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            parse_database_dsn(raw.as_str())
        },
    );
    module.set_native_fn(
        "parse_pg_dsn",
        |raw: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            parse_database_dsn(raw.as_str())
        },
    );
    module
}

fn parse_database_dsn(raw: &str) -> Result<Map, Box<EvalAltResult>> {
    let url = Url::parse(raw).map_err(|error| rhai_runtime_error(error.to_string()))?;
    let mut value = url_to_map(&url);
    let mut database = String::new();
    if let Some(mut segments) = url.path_segments() {
        if let Some(first) = segments.next() {
            database = first.to_owned();
        }
    } else {
        database = url.path().trim_start_matches('/').to_owned();
    }
    value.insert("database".into(), database.into());
    Ok(value)
}

fn url_to_map(url: &Url) -> Map {
    let mut query = Map::new();
    for (key, value) in url.query_pairs() {
        query.insert(key.to_string().into(), value.to_string().into());
    }

    let segments = url
        .path_segments()
        .map(|parts| {
            parts
                .filter(|part| !part.is_empty())
                .map(|part| part.to_owned().into())
                .collect::<Array>()
        })
        .unwrap_or_default();

    let mut value = Map::new();
    value.insert("scheme".into(), url.scheme().into());
    value.insert("username".into(), url.username().into());
    value.insert(
        "password".into(),
        url.password().map(str::to_owned).unwrap_or_default().into(),
    );
    value.insert("host".into(), url.host_str().unwrap_or_default().into());
    value.insert("port".into(), i64::from(url.port().unwrap_or(0)).into());
    value.insert("path".into(), url.path().into());
    value.insert("path_segments".into(), segments.into());
    value.insert(
        "query_string".into(),
        url.query().map(str::to_owned).unwrap_or_default().into(),
    );
    value.insert("query".into(), query.into());
    value.insert(
        "fragment".into(),
        url.fragment().map(str::to_owned).unwrap_or_default().into(),
    );
    value
}

fn build_json_module(context: Arc<ScriptContext>) -> rhai::Module {
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
    module.set_native_fn(
        "stringify_compact",
        |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
            let decoded: serde_json::Value = rhai::serde::from_dynamic(&value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            serde_json::to_string(&decoded).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "read_file",
        move |path: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let raw = std::fs::read_to_string(&path).map_err(|error| {
                rhai_runtime_error(format!("failed to read {}: {error}", path.display()))
            })?;
            let value: serde_json::Value = serde_json::from_str(&raw)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "write_file",
        move |path: ImmutableString, value: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    rhai_runtime_error(format!("failed to create {}: {error}", parent.display()))
                })?;
            }
            let decoded: serde_json::Value = rhai::serde::from_dynamic(&value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            let rendered = serde_json::to_string_pretty(&decoded)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            std::fs::write(&path, rendered).map_err(|error| {
                rhai_runtime_error(format!("failed to write {}: {error}", path.display()))
            })
        },
    );
    module
}

fn build_toml_module(context: Arc<ScriptContext>) -> rhai::Module {
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
    let file_context = context.clone();
    module.set_native_fn(
        "read_file",
        move |path: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let raw = std::fs::read_to_string(&path).map_err(|error| {
                rhai_runtime_error(format!("failed to read {}: {error}", path.display()))
            })?;
            let value: toml::Value =
                toml::from_str(&raw).map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "write_file",
        move |path: ImmutableString, value: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    rhai_runtime_error(format!("failed to create {}: {error}", parent.display()))
                })?;
            }
            let decoded: toml::Value = rhai::serde::from_dynamic(&value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            let rendered = toml::to_string_pretty(&decoded)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            std::fs::write(&path, rendered).map_err(|error| {
                rhai_runtime_error(format!("failed to write {}: {error}", path.display()))
            })
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
        "parse_int",
        |value: Dynamic| -> Result<i64, Box<EvalAltResult>> {
            value
                .to_string()
                .trim()
                .parse::<i64>()
                .map_err(|error| rhai_runtime_error(error.to_string()))
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

fn build_regex_module() -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "is_match",
        |pattern: ImmutableString, value: Dynamic| -> Result<bool, Box<EvalAltResult>> {
            let matcher = regex::Regex::new(pattern.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            if value.is_unit() {
                Ok(false)
            } else {
                Ok(matcher.is_match(&value.to_string()))
            }
        },
    );
    module.set_native_fn(
        "replace",
        |pattern: ImmutableString,
         value: Dynamic,
         replacement: ImmutableString|
         -> Result<String, Box<EvalAltResult>> {
            let matcher = regex::Regex::new(pattern.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            if value.is_unit() {
                Ok(String::new())
            } else {
                Ok(matcher
                    .replace_all(&value.to_string(), replacement.as_str())
                    .to_string())
            }
        },
    );
    module.set_native_fn(
        "captures",
        |pattern: ImmutableString, value: Dynamic| -> Result<Map, Box<EvalAltResult>> {
            let matcher = regex::Regex::new(pattern.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            let mut result = Map::new();
            if value.is_unit() {
                result.insert("matched".into(), false.into());
                result.insert("groups".into(), Array::new().into());
                result.insert("named".into(), Map::new().into());
                return Ok(result);
            }

            let rendered = value.to_string();
            let Some(captures) = matcher.captures(&rendered) else {
                result.insert("matched".into(), false.into());
                result.insert("groups".into(), Array::new().into());
                result.insert("named".into(), Map::new().into());
                return Ok(result);
            };

            let groups = captures
                .iter()
                .map(|entry| match entry {
                    Some(value) => value.as_str().to_owned().into(),
                    None => ().into(),
                })
                .collect::<Array>();
            let mut named = Map::new();
            for name in matcher.capture_names().flatten() {
                let value = captures
                    .name(name)
                    .map(|entry| entry.as_str().to_owned().into())
                    .unwrap_or_else(|| ().into());
                named.insert(name.into(), value);
            }
            result.insert("matched".into(), true.into());
            result.insert("groups".into(), groups.into());
            result.insert("named".into(), named.into());
            Ok(result)
        },
    );
    module.set_native_fn(
        "escape",
        |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
            if value.is_unit() {
                Ok(regex::escape(""))
            } else {
                Ok(regex::escape(&value.to_string()))
            }
        },
    );
    module
}
