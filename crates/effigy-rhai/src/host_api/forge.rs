use std::process::{Command as ProcessCommand, Stdio};
use std::sync::Arc;

use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map};
use serde_json::Value;

use crate::surface::MODULE_FORGE;
use crate::{map_to_json_object, process_result_map};

use super::{rhai_runtime_error, ScriptContext};

const DEFAULT_PR_FIELDS: &str = "number,title,state,url,headRefName,baseRefName,author";

pub(super) fn register_forge_module(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_static_module(MODULE_FORGE, std::rc::Rc::new(build_forge_module(context)));
}

fn build_forge_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    let forge_context = context.clone();
    module.set_native_fn("provider", move || -> Result<String, Box<EvalAltResult>> {
        Ok(resolve_provider(&forge_context, None))
    });
    let forge_context = context.clone();
    module.set_native_fn(
        "provider",
        move |options: Map| -> Result<String, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            Ok(resolve_provider(&forge_context, Some(&options)))
        },
    );
    let forge_context = context.clone();
    module.set_native_fn("status", move || -> Result<Map, Box<EvalAltResult>> {
        forge_status(&forge_context, None)
    });
    let forge_context = context.clone();
    module.set_native_fn(
        "status",
        move |options: Map| -> Result<Map, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            forge_status(&forge_context, Some(&options))
        },
    );
    let forge_context = context.clone();
    module.set_native_fn(
        "pr_view",
        move |options: Map| -> Result<Dynamic, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            run_pr_view(&forge_context, &options)
        },
    );
    let forge_context = context.clone();
    module.set_native_fn(
        "pr_list",
        move |options: Map| -> Result<Array, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            run_pr_list(&forge_context, &options)
        },
    );
    let forge_context = context.clone();
    module.set_native_fn(
        "pr_create",
        move |options: Map| -> Result<Map, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            run_pr_create(&forge_context, &options)
        },
    );
    let forge_context = context.clone();
    module.set_native_fn(
        "pr_checkout",
        move |number: i64| -> Result<Map, Box<EvalAltResult>> {
            run_pr_checkout(&forge_context, number, None)
        },
    );
    let forge_context = context;
    module.set_native_fn(
        "pr_checkout",
        move |number: i64, options: Map| -> Result<Map, Box<EvalAltResult>> {
            let options = map_to_json_object(options)?;
            run_pr_checkout(&forge_context, number, Some(&options))
        },
    );
    module
}

fn forge_status(
    context: &ScriptContext,
    options: Option<&serde_json::Map<String, Value>>,
) -> Result<Map, Box<EvalAltResult>> {
    let provider = resolve_provider(context, options);
    let remote_url = default_remote_url(context).unwrap_or_default();
    let adapter = if provider == "github" { "gh" } else { "" };
    let available = provider == "github" && command_success(context, "gh", &["--version"]);
    let authenticated = available && command_success(context, "gh", &["auth", "status"]);

    let mut map = Map::new();
    map.insert("provider".into(), provider.into());
    map.insert("remote_url".into(), remote_url.into());
    map.insert("adapter".into(), adapter.into());
    map.insert("available".into(), Dynamic::from_bool(available));
    map.insert("authenticated".into(), Dynamic::from_bool(authenticated));
    Ok(map)
}

fn run_pr_view(
    context: &ScriptContext,
    options: &serde_json::Map<String, Value>,
) -> Result<Dynamic, Box<EvalAltResult>> {
    require_github_provider(context, options)?;
    let selector = string_option(options, "number")?.or(string_option(options, "selector")?);
    let fields = string_option(options, "fields")?.unwrap_or_else(|| DEFAULT_PR_FIELDS.to_owned());
    let mut args = vec!["pr".to_owned(), "view".to_owned()];
    if let Some(selector) = selector {
        args.push(selector);
    }
    args.push("--json".to_owned());
    args.push(fields);
    run_gh_json_owned(context, &args)
}

fn run_pr_list(
    context: &ScriptContext,
    options: &serde_json::Map<String, Value>,
) -> Result<Array, Box<EvalAltResult>> {
    require_github_provider(context, options)?;
    let fields = string_option(options, "fields")?.unwrap_or_else(|| DEFAULT_PR_FIELDS.to_owned());
    let mut args = vec![
        "pr".to_owned(),
        "list".to_owned(),
        "--json".to_owned(),
        fields,
    ];
    push_optional_flag(&mut args, options, "state", "--state")?;
    push_optional_flag(&mut args, options, "base", "--base")?;
    push_optional_flag(&mut args, options, "head", "--head")?;
    push_optional_flag(&mut args, options, "author", "--author")?;
    push_optional_flag(&mut args, options, "search", "--search")?;
    if let Some(limit) = number_option(options, "limit")? {
        args.push("--limit".to_owned());
        args.push(limit.to_string());
    }
    let value = run_gh_json_value_owned(context, &args)?;
    match json_to_dynamic(value) {
        dynamic if dynamic.is_array() => Ok(dynamic.cast::<Array>()),
        _ => Err(rhai_runtime_error("gh pr list did not return a JSON array")),
    }
}

