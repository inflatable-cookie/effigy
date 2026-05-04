use std::path::Path;

use serde_yaml;

use super::model::*;
use crate::runner::error::RunnerError;

pub(super) fn build_render_export(
    model: &DeployModel,
    path: &Path,
) -> Result<RenderExportPlan, RunnerError> {
    let postgres = model
        .backing_services
        .iter()
        .find(|service| service.kind == "postgres" && service.required);

    let database_name = postgres.map(|_| format!("{}-postgres", model.app.name));

    let services = model
        .services
        .iter()
        .map(|service| render_service_from_model(service, database_name.as_deref()))
        .collect::<Result<Vec<_>, _>>()?;

    let databases = database_name
        .map(|name| {
            vec![RenderDatabase {
                name,
                plan: "basic-256mb".to_owned(),
                database_name: Some("app".to_owned()),
            }]
        })
        .unwrap_or_default();

    let blueprint = RenderBlueprint {
        services,
        databases,
    };
    let render_yaml = serde_yaml::to_string(&blueprint).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode render.yaml: {error}"))
    })?;

    let warnings = model
        .warnings
        .iter()
        .cloned()
        .chain(
            model
                .services
                .iter()
                .flat_map(|service| service.warnings.clone()),
        )
        .collect();

    let _ = path;

    Ok(RenderExportPlan {
        render_yaml,
        warnings,
    })
}

fn render_service_from_model(
    service: &DeployService,
    database_name: Option<&str>,
) -> Result<RenderService, RunnerError> {
    if !service.volumes.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "render export does not support persistent app volumes yet (`{}`)",
            service.name
        )));
    }

    let env_vars = render_env_vars(service, database_name)?;

    match service.role.as_str() {
        "static" => {
            let output = service.output.as_ref().ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "render export requires static output metadata for `{}`",
                    service.name
                ))
            })?;
            let fallback = output.fallback.as_ref().ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "render export requires static fallback metadata for `{}`",
                    service.name
                ))
            })?;
            let build = service.build.as_ref().ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "render export requires build metadata for `{}`",
                    service.name
                ))
            })?;

            Ok(RenderService {
                name: service.name.clone(),
                service_type: "web".to_owned(),
                runtime: Some("static".to_owned()),
                root_dir: Some(service.source_root.clone()),
                build_command: Some(build.command.clone()),
                start_command: None,
                pre_deploy_command: None,
                static_publish_path: Some(format!("{}/{}", service.source_root, output.path)),
                health_check_path: None,
                domains: service.domains.clone(),
                routes: vec![RenderRoute {
                    route_type: "rewrite".to_owned(),
                    source: "/*".to_owned(),
                    destination: format!("/{}", fallback),
                }],
                env_vars,
            })
        }
        "web" => {
            let build = service.build.as_ref().ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "render export requires build metadata for `{}`",
                    service.name
                ))
            })?;
            let start = service.start.as_ref().ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "render export requires start metadata for `{}`",
                    service.name
                ))
            })?;

            Ok(RenderService {
                name: service.name.clone(),
                service_type: "web".to_owned(),
                runtime: Some(service.runtime.clone()),
                root_dir: Some(service.source_root.clone()),
                build_command: Some(build.command.clone()),
                start_command: Some(start.command.clone()),
                pre_deploy_command: service.release.as_ref().map(|step| step.command.clone()),
                static_publish_path: None,
                health_check_path: service.health.as_ref().map(|health| health.path.clone()),
                domains: service.domains.clone(),
                routes: Vec::new(),
                env_vars,
            })
        }
        "worker" => {
            let build = service.build.as_ref().ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "render export requires build metadata for `{}`",
                    service.name
                ))
            })?;
            let start = service.start.as_ref().ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "render export requires start metadata for `{}`",
                    service.name
                ))
            })?;

            Ok(RenderService {
                name: service.name.clone(),
                service_type: "worker".to_owned(),
                runtime: Some(service.runtime.clone()),
                root_dir: Some(service.source_root.clone()),
                build_command: Some(build.command.clone()),
                start_command: Some(start.command.clone()),
                pre_deploy_command: None,
                static_publish_path: None,
                health_check_path: None,
                domains: Vec::new(),
                routes: Vec::new(),
                env_vars,
            })
        }
        "cron" => Err(RunnerError::task_invocation(
            "render export does not support `cron` services yet".to_owned(),
        )),
        other => Err(RunnerError::task_invocation(format!(
            "render export does not support service role `{other}` yet"
        ))),
    }
}

fn render_env_vars(
    service: &DeployService,
    database_name: Option<&str>,
) -> Result<Vec<RenderEnvVar>, RunnerError> {
    let mut vars = service
        .env
        .iter()
        .map(|(key, value)| RenderEnvVar {
            key: key.clone(),
            value: Some(value.clone()),
            sync: None,
            from_database: None,
        })
        .collect::<Vec<_>>();

    for secret_ref in &service.secret_refs {
        if secret_ref == "DATABASE_URL" {
            let Some(database_name) = database_name else {
                return Err(RunnerError::task_invocation(format!(
                    "render export cannot satisfy `DATABASE_URL` for `{}` without managed postgres",
                    service.name
                )));
            };
            vars.push(RenderEnvVar {
                key: secret_ref.clone(),
                value: None,
                sync: None,
                from_database: Some(RenderFromDatabase {
                    name: database_name.to_owned(),
                    property: "connectionString".to_owned(),
                }),
            });
        } else {
            vars.push(RenderEnvVar {
                key: secret_ref.clone(),
                value: None,
                sync: Some(false),
                from_database: None,
            });
        }
    }

    Ok(vars)
}
