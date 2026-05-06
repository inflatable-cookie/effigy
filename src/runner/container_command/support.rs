use std::ffi::OsString;
use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{reset_commands, DockerCommand, VolumeClassification};
use effigy_container_manager::{ContainerBackendDetection, ContainerManager};
use effigy_containers::{
    compose::compose_args,
    exec::{
        list_running_compose_containers_for_profile, run_docker_capture, ContainerExecError,
        RunningComposeContainer,
    },
    health::wait_for_ready,
    EffectiveContainerPolicy,
};
use effigy_core::shell::shell_quote;
use serde_json::json;

use super::gateway_registration::{
    resolve_gateway_tcp_alias_routes_for_container, RegisteredGatewayRoute,
};
use super::RunnerError;

pub(super) fn wait_for_container_ready(
    policy: &EffectiveContainerPolicy,
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Option<&'static str>, RunnerError> {
    wait_for_ready(
        &policy.name,
        policy.health_check.as_deref(),
        policy.health_timeout_secs,
        stop_requested,
    )
    .map_err(RunnerError::task_invocation)
}

pub(in crate::runner) fn validate_running_container_runtime_match(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let rows = match list_running_compose_containers_for_profile(&policy.profile) {
        Ok(rows) => rows,
        Err(error) if exec_error_means_runtime_not_running(&error) => return Ok(()),
        Err(error) => return Err(RunnerError::task_invocation(error.to_string())),
    };
    if let Some(message) = running_container_runtime_mismatch(repo_root, policy, &rows) {
        return Err(RunnerError::task_invocation(message));
    }
    Ok(())
}

fn exec_error_means_runtime_not_running(error: &ContainerExecError) -> bool {
    match error {
        ContainerExecError::Failure { stderr, .. } => {
            stderr.contains("level=fatal msg=\"colima is not running\"")
                || stderr.contains("Cannot connect to the Docker daemon")
                || stderr.contains("daemon is not running")
        }
        ContainerExecError::Launch { .. } => false,
    }
}

fn running_container_runtime_mismatch(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
) -> Option<String> {
    let repo_rows = rows
        .iter()
        .filter(|row| {
            row.working_dir.as_deref().is_some_and(|working_dir| {
                effigy_runtime::read::working_dir_belongs_to_repo(working_dir, repo_root)
            })
        })
        .collect::<Vec<_>>();
    if repo_rows.is_empty() {
        return None;
    }

    let expected_project_running = repo_rows.iter().any(|row| {
        row.project_name.as_deref() == Some(policy.project_name.as_str())
            && row.service.as_deref() == Some(policy.primary_service.as_str())
    });
    if expected_project_running {
        return None;
    }

    let mut stale_projects = repo_rows
        .iter()
        .filter(|row| row.service.as_deref() == Some(policy.primary_service.as_str()))
        .filter_map(|row| row.project_name.as_deref())
        .filter(|project_name| *project_name != policy.project_name)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    stale_projects.sort();
    stale_projects.dedup();
    if stale_projects.is_empty() {
        return None;
    }

    let stale = stale_projects
        .iter()
        .map(|project| format!("`{project}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "container `{}` expects Compose project `{}`, but this repo still has running `{}` service container(s) under {}. `project_name` changed while the old runtime was still up; stop the stale project first, then start the current one again.",
        policy.name, policy.project_name, policy.primary_service, stale
    ))
}

pub(super) fn rewrite_manifest_for_ejected_compose(
    repo_root: &Path,
    container_name: &str,
    compose_path: &Path,
) -> Result<(), RunnerError> {
    let manifest_path = repo_root.join("effigy.toml");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|error| RunnerError::task_invocation_failed_read(&manifest_path, error))?;
    let mut document = raw.parse::<toml_edit::DocumentMut>().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse {} while finalizing eject: {error}",
            manifest_path.display()
        ))
    })?;

    let containers = document["containers"].as_table_like_mut().ok_or_else(|| {
        RunnerError::task_invocation("manifest missing `[containers]` while finalizing eject")
    })?;
    let container = containers
        .get_mut(container_name)
        .and_then(|item| item.as_table_like_mut())
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "manifest missing `[containers.{container_name}]` while finalizing eject"
            ))
        })?;
    container.remove("services");
    let relative = compose_path
        .strip_prefix(repo_root)
        .unwrap_or(compose_path)
        .display()
        .to_string();
    container.insert("compose_file", toml_edit::value(relative));

    std::fs::write(&manifest_path, document.to_string())
        .map_err(|error| RunnerError::task_invocation_failed_write(&manifest_path, error))
}

