use std::path::Path;

use super::model::*;
use crate::runner::error::RunnerError;

pub(super) fn build_railway_export(
    model: &DeployModel,
    path: &Path,
) -> Result<RailwayExportPlan, RunnerError> {
    let mut files = Vec::new();
    let mut report_services = Vec::new();
    let warnings = collect_model_warnings(model);
    let mut required_resources = Vec::new();
    let mut required_variables = Vec::new();
    let mut required_domains = Vec::new();

    for backing_service in &model.backing_services {
        match backing_service.kind.as_str() {
            "postgres" => required_resources.push(RailwayReportResource {
                kind: "postgres".to_owned(),
                name: backing_service.name.clone(),
                required: backing_service.required,
                consumers: backing_service.consumers.clone(),
                action: "create_or_attach_provider_service".to_owned(),
                notes: Some(
                    "Create or attach a Railway Postgres service before wiring DATABASE_URL"
                        .to_owned(),
                ),
            }),
            other => {
                return Err(RunnerError::task_invocation(format!(
                    "railway export does not support backing service kind `{other}` yet"
                )));
            }
        }
    }

    for service in &model.services {
        if !service.volumes.is_empty() {
            return Err(RunnerError::task_invocation(format!(
                "railway export does not support persistent app volumes yet (`{}`)",
                service.name
            )));
        }

        let file = railway_file_from_model(service)?;
        let relative_path = format!("services/{}/railway.toml", service.name);
        let encoded = toml::to_string_pretty(&file).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to encode Railway config for `{}`: {error}",
                service.name
            ))
        })?;
        files.push(DeployExportFile {
            relative_path: relative_path.clone(),
            contents: encoded,
        });

        report_services.push(RailwayReportService {
            name: service.name.clone(),
            role: service.role.clone(),
            source_root: service.source_root.clone(),
            config_path: relative_path,
            start_command: service.start.as_ref().map(|step| step.command.clone()),
            domains: service.domains.clone(),
            operator_steps: railway_operator_steps(service),
        });

        for secret_ref in &service.secret_refs {
            let notes = if secret_ref == "DATABASE_URL" {
                Some(
                    "Wire this from the attached Railway Postgres service using a service variable or reference variable"
                        .to_owned(),
                )
            } else {
                Some(
                    "Set this in Railway service variables; Effigy does not emit secret values"
                        .to_owned(),
                )
            };

            required_variables.push(RailwayReportVariable {
                service: service.name.clone(),
                name: secret_ref.clone(),
                source: if secret_ref == "DATABASE_URL" {
                    "provider_reference".to_owned()
                } else {
                    "operator_secret".to_owned()
                },
                required: true,
                notes,
            });
        }

        if !service.domains.is_empty() {
            required_domains.push(RailwayReportDomain {
                service: service.name.clone(),
                hosts: service.domains.clone(),
                action: "attach_public_domains_in_railway".to_owned(),
            });
        }
    }

    let report = RailwayExportReport {
        schema: "effigy.deploy.export.railway.report.v1".to_owned(),
        schema_version: 1,
        app_name: model.app.name.clone(),
        path: path.display().to_string(),
        services: report_services,
        required_resources,
        required_variables,
        required_domains,
        warnings: warnings.clone(),
    };

    let report_json = serde_json::to_string_pretty(&report).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode railway report.json: {error}"))
    })?;
    files.push(DeployExportFile {
        relative_path: "report.json".to_owned(),
        contents: report_json,
    });

    Ok(RailwayExportPlan { files, warnings })
}

fn railway_file_from_model(service: &DeployService) -> Result<RailwayConfigFile, RunnerError> {
    let build = service.build.as_ref().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "railway export requires build metadata for `{}`",
            service.name
        ))
    })?;

    match service.role.as_str() {
        "static" => {
            let output = service.output.as_ref().ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "railway export requires static output metadata for `{}`",
                    service.name
                ))
            })?;
            if output.fallback.is_none() {
                return Err(RunnerError::task_invocation(format!(
                    "railway export requires static fallback metadata for `{}`",
                    service.name
                )));
            }

            Ok(RailwayConfigFile {
                build: RailwayBuildConfig {
                    builder: "RAILPACK".to_owned(),
                    build_command: Some(build.command.clone()),
                },
                deploy: RailwayDeployConfig {
                    start_command: None,
                    pre_deploy_command: None,
                    healthcheck_path: None,
                    healthcheck_timeout: None,
                    restart_policy_type: None,
                    restart_policy_max_retries: None,
                },
            })
        }
        "web" => {
            let start = service.start.as_ref().ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "railway export requires start metadata for `{}`",
                    service.name
                ))
            })?;
            Ok(RailwayConfigFile {
                build: RailwayBuildConfig {
                    builder: "RAILPACK".to_owned(),
                    build_command: Some(build.command.clone()),
                },
                deploy: RailwayDeployConfig {
                    start_command: Some(start.command.clone()),
                    pre_deploy_command: service.release.as_ref().map(|step| step.command.clone()),
                    healthcheck_path: service.health.as_ref().map(|health| health.path.clone()),
                    healthcheck_timeout: Some(100),
                    restart_policy_type: Some("ON_FAILURE".to_owned()),
                    restart_policy_max_retries: Some(10),
                },
            })
        }
        "worker" => {
            let start = service.start.as_ref().ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "railway export requires start metadata for `{}`",
                    service.name
                ))
            })?;
            Ok(RailwayConfigFile {
                build: RailwayBuildConfig {
                    builder: "RAILPACK".to_owned(),
                    build_command: Some(build.command.clone()),
                },
                deploy: RailwayDeployConfig {
                    start_command: Some(start.command.clone()),
                    pre_deploy_command: None,
                    healthcheck_path: None,
                    healthcheck_timeout: None,
                    restart_policy_type: Some("ON_FAILURE".to_owned()),
                    restart_policy_max_retries: Some(10),
                },
            })
        }
        "cron" => Err(RunnerError::task_invocation(
            "railway export does not support `cron` services yet".to_owned(),
        )),
        other => Err(RunnerError::task_invocation(format!(
            "railway export does not support service role `{other}` yet"
        ))),
    }
}

fn railway_operator_steps(service: &DeployService) -> Vec<String> {
    let mut steps = Vec::new();

    if !service.domains.is_empty() {
        steps.push("attach public domains in Railway".to_owned());
    }
    if service
        .secret_refs
        .iter()
        .any(|secret| secret == "DATABASE_URL")
    {
        steps.push("wire DATABASE_URL from the Railway Postgres service".to_owned());
    }
    if service
        .secret_refs
        .iter()
        .any(|secret| secret != "DATABASE_URL")
    {
        steps.push("set remaining secret values in Railway variables".to_owned());
    }

    steps
}

pub(super) fn collect_model_warnings(model: &DeployModel) -> Vec<DeployWarning> {
    model
        .warnings
        .iter()
        .cloned()
        .chain(
            model
                .services
                .iter()
                .flat_map(|service| service.warnings.clone()),
        )
        .collect()
}
