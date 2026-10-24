use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use effigy_manifest::task_runtime::{ManifestManagedRun, ManifestTask};
use serde::Deserialize;

use super::config::parse_deploy_config;
use super::model::*;
use crate::runner::error::RunnerError;
use crate::runner::manifest::{load_task_manifest, load_task_manifest_with_inspection};

/// Service runtimes the deployment model understands.
///
/// `static` describes output served as files with no application runtime;
/// provider packages use it instead of approximating a static site as `node`.
const SERVICE_RUNTIMES: [&str; 10] = [
    "static", "node", "bun", "deno", "rust", "python", "go", "php", "ruby", "java",
];

/// Backing-service modes the deployment model understands.
///
/// `managed` means the provider platform owns the resource. `external` means
/// the operator supplies an endpoint Effigy does not provision.
const BACKING_SERVICE_MODES: [&str; 2] = ["managed", "external"];

/// Hostname suffixes that only resolve on a development machine.
const RESERVED_LOCAL_DOMAINS: [&str; 3] = [".test", ".localhost", ".local"];

const PROVENANCE_ENVIRONMENT: &str = "environment";
const PROVENANCE_SHARED: &str = "shared";
const PROVENANCE_MISSING: &str = "missing";

pub(super) fn derive_deploy_model(
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

/// `(key, host)` pairs for every resolved domain that only resolves locally.
pub(super) fn reserved_local_domains(
    domains: &[DeployDomain],
    services: &[DeployService],
) -> Vec<(String, String)> {
    let mut reserved = Vec::new();
    let mut seen = BTreeSet::new();
    for domain in domains {
        if is_reserved_local_domain(&domain.host) && seen.insert(domain.host.clone()) {
            reserved.push(("domains".to_owned(), domain.host.clone()));
        }
    }
    for service in services {
        for host in &service.domains {
            if is_reserved_local_domain(host) && seen.insert(host.clone()) {
                reserved.push((format!("services.{}.domains", service.name), host.clone()));
            }
        }
    }
    reserved
}

fn is_reserved_local_domain(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || RESERVED_LOCAL_DOMAINS
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

struct EffectiveModel {
    model: ManifestDeployModelConfig,
    provenance: BTreeMap<String, String>,
    missing: BTreeSet<String>,
}

fn resolve_effective_model(
    mut model: ManifestDeployModelConfig,
    overrides: Option<ManifestDeployModelOverride>,
) -> Result<EffectiveModel, RunnerError> {
    let mut provenance = BTreeMap::new();
    let missing = BTreeSet::new();

    if let Some(overrides) = overrides {
        if let Some(app) = overrides.app.as_ref() {
            merge_app(&mut model.app, app, &mut provenance);
        }
        if let Some(domains) = overrides.domains {
            model.domains = domains;
            mark(&mut provenance, "domains", true);
        }
        if let Some(warnings) = overrides.warnings {
            model.warnings = Some(warnings);
            mark(&mut provenance, "warnings", true);
        }

        let declared_services = model
            .services
            .iter()
            .map(|service| service.name.clone())
            .collect::<Vec<_>>();
        for service_override in &overrides.services {
            let Some(service) = model
                .services
                .iter_mut()
                .find(|service| service.name == service_override.name)
            else {
                return Err(RunnerError::task_invocation(format!(
                    "`[deploy.<env>.model.services]` overrides service `{}`, which the shared `[deploy.model.services]` does not declare; declared services: {}",
                    service_override.name,
                    join_or_none(&declared_services)
                )));
            };
            merge_service(service, service_override, &mut provenance)?;
        }

        let declared_backing = model
            .backing_services
            .iter()
            .map(|service| service.name.clone())
            .collect::<Vec<_>>();
        for backing_override in &overrides.backing_services {
            let Some(service) = model
                .backing_services
                .iter_mut()
                .find(|service| service.name == backing_override.name)
            else {
                return Err(RunnerError::task_invocation(format!(
                    "`[deploy.<env>.model.backing_services]` overrides backing service `{}`, which the shared `[deploy.model.backing_services]` does not declare; declared backing services: {}",
                    backing_override.name,
                    join_or_none(&declared_backing)
                )));
            };
            merge_backing_service(service, backing_override, &mut provenance);
        }

        let declared_secrets = model
            .secrets
            .iter()
            .map(|secret| secret.name.clone())
            .collect::<Vec<_>>();
        for secret_override in &overrides.secrets {
            let Some(secret) = model
                .secrets
                .iter_mut()
                .find(|secret| secret.name == secret_override.name)
            else {
                return Err(RunnerError::task_invocation(format!(
                    "`[deploy.<env>.model.secrets]` overrides secret `{}`, which the shared `[deploy.model.secrets]` does not declare; declared secrets: {}",
                    secret_override.name,
                    join_or_none(&declared_secrets)
                )));
            };
            merge_secret(secret, secret_override, &mut provenance);
        }
    }

    record_shared_provenance(&model, &mut provenance);
    Ok(EffectiveModel {
        model,
        provenance,
        missing,
    })
}

fn mark(provenance: &mut BTreeMap<String, String>, key: &str, from_environment: bool) {
    provenance.insert(
        key.to_owned(),
        if from_environment {
            PROVENANCE_ENVIRONMENT
        } else {
            PROVENANCE_SHARED
        }
        .to_owned(),
    );
}

fn merge_app(
    app: &mut ManifestDeployModelApp,
    overrides: &ManifestDeployModelAppOverride,
    provenance: &mut BTreeMap<String, String>,
) {
    if let Some(value) = &overrides.project_name {
        app.project_name = value.clone();
        mark(provenance, "app.project_name", true);
    }
    if let Some(value) = &overrides.bundle {
        app.bundle = Some(value.clone());
        mark(provenance, "app.bundle", true);
    }
    if let Some(value) = &overrides.source_root {
        app.source_root = Some(value.clone());
        mark(provenance, "app.source_root", true);
    }
    if let Some(value) = &overrides.notes {
        app.notes = Some(value.clone());
        mark(provenance, "app.notes", true);
    }
}

fn merge_service(
    service: &mut ManifestDeployModelService,
    overrides: &ManifestDeployModelServiceOverride,
    provenance: &mut BTreeMap<String, String>,
) -> Result<(), RunnerError> {
    let prefix = format!("services.{}", service.name);
    if let Some(value) = &overrides.role {
        service.role = value.clone();
        mark(provenance, &format!("{prefix}.role"), true);
    }
    if let Some(value) = &overrides.runtime {
        service.runtime = value.clone();
        mark(provenance, &format!("{prefix}.runtime"), true);
    }
    if let Some(value) = &overrides.source_root {
        service.source_root = value.clone();
        mark(provenance, &format!("{prefix}.source_root"), true);
    }
    if let Some(value) = &overrides.build {
        service.build = Some(merge_task_step(
            service.build.take(),
            value,
            &format!("{prefix}.build"),
        )?);
        mark(provenance, &format!("{prefix}.build"), true);
    }
    if let Some(value) = &overrides.start {
        service.start = Some(merge_task_step(
            service.start.take(),
            value,
            &format!("{prefix}.start"),
        )?);
        mark(provenance, &format!("{prefix}.start"), true);
    }
    if let Some(value) = &overrides.release {
        service.release = Some(merge_task_step(
            service.release.take(),
            value,
            &format!("{prefix}.release"),
        )?);
        mark(provenance, &format!("{prefix}.release"), true);
    }
    if let Some(value) = &overrides.health {
        service.health = Some(merge_health(
            service.health.take(),
            value,
            &format!("{prefix}.health"),
        )?);
        mark(provenance, &format!("{prefix}.health"), true);
    }
    if let Some(value) = &overrides.output {
        service.output = Some(merge_output(
            service.output.take(),
            value,
            &format!("{prefix}.output"),
        )?);
        mark(provenance, &format!("{prefix}.output"), true);
    }
    if let Some(value) = overrides.port {
        service.port = Some(value);
        mark(provenance, &format!("{prefix}.port"), true);
    }
    if let Some(value) = &overrides.domains {
        service.domains = value.clone();
        mark(provenance, &format!("{prefix}.domains"), true);
    }
    if let Some(value) = &overrides.env {
        let target = service.env.get_or_insert_with(BTreeMap::new);
        for (key, value) in value {
            target.insert(key.clone(), value.clone());
            mark(provenance, &format!("{prefix}.env.{key}"), true);
        }
    }
    if let Some(value) = &overrides.env_unset {
        let target = service.env.get_or_insert_with(BTreeMap::new);
        for key in value {
            target.remove(key);
            mark(provenance, &format!("{prefix}.env.{key}"), true);
        }
    }
    if let Some(value) = &overrides.secret_refs {
        service.secret_refs = Some(value.clone());
        mark(provenance, &format!("{prefix}.secret_refs"), true);
    }
    if let Some(value) = &overrides.volumes {
        service.volumes = Some(value.clone());
        mark(provenance, &format!("{prefix}.volumes"), true);
    }
    if let Some(value) = &overrides.warnings {
        service.warnings = Some(value.clone());
        mark(provenance, &format!("{prefix}.warnings"), true);
    }
    Ok(())
}

fn merge_task_step(
    shared: Option<ManifestDeployModelTaskStep>,
    overrides: &ManifestDeployModelTaskStepOverride,
    key: &str,
) -> Result<ManifestDeployModelTaskStep, RunnerError> {
    let mut step = shared.unwrap_or(ManifestDeployModelTaskStep {
        task: String::new(),
        optional: None,
        omit_service_if_missing: None,
        warn_if_missing: None,
        warning_code: None,
        warning_target: None,
        warning_message: None,
        warning_severity: None,
    });
    if let Some(value) = &overrides.task {
        step.task = value.clone();
    }
    if let Some(value) = overrides.optional {
        step.optional = Some(value);
    }
    if let Some(value) = overrides.omit_service_if_missing {
        step.omit_service_if_missing = Some(value);
    }
    if let Some(value) = overrides.warn_if_missing {
        step.warn_if_missing = Some(value);
    }
    if let Some(value) = &overrides.warning_code {
        step.warning_code = Some(value.clone());
    }
    if let Some(value) = &overrides.warning_target {
        step.warning_target = Some(value.clone());
    }
    if let Some(value) = &overrides.warning_message {
        step.warning_message = Some(value.clone());
    }
    if let Some(value) = &overrides.warning_severity {
        step.warning_severity = Some(value.clone());
    }
    if step.task.trim().is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "deployment model override `{key}` must set `task`; the shared `[deploy.model]` has no step for it"
        )));
    }
    Ok(step)
}

