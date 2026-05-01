use std::path::{Path, PathBuf};
use std::sync::Arc;

use effigy_rhai::{
    execute_rhai_script, install_stop_requested_flag, load_script, load_script_args_from_env,
    required_env, EffigyCommandError, HostCallbacks, HostCommandOutput, ScriptContext,
    EFFIGY_RHAI_REPO_ROOT, EFFIGY_RHAI_TASK_NAME,
};

use effigy_cli::{
    BundleArgs, BundleSubcommand, ContainerArgs, ContainerDataSubcommand, ContainerSubcommand,
    DocsArgs, DocsBlockRequirement, DocsSubcommand, DoctorArgs, GatewayArgs, GatewaySubcommand,
    InternalRhaiArgs, ServiceArgs, ServiceSubcommand, TaskInvocation, TasksArgs,
};
use serde_json::Value;

use super::command_context::EmbeddedRepoOverrideMode;
use super::embedded_runner::parse_embedded_command;
use super::error::RunnerError;
use super::execute::api::run_manifest_task_with_cwd;
pub(in crate::runner) fn run_internal_rhai(args: InternalRhaiArgs) -> Result<String, RunnerError> {
    execute_repo_rhai_script(
        &required_repo_root(&args)?,
        &required_task_name(&args)?,
        &args.file,
        &load_script_args_for_internal(&args)?,
    )?;
    Ok(String::new())
}

pub(in crate::runner) fn execute_repo_rhai_script(
    repo_root: &Path,
    task_name: &str,
    file: &Path,
    args: &[String],
) -> Result<(), RunnerError> {
    let context = ScriptContext {
        cwd: repo_root.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        task_name: task_name.to_owned(),
        stop_requested: install_stop_requested_flag().map_err(map_rhai_error)?,
    };
    let script = load_script(file, &context.cwd).map_err(map_rhai_error)?;
    execute_rhai_script(&context, &script, args, &host_callbacks()).map_err(map_rhai_error)
}

fn required_repo_root(args: &InternalRhaiArgs) -> Result<PathBuf, RunnerError> {
    if let Some(path) = &args.repo_root {
        Ok(path.clone())
    } else {
        Ok(PathBuf::from(
            required_env(EFFIGY_RHAI_REPO_ROOT).map_err(map_rhai_error)?,
        ))
    }
}

fn required_task_name(args: &InternalRhaiArgs) -> Result<String, RunnerError> {
    if let Some(task_name) = &args.task_name {
        Ok(task_name.clone())
    } else {
        required_env(EFFIGY_RHAI_TASK_NAME).map_err(map_rhai_error)
    }
}

fn load_script_args_for_internal(args: &InternalRhaiArgs) -> Result<Vec<String>, RunnerError> {
    if args.args.is_empty() {
        match load_script_args_from_env() {
            Ok(values) => Ok(values),
            Err(_) if args.repo_root.is_some() || args.task_name.is_some() => Ok(Vec::new()),
            Err(error) => Err(map_rhai_error(error)),
        }
    } else {
        Ok(args.args.clone())
    }
}

