use std::collections::BTreeMap;
use std::path::Path;

use effigy_manifest::task_runtime::{ManifestManagedRun, ManifestTask};
use serde::Deserialize;

use super::model::*;
use crate::runner::error::RunnerError;
use crate::runner::manifest::{load_task_manifest, load_task_manifest_with_inspection};

pub(super) fn derive_deploy_model(repo_root: &Path) -> Result<DeployModel, RunnerError> {
    let loaded =
        load_task_manifest_with_inspection(&repo_root.join(effigy_manifest::TASK_MANIFEST_FILE))?;
    let deploy_value = loaded
        .effective_value
        .get("deploy")
        .cloned()
        .ok_or_else(|| {
            RunnerError::task_invocation("`deploy model` requires `[deploy.model]`".to_owned())
        })?;
    let model_value = deploy_value.get("model").cloned().ok_or_else(|| {
        RunnerError::task_invocation("`deploy model` requires `[deploy.model]`".to_owned())
    })?;
    let model: ManifestDeployModelConfig = model_value.try_into().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse composed `[deploy.model]` config: {error}"
        ))
    })?;

    let repo_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("app")
        .to_owned();

    let mut services = Vec::with_capacity(model.services.len());
    for service in &model.services {
        if let Some(derived) = derive_service(repo_root, service)? {
            services.push(derived);
        }
    }

    Ok(DeployModel {
        schema: "deploy.model.v1".to_owned(),
        schema_version: 1,
        app: DeployApp {
            name: repo_name,
            bundle: model.app.bundle.clone(),
            project_name: model.app.project_name.clone(),
            source_root: model.app.source_root.clone(),
            notes: model.app.notes.clone(),
        },
        services,
        backing_services: model
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
        domains: model
            .domains
            .into_iter()
            .map(|domain| DeployDomain {
                host: domain.host,
                service: domain.service,
                tls: domain.tls,
            })
            .collect(),
        secrets: model
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
        warnings: model.warnings.unwrap_or_default(),
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelConfig {
    app: ManifestDeployModelApp,
    #[serde(default)]
    services: Vec<ManifestDeployModelService>,
    #[serde(default)]
    backing_services: Vec<ManifestDeployModelBackingService>,
    #[serde(default)]
    domains: Vec<ManifestDeployModelDomain>,
    #[serde(default)]
    secrets: Vec<ManifestDeployModelSecret>,
    #[serde(default)]
    warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelApp {
    project_name: String,
    #[serde(default)]
    bundle: Option<String>,
    #[serde(default)]
    source_root: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelService {
    name: String,
    role: String,
    runtime: String,
    source_root: String,
    #[serde(default)]
    build: Option<ManifestDeployModelTaskStep>,
    #[serde(default)]
    start: Option<ManifestDeployModelTaskStep>,
    #[serde(default)]
    release: Option<ManifestDeployModelTaskStep>,
    #[serde(default)]
    health: Option<ManifestDeployModelHealth>,
    #[serde(default)]
    output: Option<ManifestDeployModelOutput>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    secret_refs: Option<Vec<String>>,
    #[serde(default)]
    volumes: Option<Vec<String>>,
    #[serde(default)]
    warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelTaskStep {
    task: String,
    #[serde(default)]
    optional: Option<bool>,
    #[serde(default)]
    omit_service_if_missing: Option<bool>,
    #[serde(default)]
    warn_if_missing: Option<bool>,
    #[serde(default)]
    warning_code: Option<String>,
    #[serde(default)]
    warning_target: Option<String>,
    #[serde(default)]
    warning_message: Option<String>,
    #[serde(default)]
    warning_severity: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelHealth {
    kind: String,
    path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelOutput {
    kind: String,
    path: String,
    #[serde(default)]
    fallback: Option<String>,
    #[serde(default)]
    detect_static_fallback: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelBackingService {
    name: String,
    kind: String,
    mode: String,
    #[serde(default = "default_true")]
    required: bool,
    #[serde(default)]
    consumers: Vec<String>,
    #[serde(default)]
    warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelDomain {
    host: String,
    service: String,
    tls: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelSecret {
    name: String,
    #[serde(default)]
    services: Vec<String>,
    #[serde(default = "default_true")]
    required: bool,
    source: String,
    #[serde(default)]
    notes: Option<String>,
}

fn default_true() -> bool {
    true
}
