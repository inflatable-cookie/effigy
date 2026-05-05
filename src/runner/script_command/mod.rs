use std::path::{Path, PathBuf};
use std::sync::Arc;

use effigy_rhai::{
    execute_rhai_script, install_stop_requested_flag, load_script, load_script_args_from_env,
    required_env, surface::*, EffigyCommandError, HostCallbacks, HostCommandOutput, ScriptContext,
    EFFIGY_RHAI_REPO_ROOT, EFFIGY_RHAI_TASK_NAME,
};

use effigy_cli::{
    BundleArgs, BundleSubcommand, ChangelogArgs, ChangelogSubcommand, ContainerArgs,
    ContainerDataSubcommand, ContainerSubcommand, ContractsArgs, ContractsCheckMode,
    ContractsSelectionPrintMode, ContractsSubcommand, DemoArgs, DemoListQuery, DemoSubcommand,
    DeployArgs, DeployExportProvider, DeploySubcommand, DocsArgs, DocsBlockRequirement,
    DocsSubcommand, DoctorArgs, GatewayArgs, GatewaySubcommand, InternalRhaiArgs, ServiceArgs,
    ServiceSubcommand, SystemArgs, SystemSubcommand, TaskInvocation, TasksArgs,
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
        container_down: Arc::new(|repo_root, name, all| {
            run_container_helper(
                repo_root,
                ContainerSubcommand::Down {
                    name: if all { None } else { Some(name.to_owned()) },
                    all,
                },
            )
            .map_err(|error| error.to_string())
        }),
        container_shell: Arc::new(|repo_root, name, service, command| {
            run_container_helper(
                repo_root,
                ContainerSubcommand::Shell {
                    name: Some(name.to_owned()),
                    service: service.map(str::to_owned),
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
        container_exec_with_options: Arc::new(|repo_root, name, service, command, options| {
            let name = if name.is_empty() { None } else { Some(name) };
            let stdin_file = options
                .get("stdin_file")
                .and_then(Value::as_str)
                .map(std::path::PathBuf::from);
            let output = crate::runner::container_command::run_container_exec_capture_with_options(
                repo_root,
                name,
                service,
                command,
                stdin_file.as_deref(),
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
        FEATURE_TASKS_LIST | FEATURE_CATALOG_TASKS => run_typed_command(
            repo_root,
            effigy_cli::Command::Tasks(TasksArgs {
                repo_override: Some(repo_root.to_path_buf()),
                task_name: string_option(&options, "task")?,
                resolve_selector: string_option(&options, "resolve")?,
                output_json: true,
                pretty_json: false,
            }),
        ),
        FEATURE_CONFIG_EFFECTIVE => run_config_effective(repo_root),
        FEATURE_CONFIG_RAW => run_config_raw(repo_root),
        FEATURE_CONFIG_GET => run_config_get(repo_root, &required_string(&options, "path")?),
        FEATURE_TASKS_RESOLVE => run_typed_command(
            repo_root,
            effigy_cli::Command::Tasks(TasksArgs {
                repo_override: Some(repo_root.to_path_buf()),
                task_name: None,
                resolve_selector: Some(required_string(&options, "selector")?),
                output_json: true,
                pretty_json: false,
            }),
        ),
        FEATURE_TASKS_INFO => run_typed_command(
            repo_root,
            effigy_cli::Command::Tasks(TasksArgs {
                repo_override: Some(repo_root.to_path_buf()),
                task_name: Some(required_string(&options, "selector")?),
                resolve_selector: None,
                output_json: true,
                pretty_json: false,
            }),
        ),
        FEATURE_CONTAINER_STATUS => {
            if let Some(name) = string_option(&options, "name")? {
                run_container_json(
                    repo_root,
                    ContainerSubcommand::Status {
                        name: Some(name),
                        all: false,
                    },
                )
            } else if bool_option(&options, "all")?.unwrap_or(false) {
                run_typed_command(
                    repo_root,
                    effigy_cli::Command::Container(ContainerArgs {
                        subcommand: ContainerSubcommand::Status {
                            name: None,
                            all: true,
                        },
                        repo_override: None,
                        output_json: true,
                    }),
                )
            } else {
                Err(RunnerError::task_invocation(
                    "`container::status(...)` requires either a `name` or `all = true`",
                ))
            }
        }
        FEATURE_CONTAINER_LOGS => {
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
        FEATURE_CONTAINER_RESET => run_container_json(
            repo_root,
            ContainerSubcommand::Reset {
                name: Some(required_string(&options, "name")?),
                keep_data: bool_option(&options, "keep_data")?.unwrap_or(false),
            },
        ),
        FEATURE_CONTAINER_DATA => {
            let name = required_string(&options, "name")?;
            match required_string(&options, "operation")?.as_str() {
                "list" => run_container_json(
                    repo_root,
                    ContainerSubcommand::Data {
                        name: Some(name),
                        subcommand: ContainerDataSubcommand::List,
                    },
                ),
                "export" => run_container_json(
                    repo_root,
                    ContainerSubcommand::Data {
                        name: Some(name),
                        subcommand: ContainerDataSubcommand::Export {
                            volume: required_string(&options, "volume")?,
                            path: PathBuf::from(required_string(&options, "path")?),
                        },
                    },
                ),
                "import" => run_container_json(
                    repo_root,
                    ContainerSubcommand::Data {
                        name: Some(name),
                        subcommand: ContainerDataSubcommand::Import {
                            volume: required_string(&options, "volume")?,
                            path: PathBuf::from(required_string(&options, "path")?),
                            yes: true,
                        },
                    },
                ),
                "pull_production" => run_container_json(
                    repo_root,
                    ContainerSubcommand::Data {
                        name: Some(name),
                        subcommand: ContainerDataSubcommand::PullProduction { yes: true },
                    },
                ),
                other => Err(RunnerError::task_invocation(format!(
                    "`container::data(...)` does not support operation `{other}`"
                ))),
            }
        }
        FEATURE_CONTAINER_EJECT => run_container_json(
            repo_root,
            ContainerSubcommand::Eject {
                name: Some(required_string(&options, "name")?),
            },
        ),
        FEATURE_CONTAINER_STATS => run_typed_command(
            repo_root,
            effigy_cli::Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Stats { all: true },
                repo_override: None,
                output_json: true,
            }),
        ),
        FEATURE_DOCS_CHECK_LINKS => run_docs_json(
            repo_root,
            DocsSubcommand::CheckLinks {
                paths: path_array(&options, "paths")?,
            },
        ),
        FEATURE_DOCS_CHECK_JSON_EXAMPLES => run_docs_json(
            repo_root,
            DocsSubcommand::CheckJsonExamples {
                file: path_option(&options, "file")?,
                section: string_option(&options, "section")?,
                min_blocks: usize_option(&options, "min_blocks")?,
                required: string_array(&options, "required")?,
                required_blocks: docs_block_requirements(&options)?,
            },
        ),
        FEATURE_DOCS_CHECK_HEADINGS => run_docs_json(
            repo_root,
            DocsSubcommand::CheckHeadings {
                paths: path_array(&options, "paths")?,
                required_headings: string_array(&options, "required_headings")?,
            },
        ),
        FEATURE_DOCS_CHECK_PATHS => run_docs_json(
            repo_root,
            DocsSubcommand::CheckPaths {
                paths: path_array(&options, "paths")?,
            },
        ),
        FEATURE_DOCS_CHECK_CONTAINS => run_docs_json(
            repo_root,
            DocsSubcommand::CheckContains {
                paths: path_array(&options, "paths")?,
                required_text: string_array_any(&options, &["required_text", "required"])?,
            },
        ),
        FEATURE_DOCS_CHECK_FORBIDDEN => run_docs_json(
            repo_root,
            DocsSubcommand::CheckForbidden {
                paths: path_array(&options, "paths")?,
                forbidden_text: string_array_any(&options, &["forbidden_text", "forbidden"])?,
            },
        ),
        FEATURE_DOCS_CHECK_INDEX => run_docs_json(
            repo_root,
            DocsSubcommand::CheckIndex {
                policy_index: string_option(&options, "policy_index")?,
                dir: path_option(&options, "dir")?,
                index: path_option(&options, "index")?,
            },
        ),
        FEATURE_DOCS_CHECK_NEXT_ACTION => run_docs_json(
            repo_root,
            DocsSubcommand::CheckNextAction {
                policy_name: string_option(&options, "policy_name")?,
            },
        ),
        FEATURE_DOCS_CHECK_WORKFLOW_PATHS => run_docs_json(
            repo_root,
            DocsSubcommand::CheckWorkflowPaths {
                dir: path_option(&options, "dir")?,
            },
        ),
        FEATURE_DOCS_ADD_LOG_INDEX => run_docs_json(
            repo_root,
            DocsSubcommand::AddLogIndex {
                log_path: PathBuf::from(required_string(&options, "log_path")?),
            },
        ),
        FEATURE_BUNDLE_LIST => run_typed_command(
            repo_root,
            effigy_cli::Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::List,
                output_json: true,
            }),
        ),
        FEATURE_BUNDLE_INSPECT => run_typed_command(
            repo_root,
            effigy_cli::Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::Inspect {
                    bundle: required_string(&options, "bundle")?,
                },
                output_json: true,
            }),
        ),
        FEATURE_BUNDLE_EMIT => run_typed_command(
            repo_root,
            effigy_cli::Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::Export {
                    bundle: required_string(&options, "bundle")?,
                    path: PathBuf::from(required_string(&options, "path")?),
                },
                output_json: true,
            }),
        ),
        FEATURE_SERVICE_LIST => run_typed_command(
            repo_root,
            effigy_cli::Command::Service(ServiceArgs {
                subcommand: ServiceSubcommand::List,
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_SERVICE_EXTRACT => run_typed_command(
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
        FEATURE_GATEWAY_STATUS => run_gateway_json(GatewaySubcommand::Status),
        FEATURE_GATEWAY_SETUP_TLS => run_gateway_json(GatewaySubcommand::SetupTls),
        FEATURE_GATEWAY_UP => run_gateway_json(GatewaySubcommand::Up),
        FEATURE_GATEWAY_DOWN => run_gateway_json(GatewaySubcommand::Down),
        FEATURE_DOCTOR_RUN => run_typed_command(
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
        FEATURE_SCAN_GOD_FILES => {
            run_builtin_json(repo_root, "scan", scan_args("god-files", &options)?)
        }
        FEATURE_SCAN_DUPLICATE_BLOCKS => {
            run_builtin_json(repo_root, "scan", scan_args("duplicate-blocks", &options)?)
        }
        FEATURE_SCAN_COMMENT_RATIO => {
            run_builtin_json(repo_root, "scan", scan_args("comment-ratio", &options)?)
        }
        FEATURE_SCAN_GENERATED_ASSETS => {
            run_builtin_json(repo_root, "scan", scan_args("generated-assets", &options)?)
        }
        FEATURE_SCAN_GENERATED_IN_SRC => {
            run_builtin_json(repo_root, "scan", scan_args("generated-in-src", &options)?)
        }
        FEATURE_SCAN_ATTENTION_MARKERS => {
            run_builtin_json(repo_root, "scan", scan_args("attention-markers", &options)?)
        }
        FEATURE_SCAN_STALE_SUPPRESSIONS => run_builtin_json(
            repo_root,
            "scan",
            scan_args("stale-suppressions", &options)?,
        ),
        FEATURE_CACHE_INSPECT => {
            run_builtin_json(repo_root, "cache", cache_inspect_args(&options)?)
        }
        FEATURE_CACHE_INVALIDATE => {
            run_builtin_json(repo_root, "cache", cache_invalidate_args(&options)?)
        }
        FEATURE_CONTRACTS_CHECK_JSON => run_typed_command(
            repo_root,
            effigy_cli::Command::Contracts(ContractsArgs {
                subcommand: ContractsSubcommand::CheckJson {
                    index_path: path_option(&options, "index")?,
                    mode: match string_option(&options, "mode")?.as_deref() {
                        Some("full") => ContractsCheckMode::Full,
                        _ => ContractsCheckMode::Fast,
                    },
                    changed_only_base: string_option(&options, "changed_only")?,
                    print_selected: match string_option(&options, "print_selected")?.as_deref() {
                        Some("json") => ContractsSelectionPrintMode::Json,
                        Some("text") => ContractsSelectionPrintMode::Text,
                        _ => ContractsSelectionPrintMode::None,
                    },
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_CONTRACTS_VALIDATE_SELECTION => run_typed_command(
            repo_root,
            effigy_cli::Command::Contracts(ContractsArgs {
                subcommand: ContractsSubcommand::ValidateSelection {
                    contract_path: path_option(&options, "contract")?,
                    artifact_path: path_option(&options, "artifact")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_MODEL => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Model,
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_EMIT => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Export {
                    provider: match required_string(&options, "provider")?.as_str() {
                        "render" => DeployExportProvider::Render,
                        "railway" => DeployExportProvider::Railway,
                        other => {
                            return Err(RunnerError::task_invocation(format!(
                                "`deploy::emit(...)` does not support provider `{other}`"
                            )));
                        }
                    },
                    path: PathBuf::from(required_string(&options, "path")?),
                    plan: bool_option(&options, "plan")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_SYSTEM_STATUS => run_typed_command(
            repo_root,
            effigy_cli::Command::System(SystemArgs {
                subcommand: SystemSubcommand::Status,
                system: string_option(&options, "system")?,
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_SYSTEM_LOGS => {
            if bool_option(&options, "follow")?.unwrap_or(false) {
                return Err(RunnerError::task_invocation(
                    "`system_logs` does not support `follow = true` from Rhai",
                ));
            }
            run_typed_command(
                repo_root,
                effigy_cli::Command::System(SystemArgs {
                    subcommand: SystemSubcommand::Logs { follow: false },
                    system: string_option(&options, "system")?,
                    repo_override: Some(repo_root.to_path_buf()),
                    output_json: true,
                }),
            )
        }
        FEATURE_DEMO_LIST => run_typed_command(
            repo_root,
            effigy_cli::Command::Demo(DemoArgs {
                subcommand: DemoSubcommand::List {
                    query: DemoListQuery {
                        search: string_option(&options, "search")?,
                        owner: string_option(&options, "owner")?,
                        tag: string_option(&options, "tag")?,
                        mode: None,
                        cover: string_option(&options, "cover")?,
                        status: None,
                        gap: None,
                        stale_only: bool_option(&options, "stale_only")?.unwrap_or(false),
                        group_by: None,
                    },
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEMO_INSPECT => run_typed_command(
            repo_root,
            effigy_cli::Command::Demo(DemoArgs {
                subcommand: DemoSubcommand::Inspect {
                    demo_id: required_string(&options, "demo_id")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEMO_HISTORY => run_typed_command(
            repo_root,
            effigy_cli::Command::Demo(DemoArgs {
                subcommand: DemoSubcommand::History {
                    demo_id: required_string(&options, "demo_id")?,
                    limit: usize_option(&options, "limit")?,
                    outcome: match string_option(&options, "outcome")?.as_deref() {
                        Some("passed") => Some(effigy_cli::DemoHistoryOutcome::Passed),
                        Some("failed") => Some(effigy_cli::DemoHistoryOutcome::Failed),
                        Some("terminated") => Some(effigy_cli::DemoHistoryOutcome::Terminated),
                        _ => None,
                    },
                    attempt_id: string_option(&options, "attempt_id")?,
                    attempt_ordinal: usize_option(&options, "attempt_ordinal")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_CHANGELOG_VALIDATE => run_typed_command(
            repo_root,
            effigy_cli::Command::Changelog(ChangelogArgs {
                subcommand: ChangelogSubcommand::Validate,
                file: path_option(&options, "file")?,
                output_json: true,
            }),
        ),
        FEATURE_CHANGELOG_EXTRACT => run_typed_command(
            repo_root,
            effigy_cli::Command::Changelog(ChangelogArgs {
                subcommand: ChangelogSubcommand::Extract {
                    version: required_string(&options, "version")?,
                },
                file: path_option(&options, "file")?,
                output_json: true,
            }),
        ),
        FEATURE_UNLOCK_SCOPES => {
            let mut args = vec!["unlock".to_owned()];
            if bool_option(&options, "all")?.unwrap_or(false) {
                args.push("--all".to_owned());
            }
            args.extend(string_array(&options, "scopes")?);
            args.push("--yes".to_owned());
            run_builtin_json(repo_root, "unlock", args)
        }
        FEATURE_TEST_PLAN => {
            let mut args = vec!["test".to_owned(), "--plan".to_owned()];
            if let Some(suite) = string_option(&options, "suite")? {
                args.push(suite);
            }
            run_builtin_json(repo_root, "test", args)
        }
        other if FEATURE_NAMES.contains(&other) => Err(RunnerError::Ui(format!(
            "known Rhai feature `{other}` is not wired to a runner dispatch path"
        ))),
        other => Err(RunnerError::task_invocation(format!(
            "unknown Rhai feature `{other}`"
        ))),
    }
}

#[cfg(test)]
fn is_runner_dispatch_feature(feature: &str) -> bool {
    matches!(
        feature,
        FEATURE_TASKS_LIST
            | FEATURE_CATALOG_TASKS
            | FEATURE_CONFIG_EFFECTIVE
            | FEATURE_CONFIG_RAW
            | FEATURE_CONFIG_GET
            | FEATURE_TASKS_RESOLVE
            | FEATURE_TASKS_INFO
            | FEATURE_CONTAINER_STATUS
            | FEATURE_CONTAINER_LOGS
            | FEATURE_CONTAINER_RESET
            | FEATURE_CONTAINER_DATA
            | FEATURE_CONTAINER_EJECT
            | FEATURE_CONTAINER_STATS
            | FEATURE_DOCS_CHECK_LINKS
            | FEATURE_DOCS_CHECK_JSON_EXAMPLES
            | FEATURE_DOCS_CHECK_HEADINGS
            | FEATURE_DOCS_CHECK_PATHS
            | FEATURE_DOCS_CHECK_CONTAINS
            | FEATURE_DOCS_CHECK_FORBIDDEN
            | FEATURE_DOCS_CHECK_INDEX
            | FEATURE_DOCS_CHECK_NEXT_ACTION
            | FEATURE_DOCS_CHECK_WORKFLOW_PATHS
            | FEATURE_DOCS_ADD_LOG_INDEX
            | FEATURE_BUNDLE_LIST
            | FEATURE_BUNDLE_INSPECT
            | FEATURE_BUNDLE_EMIT
            | FEATURE_SERVICE_LIST
            | FEATURE_SERVICE_EXTRACT
            | FEATURE_GATEWAY_STATUS
            | FEATURE_GATEWAY_SETUP_TLS
            | FEATURE_GATEWAY_UP
            | FEATURE_GATEWAY_DOWN
            | FEATURE_DOCTOR_RUN
            | FEATURE_SCAN_GOD_FILES
            | FEATURE_SCAN_DUPLICATE_BLOCKS
            | FEATURE_SCAN_COMMENT_RATIO
            | FEATURE_SCAN_GENERATED_ASSETS
            | FEATURE_SCAN_GENERATED_IN_SRC
            | FEATURE_SCAN_ATTENTION_MARKERS
            | FEATURE_SCAN_STALE_SUPPRESSIONS
            | FEATURE_CACHE_INSPECT
            | FEATURE_CACHE_INVALIDATE
            | FEATURE_CONTRACTS_CHECK_JSON
            | FEATURE_CONTRACTS_VALIDATE_SELECTION
            | FEATURE_DEPLOY_MODEL
            | FEATURE_DEPLOY_EMIT
            | FEATURE_SYSTEM_STATUS
            | FEATURE_SYSTEM_LOGS
            | FEATURE_DEMO_LIST
            | FEATURE_DEMO_INSPECT
            | FEATURE_DEMO_HISTORY
            | FEATURE_CHANGELOG_VALIDATE
            | FEATURE_CHANGELOG_EXTRACT
            | FEATURE_UNLOCK_SCOPES
            | FEATURE_TEST_PLAN
    )
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
mod tests;
