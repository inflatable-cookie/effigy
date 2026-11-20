//! The human view of the runner's day-to-day refusals.
//!
//! A `RunnerError`'s `Display` sentence names the failure, but it also glues
//! the manifest path, the catalog list and the backend diagnostic onto that one
//! line. This module lifts the typed facts into an [`effigy_ui::HumanError`]: a
//! short headline, a reason, a `Next:` action, and the machine-oriented facts
//! under `Details:`. `Display` keeps its sentence, so a `--json`
//! `error.message` and every existing `to_string()` test are unchanged.
//!
//! Only the variants a person meets every day are mapped here. Everything else
//! keeps the `Display`-lifted rendering, which is right for a sentence that is
//! already one clean line.

use std::path::Path;

use effigy_core::build_info::StaleLocalInstall;
use effigy_gateway::EndpointStoreRefusal;
use effigy_ui::HumanError;

use super::RunnerError;
use crate::runner::host_process::ManagedHostListenerFailure;

/// The next step after a manifest that only a newer binary understands.
const INSTALL_LOCAL_NEXT: &str =
    "Run `effigy install:local` (or `cargo run --bin effigy -- install:local`) from the checkout root";
/// The next step after an unknown task name: the inventory names the real one.
const LIST_TASKS_NEXT: &str = "Run `effigy tasks` to list available tasks";
/// The next step when the gateway's own state is the question.
const GATEWAY_STATUS_NEXT: &str = "Run `effigy gateway status` to inspect the gateway";
/// A remedy clause starts with one of these, so it can move to `Next:` without
/// taking an explanation with it.
const REMEDY_VERBS: [&str; 14] = [
    "move", "remove", "rebuild", "run", "set", "use", "add", "change", "keep", "delete", "rename",
    "refresh", "give", "pick",
];

/// The mapped human view for a variant this module owns, or `None` when the
/// variant keeps its `Display`-derived rendering.
pub(super) fn mapped(error: &RunnerError) -> Option<HumanError> {
    let human = match error {
        RunnerError::TaskManifestRead { path, error } => manifest_read(path, error),
        RunnerError::TaskManifestParse {
            path,
            error,
            stale_local_install,
        } => manifest_parse(path, error, stale_local_install.as_deref()),
        RunnerError::TaskManifestCompose { path, detail } => manifest_compose(path, detail),
        RunnerError::TaskCatalogsMissing { root } => catalogs_missing(root),
        RunnerError::TaskNotFound { name, path } => task_not_found(name, path),
        RunnerError::TaskNotFoundAny { name, catalogs } => task_not_found_any(name, catalogs),
        RunnerError::TaskAmbiguous { name, candidates } => task_ambiguous(name, candidates),
        RunnerError::TaskCatalogPrefixNotFound { prefix, available } => {
            catalog_prefix_not_found(prefix, available)
        }
        RunnerError::TaskCatalogAliasConflict {
            alias,
            first_path,
            second_path,
        } => catalog_alias_conflict(alias, first_path, second_path),
        RunnerError::TaskCommandLaunch { command, error } => task_command_launch(command, error),
        RunnerError::ContainerSurfaceRegistryMissing => container_registry_missing(),
        RunnerError::ContainerSurfaceDefaultTargetMissing => container_default_target_missing(),
        RunnerError::ContainerSurfaceNotDefined { container } => container_not_defined(container),
        RunnerError::ContainerSurfaceNotRunning { container } => container_not_running(container),
        RunnerError::ContainerSurfacePolicy {
            phase,
            container,
            detail,
        } => container_surface_policy(phase, container, detail),
        RunnerError::ContainerWaitRefused {
            environment,
            diagnostic,
            details,
            ..
        } => container_wait_refused(environment, diagnostic, details),
        RunnerError::ManagedHostReadiness {
            process,
            diagnostic,
            observed_status,
        } => managed_host_readiness(process, diagnostic, *observed_status),
        RunnerError::ManagedListenerExposure {
            environment,
            exposed,
            failures,
        } => managed_listener_exposure(environment, exposed, failures),
        RunnerError::WorkspaceSessionCleanup {
            shell_error,
            cleanup_error,
        } => workspace_session_cleanup(shell_error, cleanup_error),
        RunnerError::ContainerRuntimePolicy { phase, detail } => {
            container_runtime_policy(phase, detail)
        }
        RunnerError::EnvironmentUnavailable { capability, detail } => {
            environment_unavailable(capability, detail)
        }
        RunnerError::GatewayRouteTable {
            phase,
            path,
            detail,
        } => gateway_route_table(phase, path, detail),
        RunnerError::GatewayRouteRegistration {
            phase,
            domain,
            detail,
        } => gateway_route_registration(phase, domain, detail),
        RunnerError::GatewayRouteShape { phase, detail } => gateway_route_shape(phase, detail),
        RunnerError::GatewayLoopback { phase, detail } => gateway_loopback(phase, detail),
        RunnerError::GatewayRuntimeTarget { phase, detail } => {
            gateway_runtime_target(phase, detail)
        }
        RunnerError::GatewayEndpointStore { kind, reason } => gateway_endpoint_store(*kind, reason),
        _ => return None,
    };
    Some(human)
}

