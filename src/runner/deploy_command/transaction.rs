use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::derive::derive_deploy_model;
use super::provider_package::{resolve_provider_package, ManifestDeployProviderConfig};
use crate::runner::error::RunnerError;
use crate::runner::manifest::load_task_manifest_with_inspection;
use crate::runner::render::render_command_result;

const PLAN_SCHEMA: &str = "effigy.deploy.plan.v1";
const APPLY_SCHEMA: &str = "effigy.deploy.apply.v1";
const STATUS_SCHEMA: &str = "effigy.deploy.status.v1";
const HISTORY_SCHEMA: &str = "effigy.deploy.history.v1";

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

pub(super) fn run_deploy_apply(
    repo_root: &Path,
    env: &str,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if !yes {
        return Err(RunnerError::task_invocation(
            "`deploy apply` is plan-only unless `--yes` is supplied; run `effigy deploy plan <ENV>` first".to_owned(),
        ));
    }
    let plan = build_deploy_plan(repo_root, env)?;
    if !plan.blockers.is_empty() {
        let text = render_deploy_plan_text(&plan);
        return render_command_result(output_json, false, json_value(&plan)?, text);
    }

    let started_at = iso_timestamp(SystemTime::now());
    let active_path = deploy_active_path(repo_root, env);
    if let Some(parent) = active_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to create deploy active directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    write_json_report(repo_root, &[&active_path], &plan)?;

    let provider_operation = provider_apply_report(&plan);
    let status = if provider_operation.status == "succeeded" {
        "succeeded"
    } else {
        "failed"
    };
    let report = DeployApplyReport {
        schema: APPLY_SCHEMA.to_owned(),
        schema_version: 1,
        deployment_id: plan.deployment_id.clone(),
        env: env.to_owned(),
        provider: plan.provider.clone(),
        status: status.to_owned(),
        started_at,
        finished_at: iso_timestamp(SystemTime::now()),
        code: plan.code.clone(),
        release_policy: plan.release_policy.clone(),
        state: DeployApplyStateReport {
            status: if plan.state.is_some() {
                "planned".to_owned()
            } else {
                "skipped".to_owned()
            },
            lineage_id: plan.state.as_ref().map(|state| state.lineage_id.clone()),
            apply_report_path: None,
        },
        provider_operation,
        hooks: plan
            .hooks
            .iter()
            .map(|hook| DeployHookResult {
                stage: hook.stage.clone(),
                task: hook.task.clone(),
                status: "planned".to_owned(),
            })
            .collect(),
        health_checks: plan
            .health_checks
            .iter()
            .map(|check| DeployHealthResult {
                service: check.service.clone(),
                status: "planned".to_owned(),
                path: check.path.clone(),
            })
            .collect(),
        written_report_path: None,
        written_history_path: None,
    };

    let mut report = report;
    let paths = deploy_report_paths(repo_root, env, &report.deployment_id);
    report.written_report_path = Some(path_display(&paths.latest_path, repo_root));
    report.written_history_path = Some(path_display(&paths.history_path, repo_root));
    write_json_report(
        repo_root,
        &[&paths.latest_path, &paths.history_path],
        &report,
    )?;
    let _ = fs::remove_file(active_path);

    let ok = report.status == "succeeded";
    let text = render_deploy_apply_text(&report);
    render_command_result(output_json, ok, json_value(&report)?, text)
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
        warnings: Vec::new(),
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

pub(super) fn run_deploy_redeploy(
    repo_root: &Path,
    env: &str,
    deployment: &str,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if !yes {
        return Err(RunnerError::task_invocation(
            "`deploy redeploy` requires `--yes` after reviewing deployment history".to_owned(),
        ));
    }
    let path = deploy_history_dir(repo_root, env).join(format!("{deployment}.json"));
    if !path.exists() {
        return Err(RunnerError::task_invocation(format!(
            "deployment `{deployment}` was not found in deploy history for `{env}`"
        )));
    }
    let source = read_optional_json(&path)?.ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "failed to read deployment report {}",
            path.display()
        ))
    })?;
    let started_at = iso_timestamp(SystemTime::now());
    let report = DeployRedeployReport {
        schema: APPLY_SCHEMA.to_owned(),
        schema_version: 1,
        deployment_id: format!("{}-redeploy-{}", utc_basic_timestamp(SystemTime::now()), deployment),
        env: env.to_owned(),
        provider: source
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        status: "succeeded".to_owned(),
        source_deployment: deployment.to_owned(),
        started_at,
        finished_at: iso_timestamp(SystemTime::now()),
        source_report_path: path_display(&path, repo_root),
        warnings: vec![
            "redeploy replay recorded immutable deployment evidence; provider rollback and database rollback remain out of scope".to_owned(),
        ],
        written_report_path: None,
        written_history_path: None,
    };
    let mut report = report;
    let paths = deploy_report_paths(repo_root, env, &report.deployment_id);
    report.written_report_path = Some(path_display(&paths.latest_path, repo_root));
    report.written_history_path = Some(path_display(&paths.history_path, repo_root));
    write_json_report(
        repo_root,
        &[&paths.latest_path, &paths.history_path],
        &report,
    )?;
    let text = format!(
        "[deploy] redeployed {env}\nsource: {deployment}\nreport: {}",
        report
            .written_report_path
            .as_deref()
            .unwrap_or("<not written>")
    );
    render_command_result(output_json, true, json_value(&report)?, text)
}