fn host_callbacks() -> HostCallbacks {
    HostCallbacks {
        run_task: Arc::new(|cwd, task, args| {
            let invocation = TaskInvocation {
                name: task.to_owned(),
                args: args.to_vec(),
            };
            run_manifest_task_with_cwd(&invocation, cwd.to_path_buf())
                .map_err(|error| error.to_string())
        }),
        run_effigy: Arc::new(|repo_root, args, force_json| {
            run_effigy_command(repo_root, args, force_json).map_err(|error| EffigyCommandError {
                message: error.to_string(),
                rendered_output: error.rendered_output().unwrap_or_default().to_owned(),
            })
        }),
        run_feature: Arc::new(|repo_root, feature, options| {
            run_rhai_feature(repo_root, feature, options).map_err(|error| EffigyCommandError {
                message: error.to_string(),
                rendered_output: error.rendered_output().unwrap_or_default().to_owned(),
            })
        }),
        container_up: Arc::new(|repo_root, name, detach| {
            run_container_helper(
                repo_root,
                ContainerSubcommand::Up {
                    name: Some(name.to_owned()),
                    attach: !detach,
                    detach,
                },
            )
            .map_err(|error| error.to_string())
        }),
        container_down: Arc::new(|repo_root, name| {
            run_container_helper(
                repo_root,
                ContainerSubcommand::Down {
                    name: Some(name.to_owned()),
                    all: false,
                },
            )
            .map_err(|error| error.to_string())
        }),
        container_shell: Arc::new(|repo_root, name, command| {
            run_container_helper(
                repo_root,
                ContainerSubcommand::Shell {
                    name: Some(name.to_owned()),
                    service: None,
                    command: Some(command.to_owned()),
                },
            )
            .map_err(|error| error.to_string())
        }),
        container_exec: Arc::new(|repo_root, name, service, command| {
            let name = if name.is_empty() { None } else { Some(name) };
            let output = crate::runner::container_command::run_container_exec_capture(
                repo_root, name, service, command,
            )
            .map_err(|error| error.to_string())?;
            Ok(HostCommandOutput {
                status: i64::from(output.status.code().unwrap_or(-1)),
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }),
    }
}

fn run_rhai_feature(
    repo_root: &Path,
    feature: &str,
    options: Value,
) -> Result<String, RunnerError> {
    match feature {
        "tasks.list" | "catalog.tasks" => run_typed_command(
            repo_root,
            effigy_cli::Command::Tasks(TasksArgs {
                repo_override: Some(repo_root.to_path_buf()),
                task_name: string_option(&options, "task")?,
                resolve_selector: string_option(&options, "resolve")?,
                output_json: true,
                pretty_json: false,
            }),
        ),
        "config.effective" => run_config_effective(repo_root),
        "config.raw" => run_config_raw(repo_root),
        "config.get" => run_config_get(repo_root, &required_string(&options, "path")?),
        "tasks.resolve" => run_typed_command(
            repo_root,
            effigy_cli::Command::Tasks(TasksArgs {
                repo_override: Some(repo_root.to_path_buf()),
                task_name: None,
                resolve_selector: Some(required_string(&options, "selector")?),
                output_json: true,
                pretty_json: false,
            }),
        ),
        "tasks.info" => run_typed_command(
            repo_root,
            effigy_cli::Command::Tasks(TasksArgs {
                repo_override: Some(repo_root.to_path_buf()),
                task_name: Some(required_string(&options, "selector")?),
                resolve_selector: None,
                output_json: true,
                pretty_json: false,
            }),
        ),
        "container.status" => run_container_json(
            repo_root,
            ContainerSubcommand::Status {
                name: Some(required_string(&options, "name")?),
                all: false,
            },
        ),
        "container.status_all" => run_typed_command(
            repo_root,
            effigy_cli::Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Status {
                    name: None,
                    all: true,
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        "container.logs" => {
            if bool_option(&options, "follow")?.unwrap_or(false) {
                return Err(RunnerError::task_invocation(
                    "`container_logs` does not support `follow = true` from Rhai",
                ));
            }
            run_container_json(
                repo_root,
                ContainerSubcommand::Logs {
                    name: Some(required_string(&options, "name")?),
                    service: string_option(&options, "service")?,
                    follow: false,
                },
            )
        }
        "container.reset" => run_container_json(
            repo_root,
            ContainerSubcommand::Reset {
                name: Some(required_string(&options, "name")?),
                keep_data: bool_option(&options, "keep_data")?.unwrap_or(false),
            },
        ),
        "container.data_list" => run_container_json(
            repo_root,
            ContainerSubcommand::Data {
                name: Some(required_string(&options, "name")?),
                subcommand: ContainerDataSubcommand::List,
            },
        ),
        "container.data_export" => run_container_json(
            repo_root,
            ContainerSubcommand::Data {
                name: Some(required_string(&options, "name")?),
                subcommand: ContainerDataSubcommand::Export {
                    volume: required_string(&options, "volume")?,
                    path: PathBuf::from(required_string(&options, "path")?),
                },
            },
        ),
        "container.data_import" => run_container_json(
            repo_root,
            ContainerSubcommand::Data {
                name: Some(required_string(&options, "name")?),
                subcommand: ContainerDataSubcommand::Import {
                    volume: required_string(&options, "volume")?,
                    path: PathBuf::from(required_string(&options, "path")?),
                },
            },
        ),
        "container.data_pull_production" => run_container_json(
            repo_root,
            ContainerSubcommand::Data {
                name: Some(required_string(&options, "name")?),
                subcommand: ContainerDataSubcommand::PullProduction,
            },
        ),
        "container.eject" => run_container_json(
            repo_root,
            ContainerSubcommand::Eject {
                name: Some(required_string(&options, "name")?),
            },
        ),
        "container.stats_all" => run_typed_command(
            repo_root,
            effigy_cli::Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Stats { all: true },
                repo_override: None,
                output_json: true,
            }),
        ),
        "docs.check_links" => run_docs_json(
            repo_root,
            DocsSubcommand::CheckLinks {
                paths: path_array(&options, "paths")?,
            },
        ),
        "docs.check_json_examples" => run_docs_json(
            repo_root,
            DocsSubcommand::CheckJsonExamples {
                file: path_option(&options, "file")?,
                section: string_option(&options, "section")?,
                min_blocks: usize_option(&options, "min_blocks")?,
                required: string_array(&options, "required")?,
                required_blocks: docs_block_requirements(&options)?,
            },
        ),
        "docs.check_headings" => run_docs_json(
            repo_root,
            DocsSubcommand::CheckHeadings {
                paths: path_array(&options, "paths")?,
                required_headings: string_array(&options, "required_headings")?,
            },
        ),
        "docs.check_paths" => run_docs_json(
            repo_root,
            DocsSubcommand::CheckPaths {
                paths: path_array(&options, "paths")?,
            },
        ),
        "docs.check_contains" => run_docs_json(
            repo_root,
            DocsSubcommand::CheckContains {
                paths: path_array(&options, "paths")?,
                required_text: string_array_any(&options, &["required_text", "required"])?,
            },
        ),
        "docs.check_forbidden" => run_docs_json(
            repo_root,
            DocsSubcommand::CheckForbidden {
                paths: path_array(&options, "paths")?,
                forbidden_text: string_array_any(&options, &["forbidden_text", "forbidden"])?,
            },
        ),
        "docs.check_index" => run_docs_json(
            repo_root,
            DocsSubcommand::CheckIndex {
                policy_index: string_option(&options, "policy_index")?,
                dir: path_option(&options, "dir")?,
                index: path_option(&options, "index")?,
            },
        ),
        "docs.check_next_action" => run_docs_json(
            repo_root,
            DocsSubcommand::CheckNextAction {
                policy_name: string_option(&options, "policy_name")?,
            },
        ),
        "docs.check_workflow_paths" => run_docs_json(
            repo_root,
            DocsSubcommand::CheckWorkflowPaths {
                dir: path_option(&options, "dir")?,
            },
        ),
        "docs.add_log_index" => run_docs_json(
            repo_root,
            DocsSubcommand::AddLogIndex {
                log_path: PathBuf::from(required_string(&options, "log_path")?),
            },
        ),
        "bundle.list" => run_typed_command(
            repo_root,
            effigy_cli::Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::List,
                output_json: true,
            }),
        ),
        "bundle.inspect" => run_typed_command(
            repo_root,
            effigy_cli::Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::Inspect {
                    bundle: required_string(&options, "bundle")?,
                },
                output_json: true,
            }),
        ),
        "bundle.export" => run_typed_command(
            repo_root,
            effigy_cli::Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::Export {
                    bundle: required_string(&options, "bundle")?,
                    path: PathBuf::from(required_string(&options, "path")?),
                },
                output_json: true,
            }),
        ),
        "service.list" => run_typed_command(
            repo_root,
            effigy_cli::Command::Service(ServiceArgs {
                subcommand: ServiceSubcommand::List,
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        "service.extract" => run_typed_command(
            repo_root,
            effigy_cli::Command::Service(ServiceArgs {
                subcommand: ServiceSubcommand::Extract {
                    service: required_string(&options, "service")?,
                    dir: path_option(&options, "dir")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        "gateway.status" => run_gateway_json(GatewaySubcommand::Status),
        "gateway.setup_tls" => run_gateway_json(GatewaySubcommand::SetupTls),
        "gateway.up" => run_gateway_json(GatewaySubcommand::Up),
        "gateway.down" => run_gateway_json(GatewaySubcommand::Down),
        "doctor.run" => run_typed_command(
            repo_root,
            effigy_cli::Command::Doctor(DoctorArgs {
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
                fix: bool_option(&options, "fix")?.unwrap_or(false),
                verbose: bool_option(&options, "verbose")?.unwrap_or(false),
                explain: string_option(&options, "explain")?.map(|name| TaskInvocation {
                    name,
                    args: Vec::new(),
                }),
            }),
        ),
        "scan.god_files" => run_builtin_json(repo_root, "scan", scan_args("god-files", &options)?),
        "scan.duplicate_blocks" => {
            run_builtin_json(repo_root, "scan", scan_args("duplicate-blocks", &options)?)
        }
        "scan.comment_ratio" => {
            run_builtin_json(repo_root, "scan", scan_args("comment-ratio", &options)?)
        }
        "scan.generated_assets" => {
            run_builtin_json(repo_root, "scan", scan_args("generated-assets", &options)?)
        }
        "scan.generated_in_src" => {
            run_builtin_json(repo_root, "scan", scan_args("generated-in-src", &options)?)
        }
        "scan.attention_markers" => {
            run_builtin_json(repo_root, "scan", scan_args("attention-markers", &options)?)
        }
        "scan.stale_suppressions" => run_builtin_json(
            repo_root,
            "scan",
            scan_args("stale-suppressions", &options)?,
        ),
        "cache.inspect" => run_builtin_json(repo_root, "cache", cache_inspect_args(&options)?),
        "cache.invalidate" => {
            run_builtin_json(repo_root, "cache", cache_invalidate_args(&options)?)
        }
        other => Err(RunnerError::task_invocation(format!(
            "unknown Rhai feature `{other}`"
        ))),
    }
}

fn run_typed_command(
    _repo_root: &Path,
    command: effigy_cli::Command,
) -> Result<String, RunnerError> {
    crate::runner::run_command(command)
}

fn run_config_effective(repo_root: &Path) -> Result<String, RunnerError> {
    let loaded = crate::runner::manifest::load_task_manifest_with_inspection(
        &repo_root.join(effigy_manifest::TASK_MANIFEST_FILE),
    )?;
    let config = toml_value_to_json(loaded.effective_value)?;
    Ok(serde_json::json!({
        "schema": "effigy.rhai.config.effective.v1",
        "schema_version": 1,
        "ok": true,
        "manifest_path": loaded.manifest_path,
        "bundle_root": loaded.bundle_root,
        "evaluation_order": loaded.evaluation_order,
        "include_graph": loaded.include_graph,
        "overridden_paths": loaded.overridden_paths,
        "value_sources": loaded.value_sources,
        "config": config,
    })
    .to_string())
}

fn run_config_raw(repo_root: &Path) -> Result<String, RunnerError> {
    let manifest_path = repo_root.join(effigy_manifest::TASK_MANIFEST_FILE);
    let raw =
        std::fs::read_to_string(&manifest_path).map_err(|error| RunnerError::TaskManifestRead {
            path: manifest_path.clone(),
            error,
        })?;
    let value =
        toml::from_str::<toml::Value>(&raw).map_err(|error| RunnerError::TaskManifestParse {
            path: manifest_path.clone(),
            error,
        })?;
    Ok(serde_json::json!({
        "schema": "effigy.rhai.config.raw.v1",
        "schema_version": 1,
        "ok": true,
        "manifest_path": manifest_path,
        "config": toml_value_to_json(value)?,
    })
    .to_string())
}

fn run_config_get(repo_root: &Path, path: &str) -> Result<String, RunnerError> {
    let loaded = crate::runner::manifest::load_task_manifest_with_inspection(
        &repo_root.join(effigy_manifest::TASK_MANIFEST_FILE),
    )?;
    let value = get_toml_path(&loaded.effective_value, path).cloned();
    Ok(serde_json::json!({
        "schema": "effigy.rhai.config.get.v1",
        "schema_version": 1,
        "ok": true,
        "path": path,
        "found": value.is_some(),
        "value": value.map(toml_value_to_json).transpose()?,
    })
    .to_string())
}

fn get_toml_path<'a>(value: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            return None;
        }
        current = current.as_table()?.get(segment)?;
    }
    Some(current)
}

fn toml_value_to_json(value: toml::Value) -> Result<serde_json::Value, RunnerError> {
    serde_json::to_value(value)
        .map_err(|error| RunnerError::Ui(format!("failed to encode config value: {error}")))
}

fn run_docs_json(repo_root: &Path, subcommand: DocsSubcommand) -> Result<String, RunnerError> {
    run_typed_command(
        repo_root,
        effigy_cli::Command::Docs(DocsArgs {
            subcommand,
            repo_override: Some(repo_root.to_path_buf()),
            output_json: true,
        }),
    )
}

fn run_container_json(
    repo_root: &Path,
    subcommand: ContainerSubcommand,
) -> Result<String, RunnerError> {
    run_typed_command(
        repo_root,
        effigy_cli::Command::Container(ContainerArgs {
            subcommand,
            repo_override: Some(repo_root.to_path_buf()),
            output_json: true,
        }),
    )
}

fn run_gateway_json(subcommand: GatewaySubcommand) -> Result<String, RunnerError> {
    crate::runner::run_command(effigy_cli::Command::Gateway(GatewayArgs {
        subcommand,
        output_json: true,
    }))
}

fn run_builtin_json(
    repo_root: &Path,
    task: &str,
    mut args: Vec<String>,
) -> Result<String, RunnerError> {
    if !args.iter().any(|arg| arg == "--json") {
        args.push("--json".to_owned());
    }
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: task.to_owned(),
            args,
        },
        repo_root.to_path_buf(),
    )
}