pub(super) fn annotate_registered_gateway_routes(
    report: &mut effigy_containers::ContainerCommandReport,
    routes: &[RegisteredGatewayRoute],
) {
    if routes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_routes".to_owned(),
            json!(routes
                .iter()
                .map(|route| json!({
                    "action": "registered",
                    "domain": route.domain,
                    "target": route.target,
                    "dns_ip": route.dns_ip.map(|value| value.to_string()),
                    "tls": route.tls,
                }))
                .collect::<Vec<_>>()),
        );
    }
    for route in routes {
        report.success_text.push('\n');
        match route.target.as_deref() {
            Some(target) => report.success_text.push_str(&format!(
                "[gateway] registered {} -> {}",
                route.domain, target
            )),
            None => report.success_text.push_str(&format!(
                "[gateway] registered {} -> dns {}",
                route.domain,
                route
                    .dns_ip
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "default".to_owned())
            )),
        }
    }
}

pub(super) fn install_primary_service_tcp_alias_hosts(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
) -> Result<Vec<String>, RunnerError> {
    let pairs = tcp_alias_host_pairs(policy, routes);
    if pairs.is_empty() {
        return Ok(Vec::new());
    }

    let script = render_tcp_alias_hosts_script(&pairs);
    let tail = vec![
        "exec",
        "-T",
        "--user",
        "root",
        policy.primary_service.as_str(),
        "sh",
        "-lc",
        script.as_str(),
    ];
    run_docker_capture(
        repo_root,
        policy,
        &compose_args(policy, tail),
        "docker compose exec tcp alias hosts",
    )
    .map_err(RunnerError::from)?;

    Ok(pairs
        .into_iter()
        .map(|(domain, host)| format!("{domain} -> {host}"))
        .collect())
}

pub(in crate::runner) fn reconcile_primary_service_tcp_alias_hosts(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let routes = resolve_gateway_tcp_alias_routes_for_container(repo_root, policy)?;
    install_primary_service_tcp_alias_hosts(repo_root, policy, &routes)
}

fn tcp_alias_host_pairs(
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
) -> Vec<(String, String)> {
    let shared_hosts = policy
        .shared_services
        .iter()
        .map(|shared| (shared.service_name.as_str(), shared.host.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut pairs = std::collections::BTreeSet::new();
    for route in routes {
        let Some(service) = route.service.as_deref() else {
            continue;
        };
        if route.tcp_port.is_none() {
            continue;
        }
        let host = shared_hosts.get(service).copied().unwrap_or(service);
        pairs.insert((route.domain.clone(), host.to_owned()));
    }
    pairs.into_iter().collect()
}

fn render_tcp_alias_hosts_script(pairs: &[(String, String)]) -> String {
    let mut script = String::from(
        r#"set -eu
patch_alias() {
  alias="$1"
  service="$2"
  ip="$(getent hosts "$service" | awk 'NR == 1 { print $1 }')"
  if [ -z "$ip" ]; then
    printf '[effigy] could not resolve service `%s` for alias `%s`\n' "$service" "$alias" >&2
    exit 1
  fi
  tmp="$(mktemp)"
  awk -v alias="$alias" '{
    keep=1
    for (i = 2; i <= NF; i++) {
      if ($i == alias) {
        keep=0
      }
    }
    if (keep) {
      print
    }
  }' /etc/hosts > "$tmp"
  cat "$tmp" > /etc/hosts
  rm -f "$tmp"
  printf '%s %s\n' "$ip" "$alias" >> /etc/hosts
}
"#,
    );
    for (domain, service) in pairs {
        script.push_str("patch_alias ");
        script.push_str(&shell_quote(domain));
        script.push(' ');
        script.push_str(&shell_quote(service));
        script.push('\n');
    }
    script
}

pub(super) fn ensure_shared_services_running(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let mut notes = Vec::new();
    for service in &policy.shared_services {
        let workdir = service.compose_file.parent().ok_or_else(|| {
            RunnerError::task_invocation("shared service compose file has no parent directory")
        })?;
        run_shared_compose_capture(
            workdir,
            &policy.profile,
            &shared_compose_args(service, ["up", "-d"]),
            &format!("docker compose up (shared {})", service.service_name),
        )?;
        notes.push(format!(
            "{} [{}] -> {}:{}",
            service.service_name, service.catalog, service.host, service.host_port
        ));
    }
    Ok(notes)
}

pub(super) fn remove_reset_volumes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    classification: &VolumeClassification,
) -> Result<(), RunnerError> {
    for command in reset_commands(classification) {
        run_runtime_volume_capture(repo_root, &policy.profile, &command)?;
    }
    Ok(())
}

