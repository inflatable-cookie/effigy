use std::path::Path;

use serde_json::Value as JsonValue;

use super::implementation::ContainerExecError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningComposeContainer {
    pub container_name: String,
    pub status: String,
    pub ports: Vec<String>,
    pub project_name: Option<String>,
    pub working_dir: Option<String>,
    pub service: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningComposeContainerProfiled {
    pub profile: String,
    pub row: RunningComposeContainer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningContainerStats {
    pub container_name: String,
    pub cpu_percent: Option<String>,
    pub memory_usage: Option<String>,
    pub memory_percent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningContainerStatsCapture {
    pub stats: Vec<RunningContainerStats>,
    pub warning: Option<String>,
}

pub(super) fn docker_failure_looks_like_colima_dns_outage(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    (combined.contains("registry-1.docker.io")
        || combined.contains("docker.io")
        || combined.contains("failed to resolve source metadata"))
        && combined.contains("lookup")
        && combined.contains("connection refused")
}

pub(super) fn docker_failure_looks_like_colima_runtime_state_loss(
    stdout: &str,
    stderr: &str,
) -> bool {
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    combined.contains("error retrieving current runtime: empty value")
        || (combined.contains("current runtime") && combined.contains("empty value"))
}

pub(super) fn parse_running_compose_containers(
    stdout: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let mut rows = Vec::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let Some((container_name, status, ports, project_name, working_dir, service)) =
            parse_running_compose_container_row(line)
        else {
            continue;
        };

        if container_name.is_empty() || status.is_empty() {
            return Err(ContainerExecError::Failure {
                command: "docker ps".to_owned(),
                code: None,
                stdout: stdout.to_owned(),
                stderr: format!("failed to parse docker ps row: {line}"),
            });
        }

        rows.push(RunningComposeContainer {
            container_name,
            status,
            ports,
            project_name,
            working_dir,
            service,
        });
    }

    Ok(rows)
}

fn parse_running_compose_container_row(
    line: &str,
) -> Option<(
    String,
    String,
    Vec<String>,
    Option<String>,
    Option<String>,
    Option<String>,
)> {
    let trimmed = line.trim();
    if trimmed.eq_ignore_ascii_case("name status") {
        return None;
    }

    if line.contains('\t') {
        let mut parts = line.splitn(6, '\t');
        let container_name = parts.next().unwrap_or_default().trim().to_owned();
        let status = parts.next().unwrap_or_default().trim().to_owned();
        let ports = parts
            .next()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let project_name = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let working_dir = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let service = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);

        return Some((
            container_name,
            status,
            ports,
            project_name,
            working_dir,
            service,
        ));
    }

    let mut parts = trimmed.split_whitespace();
    let container_name = parts.next()?.to_owned();
    let status = parts.collect::<Vec<_>>().join(" ");
    Some((container_name, status, Vec::new(), None, None, None))
}

pub(super) fn parse_running_container_stats(
    stdout: &str,
) -> Result<Vec<RunningContainerStats>, ContainerExecError> {
    let mut rows = Vec::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let parsed: JsonValue =
            serde_json::from_str(line).map_err(|error| ContainerExecError::Failure {
                command: "docker stats".to_owned(),
                code: None,
                stdout: stdout.to_owned(),
                stderr: format!("failed to parse stats row as json: {error}"),
            })?;
        let Some(object) = parsed.as_object() else {
            return Err(ContainerExecError::Failure {
                command: "docker stats".to_owned(),
                code: None,
                stdout: stdout.to_owned(),
                stderr: format!("stats row was not a json object: {line}"),
            });
        };
        let container_name = json_string_field(object, &["Name", "name", "Container", "container"])
            .ok_or_else(|| ContainerExecError::Failure {
                command: "docker stats".to_owned(),
                code: None,
                stdout: stdout.to_owned(),
                stderr: format!("stats row missing container name field: {line}"),
            })?;
        rows.push(RunningContainerStats {
            container_name,
            cpu_percent: json_string_field(object, &["CPUPerc", "cpu_percent", "CPU"]),
            memory_usage: json_string_field(object, &["MemUsage", "memory_usage", "Memory"]),
            memory_percent: json_string_field(object, &["MemPerc", "memory_percent"]),
        });
    }

    Ok(rows)
}