fn merge_health(
    shared: Option<ManifestDeployModelHealth>,
    overrides: &ManifestDeployModelHealthOverride,
    key: &str,
) -> Result<ManifestDeployModelHealth, RunnerError> {
    let mut health = shared.unwrap_or(ManifestDeployModelHealth {
        kind: String::new(),
        path: String::new(),
    });
    if let Some(value) = &overrides.kind {
        health.kind = value.clone();
    }
    if let Some(value) = &overrides.path {
        health.path = value.clone();
    }
    if health.kind.trim().is_empty() || health.path.trim().is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "deployment model override `{key}` must set both `kind` and `path`; the shared `[deploy.model]` has no health check for it"
        )));
    }
    Ok(health)
}

fn merge_output(
    shared: Option<ManifestDeployModelOutput>,
    overrides: &ManifestDeployModelOutputOverride,
    key: &str,
) -> Result<ManifestDeployModelOutput, RunnerError> {
    let mut output = shared.unwrap_or(ManifestDeployModelOutput {
        kind: String::new(),
        path: String::new(),
        fallback: None,
        detect_static_fallback: None,
    });
    if let Some(value) = &overrides.kind {
        output.kind = value.clone();
    }
    if let Some(value) = &overrides.path {
        output.path = value.clone();
    }
    if let Some(value) = &overrides.fallback {
        output.fallback = Some(value.clone());
    }
    if let Some(value) = overrides.detect_static_fallback {
        output.detect_static_fallback = Some(value);
    }
    if output.kind.trim().is_empty() || output.path.trim().is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "deployment model override `{key}` must set both `kind` and `path`; the shared `[deploy.model]` has no output for it"
        )));
    }
    Ok(output)
}

