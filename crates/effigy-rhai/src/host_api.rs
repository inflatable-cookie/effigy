use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::Utc;
use effigy_core::path_error_text::{failed_to_read_path, failed_to_write_path};
use effigy_core::shell::shell_quote;
use effigy_env::dotenv::parse_dotenv_entries;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map};
use serde_json::{json, Value};

use super::{
    allocate_temp_dir, configure_process_command, dynamic_array_to_strings, effigy_result_map,
    emit_host_log, generate_jwt_env_keys_dynamic, generate_random_base64, host_command_output_map,
    module_feature_get_value, module_feature_no_args, module_feature_options,
    module_feature_string, module_feature_string_options, module_feature_three_strings,
    module_feature_two_strings, process_result_map, reject_recursive_effigy_process,
    resolve_runtime_path, rhai_runtime_error, run_http_request, run_process_streaming,
    run_process_teeing, search_files, with_local_node_bin_path, HostCallbacks, ScriptContext,
};

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
    engine.register_static_module("time", std::rc::Rc::new(build_time_module(context.clone())));
    engine.register_static_module("path", std::rc::Rc::new(build_path_module()));
    engine.register_static_module("fs", std::rc::Rc::new(build_fs_module(context.clone())));
    engine.register_static_module("process", std::rc::Rc::new(build_process_module(context.clone())));
    engine.register_static_module("http", std::rc::Rc::new(build_http_module(context.clone())));
    engine.register_static_module("json", std::rc::Rc::new(build_json_module()));
    engine.register_static_module("toml", std::rc::Rc::new(build_toml_module()));
    engine.register_static_module("str", std::rc::Rc::new(build_str_module()));
    engine.register_static_module("random", std::rc::Rc::new(build_random_module()));
    engine.register_static_module("search", std::rc::Rc::new(build_search_module(context.clone())));
    engine.register_static_module("config", std::rc::Rc::new(build_config_module(context.clone(), callbacks.clone())));
    engine.register_static_module("task", std::rc::Rc::new(build_task_module(context.clone(), callbacks.clone())));
    engine.register_static_module("container", std::rc::Rc::new(build_container_module(context.clone(), callbacks.clone())));
    engine.register_static_module("scan", std::rc::Rc::new(build_scan_module(context.clone(), callbacks.clone())));
    engine.register_static_module("docs", std::rc::Rc::new(build_docs_module(context.clone(), callbacks.clone())));
    engine.register_static_module("deploy", std::rc::Rc::new(build_deploy_module(context.clone(), callbacks.clone())));
    engine.register_static_module("system", std::rc::Rc::new(build_system_module(context.clone(), callbacks.clone())));
    engine.register_static_module("demo", std::rc::Rc::new(build_demo_module(context.clone(), callbacks.clone())));
    engine.register_static_module("changelog", std::rc::Rc::new(build_changelog_module(context.clone(), callbacks.clone())));
    engine.register_static_module("cache", std::rc::Rc::new(build_cache_module(context.clone(), callbacks.clone())));
    engine.register_static_module("gateway", std::rc::Rc::new(build_gateway_module(context.clone(), callbacks.clone())));
    engine.register_static_module("bundle", std::rc::Rc::new(build_bundle_module(context.clone(), callbacks.clone())));
    engine.register_static_module("service", std::rc::Rc::new(build_service_module(context.clone(), callbacks.clone())));
    engine.register_static_module("catalog", std::rc::Rc::new(build_catalog_module(context.clone(), callbacks.clone())));
    engine.register_static_module("doctor", std::rc::Rc::new(build_doctor_module(context.clone(), callbacks.clone())));
    engine.register_static_module("contracts", std::rc::Rc::new(build_contracts_module(context.clone(), callbacks.clone())));
    engine.register_static_module("unlock", std::rc::Rc::new(build_unlock_module(context.clone(), callbacks.clone())));
    engine.register_static_module("test", std::rc::Rc::new(build_test_module(context.clone(), callbacks.clone())));
    engine.register_static_module("effigy", std::rc::Rc::new(build_effigy_module(context.clone(), callbacks.clone())));
}

