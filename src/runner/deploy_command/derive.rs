use std::path::Path;

use effigy_manifest::task_runtime::{ManifestManagedRun, ManifestTask};

use super::model::*;
use crate::runner::error::RunnerError;
use crate::runner::manifest::{load_task_manifest, load_task_manifest_with_inspection};

pub(super) fn derive_deploy_model(repo_root: &Path) -> Result<DeployModel, RunnerError> {
    let loaded =
        load_task_manifest_with_inspection(&repo_root.join(effigy_manifest::TASK_MANIFEST_FILE))?;
    let bundle = loaded.manifest.bundle.as_ref().ok_or_else(|| {
        RunnerError::task_invocation("`deploy model` requires a bundle-backed repo".to_owned())
    })?;
    let base = match bundle.base.as_ref() {
        Some(effigy_manifest::ManifestBundleBase::Shipped { name }) => name.as_str(),
        Some(effigy_manifest::ManifestBundleBase::Path { .. }) => {
            return Err(RunnerError::task_invocation(
                "`deploy model` does not support `[bundle].base = { type = \"path\", ... }` yet"
                    .to_owned(),
            ));
        }
        Some(effigy_manifest::ManifestBundleBase::Git { .. }) => {
            return Err(RunnerError::task_invocation(
                "`deploy model` does not support git bundle sources yet".to_owned(),
            ));
        }
        Some(effigy_manifest::ManifestBundleBase::Oci { .. }) => {
            return Err(RunnerError::task_invocation(
                "`deploy model` does not support OCI bundle sources yet".to_owned(),
            ));
        }
        None => {
            return Err(RunnerError::task_invocation(
                "`deploy model` requires a bundle base selection".to_owned(),
            ));
        }
    };

    match base {
        "underlay" => derive_underlay_model(repo_root, bundle),
        other => Err(RunnerError::task_invocation(format!(
            "`deploy model` currently supports only the shipped `underlay` bundle, got `{other}`"
        ))),
    }
}