// ---------------------------------------------------------------- manifest

fn manifest_read(path: &Path, error: &std::io::Error) -> HumanError {
    let reason = match error.kind() {
        std::io::ErrorKind::NotFound => "The manifest does not exist at that path.".to_owned(),
        std::io::ErrorKind::PermissionDenied => {
            "The manifest exists there but this user cannot read it.".to_owned()
        }
        std::io::ErrorKind::IsADirectory => {
            "That path is a directory, not the manifest file.".to_owned()
        }
        _ => "The manifest could not be opened.".to_owned(),
    };
    HumanError::new("Could not read effigy.toml")
        .explain(reason)
        .detail("manifest", path.display().to_string())
        .detail("cause", error.to_string())
}

fn manifest_parse(
    path: &Path,
    error: &toml::de::Error,
    stale_local_install: Option<&StaleLocalInstall>,
) -> HumanError {
    let summary = parse_summary(error);
    let next_action = parse_next_action(error);
    let mut human = HumanError::new("Invalid effigy.toml")
        .explain(summary.clone())
        .detail("manifest", path.display().to_string())
        .detail(
            "cause",
            if next_action.is_some() {
                summary
            } else {
                error.to_string()
            },
        );
    if let Some(next_action) = next_action {
        human = human.next_action(next_action);
    }
    if let Some(stale) = stale_local_install {
        human = human
            .explain(format!(
                "The `effigy` installed at {} is behind this checkout, so it does not understand this manifest.",
                stale.executable.display()
            ))
            .next_action(INSTALL_LOCAL_NEXT)
            .detail("installed", stale.installed_identity.clone())
            .detail("current", stale.current_identity.clone());
    }
    human
}

/// The parse error's first line, with the message `toml` prints last when it
/// adds one: `TOML parse error at line 1, column 11: missing value`.
fn parse_summary(error: &toml::de::Error) -> String {
    let rendered = error.to_string();
    let lines: Vec<&str> = rendered
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let Some(first) = lines.first() else {
        return String::new();
    };
    let next_index = lines
        .iter()
        .position(|line| line.starts_with("Next:"))
        .unwrap_or(lines.len());
    let message = lines
        .iter()
        .take(next_index)
        .rev()
        .copied()
        .filter(|line| !is_parse_snippet(line))
        .find(|line| line != first);
    match message {
        Some(message) => format!("{first}: {message}"),
        None => (*first).to_owned(),
    }
}