#[derive(serde::Deserialize)]
struct InspectContainerRecord {
    #[serde(rename = "Config")]
    config: Option<InspectContainerConfig>,
    #[serde(rename = "Mounts", default)]
    mounts: Vec<InspectMount>,
}

#[derive(serde::Deserialize)]
struct InspectContainerConfig {
    #[serde(rename = "WorkingDir")]
    working_dir: Option<String>,
}

#[derive(serde::Deserialize)]
struct InspectMount {
    #[serde(rename = "Type")]
    mount_type: Option<String>,
    #[serde(rename = "Source")]
    source: Option<String>,
    #[serde(rename = "Destination")]
    destination: Option<String>,
}

pub(super) fn infer_host_working_dir_from_inspect(stdout: &str) -> Result<Option<String>, String> {
    let records: Vec<InspectContainerRecord> = serde_json::from_str(stdout)
        .map_err(|error| format!("failed to parse inspect json: {error}"))?;
    let Some(record) = records.first() else {
        return Ok(None);
    };
    let container_working_dir = record
        .config
        .as_ref()
        .and_then(|config| config.working_dir.as_deref())
        .filter(|value| !value.is_empty());

    if let Some(container_working_dir) = container_working_dir {
        let best = record
            .mounts
            .iter()
            .filter(|mount| mount.mount_type.as_deref() == Some("bind"))
            .filter_map(|mount| {
                let source = mount.source.as_deref()?;
                let destination = mount.destination.as_deref()?;
                if container_working_dir == destination {
                    return Some((destination.len(), source.to_owned()));
                }
                let prefix = format!("{destination}/");
                container_working_dir.strip_prefix(&prefix).map(|suffix| {
                    (
                        destination.len(),
                        Path::new(source).join(suffix).display().to_string(),
                    )
                })
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, host_path)| host_path);
        if best.is_some() {
            return Ok(best);
        }
    }

    Ok(record
        .mounts
        .iter()
        .filter(|mount| mount.mount_type.as_deref() == Some("bind"))
        .filter_map(|mount| mount.source.as_deref())
        .find(|source| Path::new(source).join("effigy.toml").is_file())
        .map(str::to_owned))
}