fn build_deploy_plan(repo_root: &Path, env: &str) -> Result<DeployPlanReport, RunnerError> {
    let loaded =
        load_task_manifest_with_inspection(&repo_root.join(effigy_manifest::TASK_MANIFEST_FILE))?;
    let deploy_value = loaded
        .effective_value
        .get("deploy")
        .cloned()
        .ok_or_else(|| {
            RunnerError::task_invocation(
                "no `[deploy]` section found in the composed manifest".to_owned(),
            )
        })?;
    let config: ManifestDeployConfig = deploy_value.try_into().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse composed `[deploy]` config: {error}"
        ))
    })?;
    let env_config = config.envs.get(env).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "deploy environment `{env}` is not defined in `[deploy]`; available environments: {}",
            config.envs.keys().cloned().collect::<Vec<_>>().join(", ")
        ))
    })?;
    let model = derive_deploy_model(repo_root)?;
    let provider_package =
        resolve_provider_package(repo_root, &env_config.provider, &config.providers)?;
    let code = resolve_code_ref(repo_root, &env_config.code_ref)?;
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
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
    let provider_preflight =
        provider_preflight_report(env_config, &model, provider_package.as_ref());
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
        provider: env_config.provider.clone(),
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
    env_config: &ManifestDeployEnvConfig,
    model: &super::model::DeployModel,
    provider_package: Option<&super::provider_package::DeployProviderPackage>,
) -> DeployProviderPreflightReport {
    let provider = env_config.provider.trim().to_ascii_lowercase();
    if !matches!(provider.as_str(), "railway" | "render") {
        return DeployProviderPreflightReport {
            status: "blocked".to_owned(),
            checks: vec![DeployProviderCheck {
                name: "provider-adapter".to_owned(),
                status: "blocked".to_owned(),
                target: Some(env_config.provider.clone()),
                message: Some("supported providers in this deployment transaction surface are railway and render".to_owned()),
            }],
            blockers: vec![format!(
                "deploy provider `{}` is not supported; expected `railway` or `render`",
                env_config.provider
            )],
        };
    }

    let mut checks = Vec::new();
    if let Some(package) = provider_package {
        checks.push(DeployProviderCheck {
            name: "provider-package".to_owned(),
            status: "planned".to_owned(),
            target: Some(package.root.display().to_string()),
            message: Some(format!(
                "{} {} resolved from provider package",
                package.descriptor.provider.display_name, package.descriptor.provider.version
            )),
        });
        let policy_blockers =
            provider_package_policy_blockers(&env_config.provider, &package.descriptor.policy);
        if !policy_blockers.is_empty() {
            return DeployProviderPreflightReport {
                status: "blocked".to_owned(),
                checks,
                blockers: policy_blockers,
            };
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
    checks.extend(provider_adapter_checks(&provider, model));
    DeployProviderPreflightReport {
        status: "planned".to_owned(),
        checks,
        blockers: Vec::new(),
    }
}

fn provider_package_policy_blockers(
    provider: &str,
    policy: &super::provider_package::DeployProviderPolicy,
) -> Vec<String> {
    [
        (policy.creates_projects, "create projects"),
        (policy.creates_services, "create services"),
        (policy.creates_resources, "create resources"),
        (policy.creates_variables, "create variables"),
        (policy.creates_domains, "create domains"),
        (policy.prints_secret_values, "print secret values"),
    ]
    .into_iter()
    .filter_map(|(enabled, action)| {
        enabled.then(|| {
            format!(
                "deploy provider `{provider}` package policy is not allowed to {action} in the current deployment transaction surface"
            )
        })
    })
    .collect()
}

fn provider_adapter_checks(
    provider: &str,
    model: &super::model::DeployModel,
) -> Vec<DeployProviderCheck> {
    match provider {
        "railway" => vec![DeployProviderCheck {
            name: "provider-adapter".to_owned(),
            status: "planned".to_owned(),
            target: Some("railway".to_owned()),
            message: Some(
                "Railway CLI-backed preflight/apply is deferred to live provider setup".to_owned(),
            ),
        }],
        "render" => {
            let env_targets = model
                .secrets
                .iter()
                .map(|secret| secret.name.clone())
                .collect::<Vec<_>>()
                .join(",");
            let domain_targets = model
                .domains
                .iter()
                .map(|domain| domain.host.clone())
                .collect::<Vec<_>>()
                .join(",");
            vec![
                DeployProviderCheck {
                    name: "provider-adapter".to_owned(),
                    status: "planned".to_owned(),
                    target: Some("render".to_owned()),
                    message: Some(
                        "Render adapter uses the shared deployment transaction boundary; live Render API/CLI mutation is deferred until provider credentials and services exist".to_owned(),
                    ),
                },
                DeployProviderCheck {
                    name: "variables".to_owned(),
                    status: "planned".to_owned(),
                    target: if env_targets.is_empty() {
                        None
                    } else {
                        Some(env_targets)
                    },
                    message: Some("Render variables are validated by name only; Effigy never prints or creates secret values".to_owned()),
                },
                DeployProviderCheck {
                    name: "domains".to_owned(),
                    status: "planned".to_owned(),
                    target: if domain_targets.is_empty() {
                        None
                    } else {
                        Some(domain_targets)
                    },
                    message: Some("Render domains must already exist or be attached by the operator before live apply".to_owned()),
                },
            ]
        }
        _ => Vec::new(),
    }
}

fn provider_apply_report(plan: &DeployPlanReport) -> DeployProviderOperationReport {
    let provider = plan.provider.trim().to_ascii_lowercase();
    DeployProviderOperationReport {
        status: "succeeded".to_owned(),
        provider_deployment_id: Some(format!("{}-{}", plan.provider, plan.deployment_id)),
        services: Vec::new(),
        warnings: vec![match provider.as_str() {
            "render" => "Render adapter recorded the transaction boundary; live Render API/CLI mutation is deferred until provider credentials and existing services are configured".to_owned(),
            "railway" => "Railway adapter recorded the transaction boundary; live Railway CLI mutation is deferred until provider credentials and existing services are configured".to_owned(),
            _ => "provider adapter recorded the transaction boundary; live provider mutation is deferred".to_owned(),
        }],
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

fn deploy_active_path(repo_root: &Path, env: &str) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("runtime")
        .join("deploy")
        .join("active")
        .join(format!("{}.json", safe_path_component(env)))
}

fn deploy_latest_path(repo_root: &Path, env: &str) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("reports")
        .join("deploy")
        .join(safe_path_component(env))
        .join("latest.json")
}

fn deploy_history_dir(repo_root: &Path, env: &str) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("reports")
        .join("deploy")
        .join(safe_path_component(env))
        .join("history")
}

fn deploy_report_paths(repo_root: &Path, env: &str, deployment_id: &str) -> DeployReportPaths {
    DeployReportPaths {
        latest_path: deploy_latest_path(repo_root, env),
        history_path: deploy_history_dir(repo_root, env)
            .join(format!("{}.json", safe_path_component(deployment_id))),
    }
}

struct DeployReportPaths {
    latest_path: PathBuf,
    history_path: PathBuf,
}

fn write_json_report<T: Serialize>(
    repo_root: &Path,
    paths: &[&Path],
    report: &T,
) -> Result<(), RunnerError> {
    let encoded = serde_json::to_string_pretty(report)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    for path in paths {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to create deploy report directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        fs::write(path, format!("{encoded}\n")).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write deploy report {}: {error}",
                path_display(path, repo_root)
            ))
        })?;
    }
    Ok(())
}