fn parse_next_action(error: &toml::de::Error) -> Option<String> {
    let rendered = error.to_string();
    let lines: Vec<&str> = rendered
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let index = lines.iter().position(|line| line.starts_with("Next:"))?;
    let inline = lines[index].trim_start_matches("Next:").trim();
    Some(if inline.is_empty() {
        lines
            .iter()
            .skip(index + 1)
            .copied()
            .find(|line| !line.starts_with("Details:"))
            .unwrap_or_default()
            .to_owned()
    } else {
        inline.to_owned()
    })
}

/// A TOML error's snippet furniture (`|`, `1 | key = value`, `|     ^`), which
/// says nothing a person needs once the line and column are named.
fn is_parse_snippet(line: &str) -> bool {
    if line.starts_with('|') {
        return true;
    }
    match line.split_once(' ') {
        Some((number, rest)) => {
            !number.is_empty()
                && number.chars().all(|character| character.is_ascii_digit())
                && rest.starts_with('|')
        }
        None => false,
    }
}

fn manifest_compose(path: &Path, detail: &str) -> HumanError {
    let detail = strip_compose_prefix(detail);
    if is_structured_compose_detail(detail) {
        return structured_manifest_compose(path, detail);
    }
    let (statement, remedy) = split_remedy(detail);
    let mut human = HumanError::new("Invalid effigy.toml")
        .explain(statement)
        .detail("manifest", path.display().to_string());
    if let Some(remedy) = remedy {
        human = human.next_action(remedy);
    }
    human
}

fn is_structured_compose_detail(detail: &str) -> bool {
    detail.contains("\n  Next:")
        || detail.contains("\nNext:")
        || detail.contains("\n  Details:")
        || detail.contains("\nDetails:")
        || detail.contains("\n\n")
}

fn structured_manifest_compose(path: &Path, detail: &str) -> HumanError {
    enum Section {
        Explanation,
        Next,
        Details,
    }

    let mut lines = detail.lines();
    let headline = lines
        .by_ref()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Invalid effigy.toml");

    let mut explanations = Vec::new();
    let mut next_action = String::new();
    let mut details: Vec<(String, String)> = Vec::new();
    let mut section = Section::Explanation;
    let mut detail_key_indent: Option<usize> = None;

    for raw_line in lines {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(action) = trimmed
            .strip_prefix("Next:")
            .or_else(|| trimmed.strip_prefix("next:"))
        {
            section = Section::Next;
            let action = action.trim();
            if !action.is_empty() {
                if !next_action.is_empty() {
                    next_action.push('\n');
                }
                next_action.push_str(action);
            }
            continue;
        }
        if trimmed.eq_ignore_ascii_case("Details:") {
            section = Section::Details;
            continue;
        }
        match section {
            Section::Explanation => explanations.push(trimmed.to_owned()),
            Section::Next => {
                if !next_action.is_empty() {
                    next_action.push('\n');
                }
                next_action.push_str(trimmed);
            }
            Section::Details => {
                let indent = raw_line.len() - raw_line.trim_start().len();
                let is_top_level_row = detail_key_indent.is_none_or(|base| indent <= base);
                if is_top_level_row {
                    if let Some((label, value)) = split_detail_row(trimmed) {
                        detail_key_indent.get_or_insert(indent);
                        details.push((label.to_owned(), value.to_owned()));
                        continue;
                    }
                }
                if let Some((_, value)) = details.last_mut() {
                    if !value.is_empty() {
                        value.push('\n');
                    }
                    value.push_str(trimmed);
                } else {
                    explanations.push(trimmed.to_owned());
                }
            }
        }
    }

    let mut human = HumanError::new(headline);
    for line in explanations {
        human = human.explain(line);
    }
    if !next_action.trim().is_empty() {
        human = human.next_action(next_action);
    }
    let has_manifest = details.iter().any(|(label, _)| label == "manifest");
    for (label, value) in details {
        human = human.detail(label, value);
    }
    if !has_manifest {
        human = human.detail("manifest", path.display().to_string());
    }
    human
}