fn json_string_field(object: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_running_compose_containers_splits_tab_fields() {
        let parsed = parse_running_compose_containers(
            "demo-app-1\tUp 2 minutes\t0.0.0.0:18080->80/tcp, :::18080->80/tcp\tdemo-web-dev\t/tmp/demo\tapp\n",
        )
        .expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].container_name, "demo-app-1");
        assert_eq!(parsed[0].project_name.as_deref(), Some("demo-web-dev"));
        assert_eq!(parsed[0].working_dir.as_deref(), Some("/tmp/demo"));
        assert_eq!(parsed[0].service.as_deref(), Some("app"));
        assert_eq!(parsed[0].ports.len(), 2);
    }

    #[test]
    fn parse_running_compose_containers_skips_plain_table_header() {
        let parsed = parse_running_compose_containers("NAME STATUS\n").expect("parse");
        assert!(parsed.is_empty());
    }

    #[test]
    fn parse_running_compose_containers_accepts_plain_table_rows() {
        let parsed = parse_running_compose_containers("NAME STATUS\napp running\n").expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].container_name, "app");
        assert_eq!(parsed[0].status, "running");
        assert!(parsed[0].ports.is_empty());
        assert_eq!(parsed[0].project_name, None);
        assert_eq!(parsed[0].working_dir, None);
        assert_eq!(parsed[0].service, None);
    }

    #[test]
    fn infer_host_working_dir_from_inspect_maps_container_working_dir_through_bind_mount() {
        let inferred = infer_host_working_dir_from_inspect(
            r#"[{
              "Config": { "WorkingDir": "/workspace-root/underlay-reference" },
              "Mounts": [
                { "Type": "bind", "Source": "/Users/tom/Dev/projects/underlay-reference", "Destination": "/workspace-root/underlay-reference" },
                { "Type": "bind", "Source": "/Users/tom/Dev/projects/underlay", "Destination": "/workspace-root/underlay" }
              ]
            }]"#,
        )
        .expect("inspect parse");

        assert_eq!(
            inferred.as_deref(),
            Some("/Users/tom/Dev/projects/underlay-reference")
        );
    }

    #[test]
    fn infer_host_working_dir_from_inspect_prefers_longest_matching_bind_mount() {
        let inferred = infer_host_working_dir_from_inspect(
            r#"[{
              "Config": { "WorkingDir": "/var/www/cbs/subdir" },
              "Mounts": [
                { "Type": "bind", "Source": "/Users/tom/Dev/test", "Destination": "/var/www" },
                { "Type": "bind", "Source": "/Users/tom/Dev/test/cbs", "Destination": "/var/www/cbs" }
              ]
            }]"#,
        )
        .expect("inspect parse");

        assert_eq!(inferred.as_deref(), Some("/Users/tom/Dev/test/cbs/subdir"));
    }

    #[test]
    fn infer_host_working_dir_from_inspect_falls_back_to_repo_root_bind_mount() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("demo");
        std::fs::create_dir_all(&repo).expect("mkdir repo");
        std::fs::write(repo.join("effigy.toml"), "[manifest]\n").expect("write manifest");

        let inferred = infer_host_working_dir_from_inspect(&format!(
            r#"[{{
              "Config": {{ "WorkingDir": null }},
              "Mounts": [
                {{ "Type": "bind", "Source": "{}", "Destination": "/workspace-root/demo" }},
                {{ "Type": "bind", "Source": "/Users/tom/.gitconfig", "Destination": "/home/dev/.gitconfig" }}
              ]
            }}]"#,
            repo.display()
        ))
        .expect("inspect parse");

        assert_eq!(inferred.as_deref(), Some(repo.to_string_lossy().as_ref()));
    }

    #[test]
    fn parse_running_container_stats_reads_json_lines() {
        let parsed = parse_running_container_stats(
            r#"{"Name":"demo-app-1","CPUPerc":"1.25%","MemUsage":"12.4MiB / 8GiB","MemPerc":"0.15%"}"#,
        )
        .expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].container_name, "demo-app-1");
        assert_eq!(parsed[0].cpu_percent.as_deref(), Some("1.25%"));
        assert_eq!(parsed[0].memory_usage.as_deref(), Some("12.4MiB / 8GiB"));
        assert_eq!(parsed[0].memory_percent.as_deref(), Some("0.15%"));
    }

    #[test]
    fn docker_failure_detection_matches_registry_dns_outage_shape() {
        assert!(docker_failure_looks_like_colima_dns_outage(
            "",
            r#"failed to solve: rust:1.88-bookworm: failed to resolve source metadata for docker.io/library/rust:1.88-bookworm: failed to do request: Head "https://registry-1.docker.io/v2/library/rust/manifests/1.88-bookworm": dial tcp: lookup registry-1.docker.io on 192.168.5.3:53: read udp 192.168.5.3:48612->192.168.5.3:53: read: connection refused"#
        ));
    }

    #[test]
    fn docker_failure_detection_ignores_unrelated_compose_errors() {
        assert!(!docker_failure_looks_like_colima_dns_outage(
            "",
            "service workspace depends on undefined service redis"
        ));
    }

    #[test]
    fn docker_failure_detection_matches_colima_runtime_state_loss() {
        assert!(docker_failure_looks_like_colima_runtime_state_loss(
            "",
            r#"time="2026-04-20T00:09:46+01:00" level=fatal msg="error retrieving current runtime: empty value""#
        ));
    }

    #[test]
    fn runtime_state_loss_detection_ignores_unrelated_errors() {
        assert!(!docker_failure_looks_like_colima_runtime_state_loss(
            "",
            "service workspace depends on undefined service redis"
        ));
    }

    #[test]
    fn runtime_state_loss_detection_matches_colima_status_failure() {
        assert!(docker_failure_looks_like_colima_runtime_state_loss(
            "",
            r#"time="2026-04-20T00:14:42+01:00" level=fatal msg="error retrieving current runtime: empty value""#
        ));
    }
}
