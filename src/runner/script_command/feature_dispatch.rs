use std::path::{Path, PathBuf};

use effigy_cli::{
    ArtifactArgs, ArtifactSubcommand, BootstrapDbSeedInput, BundleArgs, BundleSubcommand,
    ChangelogArgs, ChangelogSubcommand, ContainerArgs, ContainerCacheSubcommand,
    ContainerDataSubcommand, ContainerDbDumpInput, ContainerSubcommand, ContainerVolumeSubcommand,
    ContractsArgs, ContractsCheckMode, ContractsSelectionPrintMode, ContractsSubcommand, DemoArgs,
    DemoListQuery, DemoSubcommand, DeployArgs, DeploySubcommand, DocsArgs, DocsBlockRequirement,
    DocsSubcommand, DoctorArgs, GatewayArgs, GatewaySubcommand, ReleaseArgs,
    ReleaseEvidenceSubcommand, ReleaseSubcommand, ServiceArgs, ServiceSubcommand, StateArgs,
    StateSubcommand, SystemArgs, SystemSubcommand, TaskInvocation, TasksArgs,
};
use effigy_execution::ExecutionSurface;
use effigy_rhai::surface::*;
use serde_json::Value;

use crate::runner::error::RunnerError;

const DEFAULT_DISTRIBUTION_REPO_URL: &str = "https://github.com/inflatable-cookie/effigy.git";
const DEFAULT_DISTRIBUTION_BREW_FORMULA: &str = "inflatable-cookie/effigy/effigy";

