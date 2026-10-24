use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::config::{load_deploy_config, ArtifactPolicy, ManifestDeployEnvConfig, ReleasePolicy};
use super::derive::derive_deploy_model;
use super::provider_context::{
    build_provider_context, provider_package_policy_blockers, DeployProviderContextRequest,
};
use super::provider_package::{
    resolve_provider_package, run_provider_preflight, run_provider_status,
    DeployProviderCheckStatus, DeployProviderFetch, DeployProviderPackage,
    DeployProviderProvenance,
};
use super::report::*;
use super::text::{render_deploy_history_text, render_deploy_plan_text, render_deploy_status_text};
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

// Refuse before manifest/provider resolution: even preflight can acquire packages
// or execute scripts. No deployment effect or success record is permitted here.
pub(super) fn refuse_deploy_execution(
    operation: &str,
    env: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let message = format!(
        "`deploy {operation}` is not yet supported in v0.15; no deployment was executed. Full execution design is tracked in the deploy lane (Q-003). Use `effigy deploy model --json` or `effigy deploy export <PROVIDER> --path <DIR> --plan`."
    );
    let obligations = [
        "state: state stacks and migrations are not executed",
        "hooks: before_state, after_state and after_deploy hooks are not executed",
        "health: service health checks are not executed",
        "release gates: required release evidence is not verified",
        "digest pins: immutable artifact and code identities are not enforced",
        "provider: deployment and rollback are not executed",
    ];
    if output_json {
        Err(RunnerError::CommandJsonFailure {
            rendered: json!({
                "schema": APPLY_SCHEMA,
                "schema_version": 1,
                "ok": false,
                "status": "blocked",
                "reason": "not_supported",
                "operation": operation,
                "env": env,
                "message": message,
                "blockers": obligations,
            })
            .to_string(),
        })
    } else {
        Err(RunnerError::task_invocation(format!(
            "{message}\n{}",
            obligations.join("\n")
        )))
    }
}

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
    let deployment_id = format!(
        "{}-{}-{}",
        utc_basic_timestamp(SystemTime::now()),
        safe_path_component(env),
        code.resolved_commit
            .clone()
            .unwrap_or_else(|| "unresolved".to_owned())
    );
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

fn deploy_execution_inputs(
    repo_root: &Path,
    env: &str,
    fetch: DeployProviderFetch,
) -> Result<
    (
        ManifestDeployEnvConfig,
        super::model::DeployModel,
        Option<DeployProviderPackage>,
    ),
    RunnerError,
> {
    let config = load_deploy_config(repo_root)?;
    let env_config = config.envs.get(env).cloned().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "deploy environment `{env}` is not defined in `[deploy]`; available environments: {}",
            config.envs.keys().cloned().collect::<Vec<_>>().join(", ")
        ))
    })?;
    let model = derive_deploy_model(repo_root, Some(env))?;
    let provider_package = resolve_provider_package(
        repo_root,
        env_config.provider.adapter(),
        &config.providers,
        fetch,
    )?;
    Ok((env_config, model, provider_package))
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
        deploy_provider_context("preflight", env, env_config, model, package),
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

fn deploy_provider_context(
    phase: &str,
    env: &str,
    env_config: &ManifestDeployEnvConfig,
    model: &super::model::DeployModel,
    package: &DeployProviderPackage,
) -> Value {
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
    .expect("deploy provider context should serialize")
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
                deploy_provider_context("status", env, &env_config, &model, &provider_package),
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

fn resolve_code_ref(repo_root: &Path, requested: &str) -> Result<DeployCodeRef, RunnerError> {
    if requested == "release-tag" {
        let tag = git_output(repo_root, &["describe", "--tags", "--abbrev=0"]).ok();
        let commit = git_output(repo_root, &["rev-parse", tag.as_deref().unwrap_or("HEAD")]).ok();
        return Ok(DeployCodeRef {
            requested_ref: requested.to_owned(),
            kind: "tag".to_owned(),
            resolved_ref: tag,
            resolved_commit: commit,
        });
    }
    let (kind, value) = requested.split_once(':').unwrap_or(("branch", requested));
    let commit = match kind {
        "branch" => git_output(repo_root, &["rev-parse", value]).ok(),
        "tag" => git_output(repo_root, &["rev-parse", value]).ok(),
        "sha" => Some(value.to_owned()),
        _ => None,
    };
    Ok(DeployCodeRef {
        requested_ref: requested.to_owned(),
        kind: kind.to_owned(),
        resolved_ref: Some(value.to_owned()),
        resolved_commit: commit,
    })
}

fn git_output(repo_root: &Path, args: &[&str]) -> Result<String, RunnerError> {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|error| RunnerError::task_invocation(format!("failed to run git: {error}")))?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn utc_basic_timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    seconds.to_string()
}