// Module builders

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
    module.set_native_fn("stop_requested", move || -> Result<bool, Box<EvalAltResult>> {
        Ok(stop_context.stop_requested.load(Ordering::Relaxed))
    });
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
    module.set_native_fn("file_name", |path: ImmutableString| -> Result<String, Box<EvalAltResult>> {
        Ok(Path::new(path.as_str())
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default())
    });
    module
}

fn build_fs_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    let file_context = context.clone();
    module.set_native_fn(
        "read_file",
        move |path: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "read_lines",
        move |path: ImmutableString| -> Result<Array, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            Ok(contents.lines().map(|line| line.to_owned().into()).collect())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "write_file",
        move |path: ImmutableString, contents: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            std::fs::write(&path, contents.as_str())
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "append_file",
        move |path: ImmutableString, contents: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
            use std::io::Write;
            file.write_all(contents.as_bytes())
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "write_lines",
        move |path: ImmutableString, lines: Array| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            let rendered = dynamic_array_to_strings(&lines)
                .map_err(|error| rhai_runtime_error(error.to_string()))?
                .join("\n");
            let output = if rendered.is_empty() {
                String::new()
            } else {
                format!("{rendered}\n")
            };
            std::fs::write(&path, output)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "copy",
        move |source: ImmutableString, destination: ImmutableString| -> Result<i64, Box<EvalAltResult>> {
            let source = resolve_runtime_path(&file_context.cwd, source.as_str());
            let destination = resolve_runtime_path(&file_context.cwd, destination.as_str());
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            let copied = std::fs::copy(&source, &destination)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&destination, error)))?;
            i64::try_from(copied)
                .map_err(|_| rhai_runtime_error("copied file size exceeded Rhai integer range"))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "copy_if_missing",
        move |source: ImmutableString, destination: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let source = resolve_runtime_path(&file_context.cwd, source.as_str());
            let destination = resolve_runtime_path(&file_context.cwd, destination.as_str());
            if destination.exists() {
                return Ok(false);
            }
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            std::fs::copy(&source, &destination)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&destination, error)))?;
            Ok(true)
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "move_path",
        move |source: ImmutableString, destination: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let source = resolve_runtime_path(&file_context.cwd, source.as_str());
            let destination = resolve_runtime_path(&file_context.cwd, destination.as_str());
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            std::fs::rename(&source, &destination)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&destination, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "replace_in_file",
        move |path: ImmutableString, from: ImmutableString, to: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            let replaced = contents.replace(from.as_str(), to.as_str());
            if replaced == contents {
                return Ok(false);
            }
            std::fs::write(&path, replaced)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
            Ok(true)
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "exists",
        move |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok(resolve_runtime_path(&file_context.cwd, path.as_str()).exists())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "is_dir",
        move |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok(resolve_runtime_path(&file_context.cwd, path.as_str()).is_dir())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "is_file",
        move |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok(resolve_runtime_path(&file_context.cwd, path.as_str()).is_file())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "is_symlink",
        move |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::symlink_metadata(&path)
                .map(|metadata| metadata.file_type().is_symlink())
                .or_else(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        Ok(false)
                    } else {
                        Err(error)
                    }
                })
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "list",
        move |path: ImmutableString| -> Result<Array, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let mut entries = std::fs::read_dir(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?
                .map(|entry| {
                    entry
                        .map(|entry| entry.path().display().to_string())
                        .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
                })
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort();
            Ok(entries.into_iter().map(Into::into).collect())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "create_dir",
        move |path: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::create_dir_all(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "remove",
        move |path: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
                        std::fs::remove_dir_all(&path)
                    } else {
                        std::fs::remove_file(&path)
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
            .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "create_symlink",
        move |target: ImmutableString, link: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let target = resolve_runtime_path(&file_context.cwd, target.as_str());
            let link = resolve_runtime_path(&file_context.cwd, link.as_str());
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&target, &link)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(&link, error)))
            }
            #[cfg(not(unix))]
            {
                let _ = target;
                let _ = link;
                Err(rhai_runtime_error(
                    "Rhai symlink helpers are only supported on unix hosts".to_owned(),
                ))
            }
        },
    );
    module.set_native_fn(
        "make_temp_dir",
        move |prefix: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = allocate_temp_dir(prefix.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            std::fs::create_dir_all(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
            Ok(path.display().to_string())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_get",
        move |path: ImmutableString, key: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if !path.exists() {
                return Ok(String::new());
            }
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            Ok(parse_dotenv_entries(&contents)
                .get(key.as_str())
                .cloned()
                .unwrap_or_default())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_entries",
        move |path: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if !path.exists() {
                return Ok(Map::new());
            }
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            let mut map = Map::new();
            for (key, value) in parse_dotenv_entries(&contents) {
                map.insert(key.into(), value.into());
            }
            Ok(map)
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_set",
        move |path: ImmutableString,
              key: ImmutableString,
              value: ImmutableString|
              -> Result<bool, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let changed = update_env_file_entry(&path, key.as_str(), value.as_str())?;
            Ok(changed)
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_remove",
        move |path: ImmutableString, key: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            remove_env_file_entry(&path, key.as_str())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_get_detail",
        move |path: ImmutableString, key: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let mut map = Map::new();
            if !path.exists() {
                map.insert("file_exists".into(), Dynamic::from_bool(false));
                map.insert("key_exists".into(), Dynamic::from_bool(false));
                map.insert("value".into(), Dynamic::from(""));
                return Ok(map);
            }
            map.insert("file_exists".into(), Dynamic::from_bool(true));
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            let entries = parse_dotenv_entries(&contents);
            if let Some(value) = entries.get(key.as_str()) {
                map.insert("key_exists".into(), Dynamic::from_bool(true));
                map.insert("value".into(), Dynamic::from(value.clone()));
            } else {
                map.insert("key_exists".into(), Dynamic::from_bool(false));
                map.insert("value".into(), Dynamic::from(""));
            }
            Ok(map)
        },
    );
    module
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
        move |program: ImmutableString, args: Array, options: Map| -> Result<Map, Box<EvalAltResult>> {
            reject_recursive_effigy_process(program.as_str())?;
            let mut process = ProcessCommand::new(program.as_str());
            process.args(dynamic_array_to_strings(&args)?);
            let resolved_cwd = configure_process_command(&mut process, &process_context.cwd, Some(options))?;
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
        move |program: ImmutableString, args: Array, options: Map| -> Result<Map, Box<EvalAltResult>> {
            reject_recursive_effigy_process(program.as_str())?;
            let args = dynamic_array_to_strings(&args)?;
            super::run_process_streaming_with_options(
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
        move |program: ImmutableString, args: Array, options: Map| -> Result<Map, Box<EvalAltResult>> {
            reject_recursive_effigy_process(program.as_str())?;
            let args = dynamic_array_to_strings(&args)?;
            super::run_process_teeing_with_options(
                program.as_str(),
                &args,
                &process_context.cwd,
                Some(options),
            )
        },
    );
    module
}

fn build_http_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "request",
        move |method: ImmutableString, url: ImmutableString, options: Map| -> Result<Map, Box<EvalAltResult>> {
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
            download_http_to_path(&download_context.cwd, url.as_str(), path.as_str(), Map::new())
        },
    );
    let download_context = context.clone();
    module.set_native_fn(
        "download",
        move |url: ImmutableString, path: ImmutableString, options: Map| -> Result<Map, Box<EvalAltResult>> {
            download_http_to_path(&download_context.cwd, url.as_str(), path.as_str(), options)
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
    module.set_native_fn("trim", |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
        if value.is_unit() {
            Ok(String::new())
        } else {
            Ok(value.to_string().trim().to_owned())
        }
    });
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
        |value: Dynamic, from: ImmutableString, to: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            if value.is_unit() {
                Ok(String::new())
            } else {
                Ok(value.to_string().replace(from.as_str(), to.as_str()))
            }
        },
    );
    module.set_native_fn("split_lines", |value: Dynamic| -> Result<Array, Box<EvalAltResult>> {
        if value.is_unit() {
            Ok(Array::new())
        } else {
            Ok(value
                .to_string()
                .lines()
                .map(|line| line.to_owned().into())
                .collect())
        }
    });
    module.set_native_fn("shell_quote", |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
        if value.is_unit() {
            Ok(shell_quote(""))
        } else {
            Ok(shell_quote(&value.to_string()))
        }
    });
    module
}