fn scan_args(scan: &str, options: &Value) -> Result<Vec<String>, RunnerError> {
    let mut args = vec![scan.to_owned()];
    push_usize_flag(&mut args, options, "threshold", "--threshold")?;
    push_usize_flag(&mut args, options, "warn", "--warn")?;
    push_usize_flag(&mut args, options, "high", "--high")?;
    push_usize_flag(&mut args, options, "critical", "--critical")?;
    push_bool_flag(&mut args, options, "show_warnings", "--show-warnings")?;
    push_bool_flag(&mut args, options, "fail_on_findings", "--fail-on-findings")?;
    push_bool_flag(&mut args, options, "no_gitignore", "--no-gitignore")?;
    push_string_array_flag(&mut args, options, "include", "--include")?;
    push_string_array_flag(&mut args, options, "exclude", "--exclude")?;
    if let Some(path) = string_option(options, "markdown_out")? {
        args.push("--markdown-out".to_owned());
        args.push(path);
    }
    if let Some(format) = string_option(options, "format")? {
        args.push("--format".to_owned());
        args.push(format);
    }
    Ok(args)
}

fn cache_inspect_args(options: &Value) -> Result<Vec<String>, RunnerError> {
    let mut args = vec!["inspect".to_owned()];
    if let Some(selector) = string_option(options, "selector")? {
        args.push(selector);
    }
    Ok(args)
}

