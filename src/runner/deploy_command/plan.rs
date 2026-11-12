use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use effigy_state::{path_display, safe_path_component};

use serde_json::Value;

use super::code_ref::resolve_code_ref;
use super::config::{ArtifactPolicy, ManifestDeployEnvConfig, ReleasePolicy};
use super::inputs::deploy_execution_inputs;
use super::provider::context::{
    build_provider_context, provider_package_policy_blockers, DeployProviderContextRequest,
};
use super::provider::phase::run_provider_preflight;
use super::provider::report::DeployProviderCheckStatus;
use super::provider::{DeployProviderFetch, DeployProviderPackage};
use super::report::*;
use super::text::render_deploy_plan_text;
use crate::runner::error::RunnerError;
use crate::runner::render::render_command_result;

pub(super) fn run_deploy_plan(
    repo_root: &Path,
    env: &str,
    write_report: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let mut report = build_deploy_plan(repo_root, env)?;
    if write_report {
        let paths = deploy_report_paths(repo_root, env, &report.deployment_id);
        report.written_report_path = Some(path_display(&paths.latest_path, repo_root));
        report.written_history_path = Some(path_display(&paths.history_path, repo_root));
        write_json_report(
            repo_root,
            &[&paths.latest_path, &paths.history_path],
            &report,
        )?;
    }
    let ok = report.blockers.is_empty();
    let text = render_deploy_plan_text(&report);
    render_command_result(output_json, ok, json_value(&report)?, text)
}

/// The id of one deployment attempt: the second it started, the environment
/// and the commit it resolved, so two attempts never share a report path.
///
/// The clock prefix is whole seconds since the Unix epoch, not
/// `effigy_state`'s `YYYYMMDDThhmmssZ` stamp: the id is a file name that must
/// sort attempts lexically, and `effigy_state::paths` owns the canonical
/// timestamp format.
fn deployment_id(env: &str, resolved_commit: Option<&str>, started: SystemTime) -> String {
    format!(
        "{}-{}-{}",
        seconds_since_epoch(started),
        safe_path_component(env),
        resolved_commit.unwrap_or("unresolved")
    )
}