fn build_random_module() -> rhai::Module {
    let mut module = rhai::Module::new();
    module.set_native_fn(
        "jwt_env_keys",
        || -> Result<Dynamic, Box<EvalAltResult>> { generate_jwt_env_keys_dynamic() },
    );
    module.set_native_fn(
        "base64",
        |size: i64| -> Result<String, Box<EvalAltResult>> { generate_random_base64(size) },
    );
    module
}

fn build_search_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    let file_context = context.clone();
    module.set_native_fn(
        "files",
        move |root: ImmutableString, pattern: ImmutableString, options: Map| -> Result<Map, Box<EvalAltResult>> {
            let root = resolve_runtime_path(&file_context.cwd, root.as_str());
            search_files(&root, pattern.as_str(), options)
        },
    );
    module
}

fn build_config_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(&mut module, "effective", "config.effective", context.clone(), callbacks.clone());
    module_feature_no_args(&mut module, "raw", "config.raw", context.clone(), callbacks.clone());
    module_feature_get_value(&mut module, "get", "config.get", "path", context.clone(), callbacks.clone());
    let config_or_context = context.clone();
    let config_or_callbacks = callbacks.clone();
    module.set_native_fn(
        "get_or",
        move |path: ImmutableString, default: Dynamic| -> Result<Dynamic, Box<EvalAltResult>> {
            let output = (config_or_callbacks.run_feature)(
                &config_or_context.repo_root,
                "config.get",
                json!({ "path": path.as_str() }),
            )
            .map_err(|error| rhai_runtime_error(error.message))?;
            let value: serde_json::Value = serde_json::from_str(&output)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            let Some(found_value) = value.get("value") else {
                return Ok(default);
            };
            if found_value.is_null() {
                return Ok(default);
            }
            rhai::serde::to_dynamic(found_value.clone())
                .map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module
}

fn build_task_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    let task_context = context.clone();
    let task_callbacks = callbacks.clone();
    module.set_native_fn(
        "run",
        move |task: ImmutableString, args: Array| -> Result<String, Box<EvalAltResult>> {
            (task_callbacks.run_task)(
                &task_context.cwd,
                task.as_str(),
                &dynamic_array_to_strings(&args)?,
            )
            .map_err(rhai_runtime_error)
        },
    );
    let task_json_context = context.clone();
    let task_json_callbacks = callbacks.clone();
    module.set_native_fn(
        "run_json",
        move |task: ImmutableString, args: Array| -> Result<Dynamic, Box<EvalAltResult>> {
            let output = (task_json_callbacks.run_task)(
                &task_json_context.cwd,
                task.as_str(),
                &dynamic_array_to_strings(&args)?,
            )
            .map_err(rhai_runtime_error)?;
            let value: serde_json::Value = serde_json::from_str(&output)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module_feature_no_args(&mut module, "list", "tasks.list", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "list", "tasks.list", context.clone(), callbacks.clone());
    module_feature_string(&mut module, "resolve", "tasks.resolve", "selector", context.clone(), callbacks.clone());
    module_feature_string(&mut module, "info", "tasks.info", "selector", context.clone(), callbacks.clone());
    module
}

fn build_container_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    let container_context = context.clone();
    let container_callbacks = callbacks.clone();
    module.set_native_fn(
        "up",
        move |name: ImmutableString, detach: bool| -> Result<String, Box<EvalAltResult>> {
            (container_callbacks.container_up)(&container_context.repo_root, name.as_str(), detach)
                .map_err(rhai_runtime_error)
        },
    );
    let container_context = context.clone();
    let container_callbacks = callbacks.clone();
    module.set_native_fn(
        "down",
        move |name: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            (container_callbacks.container_down)(&container_context.repo_root, name.as_str(), false)
                .map_err(rhai_runtime_error)
        },
    );
    let container_context = context.clone();
    let container_callbacks = callbacks.clone();
    module.set_native_fn(
        "down_all",
        move || -> Result<String, Box<EvalAltResult>> {
            (container_callbacks.container_down)(&container_context.repo_root, "", true)
                .map_err(rhai_runtime_error)
        },
    );
    let container_shell_context = context.clone();
    let container_shell_callbacks = callbacks.clone();
    module.set_native_fn(
        "shell",
        move |name: ImmutableString, command: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            (container_shell_callbacks.container_shell)(
                &container_shell_context.repo_root,
                name.as_str(),
                None,
                command.as_str(),
            )
            .map_err(rhai_runtime_error)
        },
    );
    let container_shell_context = context.clone();
    let container_shell_callbacks = callbacks.clone();
    module.set_native_fn(
        "shell",
        move |name: ImmutableString, service: ImmutableString, command: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            (container_shell_callbacks.container_shell)(
                &container_shell_context.repo_root,
                name.as_str(),
                Some(service.as_str()),
                command.as_str(),
            )
            .map_err(rhai_runtime_error)
        },
    );
    let container_exec_context = context.clone();
    let container_exec_callbacks = callbacks.clone();
    module.set_native_fn(
        "exec",
        move |name: ImmutableString, command: Array| -> Result<Map, Box<EvalAltResult>> {
            Ok(host_command_output_map(
                (container_exec_callbacks.container_exec)(
                    &container_exec_context.repo_root,
                    name.as_str(),
                    None,
                    &dynamic_array_to_strings(&command)?,
                )
                .map_err(rhai_runtime_error)?,
            ))
        },
    );
    let container_exec_context = context.clone();
    let container_exec_callbacks = callbacks.clone();
    module.set_native_fn(
        "exec",
        move |name: ImmutableString, service: ImmutableString, command: Array| -> Result<Map, Box<EvalAltResult>> {
            Ok(host_command_output_map(
                (container_exec_callbacks.container_exec)(
                    &container_exec_context.repo_root,
                    name.as_str(),
                    Some(service.as_str()),
                    &dynamic_array_to_strings(&command)?,
                )
                .map_err(rhai_runtime_error)?,
            ))
        },
    );
    module_feature_string(&mut module, "status", "container.status", "name", context.clone(), callbacks.clone());
    module_feature_no_args(&mut module, "status_all", "container.status_all", context.clone(), callbacks.clone());
    module_feature_string_options(&mut module, "logs", "container.logs", "name", context.clone(), callbacks.clone());
    module_feature_string_options(&mut module, "reset", "container.reset", "name", context.clone(), callbacks.clone());
    module_feature_string(&mut module, "data_list", "container.data_list", "name", context.clone(), callbacks.clone());
    module_feature_three_strings(&mut module, "data_export", "container.data_export", ["name", "volume", "path"], context.clone(), callbacks.clone());
    module_feature_three_strings(&mut module, "data_import", "container.data_import", ["name", "volume", "path"], context.clone(), callbacks.clone());
    module_feature_string_options(&mut module, "data_pull_production", "container.data_pull_production", "name", context.clone(), callbacks.clone());
    module_feature_string(&mut module, "eject", "container.eject", "name", context.clone(), callbacks.clone());
    module_feature_no_args(&mut module, "stats_all", "container.stats_all", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "stats_all", "container.stats_all", context.clone(), callbacks.clone());
    module
}