fn merge_backing_service(
    service: &mut ManifestDeployModelBackingService,
    overrides: &ManifestDeployModelBackingServiceOverride,
    provenance: &mut BTreeMap<String, String>,
) {
    let prefix = format!("backing_services.{}", service.name);
    if let Some(value) = &overrides.kind {
        service.kind = value.clone();
        mark(provenance, &format!("{prefix}.kind"), true);
    }
    if let Some(value) = &overrides.mode {
        service.mode = value.clone();
        mark(provenance, &format!("{prefix}.mode"), true);
    }
    if let Some(value) = overrides.required {
        service.required = value;
        mark(provenance, &format!("{prefix}.required"), true);
    }
    if let Some(value) = &overrides.consumers {
        service.consumers = value.clone();
        mark(provenance, &format!("{prefix}.consumers"), true);
    }
    if let Some(value) = &overrides.warnings {
        service.warnings = Some(value.clone());
        mark(provenance, &format!("{prefix}.warnings"), true);
    }
}

fn merge_secret(
    secret: &mut ManifestDeployModelSecret,
    overrides: &ManifestDeployModelSecretOverride,
    provenance: &mut BTreeMap<String, String>,
) {
    let prefix = format!("secrets.{}", secret.name);
    if let Some(value) = &overrides.services {
        secret.services = value.clone();
        mark(provenance, &format!("{prefix}.services"), true);
    }
    if let Some(value) = overrides.required {
        secret.required = value;
        mark(provenance, &format!("{prefix}.required"), true);
    }
    if let Some(value) = &overrides.source {
        secret.source = value.clone();
        mark(provenance, &format!("{prefix}.source"), true);
    }
    if let Some(value) = &overrides.notes {
        secret.notes = Some(value.clone());
        mark(provenance, &format!("{prefix}.notes"), true);
    }
}