fn cache_invalidate_args(options: &Value) -> Result<Vec<String>, RunnerError> {
    let mut args = vec!["invalidate".to_owned()];
    if bool_option(options, "all")?.unwrap_or(false) {
        args.push("--all".to_owned());
    }
    args.extend(string_array(options, "selectors")?);
    if let Some(selector) = string_option(options, "selector")? {
        args.push(selector);
    }
    Ok(args)
}

fn docs_block_requirements(options: &Value) -> Result<Vec<DocsBlockRequirement>, RunnerError> {
    let Some(value) = options.get("required_blocks") else {
        return Ok(Vec::new());
    };
    let array = value.as_array().ok_or_else(|| {
        RunnerError::task_invocation("`required_blocks` must be an array of maps")
    })?;
    array
        .iter()
        .map(|entry| {
            Ok(DocsBlockRequirement {
                block_index: required_usize(entry, "block_index")?,
                needle: required_string(entry, "needle")?,
            })
        })
        .collect()
}

fn push_bool_flag(
    args: &mut Vec<String>,
    options: &Value,
    key: &str,
    flag: &str,
) -> Result<(), RunnerError> {
    if bool_option(options, key)?.unwrap_or(false) {
        args.push(flag.to_owned());
    }
    Ok(())
}

fn push_usize_flag(
    args: &mut Vec<String>,
    options: &Value,
    key: &str,
    flag: &str,
) -> Result<(), RunnerError> {
    if let Some(value) = usize_option(options, key)? {
        args.push(flag.to_owned());
        args.push(value.to_string());
    }
    Ok(())
}

