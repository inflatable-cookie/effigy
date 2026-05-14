use std::path::{Path, PathBuf};
use std::time::Duration;

use rhai::{Dynamic, EvalAltResult, Map};
use serde_json::{json, Value};

pub(crate) fn search_files(
    root: &Path,
    pattern: &str,
    options: Map,
) -> Result<Map, Box<EvalAltResult>> {
    let options = crate::map_to_json_object(options)?;
    let glob = crate::json_object_string_option(&options, "glob")?;
    let literal = crate::json_object_bool_option(&options, "literal")?.unwrap_or(false);
    let matcher = if literal {
        None
    } else {
        Some(
            regex::Regex::new(pattern)
                .map_err(|error| crate::rhai_runtime_error(error.to_string()))?,
        )
    };
    let mut matches = Vec::<Value>::new();
    for path in search_candidate_files(root, glob.as_deref())? {
        let contents = std::fs::read_to_string(&path)
            .map_err(|error| crate::rhai_runtime_error(crate::failed_to_read_path(&path, error)))?;
        for (index, line) in contents.lines().enumerate() {
            let matched = if let Some(matcher) = &matcher {
                matcher.is_match(line)
            } else {
                line.contains(pattern)
            };
            if matched {
                matches.push(json!({
                    "path": path.display().to_string(),
                    "line": index + 1,
                    "text": line,
                }));
            }
        }
    }

    let stdout = matches
        .iter()
        .filter_map(|entry| {
            Some(format!(
                "{}:{}:{}",
                entry.get("path")?.as_str()?,
                entry.get("line")?.as_u64()?,
                entry.get("text")?.as_str()?
            ))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut map = Map::new();
    map.insert(
        "status".into(),
        Dynamic::from_int(if matches.is_empty() { 1 } else { 0 }),
    );
    map.insert("success".into(), Dynamic::from_bool(!matches.is_empty()));
    map.insert(
        "count".into(),
        Dynamic::from_int(i64::try_from(matches.len()).unwrap_or(i64::MAX)),
    );
    map.insert("stdout".into(), stdout.into());
    map.insert("stderr".into(), String::new().into());
    map.insert(
        "matches".into(),
        rhai::serde::to_dynamic(Value::Array(matches))
            .map_err(|error| crate::rhai_runtime_error(error.to_string()))?,
    );
    Ok(map)
}

fn search_candidate_files(
    root: &Path,
    glob: Option<&str>,
) -> Result<Vec<PathBuf>, Box<EvalAltResult>> {
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    if !root.is_dir() {
        return Err(crate::rhai_runtime_error(format!(
            "search root not found: {}",
            root.display()
        )));
    }
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(glob) = glob {
            if !path_matches_simple_glob(path, glob) {
                continue;
            }
        }
        files.push(path.to_path_buf());
    }
    files.sort();
    Ok(files)
}

fn path_matches_simple_glob(path: &Path, glob: &str) -> bool {
    let rendered = path.display().to_string();
    if let Some(suffix) = glob.strip_prefix('*') {
        return rendered.ends_with(suffix);
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == glob)
}

pub(crate) fn run_http_request(
    method: &str,
    url: &str,
    options: Map,
) -> Result<Map, Box<EvalAltResult>> {
    let options = crate::map_to_json_object(options)?;
    let timeout_ms = crate::json_object_usize_option(&options, "timeout_ms")?.unwrap_or(30_000);
    let mut builder =
        reqwest::blocking::Client::builder().timeout(Duration::from_millis(timeout_ms as u64));
    if crate::json_object_bool_option(&options, "danger_accept_invalid_certs")?.unwrap_or(false) {
        builder = builder.danger_accept_invalid_certs(true);
    }
    let client = builder
        .build()
        .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
    let mut request = client.request(method, url);
    if let Some(headers) = options.get("headers") {
        let headers = headers.as_object().ok_or_else(|| {
            crate::rhai_runtime_error("`headers` must be a map of string names to string values")
        })?;
        for (name, value) in headers {
            let value = value
                .as_str()
                .ok_or_else(|| crate::rhai_runtime_error("`headers` values must be strings"))?;
            request = request.header(name, value);
        }
    }
    if let Some(body) = options.get("body") {
        let body = body
            .as_str()
            .ok_or_else(|| crate::rhai_runtime_error("`body` must be a string"))?;
        request = request.body(body.to_owned());
    }
    if let Some(json_body) = options.get("json") {
        let body = serde_json::to_string(json_body)
            .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
        request = request
            .header("content-type", "application/json")
            .body(body);
    }
    let response = request
        .send()
        .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
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
        .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
    let mut map = Map::new();
    map.insert(
        "status".into(),
        Dynamic::from_int(i64::from(status.as_u16())),
    );
    map.insert("success".into(), Dynamic::from_bool(status.is_success()));
    map.insert("body".into(), body.into());
    map.insert(
        "headers".into(),
        rhai::serde::to_dynamic(Value::Object(headers))
            .map_err(|error| crate::rhai_runtime_error(error.to_string()))?,
    );
    Ok(map)
}