/// Fill in `shared` for every key the environment did not override.
fn record_shared_provenance(
    model: &ManifestDeployModelConfig,
    provenance: &mut BTreeMap<String, String>,
) {
    let mut record = |key: String| {
        provenance
            .entry(key)
            .or_insert_with(|| PROVENANCE_SHARED.to_owned());
    };

    record("app.project_name".to_owned());
    if model.app.bundle.is_some() {
        record("app.bundle".to_owned());
    }
    if model.app.source_root.is_some() {
        record("app.source_root".to_owned());
    }
    if model.app.notes.is_some() {
        record("app.notes".to_owned());
    }

    for service in &model.services {
        let prefix = format!("services.{}", service.name);
        for field in ["role", "runtime", "source_root", "domains"] {
            record(format!("{prefix}.{field}"));
        }
        if service.build.is_some() {
            record(format!("{prefix}.build"));
        }
        if service.start.is_some() {
            record(format!("{prefix}.start"));
        }
        if service.release.is_some() {
            record(format!("{prefix}.release"));
        }
        if service.health.is_some() {
            record(format!("{prefix}.health"));
        }
        if service.output.is_some() {
            record(format!("{prefix}.output"));
        }
        if service.port.is_some() {
            record(format!("{prefix}.port"));
        }
        for key in service
            .env
            .as_ref()
            .into_iter()
            .flatten()
            .map(|(key, _)| key)
        {
            record(format!("{prefix}.env.{key}"));
        }
        if let Some(refs) = service.secret_refs.as_ref().filter(|refs| !refs.is_empty()) {
            let _ = refs;
            record(format!("{prefix}.secret_refs"));
        }
        if let Some(volumes) = service.volumes.as_ref().filter(|items| !items.is_empty()) {
            let _ = volumes;
            record(format!("{prefix}.volumes"));
        }
        if service
            .warnings
            .as_ref()
            .is_some_and(|warnings| !warnings.is_empty())
        {
            record(format!("{prefix}.warnings"));
        }
    }

    for service in &model.backing_services {
        let prefix = format!("backing_services.{}", service.name);
        record(format!("{prefix}.kind"));
        record(format!("{prefix}.mode"));
        record(format!("{prefix}.required"));
        record(format!("{prefix}.consumers"));
        if service
            .warnings
            .as_ref()
            .is_some_and(|warnings| !warnings.is_empty())
        {
            record(format!("{prefix}.warnings"));
        }
    }

    if !model.domains.is_empty() {
        record("domains".to_owned());
    }
    for secret in &model.secrets {
        let prefix = format!("secrets.{}", secret.name);
        record(format!("{prefix}.services"));
        record(format!("{prefix}.required"));
        record(format!("{prefix}.source"));
        if secret.notes.is_some() {
            record(format!("{prefix}.notes"));
        }
    }
    if model
        .warnings
        .as_ref()
        .is_some_and(|warnings| !warnings.is_empty())
    {
        record("warnings".to_owned());
    }
}

