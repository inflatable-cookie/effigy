use std::collections::{BTreeMap, BTreeSet};

use super::manifest::*;
use super::{PROVENANCE_ENVIRONMENT, PROVENANCE_SHARED};
use crate::runner::error::RunnerError;

pub(super) struct EffectiveModel {
    pub(super) model: ManifestDeployModelConfig,
    pub(super) provenance: BTreeMap<String, String>,
    pub(super) missing: BTreeSet<String>,
}

pub(super) fn resolve_effective_model(
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

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.join(", ")
    }
}