pub(super) fn annotate_shared_service_notes(
    report: &mut effigy_containers::ContainerCommandReport,
    notes: &[String],
) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            json!({
                "action": "ensured",
                "services": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[shared] ensured {note}"));
    }
}

pub(super) fn annotate_tcp_alias_host_notes(
    report: &mut effigy_containers::ContainerCommandReport,
    notes: &[String],
) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "tcp_alias_hosts".to_owned(),
            json!({
                "action": "installed",
                "aliases": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[gateway] installed container TCP alias {note}"));
    }
}

pub(super) fn annotate_warning_lines(
    report: &mut effigy_containers::ContainerCommandReport,
    warnings: &[String],
) {
    if warnings.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("warnings".to_owned(), json!(warnings));
    }
    for warning in warnings {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[warn] {warning}"));
    }
}

#[cfg(test)]
pub(super) fn annotate_left_running_shared_services(
    report: &mut effigy_containers::ContainerCommandReport,
    policy: &EffectiveContainerPolicy,
) {
    if policy.shared_services.is_empty() {
        return;
    }
    let services = policy
        .shared_services
        .iter()
        .map(|service| {
            format!(
                "{} [{}] -> {}:{}",
                service.service_name, service.catalog, service.host, service.host_port
            )
        })
        .collect::<Vec<_>>();
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            json!({
                "action": "left-running",
                "services": services,
            }),
        );
    }
    for service in services {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[shared] left running {service}"));
    }
}

fn shared_compose_args<'a>(
    service: &effigy_containers::SharedServiceBinding,
    tail: impl IntoIterator<Item = &'a str>,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("compose"),
        OsString::from("-f"),
        service.compose_file.as_os_str().to_os_string(),
        OsString::from("-p"),
        OsString::from(service.project_name.as_str()),
    ];
    args.extend(tail.into_iter().map(OsString::from));
    args
}

fn run_shared_compose_capture(
    repo_root: &Path,
    profile: &str,
    args: &[OsString],
    label: &str,
) -> Result<std::process::Output, RunnerError> {
    let (program, args) = runtime_process_invocation(profile, "docker", args)?;
    std::process::Command::new(&program)
        .current_dir(repo_root)
        .args(&args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{label} ({} {})",
                program.to_string_lossy(),
                format_args(&args)
            ),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{label} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

pub(super) fn run_runtime_volume_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    if command.program == "__effigy_volume_usage" {
        return run_runtime_volume_usage_capture(repo_root, profile, command);
    }
    let docker_args = runtime_args(&command.args);
    let (program, args) =
        runtime_process_invocation(profile, command.program.as_str(), &docker_args)?;
    std::process::Command::new(&program)
        .current_dir(repo_root)
        .args(&args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} ({} {})",
                command.description,
                program.to_string_lossy(),
                format_args(&args)
            ),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

fn run_runtime_volume_usage_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    let Some(mount_point) = command.args.first() else {
        return Err(RunnerError::task_invocation(
            "runtime volume usage command requires one mount-point argument",
        ));
    };
    let detection = ContainerBackendDetection::from_env_and_path();
    let (program, args) = if detection.docker_cli_available {
        (
            OsString::from("du"),
            vec![OsString::from("-sk"), OsString::from(mount_point)],
        )
    } else {
        (
            effigy_containers::compose::resolve_host_cli_program("colima"),
            vec![
                OsString::from("ssh"),
                OsString::from("--profile"),
                OsString::from(profile),
                OsString::from("--"),
                OsString::from("sudo"),
                OsString::from("du"),
                OsString::from("-sk"),
                OsString::from(mount_point),
            ],
        )
    };

    std::process::Command::new(&program)
        .current_dir(repo_root)
        .args(&args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} ({} {})",
                command.description,
                program.to_string_lossy(),
                format_args(&args)
            ),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

