use std::path::{Path, PathBuf};

use effigy_cli::{
    BootstrapDbSeedInput, ContainerArgs, ContainerCacheSubcommand, ContainerDataSubcommand,
    ContainerDbDumpInput, ContainerSubcommand, ContainerVolumeSubcommand,
};
use effigy_rhai::surface::*;
use serde_json::Value;

use super::*;
use crate::runner::error::RunnerError;

pub(super) fn handles(feature: &str) -> bool {
    matches!(
        feature,
        FEATURE_CONTAINER_WORKSPACE_ARTIFACT_BUILD
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
    )
}

pub(super) fn dispatch(
    repo_root: &Path,
    feature: &str,
    options: Value,
) -> Result<String, RunnerError> {
    match feature {
        FEATURE_CONTAINER_WORKSPACE_ARTIFACT_BUILD => {
            crate::runner::container_command::workspace_artifact_builder::build(
                repo_root,
                &string_array(&options, "command")?,
            )?;
            Ok(serde_json::json!({"success": true}).to_string())
        }
        FEATURE_CONTAINER_STATUS => {
            if let Some(name) = string_option(&options, "name")? {
                run_container_json(
                    repo_root,
                    ContainerSubcommand::Status {
                        name: Some(name),
                        system: None,
                        global: false,
                    },
                )
            } else if bool_option(&options, "all")?.unwrap_or(false) {
                run_typed_command(
                    repo_root,
                    effigy_cli::Command::Container(ContainerArgs {
                        subcommand: ContainerSubcommand::Status {
                            name: None,
                            system: None,
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
                    system: None,
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
        FEATURE_CONTAINER_EJECT => Err(RunnerError::task_invocation(
            "`container::eject` is retired in v0.15 with `compose_file`: generated Compose stays internal. Use `effigy container render <NAME>` to read the exact render",
        )),
        FEATURE_CONTAINER_STATS => run_typed_command(
            repo_root,
            effigy_cli::Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Stats { global: true },
                repo_override: None,
                output_json: true,
            }),
        ),

        other => Err(RunnerError::task_invocation(format!(
            "unknown Rhai feature `{other}`"
        ))),
    }
}

pub(super) fn container_db_dump_inputs(
    options: &Value,
    key: &str,
) -> Result<Vec<ContainerDbDumpInput>, RunnerError> {
    string_array(options, key)?
        .into_iter()
        .map(parse_container_db_dump_input)
        .collect()
}

pub(super) fn bootstrap_db_seed_inputs(
    options: &Value,
    key: &str,
) -> Result<Vec<BootstrapDbSeedInput>, RunnerError> {
    string_array(options, key)?
        .into_iter()
        .map(parse_bootstrap_db_seed_input)
        .collect()
}

pub(super) fn parse_container_db_dump_input(
    value: String,
) -> Result<ContainerDbDumpInput, RunnerError> {
    let (target, path) = split_targeted_path(value)?;
    Ok(ContainerDbDumpInput { target, path })
}

pub(super) fn parse_bootstrap_db_seed_input(
    value: String,
) -> Result<BootstrapDbSeedInput, RunnerError> {
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