fn build_scan_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(&mut module, "god_files", "scan.god_files", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "generated_assets", "scan.generated_assets", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "generated_in_src", "scan.generated_in_src", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "duplicate_blocks", "scan.duplicate_blocks", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "comment_ratio", "scan.comment_ratio", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "attention_markers", "scan.attention_markers", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "stale_suppressions", "scan.stale_suppressions", context.clone(), callbacks.clone());
    module
}

fn build_docs_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(&mut module, "check_links", "docs.check_links", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "check_json_examples", "docs.check_json_examples", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "check_headings", "docs.check_headings", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "check_paths", "docs.check_paths", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "check_contains", "docs.check_contains", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "check_forbidden", "docs.check_forbidden", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "check_index", "docs.check_index", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "check_next_action", "docs.check_next_action", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "check_workflow_paths", "docs.check_workflow_paths", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "add_log_index", "docs.add_log_index", context.clone(), callbacks.clone());
    module
}

fn build_deploy_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(&mut module, "model", "deploy.model", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "export_render", "deploy.export_render", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "export_railway", "deploy.export_railway", context.clone(), callbacks.clone());
    module
}

fn build_system_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(&mut module, "status", "system.status", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "logs", "system.logs", context.clone(), callbacks.clone());
    module
}

fn build_demo_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(&mut module, "list", "demo.list", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "inspect", "demo.inspect", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "history", "demo.history", context.clone(), callbacks.clone());
    module
}