fn split_detail_row(trimmed: &str) -> Option<(&str, &str)> {
    let (label, value) = trimmed.split_once(':')?;
    let label = label.trim();
    if label.is_empty() || label.chars().any(char::is_whitespace) {
        return None;
    }
    Some((label, value.trim()))
}

/// A composition refusal's detail without the internal
/// `manifest compose failed in <path>: ` prefix `Display` adds.
fn strip_compose_prefix(detail: &str) -> &str {
    let detail = detail.trim();
    match detail.strip_prefix("manifest compose failed in ") {
        Some(rest) => rest
            .split_once(": ")
            .map_or(detail, |(_, detail)| detail.trim()),
        None => detail,
    }
}

/// Split a detail's own remedy out of its explanation, when it carries one.
///
/// A detail sometimes states the problem and the fix in one sentence
/// (`...; move the command to ...`). Only a trailing clause that starts with a
/// remedy verb becomes `Next:`; `no '/' or ':'` stays part of the statement, so
/// the split never invents advice.
fn split_remedy(detail: &str) -> (&str, Option<&str>) {
    if let Some((statement, next)) = detail.split_once("\nnext:") {
        let next = next.trim();
        if !next.is_empty() {
            return (statement.trim_end(), Some(next));
        }
    }
    let Some((statement, remedy)) = detail.rsplit_once("; ") else {
        return (detail, None);
    };
    let first_word = remedy.split_whitespace().next().unwrap_or_default();
    let first_word = first_word
        .trim_matches(|character: char| !character.is_alphanumeric())
        .to_ascii_lowercase();
    if REMEDY_VERBS.contains(&first_word.as_str()) {
        (statement.trim_end(), Some(remedy.trim()))
    } else {
        (detail, None)
    }
}

// ------------------------------------------------------------------ routing

fn catalogs_missing(root: &Path) -> HumanError {
    HumanError::new("No effigy.toml found in this workspace")
        .next_action("Run `effigy init` or change to a directory with an `effigy.toml`")
        .detail("directory", root.display().to_string())
}

fn task_not_found(name: &str, path: &Path) -> HumanError {
    HumanError::new(format!("Task `{name}` is not defined"))
        .next_action(LIST_TASKS_NEXT)
        .detail("manifest", path.display().to_string())
}

fn task_not_found_any(name: &str, catalogs: &[String]) -> HumanError {
    HumanError::new(format!("Task `{name}` is not defined"))
        .next_action(LIST_TASKS_NEXT)
        .detail("catalogs", catalogs.join(", "))
}

fn task_ambiguous(name: &str, candidates: &[String]) -> HumanError {
    HumanError::new(format!("Task `{name}` matches multiple catalogs"))
        .next_action(format!(
            "Prefix the task with its catalog name (for example `<catalog>:{name}`)"
        ))
        .detail("matches", candidates.join(", "))
}

fn catalog_prefix_not_found(prefix: &str, available: &[String]) -> HumanError {
    HumanError::new(format!("Unknown task catalog `{prefix}`"))
        .next_action("Run `effigy tasks` to see available catalogs")
        .detail("available", available.join(", "))
}

fn catalog_alias_conflict(alias: &str, first_path: &Path, second_path: &Path) -> HumanError {
    HumanError::new(format!("Duplicate catalog alias `{alias}`"))
        .next_action("Give each catalog a unique `[catalog].alias` in its `effigy.toml`")
        .detail("first", first_path.display().to_string())
        .detail("second", second_path.display().to_string())
}

fn task_command_launch(command: &str, error: &std::io::Error) -> HumanError {
    HumanError::new(format!("Could not start `{command}`"))
        .detail("command", command)
        .detail("cause", error.to_string())
}

// --------------------------------------------------------------- container

fn container_registry_missing() -> HumanError {
    HumanError::new("No containers are configured in effigy.toml")
        .next_action("Add a `[containers.<name>]` section or use a bundle that defines one")
}

