use std::collections::BTreeMap;
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
    pub oneoff: bool,
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

pub(super) fn docker_failure_looks_like_colima_runtime_state_loss(
    stdout: &str,
    stderr: &str,
) -> bool {
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    combined.contains("error retrieving current runtime: empty value")
        || (combined.contains("current runtime") && combined.contains("empty value"))
}

pub fn compose_status_is_running(status: &str) -> bool {
    let lowered = status.to_ascii_lowercase();
    if compose_status_is_in_flight(status) || lowered.contains("paused") {
        return false;
    }
    if lowered.contains("exited") || lowered.contains("created") || lowered.contains("dead") {
        return false;
    }
    lowered.contains("up") || lowered.contains("running")
}

/// Statuses that say the backend is already acting on the container, so the
/// container is neither running nor a reliable start target.
fn compose_status_is_in_flight(status: &str) -> bool {
    let lowered = status.to_ascii_lowercase();
    [
        "restarting",
        "starting",
        "stopping",
        "pausing",
        "unpausing",
        "removing",
    ]
    .iter()
    .any(|word| lowered.contains(word))
}

/// A full 64-character container id, with any `sha256:` prefix removed.
pub(crate) fn parse_full_container_id(raw: &str) -> Option<String> {
    let id = raw.strip_prefix("sha256:").unwrap_or(raw).trim();
    if id.len() != 64 {
        return None;
    }
    if !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(id.to_ascii_lowercase())
}

const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";
const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";
const COMPOSE_ONEOFF_LABEL: &str = "com.docker.compose.oneoff";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VolumeRef {
    pub name: Option<String>,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub mount_type: Option<String>,
}

/// One container's identity as its own `inspect` reports it: the full id, its
/// name, its state and the Compose labels that locate its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectedContainerIdentity {
    pub id: String,
    pub name: String,
    pub status: String,
    pub running: bool,
    pub project: Option<String>,
    pub service: Option<String>,
    pub oneoff: bool,
    pub volume_refs: Vec<VolumeRef>,
}

/// Parse exactly one inspect record. An id that is not a full hexadecimal
/// container id, a missing name or a missing `State.Status` is not proof of
/// anything, so it is refused here.
pub(crate) fn parse_inspected_container_identity(
    stdout: &str,
) -> Result<InspectedContainerIdentity, String> {
    let records: Vec<InspectRecord> = serde_json::from_str(stdout)
        .map_err(|error| format!("failed to parse inspect json: {error}"))?;
    if records.len() != 1 {
        return Err(format!(
            "inspect returned {} records; exactly one container is required",
            records.len()
        ));
    }
    let record = &records[0];
    let id = parse_full_container_id(record.id.as_deref().unwrap_or("")).ok_or_else(|| {
        format!(
            "inspect Id is not a full 64-character hexadecimal container id (`{}`)",
            record.id.as_deref().unwrap_or("")
        )
    })?;
    let name = record
        .name
        .as_deref()
        .map(|value| value.trim_start_matches('/').to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "inspect record missing container name".to_owned())?;
    let status = record
        .state
        .as_ref()
        .and_then(|state| state.status.as_deref())
        .unwrap_or("")
        .trim()
        .to_owned();
    if status.is_empty() {
        return Err("inspect record missing State.Status".to_owned());
    }
    let running = record
        .state
        .as_ref()
        .and_then(|state| state.running)
        .unwrap_or(false);
    let labels = record
        .config
        .as_ref()
        .and_then(|config| config.labels.as_ref());
    let project = labels.and_then(|labels| label_value(labels, COMPOSE_PROJECT_LABEL));
    let service = labels.and_then(|labels| label_value(labels, COMPOSE_SERVICE_LABEL));
    let oneoff = labels
        .and_then(|labels| label_value(labels, COMPOSE_ONEOFF_LABEL))
        .map(|value| {
            let lowered = value.to_ascii_lowercase();
            lowered == "true" || lowered == "1" || lowered == "yes"
        })
        .unwrap_or(false);
    let volume_refs = record
        .mounts
        .iter()
        .map(|mount| VolumeRef {
            name: empty_to_none(mount.name.clone()),
            source: empty_to_none(mount.source.clone()),
            destination: empty_to_none(mount.destination.clone()),
            mount_type: empty_to_none(mount.mount_type.clone()),
        })
        .collect();
    Ok(InspectedContainerIdentity {
        id,
        name,
        status,
        running,
        project,
        service,
        oneoff,
        volume_refs,
    })
}