fn build_changelog_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(&mut module, "validate", "changelog.validate", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "extract", "changelog.extract", context.clone(), callbacks.clone());
    module
}

fn build_cache_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(&mut module, "inspect", "cache.inspect", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "inspect", "cache.inspect", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "invalidate", "cache.invalidate", context.clone(), callbacks.clone());
    module
}

fn build_gateway_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(&mut module, "status", "gateway.status", context.clone(), callbacks.clone());
    module_feature_no_args(&mut module, "setup_tls", "gateway.setup_tls", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "setup_tls", "gateway.setup_tls", context.clone(), callbacks.clone());
    module_feature_no_args(&mut module, "up", "gateway.up", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "up", "gateway.up", context.clone(), callbacks.clone());
    module_feature_no_args(&mut module, "down", "gateway.down", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "down", "gateway.down", context.clone(), callbacks.clone());
    module
}

fn build_bundle_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(&mut module, "list", "bundle.list", context.clone(), callbacks.clone());
    module_feature_string(&mut module, "inspect", "bundle.inspect", "bundle", context.clone(), callbacks.clone());
    module_feature_two_strings(&mut module, "export_bundle", "bundle.export", ["bundle", "path"], context.clone(), callbacks.clone());
    module
}

fn build_service_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(&mut module, "list", "service.list", context.clone(), callbacks.clone());
    module_feature_string_options(&mut module, "extract", "service.extract", "service", context.clone(), callbacks.clone());
    module
}

