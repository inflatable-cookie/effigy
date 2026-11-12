use std::path::Path;

use effigy_cli::{
    ContractsArgs, ContractsSelectionPrintMode, ContractsSubcommand, DoctorArgs, TaskInvocation,
    TasksArgs,
};
use effigy_rhai::surface::*;
use serde_json::Value;

use super::*;
use crate::runner::error::RunnerError;

pub(super) fn handles(feature: &str) -> bool {
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
            | FEATURE_DOCTOR_RUN
            | FEATURE_SCAN_GOD_FILES
            | FEATURE_SCAN_DUPLICATE_BLOCKS
            | FEATURE_CONTRACTS_CHECK_JSON
            | FEATURE_TEST_PLAN
    )
}

pub(super) fn dispatch(
    repo_root: &Path,
    feature: &str,
    options: Value,
) -> Result<String, RunnerError> {
    match feature {
        FEATURE_TASKS_LIST | FEATURE_CATALOG_TASKS => run_typed_command(
            repo_root,
            effigy_cli::Command::Tasks(TasksArgs {
                all: false,
                repo_override: Some(repo_root.to_path_buf()),
                task_name: string_option(&options, "task")?,
                resolve_selector: string_option(&options, "resolve")?,
                status_selector: None,
                status_all: false,
                output_json: true,
                pretty_json: false,
                qa: None,
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
                all: false,
                repo_override: Some(repo_root.to_path_buf()),
                task_name: None,
                resolve_selector: Some(required_string(&options, "selector")?),
                status_selector: None,
                status_all: false,
                output_json: true,
                pretty_json: false,
                qa: None,
            }),
        ),
        FEATURE_TASKS_INFO => run_typed_command(
            repo_root,
            effigy_cli::Command::Tasks(TasksArgs {
                all: false,
                repo_override: Some(repo_root.to_path_buf()),
                task_name: Some(required_string(&options, "selector")?),
                resolve_selector: None,
                status_selector: None,
                status_all: false,
                output_json: true,
                pretty_json: false,
                qa: None,
            }),
        ),

        FEATURE_DOCTOR_RUN => run_typed_command(
            repo_root,
            effigy_cli::Command::Doctor(DoctorArgs {
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
                fix: bool_option(&options, "fix")?.unwrap_or(false),
                verbose: bool_option(&options, "verbose")?.unwrap_or(false),
                deep: bool_option(&options, "deep")?.unwrap_or(false),
                catalog: string_option(&options, "catalog")?,
                all_catalogs: bool_option(&options, "all_catalogs")?.unwrap_or(false),
                refresh: bool_option(&options, "refresh")?.unwrap_or(false),
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
        FEATURE_CONTRACTS_CHECK_JSON => run_typed_command(
            repo_root,
            effigy_cli::Command::Contracts(ContractsArgs {
                subcommand: ContractsSubcommand::CheckJson {
                    index_path: path_option(&options, "index")?,
                    contract_path: path_option(&options, "contract")?,
                    artifact_path: path_option(&options, "artifact")?,
                    changed_only_base: string_option(&options, "changed_only")?,
                    print_selected: match string_option(&options, "print_selected")?.as_deref() {
                        Some("json") => ContractsSelectionPrintMode::Json,
                        Some("text") => ContractsSelectionPrintMode::Text,
                        _ => ContractsSelectionPrintMode::None,
                    },
                    validate_selection: bool_option(&options, "validate_selection")?
                        .unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),

        FEATURE_TEST_PLAN => {
            let mut args = vec!["test".to_owned(), "--plan".to_owned()];
            if let Some(suite) = string_option(&options, "suite")? {
                args.push(suite);
            }
            run_builtin_json(repo_root, "test", args)
        }

        other => Err(RunnerError::task_invocation(format!(
            "unknown Rhai feature `{other}`"
        ))),
    }
}

pub(super) fn run_config_effective(repo_root: &Path) -> Result<String, RunnerError> {
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

pub(super) fn run_config_raw(repo_root: &Path) -> Result<String, RunnerError> {
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
            stale_local_install: None,
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

pub(super) fn run_config_get(repo_root: &Path, path: &str) -> Result<String, RunnerError> {
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

pub(super) fn get_toml_path<'a>(value: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            return None;
        }
        current = current.as_table()?.get(segment)?;
    }
    Some(current)
}

pub(super) fn toml_value_to_json(value: toml::Value) -> Result<serde_json::Value, RunnerError> {
    serde_json::to_value(value)
        .map_err(|error| RunnerError::Ui(format!("failed to encode config value: {error}")))
}

pub(super) fn scan_args(scan: &str, options: &Value) -> Result<Vec<String>, RunnerError> {
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