fn read_optional_json(path: &Path) -> Result<Option<Value>, RunnerError> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|error| {
        RunnerError::task_invocation(format!("failed to read {}: {error}", path.display()))
    })?;
    let value = serde_json::from_str(&text).map_err(|error| {
        RunnerError::task_invocation(format!("failed to parse {}: {error}", path.display()))
    })?;
    Ok(Some(value))
}

fn json_value<T: Serialize>(value: &T) -> Result<Value, RunnerError> {
    serde_json::to_value(value).map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn render_deploy_plan_text(report: &DeployPlanReport) -> String {
    let mut lines = vec![
        format!(
            "[deploy] planned {} deployment to {}",
            report.provider, report.env
        ),
        format!("deployment: {}", report.deployment_id),
        format!("code: {}", report.code.requested_ref),
        format!("release_policy: {}", report.release_policy.mode),
    ];
    if let Some(state) = &report.state {
        lines.push(format!("state: {} ({})", state.stack, state.lineage_id));
    }
    if !report.blockers.is_empty() {
        lines.push(String::new());
        lines.push(format!("Blockers ({})", report.blockers.len()));
        lines.extend(report.blockers.iter().map(|blocker| format!("- {blocker}")));
    }
    if !report.warnings.is_empty() {
        lines.push(String::new());
        lines.push(format!("Warnings ({})", report.warnings.len()));
        lines.extend(report.warnings.iter().map(|warning| format!("- {warning}")));
    }
    lines.join("\n")
}

fn render_deploy_apply_text(report: &DeployApplyReport) -> String {
    format!(
        "[deploy] {} {} deployment to {}\ndeployment: {}\nreport: {}",
        report.status,
        report.provider,
        report.env,
        report.deployment_id,
        report
            .written_report_path
            .as_deref()
            .unwrap_or("<not written>")
    )
}

fn render_deploy_status_text(report: &DeployStatusReport) -> String {
    let latest = if report.latest.is_some() {
        "present"
    } else {
        "missing"
    };
    let active = if report.active.is_some() {
        "present"
    } else {
        "missing"
    };
    format!(
        "[deploy] status {}\nactive: {active}\nlatest: {latest}",
        report.env
    )
}

fn render_deploy_history_text(report: &DeployHistoryReport) -> String {
    let mut lines = vec![format!(
        "[deploy] history {} ({} entries)",
        report.env,
        report.entries.len()
    )];
    lines.extend(report.entries.iter().map(|entry| {
        format!(
            "- {} [{}] {}",
            entry.deployment_id, entry.status, entry.path
        )
    }));
    lines.join("\n")
}

fn path_display(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn safe_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn utc_basic_timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    seconds.to_string()
}

fn iso_timestamp(time: SystemTime) -> String {
    format!("{}Z", utc_basic_timestamp(time))
}

#[derive(Debug, Deserialize)]
struct ManifestDeployConfig {
    #[serde(default)]
    providers: BTreeMap<String, ManifestDeployProviderConfig>,
    #[serde(flatten)]
    envs: BTreeMap<String, ManifestDeployEnvConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployEnvConfig {
    provider: String,
    #[serde(default)]
    state: Option<String>,
    #[serde(default = "default_code_ref")]
    code_ref: String,
    #[serde(default)]
    release_policy: ReleasePolicy,
    #[serde(default)]
    provider_project: Option<String>,
    #[serde(default)]
    artifact_policy: ArtifactPolicy,
    #[serde(default)]
    preflight: Option<DeployPreflightConfig>,
    #[serde(default)]
    hooks: Option<DeployHooksConfig>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct DeployPreflightConfig {
    #[serde(default)]
    require_release_gates: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct DeployHooksConfig {
    #[serde(default)]
    before_state: Option<String>,
    #[serde(default)]
    after_state: Option<String>,
    #[serde(default)]
    after_deploy: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
enum ReleasePolicy {
    None,
    #[default]
    Optional,
    Required,
}

impl ReleasePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Optional => "optional",
            Self::Required => "required",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
enum ArtifactPolicy {
    MutableOk,
    #[default]
    DigestPreferred,
    DigestPinned,
}

impl ArtifactPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::MutableOk => "mutable-ok",
            Self::DigestPreferred => "digest-preferred",
            Self::DigestPinned => "digest-pinned",
        }
    }
}

fn default_code_ref() -> String {
    "branch:main".to_owned()
}

#[derive(Debug, Clone, Serialize)]
struct DeployPlanReport {
    schema: String,
    schema_version: u8,
    deployment_id: String,
    env: String,
    provider: String,
    app: DeployPlanApp,
    code: DeployCodeRef,
    release_policy: DeployReleasePolicyReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<DeployStatePlan>,
    artifact_policy: DeployArtifactPolicyReport,
    provider_preflight: DeployProviderPreflightReport,
    hooks: Vec<DeployHookPlan>,
    health_checks: Vec<DeployHealthPlan>,
    warnings: Vec<String>,
    blockers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    written_report_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    written_history_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DeployPlanApp {
    name: String,
    project_name: String,
}

#[derive(Debug, Clone, Serialize)]
struct DeployCodeRef {
    requested_ref: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DeployReleasePolicyReport {
    mode: String,
    required: bool,
    gates_required: bool,
}

#[derive(Debug, Clone, Serialize)]
struct DeployStatePlan {
    stack: String,
    lineage_id: String,
    planned_report_path: String,
}

#[derive(Debug, Clone, Serialize)]
struct DeployArtifactPolicyReport {
    mode: String,
    blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DeployProviderPreflightReport {
    status: String,
    checks: Vec<DeployProviderCheck>,
    blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DeployProviderCheck {
    name: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DeployHookPlan {
    stage: String,
    task: String,
}

#[derive(Debug, Clone, Serialize)]
struct DeployHealthPlan {
    service: String,
    kind: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct DeployApplyReport {
    schema: String,
    schema_version: u8,
    deployment_id: String,
    env: String,
    provider: String,
    status: String,
    started_at: String,
    finished_at: String,
    code: DeployCodeRef,
    release_policy: DeployReleasePolicyReport,
    state: DeployApplyStateReport,
    provider_operation: DeployProviderOperationReport,
    hooks: Vec<DeployHookResult>,
    health_checks: Vec<DeployHealthResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    written_report_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    written_history_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeployApplyStateReport {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    lineage_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    apply_report_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeployProviderOperationReport {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_deployment_id: Option<String>,
    services: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DeployHookResult {
    stage: String,
    task: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct DeployHealthResult {
    service: String,
    status: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct DeployStatusReport {
    schema: String,
    schema_version: u8,
    env: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest: Option<Value>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DeployHistoryReport {
    schema: String,
    schema_version: u8,
    env: String,
    entries: Vec<DeployHistoryItem>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DeployHistoryItem {
    path: String,
    deployment_id: String,
    schema: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct DeployRedeployReport {
    schema: String,
    schema_version: u8,
    deployment_id: String,
    env: String,
    provider: String,
    status: String,
    source_deployment: String,
    started_at: String,
    finished_at: String,
    source_report_path: String,
    warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    written_report_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    written_history_path: Option<String>,
}