fn container_default_target_missing() -> HumanError {
    HumanError::new("No default container workspace is configured")
        .next_action("Set `[systems].default` to a system whose workspace has a backing container")
}

fn container_not_defined(container: &str) -> HumanError {
    HumanError::new(format!("Container `{container}` is not defined"))
        .next_action("Check `[containers]` in `effigy.toml` or run `effigy container status`")
}

fn container_not_running(container: &str) -> HumanError {
    HumanError::new(format!("Container `{container}` is not running"))
        .next_action(format!("Start it with `effigy container {container} up`"))
}

fn container_surface_policy(phase: &str, container: &str, detail: &str) -> HumanError {
    let mut human = HumanError::new(format!("Container `{container}` failed during {phase}"))
        .detail("container", container)
        .detail("phase", phase)
        .detail("cause", detail);
    // A multi-line detail already has a summary line; a one-line detail is the
    // cause itself and needs no second copy above the block.
    if let Some(summary) = detail_summary(detail) {
        human = human.explain(summary);
    }
    human
}

fn container_wait_refused(
    environment: &str,
    diagnostic: &str,
    details: &serde_json::Value,
) -> HumanError {
    let mut human = HumanError::new(format!(
        "Container `{environment}` did not become ready in time"
    ))
    .next_action(format!(
        "Run `effigy container {environment} logs` to inspect startup output"
    ));
    let first_sentence = concise_diagnostic_sentence(diagnostic);
    if !first_sentence.is_empty() {
        human = human.explain(&first_sentence);
    }
    if let Some(waiting_summary) = waiting_on_summary(details) {
        if !first_sentence.contains(&waiting_summary) {
            human = human.explain(format!("Waiting on: {waiting_summary}"));
        }
    }
    human = human.detail("container", environment);
    if let Some(failure_kind) = details
        .get("failure_kind")
        .and_then(serde_json::Value::as_str)
    {
        human = human.detail("failure_kind", failure_kind);
    }
    human.detail("cause", diagnostic)
}

fn concise_diagnostic_sentence(diagnostic: &str) -> String {
    let Some(first_line) = diagnostic
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
    else {
        return String::new();
    };
    let without_tail = first_line
        .split_once("; last container output:")
        .or_else(|| first_line.split_once("; last runtime probe:"))
        .map_or(first_line, |(head, _)| head)
        .trim();
    without_tail.trim_end_matches(':').trim().to_owned()
}

fn waiting_on_summary(details: &serde_json::Value) -> Option<String> {
    let entries = details
        .get("waiting_on")
        .and_then(serde_json::Value::as_array)?;
    let summaries: Vec<String> = entries
        .iter()
        .filter_map(|entry| {
            let kind = entry.get("kind").and_then(serde_json::Value::as_str);
            match kind {
                Some("service") => entry
                    .get("problem")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| {
                        let service = entry.get("service").and_then(serde_json::Value::as_str)?;
                        let state = entry
                            .get("state")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("unready");
                        Some(format!("{service} ({state})"))
                    }),
                Some("health_check") => {
                    let subject = entry
                        .get("container")
                        .and_then(serde_json::Value::as_str)
                        .or_else(|| entry.get("service").and_then(serde_json::Value::as_str))
                        .unwrap_or("container");
                    let answer = entry
                        .get("answer")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unready");
                    match entry.get("status").and_then(serde_json::Value::as_u64) {
                        Some(status) => {
                            Some(format!("{subject} health check {answer} (HTTP {status})"))
                        }
                        None => Some(format!("{subject} health check {answer}")),
                    }
                }
                Some("shared_service") => {
                    let service = entry
                        .get("service")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("shared");
                    let step = entry
                        .get("step")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("readiness");
                    Some(format!("shared service `{service}` ({step})"))
                }
                Some("probe") => {
                    let probe = entry
                        .get("probe")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("runtime");
                    let error = entry
                        .get("error")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unanswered");
                    Some(format!("probe `{probe}`: {error}"))
                }
                _ => entry
                    .get("problem")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| entry.get("service").and_then(serde_json::Value::as_str))
                    .map(str::to_owned),
            }
        })
        .collect();
    (!summaries.is_empty()).then(|| summaries.join("; "))
}

