use std::path::Path;

use effigy_manifest::task_runtime::{ManifestManagedRun, ManifestTask};

use super::super::config::parse_deploy_config;
use super::manifest::*;
use super::merge::resolve_effective_model;
use super::production::{
    reserved_local_domains, suppress_development_domains, suppress_development_values,
};
use super::vocabulary::validate_vocabularies;
use super::*;
use crate::runner::error::RunnerError;
use crate::runner::manifest::{load_task_manifest, load_task_manifest_with_inspection};

pub(in crate::runner::deploy_command) fn derive_deploy_model(
    repo_root: &Path,
    requested_env: Option<&str>,
) -> Result<DeployModel, RunnerError> {
    let loaded =
        load_task_manifest_with_inspection(&repo_root.join(effigy_manifest::TASK_MANIFEST_FILE))?;
    let deploy_value = loaded
        .effective_value
        .get("deploy")
        .cloned()
        .ok_or_else(|| {
            RunnerError::task_invocation("`deploy model` requires `[deploy.model]`".to_owned())
        })?;
    let config = parse_deploy_config(&deploy_value)?;
    let resolved_env = config.resolve_environment(requested_env)?;

    let model_value = config.model.clone().ok_or_else(|| {
        RunnerError::task_invocation("`deploy model` requires `[deploy.model]`".to_owned())
    })?;
    let shared: ManifestDeployModelConfig = model_value.try_into().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse composed `[deploy.model]` config: {error}"
        ))
    })?;

    let overrides = match resolved_env
        .config
        .as_ref()
        .and_then(|config| config.model.as_ref())
    {
        Some(value) => Some(
            value
                .clone()
                .try_into::<ManifestDeployModelOverride>()
                .map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "failed to parse `[deploy.{}.model]` overrides: {error}",
                        resolved_env.name.as_deref().unwrap_or("<env>")
                    ))
                })?,
        ),
        None => None,
    };

    let non_development = !resolved_env.is_development();
    let mut effective = resolve_effective_model(shared, overrides)?;

    validate_vocabularies(&mut effective.model, resolved_env.name.is_some())?;

    let repo_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("app")
        .to_owned();

    let mut services = Vec::with_capacity(effective.model.services.len());
    for service in &effective.model.services {
        let Some(mut derived) = derive_service(repo_root, service)? else {
            continue;
        };
        if non_development {
            suppress_development_values(
                &mut derived,
                &mut effective.provenance,
                &mut effective.missing,
            );
        }
        services.push(derived);
    }

    let mut domains: Vec<DeployDomain> = effective
        .model
        .domains
        .iter()
        .map(|domain| DeployDomain {
            host: domain.host.clone(),
            service: domain.service.clone(),
            tls: domain.tls.clone(),
        })
        .collect();

    let mut warnings = resolved_env
        .notices
        .iter()
        .map(|notice| DeployWarning {
            code: "deploy-environment-notice".to_owned(),
            scope: "environment".to_owned(),
            target: resolved_env.name.clone(),
            message: notice.clone(),
            severity: "warn".to_owned(),
        })
        .collect::<Vec<_>>();
    warnings.extend(effective.model.warnings.clone().unwrap_or_default());

    if non_development {
        suppress_development_domains(
            &mut domains,
            &mut effective.provenance,
            &mut effective.missing,
            &mut warnings,
        );
    }

    let environment_state = resolved_env.state().map(str::to_owned);
    if environment_state.as_deref() == Some("production") {
        let reserved = reserved_local_domains(&domains, &services);
        if !reserved.is_empty() {
            warnings.push(DeployWarning {
                code: "reserved-local-domain".to_owned(),
                scope: "environment".to_owned(),
                target: resolved_env.name.clone(),
                message: format!(
                    "production domains must not use a reserved local suffix ({}); declare provider-managed hosts under `[deploy.{}.model]`",
                    reserved
                        .iter()
                        .map(|(_, host)| host.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    resolved_env.name.as_deref().unwrap_or("<env>")
                ),
                severity: "error".to_owned(),
            });
        }
    }

    Ok(DeployModel {
        schema: "deploy.model.v1".to_owned(),
        schema_version: 1,
        environment: resolved_env.name.clone(),
        environment_state,
        app: DeployApp {
            name: repo_name,
            bundle: effective.model.app.bundle.clone(),
            project_name: effective.model.app.project_name.clone(),
            source_root: effective.model.app.source_root.clone(),
            notes: effective.model.app.notes.clone(),
        },
        services,
        backing_services: effective
            .model
            .backing_services
            .into_iter()
            .map(|service| DeployBackingService {
                name: service.name,
                kind: service.kind,
                mode: service.mode,
                required: service.required,
                consumers: service.consumers,
                warnings: service.warnings.unwrap_or_default(),
            })
            .collect(),
        domains,
        secrets: effective
            .model
            .secrets
            .into_iter()
            .map(|secret| DeploySecret {
                name: secret.name,
                services: secret.services,
                required: secret.required,
                source: secret.source,
                notes: secret.notes,
            })
            .collect(),
        provenance: effective.provenance,
        missing: effective.missing.into_iter().collect(),
        warnings,
    })
}


