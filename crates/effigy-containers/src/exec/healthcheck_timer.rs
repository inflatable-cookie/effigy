//! Recover exact-owned stale nerdctl health-check timer/service collisions.
//!
//! nerdctl names transient systemd units `{full_container_id}.timer` and
//! `{full_container_id}.service`. After a VM restart those units can stay
//! loaded and `systemd-run` fails with "already loaded or has a fragment file".
//!
//! Recovery is allowed only for a stopped owned stack container after inspect
//! proves the full hexadecimal ID, selected project/service labels, and that
//! the unit pair is transient in the selected Colima profile. The only
//! mutations are `systemctl stop` of `{id}.timer` and `systemctl reset-failed`
//! of `{id}.service` and `{id}.timer`. Units are never deleted.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::time::Duration;

use super::implementation::ContainerExecError;
use super::parse::RunningComposeContainer;

pub(crate) const FULL_CONTAINER_ID_LEN: usize = 64;

const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";
const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";
const COMPOSE_ONEOFF_LABEL: &str = "com.docker.compose.oneoff";

/// How many times recovery re-probes the exact-owned transient unit pair after
/// `stop`/`reset-failed` while waiting for a stopped pair to unload. A stopped
/// transient unit can stay loaded for a moment; operation exit 0 alone is not
/// proof of a restart-safe state.
const MAX_RESTART_SAFE_PROBES: usize = 5;
/// Delay between bounded post-stop restart-safe probes.
const RESTART_SAFE_PROBE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VolumeRef {
    pub name: Option<String>,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub mount_type: Option<String>,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemdUnitShow {
    pub id: String,
    pub load_state: String,
    pub transient: bool,
    pub fragment_path: String,
    pub unit_file_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnitClassification {
    Transient,
    Missing,
    Persistent { fragment_path: String },
    Ambiguous { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpectedOwnership<'a> {
    pub container_name: &'a str,
    pub project_name: &'a str,
    pub service: &'a str,
    pub profile: &'a str,
    pub declared_services: &'a [&'a str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StaleTimerAction {
    Skip,
    Recovered {
        warning: String,
        volume_refs: Vec<VolumeRef>,
        stop_timer: Vec<OsString>,
        reset_failed: Vec<OsString>,
    },
    Refused {
        diagnostic: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandOutcome {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnitProbe {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub(crate) fn timer_unit_name(full_id: &str) -> String {
    format!("{full_id}.timer")
}

pub(crate) fn service_unit_name(full_id: &str) -> String {
    format!("{full_id}.service")
}

pub(crate) fn show_unit_invocation(profile: &str, unit: &str) -> (OsString, Vec<OsString>) {
    colima_systemctl_invocation(
        profile,
        &[
            "show",
            unit,
            "--property=Id",
            "--property=LoadState",
            "--property=Transient",
            "--property=FragmentPath",
            "--property=UnitFileState",
        ],
    )
}

pub(crate) fn stop_timer_invocation(profile: &str, full_id: &str) -> (OsString, Vec<OsString>) {
    colima_systemctl_invocation(profile, &["stop", &timer_unit_name(full_id)])
}

pub(crate) fn reset_failed_invocation(profile: &str, full_id: &str) -> (OsString, Vec<OsString>) {
    colima_systemctl_invocation(
        profile,
        &[
            "reset-failed",
            &service_unit_name(full_id),
            &timer_unit_name(full_id),
        ],
    )
}

fn colima_systemctl_invocation(
    profile: &str,
    systemctl_args: &[&str],
) -> (OsString, Vec<OsString>) {
    let mut args = vec![
        OsString::from("ssh"),
        OsString::from("--profile"),
        OsString::from(profile),
        OsString::from("--"),
        OsString::from("sudo"),
        OsString::from("systemctl"),
    ];
    args.extend(systemctl_args.iter().copied().map(OsString::from));
    (OsString::from("colima"), args)
}

pub(crate) fn parse_full_container_id(raw: &str) -> Option<String> {
    let id = raw.strip_prefix("sha256:").unwrap_or(raw).trim();
    if id.len() != FULL_CONTAINER_ID_LEN {
        return None;
    }
    if !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(id.to_ascii_lowercase())
}

pub(crate) fn parse_inspected_container_identity(
    stdout: &str,
) -> Result<InspectedContainerIdentity, String> {
    let records: Vec<InspectRecord> = serde_json::from_str(stdout)
        .map_err(|error| format!("failed to parse inspect json: {error}"))?;
    if records.len() != 1 {
        return Err(format!(
            "inspect returned {} records; recovery requires exactly one container",
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

pub(crate) fn parse_systemctl_show(stdout: &str) -> SystemdUnitShow {
    let mut id = String::new();
    let mut load_state = String::new();
    let mut transient = false;
    let mut fragment_path = String::new();
    let mut unit_file_state = String::new();
    for line in stdout.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Id" => id = value.trim().to_owned(),
            "LoadState" => load_state = value.trim().to_owned(),
            "Transient" => {
                transient = value.trim().eq_ignore_ascii_case("yes")
                    || value.trim().eq_ignore_ascii_case("true")
                    || value.trim() == "1";
            }
            "FragmentPath" => fragment_path = value.trim().to_owned(),
            "UnitFileState" => unit_file_state = value.trim().to_owned(),
            _ => {}
        }
    }
    SystemdUnitShow {
        id,
        load_state,
        transient,
        fragment_path,
        unit_file_state,
    }
}

pub(crate) fn classify_unit(
    show: &SystemdUnitShow,
    expected_unit: &str,
    probe: &UnitProbe,
) -> UnitClassification {
    if !probe.success && show.load_state.is_empty() && show.id.is_empty() {
        let detail = probe_detail(probe);
        return UnitClassification::Ambiguous {
            reason: format!("required systemctl show of `{expected_unit}` failed ({detail})"),
        };
    }
    if show.load_state.eq_ignore_ascii_case("not-found")
        || show.load_state.eq_ignore_ascii_case("notfound")
    {
        return UnitClassification::Missing;
    }
    if !show.id.is_empty() && show.id != expected_unit {
        return UnitClassification::Ambiguous {
            reason: format!(
                "unit Id `{}` does not match exact owned unit `{expected_unit}`",
                show.id
            ),
        };
    }
    if is_persistent_fragment(&show.fragment_path) || is_persistent_unit_file(&show.unit_file_state)
    {
        return UnitClassification::Persistent {
            fragment_path: show.fragment_path.clone(),
        };
    }
    if show.transient && is_transient_fragment(&show.fragment_path) {
        return UnitClassification::Transient;
    }
    if show.id.is_empty() && show.load_state.is_empty() && probe.success {
        return UnitClassification::Missing;
    }
    UnitClassification::Ambiguous {
        reason: format!(
            "unit `{expected_unit}` is loaded but not a verified transient health-check unit (Transient={}, FragmentPath=`{}`, LoadState=`{}`)",
            if show.transient { "yes" } else { "no" },
            show.fragment_path,
            show.load_state
        ),
    }
}

pub(crate) fn recover_owned_stale_healthcheck_units(
    expected: ExpectedOwnership<'_>,
    inspect: &UnitProbe,
    mut probe_unit: impl FnMut(&str) -> Result<UnitProbe, ContainerExecError>,
    mut run: impl FnMut(&str, &[OsString]) -> Result<CommandOutcome, ContainerExecError>,
    mut pause: impl FnMut(Duration),
) -> Result<StaleTimerAction, ContainerExecError> {
    if !inspect.success {
        return Ok(refuse(
            None,
            expected,
            &format!("container inspect failed ({})", probe_detail(inspect)),
        ));
    }
    let identity = match parse_inspected_container_identity(&inspect.stdout) {
        Ok(identity) => identity,
        Err(reason) => return Ok(refuse(None, expected, &reason)),
    };
    if let Some(reason) = ownership_refusal(&identity, expected) {
        return Ok(refuse(Some(&identity), expected, &reason));
    }

    let timer_name = timer_unit_name(&identity.id);
    let service_name = service_unit_name(&identity.id);
    let (timer, _) = probe_classified_unit(&mut probe_unit, &timer_name)?;
    let (service, _) = probe_classified_unit(&mut probe_unit, &service_name)?;
    if let Some(reason) = unit_pair_refusal(&timer, &service, &timer_name, &service_name) {
        return Ok(refuse(Some(&identity), expected, &reason));
    }

    let (stop_program, stop_timer) = stop_timer_invocation(expected.profile, &identity.id);
    let (reset_program, reset_failed) = reset_failed_invocation(expected.profile, &identity.id);
    let stop_program = stop_program.to_string_lossy().into_owned();
    let stop = run(&stop_program, &stop_timer)?;
    if !command_is_idempotent_success(&stop) {
        return Ok(refuse(
            Some(&identity),
            expected,
            &format!(
                "systemctl stop of `{timer_name}` failed ({})",
                outcome_detail(&stop)
            ),
        ));
    }
    let reset_program = reset_program.to_string_lossy().into_owned();
    let reset = run(&reset_program, &reset_failed)?;
    if !command_is_idempotent_success(&reset) {
        return Ok(refuse(
            Some(&identity),
            expected,
            &format!(
                "systemctl reset-failed of `{service_name}`/`{timer_name}` failed ({})",
                outcome_detail(&reset)
            ),
        ));
    }

    if let Some(reason) =
        verify_restart_safe(&mut probe_unit, &mut pause, &timer_name, &service_name)?
    {
        return Ok(refuse_after_mutation(Some(&identity), expected, &reason));
    }

    Ok(StaleTimerAction::Recovered {
        warning: format!(
            "recovered stale nerdctl health-check timer for owned service `{}` (`{}`); stopped `{timer_name}`, reset-failed `{service_name}`/`{timer_name}`, and verified both transient units unloaded after stop/reset before the single retry start, without deleting units",
            expected.service, identity.id
        ),
        volume_refs: identity.volume_refs,
        stop_timer,
        reset_failed,
    })
}

fn probe_classified_unit(
    probe_unit: &mut impl FnMut(&str) -> Result<UnitProbe, ContainerExecError>,
    unit: &str,
) -> Result<(UnitClassification, SystemdUnitShow), ContainerExecError> {
    let raw = probe_unit(unit)?;
    let show = parse_systemctl_show(&raw.stdout);
    let classification = classify_unit(&show, unit, &raw);
    Ok((classification, show))
}

/// After `stop`/`reset-failed` return idempotent success, prove the exact-owned
/// transient pair is actually unloaded before the caller retries start. A
/// stopped-but-still-loaded transient unit makes `systemd-run` fail again with
/// `already loaded or has a fragment file`, so operation exit 0 alone is not
/// proof. The pair is re-probed a bounded number of times: a unit that unloads
/// late succeeds once, a unit that never unloads returns a bounded diagnostic,
/// and a persistent/ambiguous/foreign unit appearing mid-flight refuses without
/// any deletion or cleanup outside the exact pair.
fn verify_restart_safe(
    probe_unit: &mut impl FnMut(&str) -> Result<UnitProbe, ContainerExecError>,
    pause: &mut impl FnMut(Duration),
    timer_name: &str,
    service_name: &str,
) -> Result<Option<String>, ContainerExecError> {
    let mut last_state: Option<(SystemdUnitShow, SystemdUnitShow)> = None;
    for attempt in 0..MAX_RESTART_SAFE_PROBES {
        let (timer, timer_show) = probe_classified_unit(probe_unit, timer_name)?;
        let (service, service_show) = probe_classified_unit(probe_unit, service_name)?;
        if let Some(reason) = unit_pair_refusal(&timer, &service, timer_name, service_name) {
            return Ok(Some(format!(
                "unit state changed during recovery: {reason}"
            )));
        }
        if matches!(timer, UnitClassification::Missing)
            && matches!(service, UnitClassification::Missing)
        {
            return Ok(None);
        }
        last_state = Some((timer_show, service_show));
        if attempt + 1 < MAX_RESTART_SAFE_PROBES {
            pause(RESTART_SAFE_PROBE_INTERVAL);
        }
    }
    let detail = last_state
        .map(|(timer_show, service_show)| {
            format!(
                "`{timer_name}` is {}; `{service_name}` is {}",
                describe_unit_show(&timer_show),
                describe_unit_show(&service_show)
            )
        })
        .unwrap_or_else(|| "unit state unavailable".to_owned());
    Ok(Some(format!(
        "the exact-owned transient health-check pair is still loaded after stop/reset and {MAX_RESTART_SAFE_PROBES} bounded re-probes ({detail}); a retry start would hit the same `already loaded or has a fragment file` collision"
    )))
}

fn describe_unit_show(show: &SystemdUnitShow) -> String {
    let load_state = if show.load_state.is_empty() {
        "(empty)"
    } else {
        show.load_state.as_str()
    };
    let unit_file_state = if show.unit_file_state.is_empty() {
        "(empty)"
    } else {
        show.unit_file_state.as_str()
    };
    format!(
        "LoadState={load_state} Transient={} FragmentPath=`{}` UnitFileState={unit_file_state}",
        if show.transient { "yes" } else { "no" },
        show.fragment_path
    )
}

pub(crate) fn expected_ownership_from_row<'a>(
    row: &'a RunningComposeContainer,
    project_name: &'a str,
    profile: &'a str,
    declared_services: &'a [&'a str],
) -> Option<ExpectedOwnership<'a>> {
    Some(ExpectedOwnership {
        container_name: row.container_name.as_str(),
        project_name,
        service: row.service.as_deref()?,
        profile,
        declared_services,
    })
}

fn ownership_refusal(
    identity: &InspectedContainerIdentity,
    expected: ExpectedOwnership<'_>,
) -> Option<String> {
    if identity.name != expected.container_name {
        return Some(format!(
            "inspect name `{}` does not match selected container `{}`",
            identity.name, expected.container_name
        ));
    }
    if identity.running || inspect_status_is_running(&identity.status) {
        return Some(format!(
            "container status `{}` is running or active; recovery is only for a stopped owned container",
            identity.status
        ));
    }
    if identity.status.eq_ignore_ascii_case("paused") {
        return Some(
            "container is paused; recovery is only for a stopped owned container".to_owned(),
        );
    }
    if !inspect_status_is_stopped(&identity.status) {
        return Some(format!(
            "container status `{}` is unknown; recovery requires a stopped inspect status",
            identity.status
        ));
    }
    match identity.project.as_deref() {
        Some(project) if project == expected.project_name => {}
        Some(project) => {
            return Some(format!(
                "inspect project `{project}` is foreign to selected project `{}`",
                expected.project_name
            ));
        }
        None => {
            return Some(
                "inspect is missing com.docker.compose.project; unlabeled containers are not owned"
                    .to_owned(),
            );
        }
    }
    match identity.service.as_deref() {
        Some(service) if service == expected.service => {}
        Some(service) => {
            return Some(format!(
                "inspect service `{service}` does not match selected service `{}`",
                expected.service
            ));
        }
        None => {
            return Some("inspect is missing com.docker.compose.service; undeclared orphans are not recovered".to_owned());
        }
    }
    if !expected.declared_services.contains(&expected.service) {
        return Some(format!(
            "service `{}` is not currently declared in the selected project",
            expected.service
        ));
    }
    if identity.oneoff {
        return Some(
            "container is a compose one-off; one-off containers are not recovered".to_owned(),
        );
    }
    None
}

fn unit_pair_refusal(
    timer: &UnitClassification,
    service: &UnitClassification,
    timer_name: &str,
    service_name: &str,
) -> Option<String> {
    for (unit, name) in [(timer, timer_name), (service, service_name)] {
        match unit {
            UnitClassification::Persistent { fragment_path } => {
                return Some(format!(
                    "unit `{name}` has a persistent fragment at `{fragment_path}`"
                ));
            }
            UnitClassification::Ambiguous { reason } => return Some(reason.clone()),
            UnitClassification::Transient | UnitClassification::Missing => {}
        }
    }
    None
}

fn refuse(
    identity: Option<&InspectedContainerIdentity>,
    expected: ExpectedOwnership<'_>,
    reason: &str,
) -> StaleTimerAction {
    StaleTimerAction::Refused {
        diagnostic: refusal_diagnostic(
            identity,
            expected,
            reason,
            "Effigy did not stop, reset, or delete systemd units.",
        ),
    }
}

/// Refusal after `stop`/`reset-failed` already ran. The mutation note must not
/// claim nothing happened, because the exact-owned transient pair was stopped
/// and reset; only unit-file deletion never occurred.
fn refuse_after_mutation(
    identity: Option<&InspectedContainerIdentity>,
    expected: ExpectedOwnership<'_>,
    reason: &str,
) -> StaleTimerAction {
    StaleTimerAction::Refused {
        diagnostic: refusal_diagnostic(
            identity,
            expected,
            reason,
            "Effigy stopped and reset-failed only the exact owned transient units and did not delete any unit file.",
        ),
    }
}

fn refusal_diagnostic(
    identity: Option<&InspectedContainerIdentity>,
    expected: ExpectedOwnership<'_>,
    reason: &str,
    mutation_note: &str,
) -> String {
    let id = identity
        .map(|identity| identity.id.as_str())
        .unwrap_or("unknown");
    let status = identity
        .map(|identity| identity.status.as_str())
        .unwrap_or("unknown");
    let project = identity
        .and_then(|identity| identity.project.as_deref())
        .unwrap_or("unknown");
    let service = identity
        .and_then(|identity| identity.service.as_deref())
        .unwrap_or(expected.service);
    let oneoff = identity
        .map(|identity| identity.oneoff.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    let mut lines = vec![
        format!(
            "refused stale nerdctl health-check unit recovery for container `{}`",
            expected.container_name
        ),
        format!(
            "inspected: id={id} status={status} project={project} service={service} oneoff={oneoff} profile={}",
            expected.profile
        ),
        format!("reason: {reason}"),
        mutation_note.to_owned(),
        "next:".to_owned(),
        format!(
            "  colima nerdctl --profile {} -- inspect {}",
            expected.profile, expected.container_name
        ),
    ];
    if parse_full_container_id(id).is_some() {
        let (_, show_timer) = show_unit_invocation(expected.profile, &timer_unit_name(id));
        let (_, show_service) = show_unit_invocation(expected.profile, &service_unit_name(id));
        lines.push(format!("  colima {}", render_os_args(&show_timer)));
        lines.push(format!("  colima {}", render_os_args(&show_service)));
    }
    lines.push(format!(
        "  colima nerdctl --profile {} -- start {}",
        expected.profile, expected.container_name
    ));
    lines.join("\n")
}

fn inspect_status_is_running(status: &str) -> bool {
    let lowered = status.to_ascii_lowercase();
    lowered == "running" || lowered == "up" || lowered.contains("running")
}

fn inspect_status_is_stopped(status: &str) -> bool {
    let lowered = status.to_ascii_lowercase();
    lowered == "exited"
        || lowered == "created"
        || lowered == "dead"
        || lowered == "stopped"
        || lowered.starts_with("exited")
}

fn is_transient_fragment(path: &str) -> bool {
    path.is_empty() || path.starts_with("/run/systemd/transient/")
}

fn is_persistent_fragment(path: &str) -> bool {
    !path.is_empty() && !is_transient_fragment(path)
}

fn is_persistent_unit_file(state: &str) -> bool {
    matches!(
        state.to_ascii_lowercase().as_str(),
        "enabled" | "enabled-runtime" | "static" | "linked" | "linked-runtime" | "generated"
    )
}

fn command_is_idempotent_success(outcome: &CommandOutcome) -> bool {
    if outcome.success {
        return true;
    }
    let text = format!("{}\n{}", outcome.stdout, outcome.stderr).to_ascii_lowercase();
    text.contains("not loaded")
        || text.contains("not-found")
        || text.contains("not found")
        || text.contains("could not be found")
        || text.contains("does not exist")
}

fn probe_detail(probe: &UnitProbe) -> String {
    let stderr = probe.stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_owned();
    }
    let stdout = probe.stdout.trim();
    if !stdout.is_empty() {
        return stdout.to_owned();
    }
    if probe.success {
        "empty output".to_owned()
    } else {
        "command failed".to_owned()
    }
}

fn outcome_detail(outcome: &CommandOutcome) -> String {
    probe_detail(&UnitProbe {
        success: outcome.success,
        stdout: outcome.stdout.clone(),
        stderr: outcome.stderr.clone(),
    })
}

fn render_os_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn label_value(labels: &BTreeMap<String, String>, key: &str) -> Option<String> {
    labels
        .get(key)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

#[derive(serde::Deserialize)]
struct InspectRecord {
    #[serde(rename = "Id", alias = "ID", alias = "id")]
    id: Option<String>,
    #[serde(rename = "Name", alias = "name")]
    name: Option<String>,
    #[serde(rename = "State")]
    state: Option<InspectState>,
    #[serde(rename = "Config")]
    config: Option<InspectConfig>,
    #[serde(rename = "Mounts", default)]
    mounts: Vec<InspectMount>,
}

#[derive(serde::Deserialize)]
struct InspectState {
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

#[derive(serde::Deserialize)]
struct InspectMount {
    #[serde(rename = "Type", alias = "type")]
    mount_type: Option<String>,
    #[serde(rename = "Name", alias = "name")]
    name: Option<String>,
    #[serde(rename = "Source", alias = "source")]
    source: Option<String>,
    #[serde(rename = "Destination", alias = "destination")]
    destination: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OTHER_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn expected() -> ExpectedOwnership<'static> {
        ExpectedOwnership {
            container_name: "acowtancy-mysql-1",
            project_name: "acowtancy-shared-mysql",
            service: "mysql",
            profile: "effigy",
            declared_services: &["mysql"],
        }
    }

    fn inspect_json(
        id: &str,
        name: &str,
        status: &str,
        running: bool,
        project: &str,
        service: &str,
        oneoff: bool,
    ) -> String {
        let running = if running { "true" } else { "false" };
        let oneoff = if oneoff { "True" } else { "False" };
        format!(
            r#"[{{
  "Id": "{id}",
  "Name": "/{name}",
  "State": {{"Status": "{status}", "Running": {running}}},
  "Config": {{
    "Labels": {{
      "com.docker.compose.project": "{project}",
      "com.docker.compose.service": "{service}",
      "com.docker.compose.oneoff": "{oneoff}"
    }}
  }},
  "Mounts": [{{
    "Type": "volume",
    "Name": "acowtancy_mysql_data",
    "Source": "/var/lib/containerd/volumes/acowtancy_mysql_data",
    "Destination": "/var/lib/mysql"
  }}]
}}]"#
        )
    }

    fn owned_inspect() -> String {
        inspect_json(
            FULL_ID,
            "acowtancy-mysql-1",
            "exited",
            false,
            "acowtancy-shared-mysql",
            "mysql",
            false,
        )
    }

    fn inspect_probe(stdout: &str) -> UnitProbe {
        UnitProbe {
            success: true,
            stdout: stdout.to_owned(),
            stderr: String::new(),
        }
    }

    fn transient_show(unit: &str) -> UnitProbe {
        UnitProbe {
            success: true,
            stdout: format!(
                "Id={unit}\nLoadState=loaded\nTransient=yes\nFragmentPath=/run/systemd/transient/{unit}\nUnitFileState=\n"
            ),
            stderr: String::new(),
        }
    }

    fn missing_show(unit: &str) -> UnitProbe {
        UnitProbe {
            success: true,
            stdout: format!(
                "Id={unit}\nLoadState=not-found\nTransient=no\nFragmentPath=\nUnitFileState=\n"
            ),
            stderr: String::new(),
        }
    }

    fn persistent_show(unit: &str) -> UnitProbe {
        UnitProbe {
            success: true,
            stdout: format!(
                "Id={unit}\nLoadState=loaded\nTransient=no\nFragmentPath=/etc/systemd/system/{unit}\nUnitFileState=enabled\n"
            ),
            stderr: String::new(),
        }
    }

    fn recover_with(
        inspect: UnitProbe,
        units: BTreeMap<String, Vec<UnitProbe>>,
    ) -> (StaleTimerAction, Vec<(String, Vec<String>)>) {
        let mut recorded = Vec::new();
        let mut remaining: BTreeMap<String, std::collections::VecDeque<UnitProbe>> = units
            .into_iter()
            .map(|(unit, probes)| (unit, probes.into()))
            .collect();
        let action = recover_owned_stale_healthcheck_units(
            expected(),
            &inspect,
            |unit| {
                remaining
                    .get_mut(unit)
                    .and_then(|probes| probes.pop_front())
                    .ok_or_else(|| ContainerExecError::Failure {
                        command: format!("systemctl show {unit}"),
                        code: None,
                        stdout: String::new(),
                        stderr: format!("unexpected unit probe `{unit}`"),
                    })
            },
            |program, args| {
                recorded.push((
                    program.to_owned(),
                    args.iter()
                        .map(|arg| arg.to_string_lossy().into_owned())
                        .collect(),
                ));
                Ok(CommandOutcome {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            },
            |_| {},
        )
        .expect("recovery helper");
        assert!(
            remaining.values().all(|probes| probes.is_empty()),
            "unconsumed unit probes: {:?}",
            remaining
                .iter()
                .filter(|(_, probes)| !probes.is_empty())
                .map(|(unit, _)| unit.clone())
                .collect::<Vec<_>>()
        );
        (action, recorded)
    }

    /// Initial state is a loaded transient pair; after stop/reset each unit is
    /// unloaded, which is the restart-safe post-state recovery must prove.
    fn loaded_then_unloaded_units() -> BTreeMap<String, Vec<UnitProbe>> {
        BTreeMap::from([
            (
                timer_unit_name(FULL_ID),
                vec![
                    transient_show(&timer_unit_name(FULL_ID)),
                    missing_show(&timer_unit_name(FULL_ID)),
                ],
            ),
            (
                service_unit_name(FULL_ID),
                vec![
                    transient_show(&service_unit_name(FULL_ID)),
                    missing_show(&service_unit_name(FULL_ID)),
                ],
            ),
        ])
    }

    fn already_gone_units() -> BTreeMap<String, Vec<UnitProbe>> {
        BTreeMap::from([
            (
                timer_unit_name(FULL_ID),
                vec![
                    missing_show(&timer_unit_name(FULL_ID)),
                    missing_show(&timer_unit_name(FULL_ID)),
                ],
            ),
            (
                service_unit_name(FULL_ID),
                vec![
                    missing_show(&service_unit_name(FULL_ID)),
                    missing_show(&service_unit_name(FULL_ID)),
                ],
            ),
        ])
    }

    fn assert_refusal(action: &StaleTimerAction, needle: &str) {
        match action {
            StaleTimerAction::Refused { diagnostic } => {
                assert!(
                    diagnostic.contains(needle),
                    "missing `{needle}` in: {diagnostic}"
                );
                assert!(diagnostic.contains("inspected:"));
                assert!(diagnostic.contains("reason:"));
                assert!(diagnostic.contains("did not stop, reset, or delete"));
                assert!(diagnostic
                    .contains("colima nerdctl --profile effigy -- inspect acowtancy-mysql-1"));
            }
            other => panic!("expected refusal containing `{needle}`, got {other:?}"),
        }
    }

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
        let identity = parse_inspected_container_identity(&owned_inspect()).expect("identity");
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

    #[test]
    fn recoverable_transient_pair_stops_only_exact_timer_and_resets_pair() {
        let (action, recorded) = recover_with(
            inspect_probe(&owned_inspect()),
            loaded_then_unloaded_units(),
        );
        let StaleTimerAction::Recovered {
            warning,
            volume_refs,
            stop_timer,
            reset_failed,
        } = action
        else {
            panic!("expected recovery, got {action:?}");
        };
        assert!(warning.contains(FULL_ID));
        assert!(warning.contains("mysql"));
        assert!(warning.contains("without deleting units"));
        assert_eq!(volume_refs[0].name.as_deref(), Some("acowtancy_mysql_data"));
        let stop: Vec<String> = stop_timer
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let reset: Vec<String> = reset_failed
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            stop,
            vec![
                "ssh".to_owned(),
                "--profile".to_owned(),
                "effigy".to_owned(),
                "--".to_owned(),
                "sudo".to_owned(),
                "systemctl".to_owned(),
                "stop".to_owned(),
                timer_unit_name(FULL_ID),
            ]
        );
        assert_eq!(
            reset,
            vec![
                "ssh".to_owned(),
                "--profile".to_owned(),
                "effigy".to_owned(),
                "--".to_owned(),
                "sudo".to_owned(),
                "systemctl".to_owned(),
                "reset-failed".to_owned(),
                service_unit_name(FULL_ID),
                timer_unit_name(FULL_ID),
            ]
        );
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].0, "colima");
        assert_eq!(recorded[1].0, "colima");
        for (_, args) in &recorded {
            let joined = args.join(" ");
            assert!(!joined.contains('*'));
            assert!(!joined.contains("daemon-reload"));
            assert!(!args.iter().any(|arg| arg == "rm" || arg == "kill"));
            assert!(!joined.contains(OTHER_ID));
            assert!(args.contains(&"effigy".to_owned()));
            assert!(args.iter().any(|arg| arg.contains(FULL_ID)));
        }
        assert_eq!(
            volume_refs,
            parse_inspected_container_identity(&owned_inspect())
                .unwrap()
                .volume_refs
        );
    }

    #[test]
    fn already_gone_units_are_idempotent_and_still_issue_exact_stop_reset() {
        let units = already_gone_units();
        let (action, recorded) = recover_with(inspect_probe(&owned_inspect()), units);
        assert!(matches!(action, StaleTimerAction::Recovered { .. }));
        assert_eq!(recorded.len(), 2);
        assert!(recorded[0].1.contains(&timer_unit_name(FULL_ID)));
        assert!(recorded[1].1.contains(&service_unit_name(FULL_ID)));
    }

    #[test]
    fn unload_proof_stop_reset_exit_zero_but_still_loaded_fails_bounded() {
        let timer = timer_unit_name(FULL_ID);
        let mut probes = 0usize;
        let mut pauses = 0usize;
        let mut mutations: Vec<String> = Vec::new();
        let action = recover_owned_stale_healthcheck_units(
            expected(),
            &inspect_probe(&owned_inspect()),
            |unit| {
                probes += 1;
                if unit == timer {
                    Ok(transient_show(unit))
                } else {
                    Ok(missing_show(unit))
                }
            },
            |program, args| {
                mutations.push(format!("{program} {}", render_os_args(args)));
                Ok(CommandOutcome {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            },
            |_| pauses += 1,
        )
        .expect("bounded refusal");
        match action {
            StaleTimerAction::Refused { diagnostic } => {
                assert!(diagnostic.contains("still loaded"), "got: {diagnostic}");
                assert!(
                    diagnostic.contains("bounded re-probes"),
                    "got: {diagnostic}"
                );
                assert!(diagnostic.contains("LoadState=loaded"), "got: {diagnostic}");
                assert!(
                    diagnostic.contains("did not delete any unit file"),
                    "got: {diagnostic}"
                );
                assert!(
                    !diagnostic.contains("did not stop, reset, or delete"),
                    "post-mutation refusal must not deny the stop/reset: {diagnostic}"
                );
            }
            other => panic!("expected bounded refusal, got {other:?}"),
        }
        assert_eq!(mutations.len(), 2);
        assert!(mutations[0].contains("stop"));
        assert!(mutations[1].contains("reset-failed"));
        assert_eq!(probes, 2 + MAX_RESTART_SAFE_PROBES * 2);
        assert_eq!(pauses, MAX_RESTART_SAFE_PROBES - 1);
    }

    #[test]
    fn unload_proof_late_unload_within_bounded_probes_recovers_once() {
        let timer = timer_unit_name(FULL_ID);
        let mut timer_probes = 0usize;
        let mut service_probes = 0usize;
        let mut pauses = 0usize;
        let action = recover_owned_stale_healthcheck_units(
            expected(),
            &inspect_probe(&owned_inspect()),
            |unit| {
                if unit == timer {
                    timer_probes += 1;
                    if timer_probes <= 2 {
                        Ok(transient_show(unit))
                    } else {
                        Ok(missing_show(unit))
                    }
                } else {
                    service_probes += 1;
                    if service_probes == 1 {
                        Ok(transient_show(unit))
                    } else {
                        Ok(missing_show(unit))
                    }
                }
            },
            |_, _| {
                Ok(CommandOutcome {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            },
            |_| pauses += 1,
        )
        .expect("late unload recovery");
        match action {
            StaleTimerAction::Recovered { warning, .. } => {
                assert!(
                    warning.contains("verified both transient units unloaded"),
                    "got: {warning}"
                );
                assert!(warning.contains("without deleting units"), "got: {warning}");
            }
            other => panic!("expected recovery after late unload, got {other:?}"),
        }
        assert_eq!(timer_probes, 3);
        assert_eq!(service_probes, 3);
        assert_eq!(pauses, 1);
    }

    #[test]
    fn unload_proof_persistent_fragment_mid_flight_refuses() {
        let timer = timer_unit_name(FULL_ID);
        let mut timer_probes = 0usize;
        let mut mutations = 0usize;
        let action = recover_owned_stale_healthcheck_units(
            expected(),
            &inspect_probe(&owned_inspect()),
            |unit| {
                if unit == timer {
                    timer_probes += 1;
                    if timer_probes == 1 {
                        Ok(transient_show(unit))
                    } else {
                        Ok(persistent_show(unit))
                    }
                } else {
                    Ok(missing_show(unit))
                }
            },
            |_, _| {
                mutations += 1;
                Ok(CommandOutcome {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            },
            |_| {},
        )
        .expect("mid-flight refusal");
        match action {
            StaleTimerAction::Refused { diagnostic } => {
                assert!(
                    diagnostic.contains("unit state changed during recovery"),
                    "got: {diagnostic}"
                );
                assert!(
                    diagnostic.contains("persistent fragment"),
                    "got: {diagnostic}"
                );
                assert!(
                    diagnostic.contains("did not delete any unit file"),
                    "got: {diagnostic}"
                );
            }
            other => panic!("expected mid-flight persistent refusal, got {other:?}"),
        }
        assert_eq!(mutations, 2);
    }

    #[test]
    fn unload_proof_post_stop_probe_failure_fails_closed() {
        let timer = timer_unit_name(FULL_ID);
        let mut timer_probes = 0usize;
        let mut mutations = 0usize;
        let result = recover_owned_stale_healthcheck_units(
            expected(),
            &inspect_probe(&owned_inspect()),
            |unit| {
                if unit == timer {
                    timer_probes += 1;
                    if timer_probes == 1 {
                        Ok(transient_show(unit))
                    } else {
                        Err(ContainerExecError::Failure {
                            command: format!("systemctl show {unit}"),
                            code: None,
                            stdout: String::new(),
                            stderr: "permission denied".to_owned(),
                        })
                    }
                } else {
                    Ok(missing_show(unit))
                }
            },
            |_, _| {
                mutations += 1;
                Ok(CommandOutcome {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            },
            |_| {},
        );
        assert!(
            result.is_err(),
            "post-stop probe failure must fail closed: {result:?}"
        );
        assert_eq!(mutations, 2);
    }

    #[test]
    fn running_container_is_refused_without_unit_commands() {
        let inspect = inspect_probe(&inspect_json(
            FULL_ID,
            "acowtancy-mysql-1",
            "running",
            true,
            "acowtancy-shared-mysql",
            "mysql",
            false,
        ));
        let (action, recorded) = recover_with(inspect, BTreeMap::new());
        assert_refusal(&action, "running or active");
        assert!(recorded.is_empty());
    }

    #[test]
    fn foreign_project_is_refused_without_unit_commands() {
        let inspect = inspect_probe(&inspect_json(
            FULL_ID,
            "acowtancy-mysql-1",
            "exited",
            false,
            "someone-else",
            "mysql",
            false,
        ));
        let (action, recorded) = recover_with(inspect, BTreeMap::new());
        assert_refusal(&action, "foreign to selected project");
        assert!(recorded.is_empty());
    }

    #[test]
    fn oneoff_is_refused_without_unit_commands() {
        let inspect = inspect_probe(&inspect_json(
            FULL_ID,
            "acowtancy-mysql-1",
            "exited",
            false,
            "acowtancy-shared-mysql",
            "mysql",
            true,
        ));
        let (action, recorded) = recover_with(inspect, BTreeMap::new());
        assert_refusal(&action, "one-off");
        assert!(recorded.is_empty());
    }

    #[test]
    fn undeclared_service_is_refused_without_unit_commands() {
        let inspect = inspect_probe(&inspect_json(
            FULL_ID,
            "acowtancy-mysql-1",
            "exited",
            false,
            "acowtancy-shared-mysql",
            "legacy-redis",
            false,
        ));
        let (action, recorded) = recover_with(inspect, BTreeMap::new());
        assert_refusal(&action, "does not match selected service");
        assert!(recorded.is_empty());
    }

    #[test]
    fn unlabeled_orphan_is_refused_without_unit_commands() {
        let stdout = format!(
            r#"[{{"Id":"{FULL_ID}","Name":"/mystery-1","State":{{"Status":"exited","Running":false}},"Config":{{"Labels":{{}}}},"Mounts":[]}}]"#
        );
        let expected = ExpectedOwnership {
            container_name: "mystery-1",
            ..expected()
        };
        let action = recover_owned_stale_healthcheck_units(
            expected,
            &inspect_probe(&stdout),
            |unit| panic!("must not probe units for unlabeled container, got {unit}"),
            |_, _| panic!("must not mutate units for unlabeled container"),
            |_| {},
        )
        .expect("refusal");
        match action {
            StaleTimerAction::Refused { diagnostic } => {
                assert!(diagnostic.contains("unlabeled"));
            }
            other => panic!("expected unlabeled refusal, got {other:?}"),
        }
    }

    #[test]
    fn unknown_status_is_refused() {
        let inspect = inspect_probe(&inspect_json(
            FULL_ID,
            "acowtancy-mysql-1",
            "restarting",
            false,
            "acowtancy-shared-mysql",
            "mysql",
            false,
        ));
        let (action, recorded) = recover_with(inspect, BTreeMap::new());
        assert_refusal(&action, "unknown");
        assert!(recorded.is_empty());
    }

    #[test]
    fn short_inspect_id_is_refused_and_not_taken_from_error_text() {
        let inspect = inspect_probe(&inspect_json(
            "dd94022f7dd0",
            "acowtancy-mysql-1",
            "exited",
            false,
            "acowtancy-shared-mysql",
            "mysql",
            false,
        ));
        let (action, recorded) = recover_with(inspect, BTreeMap::new());
        assert_refusal(&action, "full 64-character hexadecimal");
        assert!(recorded.is_empty());
        if let StaleTimerAction::Refused { diagnostic } = action {
            assert!(!diagnostic.contains(&timer_unit_name("dd94022f7dd0")));
        }
    }

    #[test]
    fn persistent_fragment_is_refused_without_stop_or_reset() {
        let units = BTreeMap::from([
            (
                timer_unit_name(FULL_ID),
                vec![persistent_show(&timer_unit_name(FULL_ID))],
            ),
            (
                service_unit_name(FULL_ID),
                vec![transient_show(&service_unit_name(FULL_ID))],
            ),
        ]);
        let (action, recorded) = recover_with(inspect_probe(&owned_inspect()), units);
        assert_refusal(&action, "persistent fragment");
        assert!(recorded.is_empty());
        if let StaleTimerAction::Refused { diagnostic } = action {
            assert!(diagnostic.contains("/etc/systemd/system/"));
            assert!(diagnostic.contains(&format!("systemctl show {FULL_ID}.timer")));
        }
    }

    #[test]
    fn mismatched_unit_id_is_ambiguous_and_refused() {
        let units = BTreeMap::from([
            (
                timer_unit_name(FULL_ID),
                vec![UnitProbe {
                    success: true,
                    stdout: format!(
                        "Id={}.timer\nLoadState=loaded\nTransient=yes\nFragmentPath=/run/systemd/transient/{}.timer\nUnitFileState=\n",
                        &FULL_ID[..12],
                        &FULL_ID[..12]
                    ),
                    stderr: String::new(),
                }],
            ),
            (
                service_unit_name(FULL_ID),
                vec![transient_show(&service_unit_name(FULL_ID))],
            ),
        ]);
        let (action, recorded) = recover_with(inspect_probe(&owned_inspect()), units);
        assert_refusal(&action, "does not match exact owned unit");
        assert!(recorded.is_empty());
    }

    #[test]
    fn unavailable_systemctl_authority_is_refused() {
        let units = BTreeMap::from([
            (
                timer_unit_name(FULL_ID),
                vec![UnitProbe {
                    success: false,
                    stdout: String::new(),
                    stderr: "sudo: systemctl: command not found".to_owned(),
                }],
            ),
            (
                service_unit_name(FULL_ID),
                vec![missing_show(&service_unit_name(FULL_ID))],
            ),
        ]);
        let (action, recorded) = recover_with(inspect_probe(&owned_inspect()), units);
        assert_refusal(&action, "systemctl show");
        assert_refusal(&action, "command not found");
        assert!(recorded.is_empty());
    }

    #[test]
    fn inspect_failure_is_refused_without_unit_commands() {
        let inspect = UnitProbe {
            success: false,
            stdout: String::new(),
            stderr: "no such container".to_owned(),
        };
        let (action, recorded) = recover_with(inspect, BTreeMap::new());
        assert_refusal(&action, "container inspect failed");
        assert!(recorded.is_empty());
    }

    #[test]
    fn stop_failure_that_is_not_gone_does_not_claim_recovery() {
        let mut recorded = Vec::new();
        let action = recover_owned_stale_healthcheck_units(
            expected(),
            &inspect_probe(&owned_inspect()),
            |unit| Ok(transient_show(unit)),
            |program, args| {
                recorded.push((
                    program.to_owned(),
                    args.iter()
                        .map(|arg| arg.to_string_lossy().into_owned())
                        .collect::<Vec<_>>(),
                ));
                Ok(CommandOutcome {
                    success: false,
                    stdout: String::new(),
                    stderr: "Access denied".to_owned(),
                })
            },
            |_| {},
        )
        .expect("refusal");
        assert_refusal(&action, "systemctl stop");
        assert_eq!(recorded.len(), 1);
        assert!(recorded[0].1.contains(&"stop".to_owned()));
        assert!(!recorded
            .iter()
            .any(|(_, args)| args.contains(&"reset-failed".to_owned())));
    }

    #[test]
    fn invocations_use_selected_profile_and_full_hex_id_without_shell() {
        let (program, stop) = stop_timer_invocation("effigy", FULL_ID);
        let (_, reset) = reset_failed_invocation("effigy", FULL_ID);
        let (_, show) = show_unit_invocation("effigy", &timer_unit_name(FULL_ID));
        assert_eq!(program, OsString::from("colima"));
        for args in [&stop, &reset, &show] {
            let rendered = render_os_args(args);
            assert!(!rendered.contains('|'));
            assert!(!rendered.contains(';'));
            assert!(!rendered.contains('$'));
            assert!(args.windows(2).any(|pair| {
                pair[0] == OsString::from("--profile") && pair[1] == OsString::from("effigy")
            }));
            assert!(!args.windows(2).any(|pair| {
                pair[0] == OsString::from("--profile") && pair[1] != OsString::from("effigy")
            }));
        }
        let timer = timer_unit_name(FULL_ID);
        let service = service_unit_name(FULL_ID);
        assert_eq!(stop.last(), Some(&OsString::from(timer.as_str())));
        assert_eq!(reset[reset.len() - 2], OsString::from(service.as_str()));
        assert_eq!(reset.last(), Some(&OsString::from(timer.as_str())));
    }
}
