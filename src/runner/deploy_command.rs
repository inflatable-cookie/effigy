use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use effigy_cli::{DeployArgs, DeployExportProvider, DeploySubcommand};
use effigy_manifest::task_runtime::{ManifestManagedRun, ManifestTask};
use regex::Regex;
use serde::Serialize;

use super::command_context::resolve_repo_root;
use super::error::RunnerError;
use super::manifest::{load_task_manifest, load_task_manifest_with_inspection};

pub(super) fn run_deploy(args: DeployArgs) -> Result<String, RunnerError> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let resolved = resolve_repo_root(cwd, args.repo_override)?;

    match args.subcommand {
        DeploySubcommand::Model => run_deploy_model(&resolved.resolved_root, args.output_json),
        DeploySubcommand::Export {
            provider,
            path,
            plan,
        } => run_deploy_export(
            &resolved.resolved_root,
            provider,
            &path,
            plan,
            args.output_json,
        ),
    }
}

fn run_deploy_model(repo_root: &Path, output_json: bool) -> Result<String, RunnerError> {
    if !output_json {
        return Err(RunnerError::task_invocation(
            "`deploy model` currently requires `--json`".to_owned(),
        ));
    }

    let model = derive_deploy_model(repo_root)?;
    serde_json::to_string_pretty(&model).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode deploy model: {error}"))
    })
}

fn run_deploy_export(
    repo_root: &Path,
    provider: DeployExportProvider,
    path: &Path,
    plan: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let model = derive_deploy_model(repo_root)?;

    match provider {
        DeployExportProvider::Render => run_deploy_export_render(&model, path, plan, output_json),
    }
}

fn derive_deploy_model(repo_root: &Path) -> Result<DeployModel, RunnerError> {
    let loaded =
        load_task_manifest_with_inspection(&repo_root.join(effigy_manifest::TASK_MANIFEST_FILE))?;
    let bundle = loaded.manifest.bundle.as_ref().ok_or_else(|| {
        RunnerError::task_invocation("`deploy model` requires a bundle-backed repo".to_owned())
    })?;
    let base = bundle.base.as_deref().ok_or_else(|| {
        RunnerError::task_invocation(
            "`deploy model` does not support `[bundle].base_path` yet".to_owned(),
        )
    })?;

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
            env: BTreeMap::new(),
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
            env: BTreeMap::new(),
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
            env: BTreeMap::new(),
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
            env: BTreeMap::new(),
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
        .map(|value| {
            value.as_str().map(|item| item.to_owned()).ok_or_else(|| {
                RunnerError::task_invocation(format!("bundle input `{key}` must be a string list"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

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

fn run_deploy_export_render(
    model: &DeployModel,
    path: &Path,
    plan: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let export = build_render_export(model, path)?;

    if !plan {
        fs::create_dir_all(path).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to create render export directory {}: {error}",
                path.display()
            ))
        })?;
        fs::write(path.join("render.yaml"), export.render_yaml.as_bytes()).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write {}: {error}",
                path.join("render.yaml").display()
            ))
        })?;
    }

    if output_json {
        return serde_json::to_string_pretty(&DeployExportResult {
            schema: "effigy.deploy.export.v1".to_owned(),
            schema_version: 1,
            provider: "render".to_owned(),
            plan,
            path: path.display().to_string(),
            files: vec!["render.yaml".to_owned()],
            warnings: export.warnings,
        })
        .map_err(|error| {
            RunnerError::task_invocation(format!("failed to encode deploy export result: {error}"))
        });
    }

    let mut lines = vec![if plan {
        format!("[deploy] planned render export to {}", path.display())
    } else {
        format!("[deploy] exported render files to {}", path.display())
    }];
    lines.push(String::new());
    lines.push("Files (1)".to_owned());
    lines.push("- render.yaml".to_owned());
    if !export.warnings.is_empty() {
        lines.push(String::new());
        lines.push(format!("Warnings ({})", export.warnings.len()));
        lines.extend(
            export
                .warnings
                .into_iter()
                .map(|warning| format!("- [{}] {}", warning.code, warning.message)),
        );
    }
    Ok(lines.join("\n"))
}

