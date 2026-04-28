use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::Utc;
use effigy_core::path_error_text::{failed_to_read_path, failed_to_write_path};
use effigy_core::shell::shell_quote;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map};

use super::{
    allocate_temp_dir, dynamic_array_to_strings, effigy_result_map, emit_host_log,
    generate_jwt_env_keys_dynamic, generate_random_base64, host_command_output_map,
    process_result_map, register_feature_get_value, register_feature_no_args,
    register_feature_options, register_feature_string, register_feature_string_options,
    register_feature_three_strings, register_feature_two_strings, reject_recursive_effigy_process,
    resolve_runtime_path, rhai_runtime_error, run_http_request, run_process_streaming,
    search_files, with_local_node_bin_path, HostCallbacks, ScriptContext,
};

pub(super) fn register_host_api(
    engine: &mut Engine,
    context: Arc<ScriptContext>,
    callbacks: HostCallbacks,
) {
    engine.register_fn("log", |message: ImmutableString| {
        let _ = emit_host_log(message.as_str(), false);
    });
    engine.register_fn("log_warn", |message: ImmutableString| {
        let _ = emit_host_log(message.as_str(), true);
    });

    engine.register_fn("env", |name: ImmutableString| -> String {
        std::env::var(name.as_str()).unwrap_or_default()
    });
    let stop_context = context.clone();
    engine.register_fn("stop_requested", move || -> bool {
        stop_context.stop_requested.load(Ordering::Relaxed)
    });
    engine.register_fn("now_utc", || -> String {
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
    });
    engine.register_fn("process_id", || -> i64 { i64::from(std::process::id()) });
    engine.register_fn("sleep_ms", |millis: i64| {
        if millis > 0 {
            thread::sleep(Duration::from_millis(millis as u64));
        }
    });
    engine.register_fn(
        "path_join",
        |base: ImmutableString, child: ImmutableString| -> String {
            PathBuf::from(base.as_str())
                .join(child.as_str())
                .display()
                .to_string()
        },
    );
    engine.register_fn("path_file_name", |path: ImmutableString| -> String {
        Path::new(path.as_str())
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    engine.register_fn("trim_string", |value: Dynamic| -> String {
        if value.is_unit() {
            String::new()
        } else {
            value.to_string().trim().to_owned()
        }
    });
    engine.register_fn("shell_quote_string", |value: Dynamic| -> String {
        if value.is_unit() {
            shell_quote("")
        } else {
            shell_quote(&value.to_string())
        }
    });
    engine.register_fn(
        "generate_jwt_env_keys",
        || -> Result<Dynamic, Box<EvalAltResult>> { generate_jwt_env_keys_dynamic() },
    );
    engine.register_fn(
        "generate_random_base64",
        |size: i64| -> Result<String, Box<EvalAltResult>> { generate_random_base64(size) },
    );

    engine.register_fn(
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
    engine.register_fn(
        "read_file",
        move |path: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
        },
    );
    let file_context = context.clone();
    engine.register_fn(
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
    engine.register_fn(
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
    engine.register_fn(
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
    engine.register_fn("path_exists", move |path: ImmutableString| -> bool {
        resolve_runtime_path(&file_context.cwd, path.as_str()).exists()
    });
    let file_context = context.clone();
    engine.register_fn(
        "list_dir",
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
    engine.register_fn("is_file", move |path: ImmutableString| -> bool {
        resolve_runtime_path(&file_context.cwd, path.as_str()).is_file()
    });
    let file_context = context.clone();
    engine.register_fn(
        "search_files",
        move |root: ImmutableString,
              pattern: ImmutableString,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            let root = resolve_runtime_path(&file_context.cwd, root.as_str());
            search_files(&root, pattern.as_str(), options)
        },
    );
    let file_context = context.clone();
    engine.register_fn(
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
    engine.register_fn(
        "create_dir",
        move |path: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::create_dir_all(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    engine.register_fn(
        "remove_path",
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
    engine.register_fn(
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

    engine.register_fn(
        "json_parse",
        |raw: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            let value: serde_json::Value = serde_json::from_str(raw.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    engine.register_fn(
        "json_stringify",
        |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
            let decoded: serde_json::Value = rhai::serde::from_dynamic(&value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            serde_json::to_string_pretty(&decoded)
                .map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    engine.register_fn(
        "toml_parse",
        |raw: ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            let value: toml::Value = toml::from_str(raw.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            rhai::serde::to_dynamic(value).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );
    engine.register_fn(
        "toml_stringify",
        |value: Dynamic| -> Result<String, Box<EvalAltResult>> {
            let decoded: toml::Value = rhai::serde::from_dynamic(&value)
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            toml::to_string_pretty(&decoded).map_err(|error| rhai_runtime_error(error.to_string()))
        },
    );

    let process_context = context.clone();
    engine.register_fn(
        "run_process",
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
    engine.register_fn(
        "run_process_stream",
        move |program: ImmutableString, args: Array| -> Result<Map, Box<EvalAltResult>> {
            reject_recursive_effigy_process(program.as_str())?;
            let args = dynamic_array_to_strings(&args)?;
            run_process_streaming(program.as_str(), &args, &process_context.cwd)
        },
    );
    engine.register_fn(
        "http_request",
        move |method: ImmutableString,
              url: ImmutableString,
              options: Map|
              -> Result<Map, Box<EvalAltResult>> {
            run_http_request(method.as_str(), url.as_str(), options)
        },
    );
    engine.register_fn(
        "http_get",
        move |url: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            run_http_request("GET", url.as_str(), Map::new())
        },
    );
    engine.register_fn(
        "http_post",
        move |url: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            run_http_request("POST", url.as_str(), Map::new())
        },
    );
    engine.register_fn(
        "http_post",
        move |url: ImmutableString, options: Map| -> Result<Map, Box<EvalAltResult>> {
            run_http_request("POST", url.as_str(), options)
        },
    );

    let task_context = context.clone();
    let task_callbacks = callbacks.clone();
    engine.register_fn(
        "run_task",
        move |task: ImmutableString, args: Array| -> Result<String, Box<EvalAltResult>> {
            (task_callbacks.run_task)(
                &task_context.cwd,
                task.as_str(),
                &dynamic_array_to_strings(&args)?,
            )
            .map_err(rhai_runtime_error)
        },
    );

    let effigy_context = context.clone();
    let effigy_callbacks = callbacks.clone();
    engine.register_fn(
        "run_effigy",
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
    engine.register_fn(
        "run_effigy_json",
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

    register_feature_no_args(
        engine,
        "config_effective",
        "config.effective",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_no_args(
        engine,
        "config_raw",
        "config.raw",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_get_value(
        engine,
        "config_get",
        "config.get",
        "path",
        context.clone(),
        callbacks.clone(),
    );

    register_feature_no_args(
        engine,
        "tasks_list",
        "tasks.list",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "tasks_list",
        "tasks.list",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_string(
        engine,
        "task_resolve",
        "tasks.resolve",
        "selector",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_string(
        engine,
        "task_info",
        "tasks.info",
        "selector",
        context.clone(),
        callbacks.clone(),
    );

    register_feature_no_args(
        engine,
        "container_status_all",
        "container.status_all",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_string(
        engine,
        "container_status",
        "container.status",
        "name",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_string_options(
        engine,
        "container_logs",
        "container.logs",
        "name",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_string_options(
        engine,
        "container_reset",
        "container.reset",
        "name",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_string(
        engine,
        "container_data_list",
        "container.data_list",
        "name",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_three_strings(
        engine,
        "container_data_export",
        "container.data_export",
        ["name", "volume", "path"],
        context.clone(),
        callbacks.clone(),
    );
    register_feature_three_strings(
        engine,
        "container_data_import",
        "container.data_import",
        ["name", "volume", "path"],
        context.clone(),
        callbacks.clone(),
    );
    register_feature_string_options(
        engine,
        "container_data_pull_production",
        "container.data_pull_production",
        "name",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_string(
        engine,
        "container_eject",
        "container.eject",
        "name",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_no_args(
        engine,
        "container_stats_all",
        "container.stats_all",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "container_stats_all",
        "container.stats_all",
        context.clone(),
        callbacks.clone(),
    );

    register_feature_options(
        engine,
        "docs_check_links",
        "docs.check_links",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "docs_check_json_examples",
        "docs.check_json_examples",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "docs_check_headings",
        "docs.check_headings",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "docs_check_paths",
        "docs.check_paths",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "docs_check_contains",
        "docs.check_contains",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "docs_check_forbidden",
        "docs.check_forbidden",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "docs_check_index",
        "docs.check_index",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "docs_check_next_action",
        "docs.check_next_action",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "docs_check_workflow_paths",
        "docs.check_workflow_paths",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "docs_add_log_index",
        "docs.add_log_index",
        context.clone(),
        callbacks.clone(),
    );

    register_feature_no_args(
        engine,
        "bundle_list",
        "bundle.list",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_string(
        engine,
        "bundle_inspect",
        "bundle.inspect",
        "bundle",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_two_strings(
        engine,
        "bundle_export",
        "bundle.export",
        ["bundle", "path"],
        context.clone(),
        callbacks.clone(),
    );
    register_feature_no_args(
        engine,
        "service_list",
        "service.list",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_string_options(
        engine,
        "service_extract",
        "service.extract",
        "service",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_no_args(
        engine,
        "catalog_tasks",
        "catalog.tasks",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "catalog_tasks",
        "catalog.tasks",
        context.clone(),
        callbacks.clone(),
    );

    register_feature_no_args(
        engine,
        "gateway_status",
        "gateway.status",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_no_args(
        engine,
        "gateway_setup_tls",
        "gateway.setup_tls",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "gateway_setup_tls",
        "gateway.setup_tls",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_no_args(
        engine,
        "gateway_up",
        "gateway.up",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "gateway_up",
        "gateway.up",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_no_args(
        engine,
        "gateway_down",
        "gateway.down",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "gateway_down",
        "gateway.down",
        context.clone(),
        callbacks.clone(),
    );

    register_feature_no_args(
        engine,
        "doctor",
        "doctor.run",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "doctor",
        "doctor.run",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "scan_god_files",
        "scan.god_files",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "scan_large_files",
        "scan.god_files",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "scan_generated",
        "scan.generated_assets",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "scan_generated_assets",
        "scan.generated_assets",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "scan_generated_in_src",
        "scan.generated_in_src",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "scan_duplicate_blocks",
        "scan.duplicate_blocks",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "scan_comment_ratio",
        "scan.comment_ratio",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "scan_attention_markers",
        "scan.attention_markers",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "scan_stale_suppressions",
        "scan.stale_suppressions",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_no_args(
        engine,
        "cache_inspect",
        "cache.inspect",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "cache_inspect",
        "cache.inspect",
        context.clone(),
        callbacks.clone(),
    );
    register_feature_options(
        engine,
        "cache_invalidate",
        "cache.invalidate",
        context.clone(),
        callbacks.clone(),
    );

    let container_context = context.clone();
    let container_callbacks = callbacks.clone();
    engine.register_fn(
        "container_up",
        move |name: ImmutableString, detach: bool| -> Result<String, Box<EvalAltResult>> {
            (container_callbacks.container_up)(&container_context.repo_root, name.as_str(), detach)
                .map_err(rhai_runtime_error)
        },
    );
    let container_context = context.clone();
    let container_callbacks = callbacks.clone();
    engine.register_fn(
        "container_down",
        move |name: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            (container_callbacks.container_down)(&container_context.repo_root, name.as_str())
                .map_err(rhai_runtime_error)
        },
    );
    let container_context = context;
    let container_exec_context = container_context.clone();
    let container_exec_callbacks = callbacks.clone();
    engine.register_fn(
        "container_exec",
        move |name: ImmutableString,
              service: ImmutableString,
              command: Array|
              -> Result<Map, Box<EvalAltResult>> {
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
    let container_exec_context = container_context.clone();
    let container_exec_callbacks = callbacks.clone();
    engine.register_fn(
        "container_exec",
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
    engine.register_fn(
        "container_shell",
        move |name: ImmutableString,
              command: ImmutableString|
              -> Result<String, Box<EvalAltResult>> {
            (callbacks.container_shell)(
                &container_context.repo_root,
                name.as_str(),
                command.as_str(),
            )
            .map_err(rhai_runtime_error)
        },
    );
}