fn managed_host_readiness(
    process: &str,
    diagnostic: &str,
    observed_status: Option<u16>,
) -> HumanError {
    let explanation = match observed_status {
        Some(status) => {
            format!("The readiness check received HTTP {status} before the process stopped.")
        }
        None => "The process failed before its readiness check passed.".to_owned(),
    };
    let mut human = HumanError::new(format!("Host process `{process}` did not become ready"))
        .explain(explanation)
        .next_action(format!(
            "Check `{process}` output or run it directly to see why it failed"
        ))
        .detail("process", process);
    if let Some(status) = observed_status {
        human = human.detail("http_status", status.to_string());
    }
    human.detail("cause", diagnostic)
}

fn managed_listener_exposure(
    environment: &str,
    exposed: &[String],
    failures: &[ManagedHostListenerFailure],
) -> HumanError {
    let mut human = HumanError::new(format!(
        "Some routes for `{environment}` could not be published"
    ))
    .explain(format!(
        "{} route(s) were withheld while {} published.",
        failures.len(),
        exposed.len()
    ))
    .next_action("Fix the failing host listener(s) listed below and re-run `effigy dev`")
    .detail("environment", environment);
    if !exposed.is_empty() {
        human = human.detail("published", exposed.join(", "));
    }
    for failure in failures {
        human = human.detail("withheld", failure.summary());
        if let Some(tail) = failure
            .output_tail
            .as_deref()
            .map(str::trim)
            .filter(|tail| !tail.is_empty())
        {
            human = human.detail(
                "output",
                effigy_ui::human::bound_text(tail, effigy_ui::human::MAX_EVIDENCE_BYTES),
            );
        }
    }
    human
}

fn workspace_session_cleanup(shell_error: &str, cleanup_error: &str) -> HumanError {
    HumanError::new("Workspace session ended with a cleanup error")
        .explain(shell_error)
        .next_action(
            "Run `effigy container status` to check whether the workspace container is still running",
        )
        .detail("cleanup", cleanup_error)
}

fn container_runtime_policy(phase: &str, detail: &str) -> HumanError {
    let mut human = HumanError::new(format!("Container runtime failed during {phase}"))
        .detail("phase", phase)
        .detail("cause", detail);
    if let Some(summary) = detail_summary(detail) {
        human = human.explain(summary);
    }
    human
}

/// The first line of a multi-line detail: the summary a person reads above the
/// block that repeats the whole diagnostic.
fn detail_summary(detail: &str) -> Option<&str> {
    let mut lines = detail
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let first = lines.next()?;
    lines.next().map(|_| first)
}

fn environment_unavailable(capability: &str, detail: &str) -> HumanError {
    HumanError::new(format!(
        "Required environment capability `{capability}` is unavailable"
    ))
    .explain(detail)
    .detail("capability", capability)
}

// ----------------------------------------------------------------- gateway

fn gateway_route_table(phase: &str, path: &Path, detail: &str) -> HumanError {
    gateway(phase, format!("Gateway route table {phase} failed"), detail)
        .detail("path", path.display().to_string())
}

fn gateway_route_registration(phase: &str, domain: &str, detail: &str) -> HumanError {
    gateway(
        phase,
        format!("Gateway route registration {phase} failed for `{domain}`"),
        detail,
    )
    .detail("domain", domain)
}

fn gateway_route_shape(phase: &str, detail: &str) -> HumanError {
    gateway(phase, "A gateway route is not valid".to_owned(), detail)
}

fn gateway_loopback(phase: &str, detail: &str) -> HumanError {
    gateway(phase, format!("Gateway loopback {phase} failed"), detail)
}