fn validate_vocabularies(
    model: &mut ManifestDeployModelConfig,
    environment_scoped: bool,
) -> Result<(), RunnerError> {
    for service in &mut model.services {
        if !SERVICE_RUNTIMES.contains(&service.runtime.as_str()) {
            return Err(RunnerError::task_invocation(format!(
                "service `{}` declares unsupported runtime `{}` in `[deploy.model.services]`; allowed runtimes: {}",
                service.name,
                service.runtime,
                SERVICE_RUNTIMES.join(", ")
            )));
        }
        let notices = if environment_scoped
            && service.role == "static"
            && service.runtime != "static"
        {
            vec![DeployWarning {
                code: "deploy-vocabulary-migration".to_owned(),
                scope: "service".to_owned(),
                target: Some(service.name.clone()),
                message: format!(
                    "service `{}` is `role = \"static\"` but declares `runtime = \"{}\"`; declare `runtime = \"static\"` instead. The approximation is accepted for one release",
                    service.name, service.runtime
                ),
                severity: "warn".to_owned(),
            }]
        } else {
            Vec::new()
        };
        push_warnings(&mut service.warnings, notices);
    }

    for service in &mut model.backing_services {
        if !BACKING_SERVICE_MODES.contains(&service.mode.as_str()) {
            return Err(RunnerError::task_invocation(format!(
                "backing service `{}` declares unsupported mode `{}` in `[deploy.model.backing_services]`; allowed modes: {}",
                service.name,
                service.mode,
                BACKING_SERVICE_MODES.join(", ")
            )));
        }
        let mentions_external = service
            .warnings
            .as_ref()
            .into_iter()
            .flatten()
            .any(|warning| {
                format!("{} {}", warning.code, warning.message)
                    .to_ascii_lowercase()
                    .contains("external")
            });
        let notices = if environment_scoped && service.mode == "managed" && mentions_external {
            vec![DeployWarning {
                code: "deploy-vocabulary-migration".to_owned(),
                scope: "backing_service".to_owned(),
                target: Some(service.name.clone()),
                message: format!(
                    "backing service `{}` is `mode = \"managed\"` but its warnings describe an external resource; declare `mode = \"external\"` instead. The approximation is accepted for one release",
                    service.name
                ),
                severity: "warn".to_owned(),
            }]
        } else {
            Vec::new()
        };
        push_warnings(&mut service.warnings, notices);
    }

    Ok(())
}

fn push_warnings(target: &mut Option<Vec<DeployWarning>>, warnings: Vec<DeployWarning>) {
    if warnings.is_empty() {
        return;
    }
    target.get_or_insert_with(Vec::new).extend(warnings);
}

/// Drop values that only describe a development run from a deployed target.
fn suppress_development_values(
    service: &mut DeployService,
    provenance: &mut BTreeMap<String, String>,
    missing: &mut BTreeSet<String>,
) {
    for (kind, step) in [
        ("build", &mut service.build),
        ("start", &mut service.start),
        ("release", &mut service.release),
    ] {
        let key = format!("services.{}.{kind}", service.name);
        let Some(command) = step.as_ref().map(|step| step.command.clone()) else {
            continue;
        };
        if provenance.get(&key).map(String::as_str) != Some(PROVENANCE_SHARED) {
            continue;
        }
        if !command_looks_development_only(&command) {
            continue;
        }
        *step = None;
        provenance.insert(key.clone(), PROVENANCE_MISSING.to_owned());
        missing.insert(key.clone());
        service.warnings.push(DeployWarning {
            code: "environment-missing-field".to_owned(),
            scope: "service".to_owned(),
            target: Some(key.clone()),
            message: format!(
                "`{key}` only declares the shared development command `{command}`; declare `[deploy.<env>.model.{key}]` for this environment"
            ),
            severity: "warn".to_owned(),
        });
    }

    let key = format!("services.{}.domains", service.name);
    if provenance.get(&key).map(String::as_str) == Some(PROVENANCE_SHARED) {
        let dropped = service
            .domains
            .iter()
            .filter(|host| is_reserved_local_domain(host))
            .cloned()
            .collect::<Vec<_>>();
        if !dropped.is_empty() {
            service
                .domains
                .retain(|host| !is_reserved_local_domain(host));
            missing.insert(key.clone());
            if service.domains.is_empty() {
                provenance.insert(key.clone(), PROVENANCE_MISSING.to_owned());
            }
            service.warnings.push(DeployWarning {
                code: "environment-missing-field".to_owned(),
                scope: "service".to_owned(),
                target: Some(key.clone()),
                message: format!(
                    "`{key}` only declares the shared development hosts {}; declare `[deploy.<env>.model.{key}]` for this environment",
                    dropped.join(", ")
                ),
                severity: "warn".to_owned(),
            });
        }
    }
}