pub(super) fn run_rhai_feature(
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
                status_selector: None,
                status_all: false,
                output_json: true,
                pretty_json: false,
            }),
        ),
        FEATURE_CONFIG_EFFECTIVE => run_config_effective(repo_root),
        FEATURE_CONFIG_RAW => run_config_raw(repo_root),
        FEATURE_CONFIG_GET => run_config_get(repo_root, &required_string(&options, "path")?),
        FEATURE_CONFIG_USER_PATH => run_builtin_json(repo_root, "config", vec!["path".to_owned()]),
        FEATURE_CONFIG_USER_GET => run_builtin_json(
            repo_root,
            "config",
            vec!["get".to_owned(), required_string(&options, "key")?],
        ),
        FEATURE_CONFIG_USER_SET => run_builtin_json(
            repo_root,
            "config",
            vec![
                "set".to_owned(),
                required_string(&options, "key")?,
                required_string(&options, "value")?,
            ],
        ),
        FEATURE_CONFIG_USER_UNSET => run_builtin_json(
            repo_root,
            "config",
            vec!["unset".to_owned(), required_string(&options, "key")?],
        ),
        FEATURE_TASKS_RESOLVE => run_typed_command(
            repo_root,
            effigy_cli::Command::Tasks(TasksArgs {
                repo_override: Some(repo_root.to_path_buf()),
                task_name: None,
                resolve_selector: Some(required_string(&options, "selector")?),
                status_selector: None,
                status_all: false,
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
                status_selector: None,
                status_all: false,
                output_json: true,
                pretty_json: false,
            }),
        ),
        FEATURE_STATE_PLAN => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::Plan {
                    manifest: path_option(&options, "manifest")?,
                    stack: string_option(&options, "stack")?,
                    write_report: bool_option(&options, "write_report")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_STATE_APPLY => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::Apply {
                    manifest: path_option(&options, "manifest")?,
                    stack: string_option(&options, "stack")?,
                    yes: bool_option(&options, "yes")?.unwrap_or(true),
                    skip_layers: string_array(&options, "skip_layers")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_STATE_CAPTURE => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::Capture {
                    manifest: path_option(&options, "manifest")?,
                    stack: string_option(&options, "stack")?,
                    profile: string_option(&options, "profile")?,
                    role: string_option(&options, "role")?,
                    source_env: string_option(&options, "source_env")?,
                    key: string_option(&options, "key")?,
                    source: string_option(&options, "source")?,
                    destination_ref: string_option(&options, "destination_ref")?,
                    hook: string_option(&options, "hook")?,
                    task: string_option(&options, "task")?,
                    yes: bool_option(&options, "yes")?.unwrap_or(true),
                    push: bool_option(&options, "push")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_STATE_HISTORY => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::History {
                    stack: required_string(&options, "stack")?,
                    kind: string_option(&options, "kind")?,
                    limit: usize_option(&options, "limit")?,
                    lineage: string_option(&options, "lineage")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_CONTAINER_STATUS => {
            if let Some(name) = string_option(&options, "name")? {
                run_container_json(
                    repo_root,
                    ContainerSubcommand::Status {
                        name: Some(name),
                        global: false,
                    },
                )
            } else if bool_option(&options, "all")?.unwrap_or(false) {
                run_typed_command(
                    repo_root,
                    effigy_cli::Command::Container(ContainerArgs {
                        subcommand: ContainerSubcommand::Status {
                            name: None,
                            global: true,
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
                wipe_data: bool_option(&options, "wipe_data")?.unwrap_or(false),
                yes: bool_option(&options, "yes")?.unwrap_or(false),
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
        FEATURE_CONTAINER_DATA_DUMP => run_container_json(
            repo_root,
            ContainerSubcommand::Data {
                name: string_option(&options, "name")?,
                subcommand: ContainerDataSubcommand::Dump {
                    db_dumps: container_db_dump_inputs(&options, "db_dumps")?,
                    push: bool_option(&options, "push")?.unwrap_or(false),
                },
            },
        ),
        FEATURE_CONTAINER_DATA_SEED => run_container_json(
            repo_root,
            ContainerSubcommand::Data {
                name: None,
                subcommand: ContainerDataSubcommand::Seed {
                    db_seeds: bootstrap_db_seed_inputs(&options, "db_seeds")?,
                    no_prompt: bool_option(&options, "no_prompt")?.unwrap_or(true),
                    yes: bool_option(&options, "yes")?.unwrap_or(true),
                },
            },
        ),
        FEATURE_CONTAINER_DATA_PULL_PRODUCTION => run_container_json(
            repo_root,
            ContainerSubcommand::Data {
                name: Some(required_string(&options, "name")?),
                subcommand: ContainerDataSubcommand::PullProduction {
                    yes: bool_option(&options, "yes")?.unwrap_or(true),
                },
            },
        ),
        FEATURE_CONTAINER_CACHE_LIST => run_container_json(
            repo_root,
            ContainerSubcommand::Cache {
                name: string_option(&options, "name")?,
                subcommand: ContainerCacheSubcommand::List {
                    global: bool_option(&options, "global")?.unwrap_or(false),
                    project: string_option(&options, "project")?,
                    kind: string_option(&options, "kind")?,
                },
            },
        ),
        FEATURE_CONTAINER_CACHE_PRUNE => run_container_json(
            repo_root,
            ContainerSubcommand::Cache {
                name: string_option(&options, "name")?,
                subcommand: ContainerCacheSubcommand::Prune {
                    global: bool_option(&options, "global")?.unwrap_or(false),
                    yes: bool_option(&options, "yes")?.unwrap_or(true),
                    project: string_option(&options, "project")?,
                    kind: string_option(&options, "kind")?,
                },
            },
        ),
        FEATURE_CONTAINER_VOLUME_LIST => run_container_json(
            repo_root,
            ContainerSubcommand::Volume {
                subcommand: ContainerVolumeSubcommand::List {
                    global: bool_option(&options, "global")?.unwrap_or(false),
                    orphans: bool_option(&options, "orphans")?.unwrap_or(false),
                    dormant: bool_option(&options, "dormant")?.unwrap_or(false),
                },
            },
        ),
        FEATURE_CONTAINER_VOLUME_PRUNE => run_container_json(
            repo_root,
            ContainerSubcommand::Volume {
                subcommand: ContainerVolumeSubcommand::Prune {
                    global: bool_option(&options, "global")?.unwrap_or(false),
                    yes: bool_option(&options, "yes")?.unwrap_or(true),
                    orphans: bool_option(&options, "orphans")?.unwrap_or(false),
                    dormant: bool_option(&options, "dormant")?.unwrap_or(false),
                },
            },
        ),
        FEATURE_CONTAINER_EJECT => run_container_json(
            repo_root,
            ContainerSubcommand::Eject {
                name: Some(required_string(&options, "name")?),
            },
        ),
        FEATURE_CONTAINER_STATS => run_typed_command(
            repo_root,
            effigy_cli::Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Stats { global: true },
                repo_override: None,
                output_json: true,
            }),
        ),
        FEATURE_DOCS_CHECK_LINKS => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Links,
                paths: path_array(&options, "paths")?,
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_JSON_EXAMPLES => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::JsonExamples,
                paths: Vec::new(),
                file: path_option(&options, "file")?,
                section: string_option(&options, "section")?,
                min_blocks: usize_option(&options, "min_blocks")?,
                required_text: string_array(&options, "required")?,
                required_blocks: docs_block_requirements(&options)?,
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_HEADINGS => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Headings,
                paths: path_array(&options, "paths")?,
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: string_array(&options, "required_headings")?,
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_PATHS => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Paths,
                paths: path_array(&options, "paths")?,
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_CONTAINS => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Contains,
                paths: path_array(&options, "paths")?,
                file: None,
                section: None,
                min_blocks: None,
                required_text: string_array_any(&options, &["required_text", "required"])?,
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_FORBIDDEN => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Forbidden,
                paths: path_array(&options, "paths")?,
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: string_array_any(&options, &["forbidden_text", "forbidden"])?,
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_INDEX => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Index,
                paths: Vec::new(),
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(string_option(&options, "policy_index")?),
                dir: Box::new(path_option(&options, "dir")?),
                index: Box::new(path_option(&options, "index")?),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_NEXT_ACTION => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::NextAction,
                paths: Vec::new(),
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(string_option(&options, "policy_name")?),
            },
        ),
        FEATURE_DOCS_CHECK_WORKFLOW_PATHS => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::WorkflowPaths,
                paths: Vec::new(),
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(path_option(&options, "dir")?),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_ADD_LOG_INDEX => run_docs_json(
            repo_root,
            DocsSubcommand::AddLogIndex {
                log_path: PathBuf::from(required_string(&options, "log_path")?),
            },
        ),
        FEATURE_BUNDLE_INSPECT => run_typed_command(
            repo_root,
            effigy_cli::Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::Inspect,
                repo_override: Some(repo_root.to_path_buf()),
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
                    provider: required_string(&options, "provider")?,
                    path: PathBuf::from(required_string(&options, "path")?),
                    plan: bool_option(&options, "plan")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_PLAN => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Plan {
                    env: required_string(&options, "env")?,
                    write_report: bool_option(&options, "write_report")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_APPLY => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Apply {
                    env: required_string(&options, "env")?,
                    yes: bool_option(&options, "yes")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_STATUS => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Status {
                    env: required_string(&options, "env")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_HISTORY => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::History {
                    env: required_string(&options, "env")?,
                    limit: usize_option(&options, "limit")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_REDEPLOY => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Redeploy {
                    env: required_string(&options, "env")?,
                    deployment: required_string(&options, "deployment")?,
                    yes: bool_option(&options, "yes")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DISTRIBUTION_VALIDATE_METADATA => run_typed_command(
            repo_root,
            effigy_cli::Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Validate {
                    tag: string_option(&options, "tag")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DISTRIBUTION_CHECK_GLIBC_FLOOR => run_typed_command(
            repo_root,
            effigy_cli::Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::CheckBinary {
                    binary_path: path_option(&options, "binary")?
                        .ok_or_else(|| RunnerError::task_invocation("`binary` is required"))?,
                    glibc_floor: required_string(&options, "max_glibc")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DISTRIBUTION_PREFLIGHT => run_typed_command(
            repo_root,
            effigy_cli::Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Preflight {
                    tag: string_option(&options, "tag")?,
                    skip_docs: bool_option(&options, "skip_docs")?.unwrap_or(false),
                    skip_smoke: bool_option(&options, "skip_smoke")?.unwrap_or(false),
                    output_path: path_option(&options, "output")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DISTRIBUTION_FIRST_PUBLISH => run_typed_command(
            repo_root,
            effigy_cli::Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Proof {
                    tag: required_string(&options, "tag")?,
                    crate_version: string_option(&options, "crate_version")?,
                    repo_url: string_option(&options, "repo_url")?
                        .unwrap_or_else(|| DEFAULT_DISTRIBUTION_REPO_URL.to_owned()),
                    brew_formula: string_option(&options, "brew_formula")?
                        .unwrap_or_else(|| DEFAULT_DISTRIBUTION_BREW_FORMULA.to_owned()),
                    skip_homebrew: bool_option(&options, "skip_homebrew")?.unwrap_or(false),
                    artifacts_dir: path_option(&options, "artifacts_dir")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DISTRIBUTION_VALIDATE_ARTIFACTS => run_typed_command(
            repo_root,
            effigy_cli::Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Evidence {
                    subcommand: ReleaseEvidenceSubcommand::Validate {
                        artifacts_dir: path_option(&options, "artifacts_dir")?.ok_or_else(
                            || RunnerError::task_invocation("`artifacts_dir` is required"),
                        )?,
                        expect_homebrew: bool_option(&options, "expect_homebrew")?.unwrap_or(false),
                    },
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DISTRIBUTION_GENERATE_CLOSEOUT => run_typed_command(
            repo_root,
            effigy_cli::Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Evidence {
                    subcommand: ReleaseEvidenceSubcommand::Closeout {
                        tag: required_string(&options, "tag")?,
                        artifacts_dir: path_option(&options, "artifacts_dir")?.ok_or_else(
                            || RunnerError::task_invocation("`artifacts_dir` is required"),
                        )?,
                        output_path: path_option(&options, "output")?,
                        owner: string_option(&options, "owner")?
                            .unwrap_or_else(|| "Platform".to_owned()),
                        expect_homebrew: bool_option(&options, "expect_homebrew")?.unwrap_or(false),
                    },
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DISTRIBUTION_WRITE_SUMMARY => run_typed_command(
            repo_root,
            effigy_cli::Command::Release(ReleaseArgs {
                subcommand: ReleaseSubcommand::Evidence {
                    subcommand: ReleaseEvidenceSubcommand::Summary {
                        tag: required_string(&options, "tag")?,
                        artifacts_dir: path_option(&options, "artifacts_dir")?.ok_or_else(
                            || RunnerError::task_invocation("`artifacts_dir` is required"),
                        )?,
                        crate_version: string_option(&options, "crate_version")?,
                        repo_url: string_option(&options, "repo_url")?
                            .unwrap_or_else(|| DEFAULT_DISTRIBUTION_REPO_URL.to_owned()),
                        brew_formula: string_option(&options, "brew_formula")?
                            .unwrap_or_else(|| DEFAULT_DISTRIBUTION_BREW_FORMULA.to_owned()),
                        homebrew_executed: bool_option(&options, "homebrew_executed")?
                            .unwrap_or(false),
                        log_files: string_array_any(&options, &["log_files", "log_file"])?,
                    },
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
                repo_override: Some(repo_root.to_path_buf()),
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
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_ARTIFACT_INSPECT => run_typed_command(
            repo_root,
            effigy_cli::Command::Artifact(ArtifactArgs {
                subcommand: ArtifactSubcommand::Inspect {
                    source: required_string(&options, "source")?,
                    farmyard_handoff: bool_option(&options, "farmyard_handoff")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_ARTIFACT_STAGE => run_typed_command(
            repo_root,
            effigy_cli::Command::Artifact(ArtifactArgs {
                subcommand: ArtifactSubcommand::Stage {
                    source: required_string(&options, "source")?,
                    farmyard_handoff: bool_option(&options, "farmyard_handoff")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_ARTIFACT_CAPTURE => run_typed_command(
            repo_root,
            effigy_cli::Command::Artifact(ArtifactArgs {
                subcommand: ArtifactSubcommand::Capture {
                    source: required_string(&options, "source")?,
                    destination: required_string(&options, "destination")?,
                    kind: string_option(&options, "kind")?,
                    environment_label: string_option(&options, "environment_label")?,
                    farmyard_handoff: bool_option(&options, "farmyard_handoff")?.unwrap_or(false),
                    push: bool_option(&options, "push")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
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
        other if rhai_feature_descriptor(other).is_some() => Err(RunnerError::Ui(format!(
            "known Rhai feature `{other}` is not wired to a runner dispatch path"
        ))),
        other => Err(RunnerError::task_invocation(format!(
            "unknown Rhai feature `{other}`"
        ))),
    }
}

#[cfg(test)]
pub(super) fn is_runner_dispatch_feature(feature: &str) -> bool {
    matches!(
        feature,
        FEATURE_TASKS_LIST
            | FEATURE_CATALOG_TASKS
            | FEATURE_CONFIG_EFFECTIVE
            | FEATURE_CONFIG_RAW
            | FEATURE_CONFIG_GET
            | FEATURE_CONFIG_USER_PATH
            | FEATURE_CONFIG_USER_GET
            | FEATURE_CONFIG_USER_SET
            | FEATURE_CONFIG_USER_UNSET
            | FEATURE_TASKS_RESOLVE
            | FEATURE_TASKS_INFO
            | FEATURE_STATE_PLAN
            | FEATURE_STATE_APPLY
            | FEATURE_STATE_CAPTURE
            | FEATURE_STATE_HISTORY
            | FEATURE_CONTAINER_STATUS
            | FEATURE_CONTAINER_LOGS
            | FEATURE_CONTAINER_RESET
            | FEATURE_CONTAINER_DATA
            | FEATURE_CONTAINER_DATA_DUMP
            | FEATURE_CONTAINER_DATA_SEED
            | FEATURE_CONTAINER_DATA_PULL_PRODUCTION
            | FEATURE_CONTAINER_CACHE_LIST
            | FEATURE_CONTAINER_CACHE_PRUNE
            | FEATURE_CONTAINER_VOLUME_LIST
            | FEATURE_CONTAINER_VOLUME_PRUNE
            | FEATURE_CONTAINER_EJECT
            | FEATURE_CONTAINER_STATS
            | FEATURE_ARTIFACT_INSPECT
            | FEATURE_ARTIFACT_STAGE
            | FEATURE_ARTIFACT_CAPTURE
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
            | FEATURE_BUNDLE_INSPECT
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
            | FEATURE_DEPLOY_PLAN
            | FEATURE_DEPLOY_APPLY
            | FEATURE_DEPLOY_STATUS
            | FEATURE_DEPLOY_HISTORY
            | FEATURE_DEPLOY_REDEPLOY
            | FEATURE_DISTRIBUTION_VALIDATE_METADATA
            | FEATURE_DISTRIBUTION_CHECK_GLIBC_FLOOR
            | FEATURE_DISTRIBUTION_PREFLIGHT
            | FEATURE_DISTRIBUTION_FIRST_PUBLISH
            | FEATURE_DISTRIBUTION_VALIDATE_ARTIFACTS
            | FEATURE_DISTRIBUTION_GENERATE_CLOSEOUT
            | FEATURE_DISTRIBUTION_WRITE_SUMMARY
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
    crate::runner::execute::api::run_manifest_task_with_surface(
        &TaskInvocation {
            name: task.to_owned(),
            args,
        },
        repo_root.to_path_buf(),
        ExecutionSurface::Rhai,
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

fn container_db_dump_inputs(
    options: &Value,
    key: &str,
) -> Result<Vec<ContainerDbDumpInput>, RunnerError> {
    string_array(options, key)?
        .into_iter()
        .map(parse_container_db_dump_input)
        .collect()
}

fn bootstrap_db_seed_inputs(
    options: &Value,
    key: &str,
) -> Result<Vec<BootstrapDbSeedInput>, RunnerError> {
    string_array(options, key)?
        .into_iter()
        .map(parse_bootstrap_db_seed_input)
        .collect()
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
        .map(|entry: &Value| {
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

fn parse_container_db_dump_input(value: String) -> Result<ContainerDbDumpInput, RunnerError> {
    let (target, path) = split_targeted_path(value)?;
    Ok(ContainerDbDumpInput { target, path })
}

fn parse_bootstrap_db_seed_input(value: String) -> Result<BootstrapDbSeedInput, RunnerError> {
    let (target, path) = split_targeted_path(value)?;
    Ok(BootstrapDbSeedInput { target, path })
}

pub(super) fn split_targeted_path(value: String) -> Result<(Option<String>, PathBuf), RunnerError> {
    if let Some((target, path)) = value.split_once('=') {
        let target = target.trim();
        let path = path.trim();
        if target.is_empty() || path.is_empty() {
            return Err(RunnerError::task_invocation(format!(
                "invalid targeted path `{value}`"
            )));
        }
        return Ok((Some(target.to_owned()), PathBuf::from(path)));
    }
    let path = value.trim();
    if path.is_empty() {
        return Err(RunnerError::task_invocation("path value must not be empty"));
    }
    Ok((None, PathBuf::from(path)))
}