fn gateway_runtime_target(phase: &str, detail: &str) -> HumanError {
    gateway(
        phase,
        format!("Gateway runtime target {phase} failed"),
        detail,
    )
}

fn gateway_endpoint_store(kind: EndpointStoreRefusal, reason: &str) -> HumanError {
    let reason_sentence = match kind {
        EndpointStoreRefusal::ObservationUnknown => {
            "The store's state could not be observed, so nothing was read or changed."
        }
        EndpointStoreRefusal::Refused => "The store refused the change.",
    };
    HumanError::new("The gateway endpoint store refused")
        .explain(reason_sentence)
        .next_action(GATEWAY_STATUS_NEXT)
        .detail("store", format!("{kind}"))
        .detail("cause", reason)
}

/// The shared gateway shape: the headline names the phase, the next step brings
/// the gateway back into view, and the phase and cause go under `Details:`.
fn gateway(phase: &str, headline: String, detail: &str) -> HumanError {
    let mut human = HumanError::new(headline)
        .next_action(GATEWAY_STATUS_NEXT)
        .detail("phase", phase)
        .detail("cause", detail);
    if let Some(summary) = detail_summary(detail) {
        human = human.explain(summary);
    }
    human
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compose_remedy_moves_to_the_next_action_without_taking_the_statement() {
        let detail = "`tasks.test` was removed in v0.11 because `effigy test` is always the \
                      built-in orchestrator; move the command to a named `[test.suites]` entry";
        assert_eq!(
            split_remedy(detail),
            (
                "`tasks.test` was removed in v0.11 because `effigy test` is always the built-in orchestrator",
                Some("move the command to a named `[test.suites]` entry")
            )
        );
        // A clause that explains instead of advising stays in the statement.
        let detail = "`containers.web.workspace_source must be `checkout` or `staged`; got `link`";
        assert_eq!(split_remedy(detail), (detail, None));
    }

    #[test]
    fn a_compose_prefix_never_reaches_the_reader() {
        assert_eq!(
            strip_compose_prefix("manifest compose failed in /repo/effigy.toml: duplicate key"),
            "duplicate key"
        );
        assert_eq!(strip_compose_prefix("duplicate key"), "duplicate key");
    }

    #[test]
    fn a_parse_summary_names_the_line_and_the_message() {
        let error: toml::de::Error = toml::from_str::<toml::Value>("unknown =").unwrap_err();
        let summary = parse_summary(&error);
        let (position, message) = summary
            .split_once(": ")
            .expect("the message follows the position");
        assert!(
            position.starts_with("TOML parse error at line 1, column"),
            "{summary}"
        );
        assert!(!message.is_empty(), "{summary}");
        assert!(
            !message.contains(" | "),
            "snippet furniture stays out: {summary}"
        );
    }

    #[test]
    fn a_retired_manifest_parse_lifts_next_action_out_of_the_explanation() {
        let error = <toml::de::Error as serde::de::Error>::custom(
            "`[bootstrap].run` is no longer supported\n\n  Next: Replace it with `[bootstrap].setup`",
        );
        let error = RunnerError::TaskManifestParse {
            path: Path::new("/repo/effigy.toml").to_path_buf(),
            error,
            stale_local_install: None,
        };

        let human = mapped(&error).expect("manifest parse errors have a human view");
        assert_eq!(human.what_failed(), "Invalid effigy.toml");
        assert_eq!(
            human.explanation_lines(),
            ["`[bootstrap].run` is no longer supported"]
        );
        assert_eq!(
            human.next_action_text(),
            Some("Replace it with `[bootstrap].setup`")
        );
        assert!(
            !human.explanation_lines()[0].contains(": Next:"),
            "the remedy belongs in Next: {human:?}"
        );
        assert!(
            human
                .detail_entries()
                .iter()
                .any(|(label, value)| { label == "manifest" && value == "/repo/effigy.toml" }),
            "manifest path stays in Details: {human:?}"
        );
    }
}