fn label_value(labels: &BTreeMap<String, String>, key: &str) -> Option<String> {
    labels
        .get(key)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[derive(serde::Deserialize)]
struct InspectRecord {
    #[serde(rename = "Id", alias = "ID", alias = "id")]
    id: Option<String>,
    #[serde(rename = "Name", alias = "name")]
    name: Option<String>,
    #[serde(rename = "State")]
    state: Option<InspectStateWord>,
    #[serde(rename = "Config")]
    config: Option<InspectConfig>,
    #[serde(rename = "Mounts", default)]
    mounts: Vec<InspectMount>,
}

#[derive(serde::Deserialize)]
struct InspectStateWord {
    #[serde(rename = "Status", alias = "status")]
    status: Option<String>,
    #[serde(rename = "Running", alias = "running")]
    running: Option<bool>,
}

#[derive(serde::Deserialize)]
struct InspectConfig {
    #[serde(rename = "Labels", default)]
    labels: Option<BTreeMap<String, String>>,
}

pub fn parse_running_compose_containers(
    stdout: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let mut rows = Vec::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let Some((container_name, status, ports, project_name, working_dir, service, oneoff)) =
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
            oneoff,
        });
    }

    Ok(rows)
}

/// Parse a complete, project-filtered inventory for an absence decision.
pub(super) fn strict_project_inventory(
    stdout: &str,
    project: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let refusal = || ContainerExecError::Failure {
        command: "runtime ps decision inventory".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "incomplete or inconsistent project inventory".to_owned(),
    };
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 7
            || fields[0].trim().is_empty()
            || fields[1].trim().is_empty()
            || fields[3].trim() != project
            || fields[5].trim().is_empty()
            || !matches!(
                fields[6].trim().to_ascii_lowercase().as_str(),
                "" | "false" | "0" | "no" | "true" | "1" | "yes"
            )
        {
            return Err(refusal());
        }
    }
    parse_running_compose_containers(stdout)
}

type RunningComposeContainerRow = (
    String,
    String,
    Vec<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
);

fn parse_running_compose_container_row(line: &str) -> Option<RunningComposeContainerRow> {
    let trimmed = line.trim();
    if trimmed.eq_ignore_ascii_case("name status") {
        return None;
    }

    if line.contains('\t') {
        let mut parts = line.splitn(7, '\t');
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
        let oneoff = parts
            .next()
            .map(str::trim)
            .is_some_and(compose_oneoff_label);

        return Some((
            container_name,
            status,
            ports,
            project_name,
            working_dir,
            service,
            oneoff,
        ));
    }

    let mut parts = trimmed.split_whitespace();
    let container_name = parts.next()?.to_owned();
    let status = parts.collect::<Vec<_>>().join(" ");
    Some((container_name, status, Vec::new(), None, None, None, false))
}

