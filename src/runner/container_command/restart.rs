//! `effigy container <NAME> restart <process>...`: replace only the named
//! environment-owned host processes (and their dependents).
//!
//! This is `up`'s start path narrowed to a set of names: the same replace step,
//! readiness wait and route publication, with every container and every other
//! host process left running. A process stopped with `effigy run stop <run>`
//! is simply started again, so pausing one process no longer costs a full
//! bring-up (Acow #756).

use std::path::Path;

use effigy_containers::{
    load_container_policy, validate_container_policy, EffectiveContainerPolicy,
};
use effigy_kernel::observe::Observed;

use super::{render_container_report, RunnerError};
use crate::runner::command_context::resolve_active_command_context;
use crate::runner::environment;
use crate::runner::host_process::{restart_host_processes_for_container, HostProcessRestartReport};

/// The payload schema of `effigy container <NAME> restart --json`.
const RESTART_SCHEMA: &str = "effigy.container.restart.v1";

pub(super) fn run_container_restart(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    processes: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(repo_override)?;
    let repo_root = context.resolved.resolved_root;
    let policy = load_container_policy(&repo_root, name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    validate_container_policy(&repo_root, &policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if let Some(warning) = environment_up(&repo_root, &policy)? {
        eprintln!("{warning}");
    }
    let report = restart_host_processes_for_container(&repo_root, &policy, processes)?;
    Ok(restart_report(&repo_root, &report, output_json))
}

/// The environment must be up: `up` is the one path that starts an
/// environment's own services, and a host process started without them has
/// nothing to talk to.
///
/// The observation is [`environment::running`], the same running stack check
/// `container status` projects. A stack the inventory cannot prove is not a
/// refusal: the restart names the observation and carries on.
fn environment_up(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<String>, RunnerError> {
    environment_up_from(environment::running(repo_root, policy), &policy.name)
}

fn environment_up_from(
    observation: Result<Observed<bool>, RunnerError>,
    environment: &str,
) -> Result<Option<String>, RunnerError> {
    match observation {
        Ok(Observed::Verified(false, _)) => Err(RunnerError::task_invocation(format!(
            "environment `{environment}` is not up; run `effigy container {environment} up`"
        ))),
        Ok(Observed::Unknown(reason)) => Ok(Some(format!(
            "[warn] host processes: the environment `{environment}` inventory is unreadable ({reason}); restarting the named processes anyway"
        ))),
        // A verified running stack, an absent journal or an unreadable
        // inventory all proceed: none of them proves the stack is down.
        _ => Ok(None),
    }
}

fn restart_report(
    repo_root: &Path,
    report: &HostProcessRestartReport,
    output_json: bool,
) -> String {
    let mut lines = Vec::with_capacity(report.processes.len());
    for process in &report.processes {
        let mut detail = Vec::new();
        if let Some(run) = &process.run {
            detail.push(format!("run {run}"));
        }
        if process.dependent {
            detail.push("dependent".to_owned());
        }
        if let Some(listener) = &process.listener {
            if !listener.address.is_empty() {
                detail.push(format!("listener {}", listener.address));
            }
            if !listener.public_url.is_empty() {
                detail.push(format!("route {}", listener.public_url));
            }
        }
        let mut line = format!("[host process] {} restarted", process.name);
        if !detail.is_empty() {
            line.push_str(&format!(" ({})", detail.join(", ")));
        }
        lines.push(line);
    }
    let json = serde_json::json!({
        "schema": RESTART_SCHEMA,
        "schema_version": 1,
        "ok": true,
        "environment": report.environment,
        "checkout": repo_root.display().to_string(),
        "restarted": report.processes,
    });
    render_container_report(
        effigy_containers::ContainerCommandReport {
            json,
            success_text: lines.join("\n"),
        },
        output_json,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::host_process::RestartedHostProcess;
    use effigy_kernel::observe::{Evidence, UnknownReason};

    fn evidence() -> Evidence {
        Evidence::new("environment inventory".into(), None).expect("fixed evidence")
    }

    /// An environment that is not up is refused with the `up` command that
    /// starts it; an inventory that cannot be read is a warning, never a
    /// refusal.
    #[test]
    fn a_stopped_environment_is_refused_with_its_up_command() {
        let error = environment_up_from(Ok(Observed::Verified(false, evidence())), "web")
            .expect_err("a stopped environment is refused");
        assert_eq!(
            error.to_string(),
            "environment `web` is not up; run `effigy container web up`"
        );

        assert_eq!(
            environment_up_from(Ok(Observed::Verified(true, evidence())), "web")
                .expect("a running environment proceeds"),
            None
        );
        let warning = environment_up_from(
            Ok(Observed::Unknown(UnknownReason::Inconsistent(
                "docker unavailable".into(),
            ))),
            "web",
        )
        .expect("an unreadable inventory proceeds")
        .expect("an unreadable inventory warns");
        assert!(warning.starts_with("[warn] "), "{warning}");
        assert!(warning.contains("docker unavailable"), "{warning}");
    }

    /// One text line per restarted process names its run, its listener and
    /// route, and whether it was restarted as a dependent; `--json` reports the
    /// same set under the container envelope.
    #[test]
    fn the_restart_report_names_each_process_its_route_and_its_dependents() {
        let report = HostProcessRestartReport {
            environment: "web".to_owned(),
            processes: vec![
                RestartedHostProcess {
                    name: "api".to_owned(),
                    run: Some("run-api".to_owned()),
                    dependent: false,
                    listener: None,
                },
                RestartedHostProcess {
                    name: "consumer".to_owned(),
                    run: Some("run-consumer".to_owned()),
                    dependent: true,
                    listener: None,
                },
            ],
        };
        let text = restart_report(Path::new("/checkout"), &report, false);
        assert_eq!(
            text,
            "[host process] api restarted (run run-api)\n\
             [host process] consumer restarted (run run-consumer, dependent)"
        );

        let json: serde_json::Value =
            serde_json::from_str(&restart_report(Path::new("/checkout"), &report, true))
                .expect("the restart payload is JSON");
        assert_eq!(json["schema"], RESTART_SCHEMA);
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["ok"], true);
        assert_eq!(json["environment"], "web");
        assert_eq!(json["checkout"], "/checkout");
        assert_eq!(json["restarted"][0]["name"], "api");
        assert_eq!(json["restarted"][0]["run"], "run-api");
        assert_eq!(json["restarted"][1]["dependent"], true);
    }
}