fn push_string_array_flag(
    args: &mut Vec<String>,
    options: &Value,
    key: &str,
    flag: &str,
) -> Result<(), RunnerError> {
    for value in string_array(options, key)? {
        args.push(flag.to_owned());
        args.push(value);
    }
    Ok(())
}

fn string_array_any(options: &Value, keys: &[&str]) -> Result<Vec<String>, RunnerError> {
    for key in keys {
        if options.get(*key).is_some() {
            return string_array(options, key);
        }
    }
    Ok(Vec::new())
}

fn path_array(options: &Value, key: &str) -> Result<Vec<PathBuf>, RunnerError> {
    Ok(string_array(options, key)?
        .into_iter()
        .map(PathBuf::from)
        .collect())
}

fn path_option(options: &Value, key: &str) -> Result<Option<PathBuf>, RunnerError> {
    Ok(string_option(options, key)?.map(PathBuf::from))
}

fn string_array(options: &Value, key: &str) -> Result<Vec<String>, RunnerError> {
    let Some(value) = options.get(key) else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| RunnerError::task_invocation(format!("`{key}` must be an array")))?;
    array
        .iter()
        .map(|entry| {
            entry.as_str().map(str::to_owned).ok_or_else(|| {
                RunnerError::task_invocation(format!("`{key}` values must be strings"))
            })
        })
        .collect()
}