fn compose_oneoff_label(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes")
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
    #[serde(rename = "Type", alias = "type")]
    mount_type: Option<String>,
    #[serde(rename = "Name", alias = "name", default)]
    name: Option<String>,
    #[serde(rename = "Source", alias = "source")]
    source: Option<String>,
    #[serde(rename = "Destination", alias = "destination")]
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
        .find(|source| effigy_core::repo_markers::has_task_manifest(Path::new(source)))
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

    /// `docker compose ps` output shapes: tab-separated fields with ports, an
    /// oneoff row, a header alone and a plain table row.
    #[test]
    fn parse_running_compose_containers_cases() {
        struct Expect {
            container_name: &'static str,
            status: &'static str,
            project: Option<&'static str>,
            working_dir: Option<&'static str>,
            service: Option<&'static str>,
            ports: usize,
            oneoff: bool,
        }
        let cases: [(&str, &str, Option<Expect>); 4] = [
            (
                "a tab-separated row",
                "demo-app-1\tUp 2 minutes\t0.0.0.0:18080->80/tcp, :::18080->80/tcp\tdemo-web-dev\t/tmp/demo\tapp\n",
                Some(Expect {
                    container_name: "demo-app-1",
                    status: "Up 2 minutes",
                    project: Some("demo-web-dev"),
                    working_dir: Some("/tmp/demo"),
                    service: Some("app"),
                    ports: 2,
                    oneoff: false,
                }),
            ),
            (
                "an oneoff container",
                "demo-app-run-deadbeef\tExited (0) 1 second ago\t\tdemo-web-dev\t/tmp/demo\tapp\tTrue\n",
                Some(Expect {
                    container_name: "demo-app-run-deadbeef",
                    status: "Exited (0) 1 second ago",
                    project: Some("demo-web-dev"),
                    working_dir: Some("/tmp/demo"),
                    service: Some("app"),
                    ports: 0,
                    oneoff: true,
                }),
            ),
            ("a header alone", "NAME STATUS\n", None),
            (
                "a plain table row",
                "NAME STATUS\napp running\n",
                Some(Expect {
                    container_name: "app",
                    status: "running",
                    project: None,
                    working_dir: None,
                    service: None,
                    ports: 0,
                    oneoff: false,
                }),
            ),
        ];
        for (name, input, expected) in cases {
            let parsed = parse_running_compose_containers(input).expect(name);
            let Some(expected) = expected else {
                assert!(parsed.is_empty(), "{name}");
                continue;
            };
            assert_eq!(parsed.len(), 1, "{name}");
            let row = &parsed[0];
            assert_eq!(row.container_name, expected.container_name, "{name}");
            assert_eq!(row.status, expected.status, "{name}");
            assert_eq!(row.project_name.as_deref(), expected.project, "{name}");
            assert_eq!(row.working_dir.as_deref(), expected.working_dir, "{name}");
            assert_eq!(row.service.as_deref(), expected.service, "{name}");
            assert_eq!(row.ports.len(), expected.ports, "{name}");
            assert_eq!(row.oneoff, expected.oneoff, "{name}");
        }
    }

    #[test]
    fn decision_inventory_refuses_rows_without_provable_service_and_project() {
        assert!(strict_project_inventory("app Up\n", "demo").is_err());
        assert!(strict_project_inventory("app\tUp\t\tdemo\t/tmp\t\tfalse\n", "demo").is_err());
        assert!(strict_project_inventory("app\tUp\t\tother\t/tmp\tapp\tfalse\n", "demo").is_err());
        assert!(strict_project_inventory("", "demo").unwrap().is_empty());
    }

    #[test]
    fn infer_host_working_dir_from_inspect_maps_container_working_dir_through_bind_mount() {
        let inferred = infer_host_working_dir_from_inspect(
            r#"[{
              "Config": { "WorkingDir": "/workspace-root/workspace-app-reference" },
              "Mounts": [
                { "Type": "bind", "Source": "/Users/tom/Dev/projects/workspace-app-reference", "Destination": "/workspace-root/workspace-app-reference" },
                { "Type": "bind", "Source": "/Users/tom/Dev/projects/platform", "Destination": "/workspace-root/platform" }
              ]
            }]"#,
        )
        .expect("inspect parse");

        assert_eq!(
            inferred.as_deref(),
            Some("/Users/tom/Dev/projects/workspace-app-reference")
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

    /// The Colima runtime-state-loss heuristic, still live for an explicit
    /// `profile recover`: its fatal is recognised and an unrelated Compose
    /// error is not.
    #[test]
    fn colima_runtime_state_loss_cases() {
        let cases: [(&str, &str, bool); 3] = [
            (
                "a fatal empty runtime value",
                r#"time="2026-04-20T00:09:46+01:00" level=fatal msg="error retrieving current runtime: empty value""#,
                true,
            ),
            (
                "the same fatal from a status command",
                r#"time="2026-04-20T00:14:42+01:00" level=fatal msg="error retrieving current runtime: empty value""#,
                true,
            ),
            (
                "an undefined service",
                "service workspace depends on undefined service redis",
                false,
            ),
        ];
        for (name, stderr, expected) in cases {
            assert_eq!(
                docker_failure_looks_like_colima_runtime_state_loss("", stderr),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn compose_status_classifies_running_versus_stopped() {
        assert!(compose_status_is_running("Up 10 seconds"));
        assert!(compose_status_is_running("Up (healthy)"));
        assert!(compose_status_is_running("running"));
        assert!(!compose_status_is_running("Exited (255) 2 minutes ago"));
        assert!(!compose_status_is_running("Created"));
        assert!(!compose_status_is_running("Paused"));
        for status in [
            "Restarting (1) 2 seconds ago",
            "Starting",
            "Stopping",
            "Removing",
            "Pausing",
        ] {
            assert!(
                !compose_status_is_running(status),
                "`{status}` in-flight status must not read as running"
            );
        }
    }

    const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn parse_full_id_rejects_short_and_non_hex() {
        assert_eq!(parse_full_container_id(FULL_ID).as_deref(), Some(FULL_ID));
        assert_eq!(
            parse_full_container_id(&format!("sha256:{FULL_ID}")).as_deref(),
            Some(FULL_ID)
        );
        assert!(parse_full_container_id("dd94022f7dd0").is_none());
        assert!(parse_full_container_id("acowtancy-mysql-1").is_none());
        assert!(parse_full_container_id(&"g".repeat(64)).is_none());
    }

    #[test]
    fn inspect_identity_reads_full_id_labels_and_volumes() {
        let identity = parse_inspected_container_identity(&format!(
            r#"[{{"Id":"{FULL_ID}","Name":"/acowtancy-mysql-1","State":{{"Status":"exited","Running":false}},"Config":{{"Labels":{{"com.docker.compose.project":"acowtancy-shared-mysql","com.docker.compose.service":"mysql","com.docker.compose.oneoff":"False"}}}},"Mounts":[{{"Type":"volume","Name":"acowtancy_mysql_data","Destination":"/var/lib/mysql"}}]}}]"#
        ))
        .expect("identity");
        assert_eq!(identity.id, FULL_ID);
        assert_eq!(identity.name, "acowtancy-mysql-1");
        assert_eq!(identity.status, "exited");
        assert!(!identity.running);
        assert_eq!(identity.project.as_deref(), Some("acowtancy-shared-mysql"));
        assert_eq!(identity.service.as_deref(), Some("mysql"));
        assert!(!identity.oneoff);
        assert_eq!(identity.volume_refs.len(), 1);
        assert_eq!(
            identity.volume_refs[0].name.as_deref(),
            Some("acowtancy_mysql_data")
        );
        assert_eq!(
            identity.volume_refs[0].destination.as_deref(),
            Some("/var/lib/mysql")
        );
    }
}
