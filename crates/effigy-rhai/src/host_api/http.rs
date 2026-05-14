use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use effigy_core::path_error_text::failed_to_write_path;
use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, Map};
use serde_json::Value;

use crate::surface::MODULE_HTTP;

use super::{resolve_runtime_path, rhai_runtime_error, ScriptContext};
use crate::network_support::run_http_request;

pub(super) fn register_http_module(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_static_module(MODULE_HTTP, std::rc::Rc::new(build_http_module(context)));
}

fn build_http_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "request",
        move |method: ImmutableString,
              url: ImmutableString,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            run_http_request(method.as_str(), url.as_str(), options)
        },
    );
    module.set_native_fn(
        "get",
        move |url: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            run_http_request("GET", url.as_str(), Map::new())
        },
    );
    module.set_native_fn(
        "post",
        move |url: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            run_http_request("POST", url.as_str(), Map::new())
        },
    );
    module.set_native_fn(
        "post",
        move |url: ImmutableString, body: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            let mut options = Map::new();
            options.insert("body".into(), body.into());
            run_http_request("POST", url.as_str(), options)
        },
    );
    module.set_native_fn(
        "post",
        move |url: ImmutableString, options: Map| -> Result<Map, Box<EvalAltResult>> {
            run_http_request("POST", url.as_str(), options)
        },
    );
    let download_context = context.clone();
    module.set_native_fn(
        "download",
        move |url: ImmutableString, path: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            download_http_to_path(
                &download_context.cwd,
                url.as_str(),
                path.as_str(),
                Map::new(),
            )
        },
    );
    let download_context = context.clone();
    module.set_native_fn(
        "download",
        move |url: ImmutableString,
              path: ImmutableString,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            download_http_to_path(&download_context.cwd, url.as_str(), path.as_str(), options)
        },
    );
    let capture_context = context.clone();
    module.set_native_fn(
        "capture",
        move |method: ImmutableString,
              url: ImmutableString,
              path: ImmutableString,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            capture_http_to_path(
                &capture_context.cwd,
                method.as_str(),
                url.as_str(),
                path.as_str(),
                options,
            )
        },
    );
    module
}

fn capture_http_to_path(
    cwd: &Path,
    method: &str,
    url: &str,
    path: &str,
    options: Map,
) -> Result<Map, Box<EvalAltResult>> {
    let options = rhai_map_to_json_object(options)?;
    let timeout_ms = json_object_usize_option(&options, "timeout_ms")?.unwrap_or(30_000);
    let mut builder =
        reqwest::blocking::Client::builder().timeout(Duration::from_millis(timeout_ms as u64));
    if json_object_bool_option(&options, "danger_accept_invalid_certs")?.unwrap_or(false) {
        builder = builder.danger_accept_invalid_certs(true);
    }
    let client = builder
        .build()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let mut request = client.request(method, url);
    if let Some(headers) = options.get("headers") {
        let headers = headers.as_object().ok_or_else(|| {
            rhai_runtime_error("`headers` must be a map of string names to string values")
        })?;
        for (name, value) in headers {
            let value = value
                .as_str()
                .ok_or_else(|| rhai_runtime_error("`headers` values must be strings"))?;
            request = request.header(name, value);
        }
    }
    if let Some(body) = options.get("body") {
        let body = body
            .as_str()
            .ok_or_else(|| rhai_runtime_error("`body` must be a string"))?;
        request = request.body(body.to_owned());
    }
    let response = request
        .send()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                Value::String(value.to_str().unwrap_or_default().to_owned()),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    let body = response
        .text()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let path = resolve_runtime_path(cwd, path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
    }
    std::fs::write(&path, &body)
        .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;

    let mut map = Map::new();
    map.insert(
        "status".into(),
        Dynamic::from_int(i64::from(status.as_u16())),
    );
    map.insert("success".into(), Dynamic::from_bool(status.is_success()));
    map.insert("path".into(), path.display().to_string().into());
    map.insert("body".into(), body.into());
    map.insert(
        "headers".into(),
        rhai::serde::to_dynamic(Value::Object(headers))
            .map_err(|error| rhai_runtime_error(error.to_string()))?,
    );
    Ok(map)
}

fn download_http_to_path(
    cwd: &Path,
    url: &str,
    path: &str,
    options: Map,
) -> Result<Map, Box<EvalAltResult>> {
    let options = rhai_map_to_json_object(options)?;
    let timeout_ms = json_object_usize_option(&options, "timeout_ms")?.unwrap_or(30_000);
    let mut builder =
        reqwest::blocking::Client::builder().timeout(Duration::from_millis(timeout_ms as u64));
    if json_object_bool_option(&options, "danger_accept_invalid_certs")?.unwrap_or(false) {
        builder = builder.danger_accept_invalid_certs(true);
    }
    let client = builder
        .build()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let mut request = client.get(url);
    if let Some(headers) = options.get("headers") {
        let headers = headers.as_object().ok_or_else(|| {
            rhai_runtime_error("`headers` must be a map of string names to string values")
        })?;
        for (name, value) in headers {
            let value = value
                .as_str()
                .ok_or_else(|| rhai_runtime_error("`headers` values must be strings"))?;
            request = request.header(name, value);
        }
    }
    let response = request
        .send()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                Value::String(value.to_str().unwrap_or_default().to_owned()),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    let bytes = response
        .bytes()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    let path = resolve_runtime_path(cwd, path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
    }
    std::fs::write(&path, &bytes)
        .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;

    let mut map = Map::new();
    map.insert(
        "status".into(),
        Dynamic::from_int(i64::from(status.as_u16())),
    );
    map.insert("success".into(), Dynamic::from_bool(status.is_success()));
    map.insert("path".into(), path.display().to_string().into());
    map.insert(
        "size".into(),
        Dynamic::from_int(
            i64::try_from(bytes.len())
                .map_err(|_| rhai_runtime_error("download size exceeded Rhai integer range"))?,
        ),
    );
    map.insert(
        "headers".into(),
        rhai::serde::to_dynamic(Value::Object(headers))
            .map_err(|error| rhai_runtime_error(error.to_string()))?,
    );
    Ok(map)
}

fn rhai_map_to_json_object(
    options: Map,
) -> Result<serde_json::Map<String, Value>, Box<EvalAltResult>> {
    let value: Value = rhai::serde::from_dynamic(&Dynamic::from_map(options))
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| rhai_runtime_error("expected options map"))
}

fn json_object_bool_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| rhai_runtime_error(format!("`{key}` must be a boolean"))),
        None => Ok(None),
    }
}

fn json_object_usize_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<usize>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(value) => value
            .as_u64()
            .map(|value| value as usize)
            .map(Some)
            .ok_or_else(|| rhai_runtime_error(format!("`{key}` must be an unsigned integer"))),
        None => Ok(None),
    }
}