fn build_catalog_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(&mut module, "tasks", "catalog.tasks", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "tasks", "catalog.tasks", context.clone(), callbacks.clone());
    module
}

fn build_doctor_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_no_args(&mut module, "run", "doctor.run", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "run", "doctor.run", context.clone(), callbacks.clone());
    module
}

fn build_contracts_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(&mut module, "check_json", "contracts.check_json", context.clone(), callbacks.clone());
    module_feature_options(&mut module, "validate_selection", "contracts.validate_selection", context.clone(), callbacks.clone());
    module
}

fn build_unlock_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(&mut module, "run", "unlock", context.clone(), callbacks.clone());
    module
}

fn build_test_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    module_feature_options(&mut module, "plan", "test.plan", context.clone(), callbacks.clone());
    module
}

fn build_effigy_module(context: Arc<ScriptContext>, callbacks: HostCallbacks) -> rhai::Module {
    let mut module = rhai::Module::new();
    let effigy_context = context.clone();
    let effigy_callbacks = callbacks.clone();
    module.set_native_fn(
        "run",
        move |args: Array| -> Result<Map, Box<EvalAltResult>> {
            let args = dynamic_array_to_strings(&args)?;
            Ok(effigy_result_map((effigy_callbacks.run_effigy)(
                &effigy_context.repo_root,
                &args,
                false,
            )))
        },
    );
    let effigy_json_context = context.clone();
    let effigy_json_callbacks = callbacks.clone();
    module.set_native_fn(
        "run_json",
        move |args: Array| -> Result<Dynamic, Box<EvalAltResult>> {
            let args = dynamic_array_to_strings(&args)?;
            let output =
                (effigy_json_callbacks.run_effigy)(&effigy_json_context.repo_root, &args, true)
                    .map_err(|error| rhai_runtime_error(error.message))?;
            let value: serde_json::Value = serde_json::from_str(&output)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    module
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

fn update_env_file_entry(path: &Path, key: &str, value: &str) -> Result<bool, Box<EvalAltResult>> {
    let rendered_entry = format!("{key}={}", render_dotenv_value(value));
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
        }
        std::fs::write(path, format!("{rendered_entry}\n"))
            .map_err(|error| rhai_runtime_error(failed_to_write_path(path, error)))?;
        return Ok(true);
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|error| rhai_runtime_error(failed_to_read_path(path, error)))?;
    let mut found = false;
    let mut changed = false;
    let mut rendered_lines = Vec::new();

    for raw_line in contents.lines() {
        if let Some(existing_key) = parse_dotenv_key(raw_line) {
            if existing_key == key {
                found = true;
                if raw_line != rendered_entry {
                    rendered_lines.push(rendered_entry.clone());
                    changed = true;
                } else {
                    rendered_lines.push(raw_line.to_owned());
                }
                continue;
            }
        }
        rendered_lines.push(raw_line.to_owned());
    }

    if !found {
        rendered_lines.push(rendered_entry);
        changed = true;
    }

    if !changed {
        return Ok(false);
    }

    let mut output = rendered_lines.join("\n");
    output.push('\n');
    std::fs::write(path, output)
        .map_err(|error| rhai_runtime_error(failed_to_write_path(path, error)))?;
    Ok(true)
}

fn remove_env_file_entry(path: &Path, key: &str) -> Result<bool, Box<EvalAltResult>> {
    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|error| rhai_runtime_error(failed_to_read_path(path, error)))?;
    let mut changed = false;
    let mut rendered_lines = Vec::new();

    for raw_line in contents.lines() {
        if parse_dotenv_key(raw_line).is_some_and(|existing_key| existing_key == key) {
            changed = true;
            continue;
        }
        rendered_lines.push(raw_line.to_owned());
    }

    if !changed {
        return Ok(false);
    }

    let mut output = rendered_lines.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    std::fs::write(path, output)
        .map_err(|error| rhai_runtime_error(failed_to_write_path(path, error)))?;
    Ok(true)
}

fn parse_dotenv_key(line: &str) -> Option<&str> {
    let mut trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    if let Some(exported) = trimmed.strip_prefix("export ") {
        trimmed = exported.trim_start();
    }
    let (key, _) = trimmed.split_once('=')?;
    let key = key.trim();
    (!key.is_empty()).then_some(key)
}

fn render_dotenv_value(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_owned();
    }
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '@'))
    {
        return value.to_owned();
    }
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}