fn string_option(options: &Value, key: &str) -> Result<Option<String>, RunnerError> {
    match options.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(RunnerError::task_invocation(format!(
            "`{key}` must be a string"
        ))),
    }
}

fn required_string(options: &Value, key: &str) -> Result<String, RunnerError> {
    string_option(options, key)?
        .ok_or_else(|| RunnerError::task_invocation(format!("`{key}` is required")))
}

fn bool_option(options: &Value, key: &str) -> Result<Option<bool>, RunnerError> {
    match options.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(RunnerError::task_invocation(format!(
            "`{key}` must be a bool"
        ))),
    }
}

fn usize_option(options: &Value, key: &str) -> Result<Option<usize>, RunnerError> {
    match options.get(key) {
        Some(Value::Number(value)) => value
            .as_u64()
            .map(|value| Some(value as usize))
            .ok_or_else(|| RunnerError::task_invocation(format!("`{key}` must be a usize"))),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(RunnerError::task_invocation(format!(
            "`{key}` must be a usize"
        ))),
    }
}

fn required_usize(options: &Value, key: &str) -> Result<usize, RunnerError> {
    usize_option(options, key)?
        .ok_or_else(|| RunnerError::task_invocation(format!("`{key}` is required")))
}

fn run_effigy_command(
    repo_root: &Path,
    args: &[String],
    force_json: bool,
) -> Result<String, RunnerError> {
    crate::runner::embedded_runner::run_embedded_command(
        repo_root,
        parse_rhai_embedded_command(repo_root, args, force_json)?,
        repo_root,
        EmbeddedRepoOverrideMode::DefaultIfMissing,
    )
}

fn parse_rhai_embedded_command(
    repo_root: &Path,
    args: &[String],
    force_json: bool,
) -> Result<effigy_cli::Command, RunnerError> {
    parse_embedded_command(
        repo_root,
        args,
        force_json,
        EmbeddedRepoOverrideMode::DefaultIfMissing,
    )
}

fn run_container_helper(
    repo_root: &Path,
    subcommand: ContainerSubcommand,
) -> Result<String, RunnerError> {
    crate::runner::run_command(effigy_cli::Command::Container(ContainerArgs {
        subcommand,
        repo_override: Some(repo_root.to_path_buf()),
        output_json: false,
    }))
}

fn map_rhai_error(error: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_rhai_embedded_command;
    use effigy_cli::{Command, DocsArgs, DocsSubcommand};
    use std::path::{Path, PathBuf};

    #[test]
    fn parse_rhai_embedded_command_defaults_repo_override_when_missing() {
        let command = parse_rhai_embedded_command(
            Path::new("/tmp/repo"),
            &["docs".to_owned(), "check-links".to_owned()],
            false,
        )
        .expect("parse rhai embedded command");

        assert!(matches!(
            command,
            Command::Docs(DocsArgs {
                subcommand: DocsSubcommand::CheckLinks { .. },
                repo_override: Some(path),
                output_json: false,
            }) if path == PathBuf::from("/tmp/repo")
        ));
    }

    #[test]
    fn parse_rhai_embedded_command_preserves_explicit_repo_override() {
        let command = parse_rhai_embedded_command(
            Path::new("/tmp/repo"),
            &[
                "docs".to_owned(),
                "check-links".to_owned(),
                "--repo".to_owned(),
                "/tmp/other".to_owned(),
            ],
            false,
        )
        .expect("parse rhai embedded command");

        assert!(matches!(
            command,
            Command::Docs(DocsArgs {
                subcommand: DocsSubcommand::CheckLinks { .. },
                repo_override: Some(path),
                output_json: false,
            }) if path == PathBuf::from("/tmp/other")
        ));
    }
}