fn derive_underlay_model(
    repo_root: &Path,
    bundle: &effigy_manifest::ManifestBundleConfig,
) -> Result<DeployModel, RunnerError> {
    let host = required_bundle_string(bundle, "host")?;
    let project_name = required_bundle_string(bundle, "project_name")?;
    let front_dir = bundle_dir_or_default(bundle, "dirs.front", "app-front")?;
    let admin_dir = bundle_dir_or_default(bundle, "dirs.admin", "app-admin")?;
    let api_dir = bundle_dir_or_default(bundle, "dirs.api", "app-api")?;
    let databases = required_bundle_string_list(bundle, "databases")?;
    let api_port = bundle_integer_or_default(bundle, "api_port", 41001)?;

    let front_manifest = load_child_manifest(repo_root, &front_dir)?;
    let admin_manifest = load_child_manifest(repo_root, &admin_dir)?;
    let api_manifest = load_child_manifest(repo_root, &api_dir)?;

    let front_domain = route_domain(
        &host,
        bundle_string_or_default(bundle, "routes.front", "")?.as_str(),
    );
    let admin_domain = route_domain(
        &host,
        bundle_string_or_default(bundle, "routes.admin", "admin")?.as_str(),
    );
    let api_domain = route_domain(
        &host,
        bundle_string_or_default(bundle, "routes.api", "api")?.as_str(),
    );

    let front_build = required_task_command(&front_manifest, "build", &front_dir)?;
    let admin_build = required_task_command(&admin_manifest, "build", &admin_dir)?;
    let api_build = required_task_command(&api_manifest, "build", &api_dir)?;
    let api_start = required_task_command(&api_manifest, "api", &api_dir)?;
    let api_release = optional_task_command(&api_manifest, "db:migrate");
    let jobs_start = optional_task_command(&api_manifest, "jobs");
    let front_fallback = detect_static_fallback(repo_root, &front_dir);
    let admin_fallback = detect_static_fallback(repo_root, &admin_dir);

    let repo_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("app")
        .to_owned();

    let mut services = vec![
        DeployService {
            name: "front".to_owned(),
            role: "static".to_owned(),
            runtime: "node".to_owned(),
            source_root: front_dir.clone(),
            build: Some(DeployCommandStep {
                command: front_build,
            }),
            start: None,
            release: None,
            health: None,
            output: Some(DeployOutput {
                kind: "directory".to_owned(),
                path: "build".to_owned(),
                fallback: front_fallback.clone(),
            }),
            port: None,
            domains: vec![front_domain.clone()],
            env: std::collections::BTreeMap::new(),
            secret_refs: Vec::new(),
            volumes: Vec::new(),
            warnings: missing_static_fallback_warning("front", front_fallback.is_none()),
        },
        DeployService {
            name: "admin".to_owned(),
            role: "static".to_owned(),
            runtime: "node".to_owned(),
            source_root: admin_dir.clone(),
            build: Some(DeployCommandStep {
                command: admin_build,
            }),
            start: None,
            release: None,
            health: None,
            output: Some(DeployOutput {
                kind: "directory".to_owned(),
                path: "build".to_owned(),
                fallback: admin_fallback.clone(),
            }),
            port: None,
            domains: vec![admin_domain.clone()],
            env: std::collections::BTreeMap::new(),
            secret_refs: Vec::new(),
            volumes: Vec::new(),
            warnings: missing_static_fallback_warning("admin", admin_fallback.is_none()),
        },
        DeployService {
            name: "api".to_owned(),
            role: "web".to_owned(),
            runtime: "rust".to_owned(),
            source_root: api_dir.clone(),
            build: Some(DeployCommandStep {
                command: api_build.clone(),
            }),
            start: Some(DeployCommandStep { command: api_start }),
            release: api_release.as_ref().map(|command| DeployCommandStep {
                command: command.clone(),
            }),
            health: Some(DeployHealth {
                kind: "http".to_owned(),
                path: "/v1/health".to_owned(),
            }),
            output: None,
            port: Some(api_port),
            domains: vec![api_domain.clone()],
            env: std::collections::BTreeMap::new(),
            secret_refs: vec!["DATABASE_URL".to_owned()],
            volumes: Vec::new(),
            warnings: api_release
                .is_none()
                .then_some(DeployWarning {
                    code: "missing-release-hook".to_owned(),
                    scope: "service".to_owned(),
                    target: Some("api".to_owned()),
                    message: "No explicit `db:migrate` release or migration command is promoted into the deployment model yet".to_owned(),
                    severity: "warn".to_owned(),
                })
                .into_iter()
                .collect(),
        },
    ];

    if let Some(jobs_start) = jobs_start {
        services.push(DeployService {
            name: "jobs".to_owned(),
            role: "worker".to_owned(),
            runtime: "rust".to_owned(),
            source_root: api_dir.clone(),
            build: Some(DeployCommandStep {
                command: api_build.clone(),
            }),
            start: Some(DeployCommandStep {
                command: jobs_start,
            }),
            release: None,
            health: None,
            output: None,
            port: None,
            domains: Vec::new(),
            env: std::collections::BTreeMap::new(),
            secret_refs: vec!["DATABASE_URL".to_owned()],
            volumes: Vec::new(),
            warnings: Vec::new(),
        });
    }

    let mut secret_services = vec!["api".to_owned()];
    if services.iter().any(|service| service.name == "jobs") {
        secret_services.push("jobs".to_owned());
    }

    let model = DeployModel {
        schema: "deploy.model.v1".to_owned(),
        schema_version: 1,
        app: DeployApp {
            name: repo_name,
            bundle: Some("underlay".to_owned()),
            project_name,
            source_root: Some(".".to_owned()),
            notes: None,
        },
        services,
        backing_services: vec![DeployBackingService {
            name: "postgres".to_owned(),
            kind: "postgres".to_owned(),
            mode: "managed".to_owned(),
            required: true,
            consumers: secret_services.clone(),
            warnings: Vec::new(),
        }],
        domains: vec![
            DeployDomain {
                host: front_domain,
                service: "front".to_owned(),
                tls: "provider_managed".to_owned(),
            },
            DeployDomain {
                host: admin_domain,
                service: "admin".to_owned(),
                tls: "provider_managed".to_owned(),
            },
            DeployDomain {
                host: api_domain,
                service: "api".to_owned(),
                tls: "provider_managed".to_owned(),
            },
        ],
        secrets: vec![DeploySecret {
            name: "DATABASE_URL".to_owned(),
            services: secret_services,
            required: true,
            source: "operator".to_owned(),
            notes: Some(format!(
                "Managed Postgres connection string for primary database `{}`",
                databases
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "app".to_owned())
            )),
        }],
        warnings: Vec::new(),
    };

    Ok(model)
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