fn run_pr_create(
    context: &ScriptContext,
    options: &serde_json::Map<String, Value>,
) -> Result<Map, Box<EvalAltResult>> {
    require_github_provider(context, options)?;
    let mut args = vec!["pr".to_owned(), "create".to_owned()];
    push_required_flag(&mut args, options, "title", "--title")?;
    push_required_flag(&mut args, options, "body", "--body")?;
    push_optional_flag(&mut args, options, "base", "--base")?;
    push_optional_flag(&mut args, options, "head", "--head")?;
    if bool_option(options, "draft")? {
        args.push("--draft".to_owned());
    }
    if bool_option(options, "web")? {
        args.push("--web".to_owned());
    }
    run_gh_map_owned(context, &args)
}

fn run_pr_checkout(
    context: &ScriptContext,
    number: i64,
    options: Option<&serde_json::Map<String, Value>>,
) -> Result<Map, Box<EvalAltResult>> {
    let empty_options = serde_json::Map::new();
    require_github_provider(context, options.unwrap_or(&empty_options))?;
    run_gh_map(context, &["pr", "checkout", &number.to_string()])
}

fn require_github_provider(
    context: &ScriptContext,
    options: &serde_json::Map<String, Value>,
) -> Result<(), Box<EvalAltResult>> {
    let provider = resolve_provider(context, Some(options));
    if provider == "github" {
        Ok(())
    } else {
        Err(rhai_runtime_error(format!(
            "unsupported forge provider `{provider}`"
        )))
    }
}

fn resolve_provider(
    context: &ScriptContext,
    options: Option<&serde_json::Map<String, Value>>,
) -> String {
    if let Some(Value::String(provider)) = options.and_then(|options| options.get("provider")) {
        return provider.clone();
    }
    default_remote_url(context)
        .map(|url| {
            if url.contains("github.com") {
                "github".to_owned()
            } else if url.contains("gitlab.com") {
                "gitlab".to_owned()
            } else {
                "unknown".to_owned()
            }
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

fn default_remote_url(context: &ScriptContext) -> Option<String> {
    let output = git_command(context)
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!url.is_empty()).then_some(url)
}

fn command_success(context: &ScriptContext, program: &str, args: &[&str]) -> bool {
    ProcessCommand::new(program)
        .args(args)
        .current_dir(&context.repo_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_gh_map(context: &ScriptContext, args: &[&str]) -> Result<Map, Box<EvalAltResult>> {
    let output = gh_command(context)
        .args(args)
        .output()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    Ok(process_result_map(output))
}

fn run_gh_map_owned(context: &ScriptContext, args: &[String]) -> Result<Map, Box<EvalAltResult>> {
    let output = gh_command(context)
        .args(args)
        .output()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    Ok(process_result_map(output))
}

fn run_gh_json_owned(
    context: &ScriptContext,
    args: &[String],
) -> Result<Dynamic, Box<EvalAltResult>> {
    let value = run_gh_json_value_owned(context, args)?;
    Ok(json_to_dynamic(value))
}

fn run_gh_json_value_owned(
    context: &ScriptContext,
    args: &[String],
) -> Result<Value, Box<EvalAltResult>> {
    let output = gh_command(context)
        .args(args)
        .output()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    if !output.status.success() {
        return Err(rhai_runtime_error(command_failure_detail(
            "gh", args, &output,
        )));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| rhai_runtime_error(error.to_string()))
}

fn gh_command(context: &ScriptContext) -> ProcessCommand {
    let mut command = ProcessCommand::new("gh");
    command.current_dir(&context.repo_root);
    command
}

fn git_command(context: &ScriptContext) -> ProcessCommand {
    let mut command = ProcessCommand::new("git");
    command.current_dir(&context.repo_root);
    command
}

fn command_failure_detail(program: &str, args: &[String], output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("{program} {} failed", args.join(" "))
    }
}

fn push_required_flag(
    args: &mut Vec<String>,
    options: &serde_json::Map<String, Value>,
    key: &str,
    flag: &str,
) -> Result<(), Box<EvalAltResult>> {
    let value = string_option(options, key)?
        .ok_or_else(|| rhai_runtime_error(format!("`{key}` is required")))?;
    args.push(flag.to_owned());
    args.push(value);
    Ok(())
}

fn push_optional_flag(
    args: &mut Vec<String>,
    options: &serde_json::Map<String, Value>,
    key: &str,
    flag: &str,
) -> Result<(), Box<EvalAltResult>> {
    if let Some(value) = string_option(options, key)? {
        args.push(flag.to_owned());
        args.push(value);
    }
    Ok(())
}

fn string_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Number(value)) => Ok(Some(value.to_string())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be a string"))),
    }
}

fn number_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<i64>, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::Number(value)) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| rhai_runtime_error(format!("`{key}` must be an integer"))),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be an integer"))),
    }
}

fn bool_option(
    options: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<bool, Box<EvalAltResult>> {
    match options.get(key) {
        Some(Value::Bool(value)) => Ok(*value),
        Some(Value::Null) | None => Ok(false),
        Some(_) => Err(rhai_runtime_error(format!("`{key}` must be a bool"))),
    }
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