fn build_render_export(model: &DeployModel, path: &Path) -> Result<RenderExportPlan, RunnerError> {
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

#[derive(Clone, Serialize)]
struct DeployModel {
    schema: String,
    schema_version: u64,
    app: DeployApp,
    services: Vec<DeployService>,
    backing_services: Vec<DeployBackingService>,
    domains: Vec<DeployDomain>,
    secrets: Vec<DeploySecret>,
    warnings: Vec<DeployWarning>,
}

#[derive(Clone, Serialize)]
struct DeployApp {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    bundle: Option<String>,
    project_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
}

#[derive(Clone, Serialize)]
struct DeployService {
    name: String,
    role: String,
    runtime: String,
    source_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    build: Option<DeployCommandStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<DeployCommandStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    release: Option<DeployCommandStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<DeployHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<DeployOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    domains: Vec<String>,
    env: BTreeMap<String, String>,
    secret_refs: Vec<String>,
    volumes: Vec<String>,
    warnings: Vec<DeployWarning>,
}

#[derive(Clone, Serialize)]
struct DeployCommandStep {
    command: String,
}

#[derive(Clone, Serialize)]
struct DeployHealth {
    kind: String,
    path: String,
}

#[derive(Clone, Serialize)]
struct DeployOutput {
    kind: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback: Option<String>,
}

fn detect_static_fallback(repo_root: &Path, dir: &str) -> Option<String> {
    let service_root = repo_root.join(dir);
    let config_names = [
        "svelte.config.js",
        "svelte.config.ts",
        "svelte.config.mjs",
        "svelte.config.cjs",
    ];
    let fallback_regex = Regex::new(r#"fallback\s*:\s*["']([^"']+)["']"#).ok()?;

    for config_name in config_names {
        let path = service_root.join(config_name);
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(captures) = fallback_regex.captures(&contents) {
            if let Some(value) = captures.get(1) {
                return Some(value.as_str().to_owned());
            }
        }
    }

    None
}

fn missing_static_fallback_warning(target: &str, missing: bool) -> Vec<DeployWarning> {
    if !missing {
        return Vec::new();
    }

    vec![DeployWarning {
        code: "missing-static-fallback".to_owned(),
        scope: "service".to_owned(),
        target: Some(target.to_owned()),
        message: "No static fallback file is declared yet for provider rewrite generation"
            .to_owned(),
        severity: "warn".to_owned(),
    }]
}

#[derive(Clone, Serialize)]
struct DeployBackingService {
    name: String,
    kind: String,
    mode: String,
    required: bool,
    consumers: Vec<String>,
    warnings: Vec<DeployWarning>,
}

#[derive(Clone, Serialize)]
struct DeployDomain {
    host: String,
    service: String,
    tls: String,
}

#[derive(Clone, Serialize)]
struct DeploySecret {
    name: String,
    services: Vec<String>,
    required: bool,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
}

#[derive(Clone, Serialize)]
struct DeployWarning {
    code: String,
    scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    message: String,
    severity: String,
}

#[derive(Serialize)]
struct DeployExportResult {
    schema: String,
    schema_version: u64,
    provider: String,
    plan: bool,
    path: String,
    files: Vec<String>,
    warnings: Vec<DeployWarning>,
}

struct RenderExportPlan {
    render_yaml: String,
    warnings: Vec<DeployWarning>,
}

#[derive(Serialize)]
struct RenderBlueprint {
    services: Vec<RenderService>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    databases: Vec<RenderDatabase>,
}

#[derive(Serialize)]
struct RenderService {
    name: String,
    #[serde(rename = "type")]
    service_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<String>,
    #[serde(rename = "rootDir", skip_serializing_if = "Option::is_none")]
    root_dir: Option<String>,
    #[serde(rename = "buildCommand", skip_serializing_if = "Option::is_none")]
    build_command: Option<String>,
    #[serde(rename = "startCommand", skip_serializing_if = "Option::is_none")]
    start_command: Option<String>,
    #[serde(rename = "preDeployCommand", skip_serializing_if = "Option::is_none")]
    pre_deploy_command: Option<String>,
    #[serde(rename = "staticPublishPath", skip_serializing_if = "Option::is_none")]
    static_publish_path: Option<String>,
    #[serde(rename = "healthCheckPath", skip_serializing_if = "Option::is_none")]
    health_check_path: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    domains: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    routes: Vec<RenderRoute>,
    #[serde(rename = "envVars", skip_serializing_if = "Vec::is_empty")]
    env_vars: Vec<RenderEnvVar>,
}

#[derive(Serialize)]
struct RenderRoute {
    #[serde(rename = "type")]
    route_type: String,
    source: String,
    destination: String,
}

#[derive(Serialize)]
struct RenderEnvVar {
    key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync: Option<bool>,
    #[serde(rename = "fromDatabase", skip_serializing_if = "Option::is_none")]
    from_database: Option<RenderFromDatabase>,
}

#[derive(Serialize)]
struct RenderFromDatabase {
    name: String,
    property: String,
}

#[derive(Serialize)]
struct RenderDatabase {
    name: String,
    plan: String,
    #[serde(rename = "databaseName", skip_serializing_if = "Option::is_none")]
    database_name: Option<String>,
}