fn required_task_command(
    manifest: &effigy_manifest::TaskManifest,
    task_name: &str,
    dir: &str,
) -> Result<String, RunnerError> {
    optional_task_command(manifest, task_name).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "required task `{task_name}` is missing or non-command in `{dir}/effigy.toml`"
        ))
    })
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

fn required_bundle_string(
    bundle: &effigy_manifest::ManifestBundleConfig,
    key: &str,
) -> Result<String, RunnerError> {
    bundle_value(bundle, key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_owned())
        .ok_or_else(|| {
            RunnerError::task_invocation(format!("missing required bundle input `{key}`"))
        })
}

fn required_bundle_string_list(
    bundle: &effigy_manifest::ManifestBundleConfig,
    key: &str,
) -> Result<Vec<String>, RunnerError> {
    let items = bundle_value(bundle, key)
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            RunnerError::task_invocation(format!("missing required bundle input `{key}`"))
        })?;

    let values = items
        .iter()
        .map(|value: &toml::Value| {
            value.as_str().map(|item| item.to_owned()).ok_or_else(|| {
                RunnerError::task_invocation(format!("bundle input `{key}` must be a string list"))
            })
        })
        .collect::<Result<Vec<String>, RunnerError>>()?;

    if values.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "bundle input `{key}` must not be empty"
        )));
    }

    Ok(values)
}

fn bundle_integer_or_default(
    bundle: &effigy_manifest::ManifestBundleConfig,
    key: &str,
    default: u16,
) -> Result<u16, RunnerError> {
    let Some(value) = bundle_value(bundle, key) else {
        return Ok(default);
    };

    let raw = value.as_integer().ok_or_else(|| {
        RunnerError::task_invocation(format!("bundle input `{key}` must be an integer"))
    })?;
    u16::try_from(raw).map_err(|_| {
        RunnerError::task_invocation(format!("bundle input `{key}` must fit into a u16"))
    })
}

fn bundle_string_or_default(
    bundle: &effigy_manifest::ManifestBundleConfig,
    key: &str,
    default: &str,
) -> Result<String, RunnerError> {
    match bundle_value(bundle, key) {
        Some(value) => value.as_str().map(|item| item.to_owned()).ok_or_else(|| {
            RunnerError::task_invocation(format!("bundle input `{key}` must be a string"))
        }),
        None => Ok(default.to_owned()),
    }
}

fn bundle_dir_or_default(
    bundle: &effigy_manifest::ManifestBundleConfig,
    key: &str,
    default: &str,
) -> Result<String, RunnerError> {
    bundle_string_or_default(bundle, key, default)
}

fn bundle_value<'a>(
    bundle: &'a effigy_manifest::ManifestBundleConfig,
    key: &str,
) -> Option<&'a toml::Value> {
    if let Some(value) = bundle.inputs.get(key) {
        return Some(value);
    }

    let mut parts = key.split('.');
    let first = parts.next()?;
    let mut value = bundle.inputs.get(first)?;

    for part in parts {
        value = value.as_table()?.get(part)?;
    }

    Some(value)
}

fn route_domain(host: &str, label: &str) -> String {
    if label.trim().is_empty() {
        host.to_owned()
    } else {
        format!("{}.{}", label.trim(), host)
    }
}