fn derive_service(
    repo_root: &Path,
    service: &ManifestDeployModelService,
) -> Result<Option<DeployService>, RunnerError> {
    let manifest = load_child_manifest(repo_root, &service.source_root)?;
    let mut warnings = service.warnings.clone().unwrap_or_default();

    let build = resolve_task_step(&manifest, &service.source_root, service.build.as_ref())?;
    let start = resolve_task_step(&manifest, &service.source_root, service.start.as_ref())?;
    let release = resolve_task_step(&manifest, &service.source_root, service.release.as_ref())?;

    if build.omit_service || start.omit_service || release.omit_service {
        return Ok(None);
    }

    warnings.extend(build.warnings);
    warnings.extend(start.warnings);
    warnings.extend(release.warnings);

    let output = match service.output.as_ref() {
        Some(output) => {
            let mut fallback = output.fallback.clone();
            if fallback.is_none() && output.detect_static_fallback.unwrap_or(false) {
                fallback = detect_static_fallback(repo_root, &service.source_root);
                warnings.extend(missing_static_fallback_warning(
                    &service.name,
                    fallback.is_none(),
                ));
            }
            Some(DeployOutput {
                kind: output.kind.clone(),
                path: output.path.clone(),
                fallback,
            })
        }
        None => None,
    };

    Ok(Some(DeployService {
        name: service.name.clone(),
        role: service.role.clone(),
        runtime: service.runtime.clone(),
        source_root: service.source_root.clone(),
        build: build.command.map(|command| DeployCommandStep { command }),
        start: start.command.map(|command| DeployCommandStep { command }),
        release: release.command.map(|command| DeployCommandStep { command }),
        health: service.health.as_ref().map(|health| DeployHealth {
            kind: health.kind.clone(),
            path: health.path.clone(),
        }),
        output,
        port: service.port,
        domains: service.domains.clone(),
        env: service.env.clone().unwrap_or_default(),
        secret_refs: service.secret_refs.clone().unwrap_or_default(),
        volumes: service.volumes.clone().unwrap_or_default(),
        warnings,
    }))
}

fn load_child_manifest(
    repo_root: &Path,
    dir: &str,
) -> Result<effigy_manifest::TaskManifest, RunnerError> {
    load_task_manifest(
        &repo_root
            .join(dir)
            .join(effigy_manifest::TASK_MANIFEST_FILE),
    )
}

struct ResolvedTaskStep {
    command: Option<String>,
    warnings: Vec<DeployWarning>,
    omit_service: bool,
}

fn resolve_task_step(
    manifest: &effigy_manifest::TaskManifest,
    dir: &str,
    step: Option<&ManifestDeployModelTaskStep>,
) -> Result<ResolvedTaskStep, RunnerError> {
    let Some(step) = step else {
        return Ok(ResolvedTaskStep {
            command: None,
            warnings: Vec::new(),
            omit_service: false,
        });
    };

    let command = optional_task_command(manifest, &step.task);
    match command {
        Some(command) => Ok(ResolvedTaskStep {
            command: Some(command),
            warnings: Vec::new(),
            omit_service: false,
        }),
        None if step.omit_service_if_missing.unwrap_or(false) => Ok(ResolvedTaskStep {
            command: None,
            warnings: Vec::new(),
            omit_service: true,
        }),
        None if step.warn_if_missing.unwrap_or(false) => Ok(ResolvedTaskStep {
            command: None,
            warnings: vec![DeployWarning {
                code: step
                    .warning_code
                    .clone()
                    .unwrap_or_else(|| "missing-task-hook".to_owned()),
                scope: "service".to_owned(),
                target: step.warning_target.clone(),
                message: step.warning_message.clone().unwrap_or_else(|| {
                    format!(
                        "Optional task `{}` is missing or non-command in `{dir}/effigy.toml`",
                        step.task
                    )
                }),
                severity: step
                    .warning_severity
                    .clone()
                    .unwrap_or_else(|| "warn".to_owned()),
            }],
            omit_service: false,
        }),
        None if step.optional.unwrap_or(false) => Ok(ResolvedTaskStep {
            command: None,
            warnings: Vec::new(),
            omit_service: false,
        }),
        None => Err(RunnerError::task_invocation(format!(
            "required task `{}` is missing or non-command in `{dir}/effigy.toml`",
            step.task
        ))),
    }
}

fn optional_task_command(
    manifest: &effigy_manifest::TaskManifest,
    task_name: &str,
) -> Option<String> {
    let task = manifest.tasks.get(task_name)?;
    extract_task_command(task)
}

fn extract_task_command(task: &ManifestTask) -> Option<String> {
    match task.run.as_ref()? {
        ManifestManagedRun::Command(command) => Some(normalize_task_command(command)),
        ManifestManagedRun::Sequence(_) => None,
    }
}

fn normalize_task_command(command: &str) -> String {
    command
        .replace(" {args}", "")
        .replace("{args}", "")
        .trim()
        .to_owned()
}