fn seconds_since_epoch(started: SystemTime) -> u64 {
    started
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn build_deploy_plan(repo_root: &Path, env: &str) -> Result<DeployPlanReport, RunnerError> {
    // A plan is a preview: it resolves the provider package offline, from the
    // cache alone, and refuses rather than fetching when the cache is missing.
    let (env_config, model, provider_package) =
        deploy_execution_inputs(repo_root, env, DeployProviderFetch::Offline)?;
    let code = resolve_code_ref(repo_root, &env_config.code_ref)?;
    let mut blockers = Vec::new();
    let mut warnings = provider_package
        .as_ref()
        .map(|package| package.warnings.clone())
        .unwrap_or_default();
    if env_config.release_policy == ReleasePolicy::Required && code.kind != "tag" {
        blockers.push(
            "release_policy `required` requires `code_ref = \"release-tag\"` or a tag ref"
                .to_owned(),
        );
    }
    if env_config.release_policy == ReleasePolicy::Required && code.resolved_commit.is_none() {
        blockers.push(
            "release_policy `required` requires a resolvable release tag or immutable commit"
                .to_owned(),
        );
    }
    if env_config.artifact_policy == ArtifactPolicy::DigestPinned {
        warnings.push("digest-pinned artifact policy will be enforced against state artifact refs in the provider execution slice".to_owned());
    }
    let state = env_config.state.as_ref().map(|stack| DeployStatePlan {
        stack: stack.clone(),
        lineage_id: format!("{stack}-planned"),
        planned_report_path: format!(".effigy/reports/state/{stack}/latest-plan.json"),
    });
    let provider_preflight = provider_preflight_report(
        repo_root,
        env,
        &env_config,
        &model,
        provider_package.as_ref(),
    )?;
    blockers.extend(provider_preflight.blockers.clone());
    let hooks = env_config
        .hooks
        .iter()
        .flat_map(|hooks| {
            [
                ("before_state", hooks.before_state.clone()),
                ("after_state", hooks.after_state.clone()),
                ("after_deploy", hooks.after_deploy.clone()),
            ]
            .into_iter()
            .filter_map(|(stage, task)| {
                task.map(|task| DeployHookPlan {
                    stage: stage.to_owned(),
                    task,
                })
            })
        })
        .collect();
    let health_checks = model
        .services
        .iter()
        .filter_map(|service| {
            service.health.as_ref().map(|health| DeployHealthPlan {
                service: service.name.clone(),
                kind: health.kind.clone(),
                path: health.path.clone(),
            })
        })
        .collect();
    let deployment_id = deployment_id(env, code.resolved_commit.as_deref(), SystemTime::now());
    Ok(DeployPlanReport {
        schema: PLAN_SCHEMA.to_owned(),
        schema_version: 1,
        deployment_id,
        env: env.to_owned(),
        provider: env_config.provider.adapter().to_owned(),
        provider_provenance: provider_package
            .as_ref()
            .map(|package| package.provenance.clone()),
        app: DeployPlanApp {
            name: model.app.name,
            project_name: env_config
                .provider_project
                .clone()
                .unwrap_or(model.app.project_name),
        },
        code,
        release_policy: DeployReleasePolicyReport {
            mode: env_config.release_policy.as_str().to_owned(),
            required: env_config.release_policy == ReleasePolicy::Required,
            gates_required: env_config
                .preflight
                .as_ref()
                .map(|preflight| preflight.require_release_gates)
                .unwrap_or(false),
        },
        state,
        artifact_policy: DeployArtifactPolicyReport {
            mode: env_config.artifact_policy.as_str().to_owned(),
            blockers: Vec::new(),
        },
        provider_preflight,
        hooks,
        health_checks,
        warnings,
        blockers,
        written_report_path: None,
        written_history_path: None,
    })
}

fn provider_preflight_report(
    repo_root: &Path,
    env: &str,
    env_config: &ManifestDeployEnvConfig,
    model: &super::model::DeployModel,
    provider_package: Option<&DeployProviderPackage>,
) -> Result<DeployProviderPreflightReport, RunnerError> {
    let provider = env_config.provider.adapter().trim().to_ascii_lowercase();
    let Some(package) = provider_package else {
        return Ok(DeployProviderPreflightReport {
            status: "blocked".to_owned(),
            checks: vec![DeployProviderCheck {
                name: "provider-package".to_owned(),
                status: DeployProviderCheckStatus::Blocked.as_str().to_owned(),
                target: Some(provider.clone()),
                message: Some(format!(
                    "deploy provider `{}` must be configured under `[deploy.providers.{}`]",
                    provider, provider
                )),
            }],
            blockers: vec![format!(
                "deploy provider `{}` has no configured provider package",
                provider,
            )],
        });
    };

    let mut checks = vec![DeployProviderCheck {
        name: "provider-package".to_owned(),
        status: DeployProviderCheckStatus::Planned.as_str().to_owned(),
        target: Some(package.root.display().to_string()),
        message: Some(format!(
            "{} {} resolved from provider package",
            package.descriptor.provider.display_name, package.descriptor.provider.version
        )),
    }];
    checks.push(DeployProviderCheck {
        name: "provider-source".to_owned(),
        status: DeployProviderCheckStatus::Planned.as_str().to_owned(),
        target: package.provenance.commit.clone(),
        message: Some(format!(
            "{} ({} source)",
            package.provenance.describe(),
            package.provenance.kind
        )),
    });
    let policy_blockers = provider_package_policy_blockers(&provider, &package.descriptor.policy);
    if !policy_blockers.is_empty() {
        return Ok(DeployProviderPreflightReport {
            status: "blocked".to_owned(),
            checks,
            blockers: policy_blockers,
        });
    }
    if let Some(report) = run_provider_preflight(
        repo_root,
        &provider,
        package,
        deploy_provider_context("preflight", env, env_config, model, package)?,
    )? {
        checks.extend(report.checks.into_iter().map(|check| DeployProviderCheck {
            name: check.name,
            status: check.status.as_str().to_owned(),
            target: check.target,
            message: check.message,
        }));
        if !report.warnings.is_empty() {
            checks.push(DeployProviderCheck {
                name: "provider-warnings".to_owned(),
                status: DeployProviderCheckStatus::Warning.as_str().to_owned(),
                target: Some(provider.clone()),
                message: Some(report.warnings.join("; ")),
            });
        }
        if !report.files.is_empty() {
            checks.push(DeployProviderCheck {
                name: "provider-files".to_owned(),
                status: DeployProviderCheckStatus::Planned.as_str().to_owned(),
                target: Some(report.files.join(",")),
                message: Some("provider package reported generated files".to_owned()),
            });
        }
        if !report.status.allows_preflight_progress() && report.blockers.is_empty() {
            return Ok(DeployProviderPreflightReport {
                status: "blocked".to_owned(),
                checks,
                blockers: vec![format!(
                    "deploy provider `{provider}` preflight returned status `{}` without explicit blockers",
                    report.status.as_str()
                )],
            });
        }
        if !report.blockers.is_empty() {
            return Ok(DeployProviderPreflightReport {
                status: "blocked".to_owned(),
                checks,
                blockers: report.blockers,
            });
        }
    }
    checks.extend([
        DeployProviderCheck {
            name: "project".to_owned(),
            status: "planned".to_owned(),
            target: env_config.provider_project.clone(),
            message: None,
        },
        DeployProviderCheck {
            name: "services".to_owned(),
            status: "planned".to_owned(),
            target: Some(
                model
                    .services
                    .iter()
                    .map(|service| service.name.clone())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            message: None,
        },
    ]);
    Ok(DeployProviderPreflightReport {
        status: "planned".to_owned(),
        checks,
        blockers: Vec::new(),
    })
}

/// The context document a provider phase reads.
///
/// Building it is fallible: the document is the only hand-off to a provider
/// script, so a projection that cannot be encoded is a refusal, never a panic
/// in the middle of a deploy.
pub(super) fn deploy_provider_context(
    phase: &str,
    env: &str,
    env_config: &ManifestDeployEnvConfig,
    model: &super::model::DeployModel,
    package: &DeployProviderPackage,
) -> Result<Value, RunnerError> {
    build_provider_context(DeployProviderContextRequest {
        phase,
        env,
        provider: env_config.provider.config_value(),
        provider_project: env_config.provider_project.as_deref(),
        package,
        state: env_config.state.as_deref(),
        code_ref: &env_config.code_ref,
        release_policy: env_config.release_policy.as_str(),
        artifact_policy: env_config.artifact_policy.as_str(),
        model,
        export_path: None,
        plan: false,
    })
}

#[cfg(test)]
mod tests {
    use super::deployment_id;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn a_deployment_id_names_the_attempts_second_environment_and_commit() {
        let started = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(
            deployment_id("production", Some("0123456789abcdef"), started),
            "1700000000-production-0123456789abcdef"
        );
        assert_eq!(
            deployment_id("uat", None, started),
            "1700000000-uat-unresolved",
            "an unpinned deploy says so instead of inventing a commit"
        );
    }
}