fn suppress_development_domains(
    domains: &mut Vec<DeployDomain>,
    provenance: &mut BTreeMap<String, String>,
    missing: &mut BTreeSet<String>,
    warnings: &mut Vec<DeployWarning>,
) {
    if provenance.get("domains").map(String::as_str) != Some(PROVENANCE_SHARED) {
        return;
    }
    let dropped = domains
        .iter()
        .filter(|domain| is_reserved_local_domain(&domain.host))
        .map(|domain| domain.host.clone())
        .collect::<Vec<_>>();
    if dropped.is_empty() {
        return;
    }
    domains.retain(|domain| !is_reserved_local_domain(&domain.host));
    missing.insert("domains".to_owned());
    if domains.is_empty() {
        provenance.insert("domains".to_owned(), PROVENANCE_MISSING.to_owned());
    }
    warnings.push(DeployWarning {
        code: "environment-missing-field".to_owned(),
        scope: "environment".to_owned(),
        target: Some("domains".to_owned()),
        message: format!(
            "`domains` only declares the shared development hosts {}; declare `[deploy.<env>.model.domains]` for this environment",
            dropped.join(", ")
        ),
        severity: "warn".to_owned(),
    });
}

fn command_looks_development_only(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    if ["localhost", "127.0.0.1", "0.0.0.0", ".test"]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return true;
    }
    lower
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|token| matches!(token, "dev" | "development" | "debug" | "local"))
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

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.join(", ")
    }
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelOverride {
    #[serde(default)]
    app: Option<ManifestDeployModelAppOverride>,
    #[serde(default)]
    services: Vec<ManifestDeployModelServiceOverride>,
    #[serde(default)]
    backing_services: Vec<ManifestDeployModelBackingServiceOverride>,
    #[serde(default)]
    domains: Option<Vec<ManifestDeployModelDomain>>,
    #[serde(default)]
    secrets: Vec<ManifestDeployModelSecretOverride>,
    #[serde(default)]
    warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelAppOverride {
    #[serde(default)]
    project_name: Option<String>,
    #[serde(default)]
    bundle: Option<String>,
    #[serde(default)]
    source_root: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelServiceOverride {
    name: String,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    runtime: Option<String>,
    #[serde(default)]
    source_root: Option<String>,
    #[serde(default)]
    build: Option<ManifestDeployModelTaskStepOverride>,
    #[serde(default)]
    start: Option<ManifestDeployModelTaskStepOverride>,
    #[serde(default)]
    release: Option<ManifestDeployModelTaskStepOverride>,
    #[serde(default)]
    health: Option<ManifestDeployModelHealthOverride>,
    #[serde(default)]
    output: Option<ManifestDeployModelOutputOverride>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    domains: Option<Vec<String>>,
    #[serde(default)]
    env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    env_unset: Option<Vec<String>>,
    #[serde(default)]
    secret_refs: Option<Vec<String>>,
    #[serde(default)]
    volumes: Option<Vec<String>>,
    #[serde(default)]
    warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelTaskStepOverride {
    #[serde(default)]
    task: Option<String>,
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
struct ManifestDeployModelHealthOverride {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelOutputOverride {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    fallback: Option<String>,
    #[serde(default)]
    detect_static_fallback: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelBackingServiceOverride {
    name: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    required: Option<bool>,
    #[serde(default)]
    consumers: Option<Vec<String>>,
    #[serde(default)]
    warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDeployModelSecretOverride {
    name: String,
    #[serde(default)]
    services: Option<Vec<String>>,
    #[serde(default)]
    required: Option<bool>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}