fn runtime_process_invocation(
    profile: &str,
    docker_program: &str,
    args: &[OsString],
) -> Result<(OsString, Vec<OsString>), RunnerError> {
    ContainerManager::defaults()
        .runtime_process_invocation(
            &ContainerBackendDetection::from_env_and_path(),
            profile,
            docker_program,
            args,
        )
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn runtime_args(args: &[String]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{
        exec_error_means_runtime_not_running, render_tcp_alias_hosts_script,
        running_container_runtime_mismatch, tcp_alias_host_pairs,
    };
    use effigy_containers::{
        exec::{ContainerExecError, RunningComposeContainer},
        EffectiveComposeSource, EffectiveContainerPolicy,
    };
    use std::path::{Path, PathBuf};

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo-web-renamed".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }

    fn test_policy_with_shared_services(
        shared_services: Vec<effigy_containers::SharedServiceBinding>,
    ) -> EffectiveContainerPolicy {
        let mut policy = test_policy();
        policy.shared_services = shared_services;
        policy
    }

    #[test]
    fn runtime_mismatch_detects_stale_project_name_for_same_repo_service() {
        let mismatch = running_container_runtime_mismatch(
            Path::new("/tmp/demo"),
            &test_policy(),
            &[RunningComposeContainer {
                container_name: "demo-app-1".to_owned(),
                status: "Up 2 minutes".to_owned(),
                ports: vec![],
                project_name: Some("demo-web-old".to_owned()),
                working_dir: Some("/tmp/demo".to_owned()),
                service: Some("app".to_owned()),
            }],
        )
        .expect("mismatch");

        assert!(mismatch.contains("expects Compose project `demo-web-renamed`"));
        assert!(mismatch.contains("under `demo-web-old`"));
    }

    #[test]
    fn runtime_mismatch_ignores_matching_project_name() {
        let mismatch = running_container_runtime_mismatch(
            Path::new("/tmp/demo"),
            &test_policy(),
            &[RunningComposeContainer {
                container_name: "demo-app-1".to_owned(),
                status: "Up 2 minutes".to_owned(),
                ports: vec![],
                project_name: Some("demo-web-renamed".to_owned()),
                working_dir: Some("/tmp/demo".to_owned()),
                service: Some("app".to_owned()),
            }],
        );

        assert!(mismatch.is_none());
    }

    #[test]
    fn runtime_not_running_errors_degrade_to_no_mismatch_probe() {
        let colima_stopped = ContainerExecError::Failure {
            command: "colima nerdctl ps".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: "time=\"2026-04-21T07:59:20+01:00\" level=fatal msg=\"colima is not running\""
                .to_owned(),
        };
        let docker_stopped = ContainerExecError::Failure {
            command: "docker ps".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: "Cannot connect to the Docker daemon".to_owned(),
        };

        assert!(exec_error_means_runtime_not_running(&colima_stopped));
        assert!(exec_error_means_runtime_not_running(&docker_stopped));
    }

    #[test]
    fn tcp_alias_host_pairs_use_gateway_tcp_routes_only() {
        let policy = test_policy();
        let routes = vec![
            super::RegisteredGatewayRoute {
                domain: "api.demo.test".to_owned(),
                target: Some("127.0.0.1:19900".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                tls: false,
                service: Some("workspace".to_owned()),
                external_target: false,
            },
            super::RegisteredGatewayRoute {
                domain: "db.demo.test".to_owned(),
                target: None,
                dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
                tcp_port: Some(5432),
                tcp_target: Some("127.0.0.1:19932".to_owned()),
                tls: false,
                service: Some("postgres".to_owned()),
                external_target: false,
            },
        ];

        assert_eq!(
            tcp_alias_host_pairs(&policy, &routes),
            vec![("db.demo.test".to_owned(), "postgres".to_owned())]
        );
    }

    #[test]
    fn tcp_alias_host_pairs_rewrite_shared_service_targets_to_shared_host() {
        let policy =
            test_policy_with_shared_services(vec![effigy_containers::SharedServiceBinding {
                service_name: "db".to_owned(),
                catalog: "mariadb".to_owned(),
                domain_label: "mysql".to_owned(),
                project_name: "effigy-shared-mariadb-deadbeef".to_owned(),
                compose_file: PathBuf::from("/tmp/shared-db/docker-compose.yml"),
                host: "host.docker.internal".to_owned(),
                host_port: 23306,
                container_port: 3306,
                host_env_vars: Vec::new(),
                port_env_vars: Vec::new(),
            }]);
        let routes = vec![super::RegisteredGatewayRoute {
            domain: "db.demo.test".to_owned(),
            target: None,
            dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 7)),
            tcp_port: Some(3306),
            tcp_target: Some("127.0.0.1:23306".to_owned()),
            tls: false,
            service: Some("db".to_owned()),
            external_target: false,
        }];

        assert_eq!(
            tcp_alias_host_pairs(&policy, &routes),
            vec![("db.demo.test".to_owned(), "host.docker.internal".to_owned())]
        );
    }

    #[test]
    fn tcp_alias_hosts_script_rewrites_existing_alias_entries() {
        let script =
            render_tcp_alias_hosts_script(&[("db.demo.test".to_owned(), "postgres".to_owned())]);

        assert!(script.contains("patch_alias 'db.demo.test' 'postgres'"));
        assert!(script.contains("awk -v alias=\"$alias\""));
        assert!(script.contains("printf '%s %s\\n' \"$ip\" \"$alias\" >> /etc/hosts"));
    }
}
