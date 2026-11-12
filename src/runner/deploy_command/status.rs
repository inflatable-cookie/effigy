use std::fs;
use std::path::Path;

use effigy_state::path_display;
use serde_json::Value;

use super::inputs::deploy_execution_inputs;
use super::plan::deploy_provider_context;
use super::provider::phase::run_provider_status;
use super::provider::{DeployProviderFetch, DeployProviderProvenance};
use super::report::*;
use super::text::{render_deploy_history_text, render_deploy_status_text};
use crate::runner::error::RunnerError;
use crate::runner::render::render_command_result;

pub(super) fn run_deploy_status(
    repo_root: &Path,
    env: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let active_path = deploy_active_path(repo_root, env);
    let latest_path = deploy_latest_path(repo_root, env);
    let active = read_optional_json(&active_path)?;
    let latest = read_optional_json(&latest_path)?;
    let provider = deploy_status_provider_report(repo_root, env)?;
    let report = DeployStatusReport {
        schema: STATUS_SCHEMA.to_owned(),
        schema_version: 1,
        env: env.to_owned(),
        active_path: active
            .as_ref()
            .map(|_| path_display(&active_path, repo_root)),
        latest_path: latest
            .as_ref()
            .map(|_| path_display(&latest_path, repo_root)),
        active,
        latest,
        provider_status: provider.provider_status,
        provider_provenance: provider.provider_provenance,
        warnings: provider.warnings,
    };
    let text = render_deploy_status_text(&report);
    render_command_result(output_json, true, json_value(&report)?, text)
}

pub(super) fn run_deploy_history(
    repo_root: &Path,
    env: &str,
    limit: Option<usize>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let history_dir = deploy_history_dir(repo_root, env);
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    if history_dir.exists() {
        let read_dir = fs::read_dir(&history_dir).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to read deploy history directory {}: {error}",
                history_dir.display()
            ))
        })?;
        for entry in read_dir {
            let entry = entry.map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to read deploy history entry in {}: {error}",
                    history_dir.display()
                ))
            })?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            match read_optional_json(&path)? {
                Some(value) => entries.push(DeployHistoryItem {
                    path: path_display(&path, repo_root),
                    deployment_id: value
                        .get("deployment_id")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_owned(),
                    schema: value
                        .get("schema")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_owned(),
                    status: value
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_owned(),
                }),
                None => warnings.push(format!(
                    "ignored unreadable history file {}",
                    path.display()
                )),
            }
        }
    }
    entries.sort_by(|left, right| right.path.cmp(&left.path));
    entries.truncate(limit.unwrap_or(20));
    let report = DeployHistoryReport {
        schema: HISTORY_SCHEMA.to_owned(),
        schema_version: 1,
        env: env.to_owned(),
        entries,
        warnings,
    };
    let text = render_deploy_history_text(&report);
    render_command_result(output_json, true, json_value(&report)?, text)
}

/// Provider half of a `deploy status` read.
///
/// Status is an observation, not a gate: when the provider package cannot be
/// resolved or its `status` phase fails, the read still succeeds and the reason
/// travels as a warning. The projection therefore carries the provider's own
/// report and the provenance of the package that produced it only when a
/// package was resolved, and the warnings collected while resolving or querying
/// it in every case.
struct DeployStatusProviderObservation {
    provider_status: Option<Value>,
    provider_provenance: Option<DeployProviderProvenance>,
    warnings: Vec<String>,
}

fn deploy_status_provider_report(
    repo_root: &Path,
    env: &str,
) -> Result<DeployStatusProviderObservation, RunnerError> {
    // Status observes a live provider, so it may refresh the cached checkout.
    match deploy_execution_inputs(repo_root, env, DeployProviderFetch::Online) {
        Ok((env_config, model, Some(provider_package))) => {
            let report = run_provider_status(
                repo_root,
                env_config.provider.adapter(),
                &provider_package,
                deploy_provider_context("status", env, &env_config, &model, &provider_package)?,
            )?;
            Ok(DeployStatusProviderObservation {
                provider_status: report.map(|report| json_value(&report)).transpose()?,
                provider_provenance: Some(provider_package.provenance.clone()),
                warnings: provider_package.warnings.clone(),
            })
        }
        Ok((env_config, _model, None)) => Ok(DeployStatusProviderObservation {
            provider_status: None,
            provider_provenance: None,
            warnings: vec![format!(
                "deploy provider `{}` has no configured provider package; provider status skipped",
                env_config.provider.adapter()
            )],
        }),
        Err(error) => Ok(DeployStatusProviderObservation {
            provider_status: None,
            provider_provenance: None,
            warnings: vec![format!("provider status skipped: {error}")],
        }),
    }
}
