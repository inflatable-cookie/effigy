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
use crate::runner::host_process::{
    restart_host_processes_for_container_with_front, HostProcessRestartReport,
};

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
    effigy_core::worktree_scope::prepare_checkout_token(&repo_root).map_err(|error| {
        RunnerError::task_invocation(format!("cannot record the checkout identity: {error}"))
    })?;
    let policy = load_container_policy(&repo_root, name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    validate_container_policy(&repo_root, &policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if let Some(warning) = environment_up_notice(&repo_root, &policy) {
        eprintln!("{warning}");
    }
    // The replace step (stop plus spawn) takes `Gate(Env)` like `up`, so `down`
    // and a concurrent bring-up cannot interleave with a restart that is
    // starting a generation (effigy#367, effigy#370). The readiness wait runs
    // after the gate is released, so a listener that never answers still
    // yields to `down` instead of holding the environment for the whole gate
    // budget (effigy#390).
    use std::io::IsTerminal;
    let marker = crate::runner::startup_ansi_front();
    let ansi_front = super::lifecycle::interactive_front(
        output_json,
        std::env::var_os("CI").is_some(),
        marker,
        std::io::stdout().is_terminal(),
    );
    let report = restart_host_processes_for_container_with_front(
        &repo_root, &policy, processes, ansi_front,
    )?;
    Ok(restart_report(&repo_root, &report, output_json))
}

/// The environment should be up: `up` is the one path that starts an
/// environment's own services, and a host process started without them has
/// nothing to talk to.
///
/// The observation is [`environment::running`], the same running stack check
/// `container status` projects, and it is advisory: an environment that
/// declares only host processes has no container rows to prove, and a stack the
/// inventory cannot read is not a refusal either. Either way the restart names
/// the observation and proceeds (effigy#370).
fn environment_up_notice(repo_root: &Path, policy: &EffectiveContainerPolicy) -> Option<String> {
    environment_up_notice_from(
        environment::running(repo_root, policy).map_err(|error| error.to_string()),
        &policy.name,
    )
}

fn environment_up_notice_from(
    observation: Result<Observed<bool>, String>,
    environment: &str,
) -> Option<String> {
    match observation {
        Ok(Observed::Verified(true, _)) | Ok(Observed::Absent(_)) => None,
        Ok(Observed::Verified(false, _)) => Some(format!(
            "[warn] host processes: environment `{environment}` is not up; restarting the named processes anyway"
        )),
        Ok(Observed::Unknown(reason)) => Some(format!(
            "[warn] host processes: the environment `{environment}` inventory is unreadable ({reason}); restarting the named processes anyway"
        )),
        Err(error) => Some(format!(
            "[warn] host processes: the environment `{environment}` inventory could not be read ({error}); restarting the named processes anyway"
        )),
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

    /// An environment that is not up, and one whose inventory cannot be read,
    /// are named and the restart proceeds: an environment that declares only
    /// host processes has no container rows to prove, so "not up" is never a
    /// refusal (effigy#370).
    #[test]
    fn a_stopped_or_unreadable_environment_warns_and_the_restart_proceeds() {
        for (observation, expected) in [
            (
                Ok(Observed::Verified(false, evidence())),
                "is not up; restarting the named processes anyway",
            ),
            (
                Ok(Observed::Unknown(UnknownReason::Inconsistent(
                    "docker unavailable".into(),
                ))),
                "inventory is unreadable",
            ),
            (Err("docker ps failed".to_owned()), "could not be read"),
        ] {
            let warning = environment_up_notice_from(observation, "web")
                .expect("an unproven environment warns");
            assert!(warning.starts_with("[warn] "), "{warning}");
            assert!(warning.contains(expected), "{warning}");
        }

        assert_eq!(
            environment_up_notice_from(Ok(Observed::Verified(true, evidence())), "web"),
            None
        );
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
