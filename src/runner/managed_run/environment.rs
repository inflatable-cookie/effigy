//! The environment phase of a managed run: intent, `up` under custody, then
//! readiness observed against deadlines from the run (papercut 2abfa556).
//!
//! Nothing waits on a fixed timer. Each wait observes its condition, starts
//! its deadline when its own phase starts, and reports what it saw. Exec
//! workloads start only once the environment is ready, so they carry no
//! readiness loop of their own, and nothing here installs a shell trap.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{OpenReason, StopCause};
use effigy_kernel::custody::{Custody, EnvValue, Lifetime, ProcessSpec, Terminal};
use effigy_kernel::observe::Observed;
use effigy_kernel::probe::ProbeCommand;
use effigy_kernel::run::{EffectSpec, Settlement};

use super::settlement::{self, Binding, EnvironmentEffect};
use crate::runner::task_engine::{self, TaskRun};

/// Poll interval for a custody child and between readiness attempts.
const POLL: Duration = Duration::from_millis(50);
/// Readiness retries back off to at most this.
const BACKOFF_MAX: Duration = Duration::from_secs(5);
/// One route probe's own bound.
const ROUTE_PROBE: Duration = Duration::from_secs(3);

/// The environment one managed run brings up, or borrows.
#[derive(Debug, Clone)]
pub(in crate::runner) struct EnvironmentPlan {
    pub(in crate::runner) name: String,
    pub(in crate::runner) repo: PathBuf,
    pub(in crate::runner) binding: Binding,
    /// The shell command that brings the environment up and returns.
    pub(in crate::runner) up: String,
    pub(in crate::runner) up_env: Vec<(String, String)>,
    /// Already running for its own owner before this run: retained at settle.
    pub(in crate::runner) borrowed: bool,
    /// The service and working directory exec workloads attach to.
    pub(in crate::runner) exec_probe: Option<(String, PathBuf)>,
    /// `health_wait` routes, probed once the environment is up.
    pub(in crate::runner) route_urls: Vec<String>,
    /// Bounds `up` itself: a cold engine pulls and boots inside it.
    pub(in crate::runner) up_budget: Duration,
    /// Bounds each readiness phase from the moment it starts.
    pub(in crate::runner) readiness_budget: Duration,
    /// Lines printed once the environment is ready.
    pub(in crate::runner) ready_lines: Vec<String>,
}

/// Why the environment phase ended without a ready environment.
#[derive(Debug)]
pub(in crate::runner) enum NotReady {
    Stopped(StopCause),
    Failed(String),
}

/// Records the environment effect, brings the environment up and observes
/// its readiness. `stop` is polled at every wait boundary.
pub(in crate::runner) fn bring_up(
    run: &mut TaskRun,
    plan: &EnvironmentPlan,
    token: &str,
    stop: &mut dyn FnMut(&mut TaskRun) -> Option<StopCause>,
) -> Result<(), NotReady> {
    let effect = EnvironmentEffect {
        repo: plan.repo.clone(),
        environment: plan.name.clone(),
        binding: plan.binding.clone(),
        token: token.to_owned(),
        borrowed: plan.borrowed,
    };
    // K3: the intent commits before `up`. A borrowed environment is retained:
    // this run never brings down what it did not start.
    let ticket = {
        let mut kernel = run.run_mut();
        let target = kernel.fence().clone();
        kernel
            .intend(EffectSpec {
                target,
                kind: settlement::ENVIRONMENT.to_owned(),
                payload: serde_json::to_value(&effect)
                    .map_err(|error| NotReady::Failed(error.to_string()))?,
                settlement: if plan.borrowed {
                    Settlement::Retain
                } else {
                    Settlement::Release(serde_json::Value::Null)
                },
                required: false,
            })
            .map_err(|error| NotReady::Failed(format!("environment intent: {error}")))?
    };
    println!(
        "[environment] {} `{}`: bringing it up (deadline {}s)",
        if plan.borrowed { "borrowing" } else { "starting" },
        plan.name,
        plan.up_budget.as_secs()
    );
    let up = run_up(run, plan, stop)?;
    run.run_mut()
        .applied(&ticket, serde_json::json!({ "up": up }))
        .map_err(|error| NotReady::Failed(format!("environment receipt: {error}")))?;
    if let Some((service, working_dir)) = &plan.exec_probe {
        exec_ready(run, plan, service, working_dir, stop)?;
    }
    if !plan.route_urls.is_empty() {
        routes_ready(run, plan, stop)?;
    }
    for line in &plan.ready_lines {
        println!("[environment] {line}");
    }
    Ok(())
}

/// `up` as a custody child of the run: its output is the run's output, and
/// the run's settle stops it if the supervisor dies inside it.
fn run_up(
    run: &mut TaskRun,
    plan: &EnvironmentPlan,
    stop: &mut dyn FnMut(&mut TaskRun) -> Option<StopCause>,
) -> Result<i32, NotReady> {
    let mut env = task_engine::spawn_environment(&plan.repo)
        .map_err(|error| NotReady::Failed(error.to_string()))?;
    for (key, value) in &plan.up_env {
        env.insert(OsString::from(key), EnvValue::Plain(OsString::from(value)));
    }
    let spec = ProcessSpec {
        argv: vec!["/bin/sh".into(), "-c".into(), plan.up.clone().into()],
        cwd: plan.repo.clone(),
        env,
        grace: Duration::from_secs(5),
        lifetime: Lifetime::Finite(plan.up_budget),
        terminal: Terminal::Inherit { capture: false },
        label: "environment up".to_owned(),
    };
    let started = Instant::now();
    let mut process = {
        let cx = task_engine::effect_context(OpenReason::DevServer);
        Custody::spawn(&mut run.run_mut(), spec, &cx)
            .map_err(|error| NotReady::Failed(format!("environment up did not start: {error:?}")))?
    };
    loop {
        if let Some(cause) = stop(run) {
            return Err(NotReady::Stopped(cause));
        }
        if process.expired() {
            return Err(NotReady::Failed(format!(
                "environment `{}` did not come up within {}s; it was still starting. \
                 Raise `health_wait_timeout_secs` for a slow first boot, or inspect it \
                 with `effigy container status`",
                plan.name,
                plan.up_budget.as_secs()
            )));
        }
        if let Observed::Verified(exit, _) = process.poll() {
            let _ = process.complete(&mut run.run_mut(), exit);
            if exit.code != 0 {
                return Err(NotReady::Failed(format!(
                    "environment `{}` did not come up: `up` exited {} after {}s",
                    plan.name,
                    exit.code,
                    started.elapsed().as_secs()
                )));
            }
            println!(
                "[environment] `{}` is up after {}s",
                plan.name,
                started.elapsed().as_secs()
            );
            return Ok(exit.code);
        }
        std::thread::sleep(POLL);
    }
}

/// Exec readiness of the service exec workloads attach to, observed with
/// backoff until its own deadline. Each attempt's result is reported.
fn exec_ready(
    run: &mut TaskRun,
    plan: &EnvironmentPlan,
    service: &str,
    working_dir: &Path,
    stop: &mut dyn FnMut(&mut TaskRun) -> Option<StopCause>,
) -> Result<(), NotReady> {
    let policy = match settlement::policy(&plan.repo, &plan.binding) {
        Ok(policy) => policy,
        Err(reason) => return Err(NotReady::Failed(reason)),
    };
    let deadline = Instant::now() + plan.readiness_budget;
    observe_until(
        run,
        &format!("exec into `{service}`"),
        deadline,
        stop,
        &mut |attempt_deadline| {
            match crate::runner::container_runtime_prep::verify_service_exec_ready(
                &plan.repo,
                &policy,
                service,
                working_dir,
                Some(attempt_deadline),
            ) {
                Ok(_) => Ok(()),
                Err(error) => Err(first_line(&error.to_string())),
            }
        },
    )
}

/// `health_wait` routes: each answers with a status other than a gateway or
/// connection failure (000, 502, 503, 504), probed under a probe guard.
fn routes_ready(
    run: &mut TaskRun,
    plan: &EnvironmentPlan,
    stop: &mut dyn FnMut(&mut TaskRun) -> Option<StopCause>,
) -> Result<(), NotReady> {
    let curl = find_program("curl").ok_or_else(|| {
        NotReady::Failed("`health_wait` needs `curl` on PATH to probe routes".to_owned())
    })?;
    let deadline = Instant::now() + plan.readiness_budget;
    for url in &plan.route_urls {
        observe_until(
            run,
            &format!("route {url}"),
            deadline,
            stop,
            &mut |_| probe_route(&curl, url),
        )?;
    }
    Ok(())
}

fn probe_route(curl: &Path, url: &str) -> Result<(), String> {
    let observe = task_engine::observe_context(ROUTE_PROBE + Duration::from_secs(1));
    let output = observe.probe_process(ProbeCommand {
        argv: vec![
            curl.into(),
            "-k".into(),
            "-s".into(),
            "-o".into(),
            "/dev/null".into(),
            "-w".into(),
            "%{http_code}".into(),
            "--max-time".into(),
            ROUTE_PROBE.as_secs().to_string().into(),
            url.into(),
        ],
        cwd: PathBuf::from("/"),
        env: std::env::vars_os()
            .map(|(key, value)| (key, EnvValue::Plain(value)))
            .collect::<BTreeMap<_, _>>(),
        deadline: ROUTE_PROBE + Duration::from_millis(500),
        max_output: 64,
    });
    match output {
        Observed::Verified(output, _) => {
            let code = String::from_utf8_lossy(&output.output).trim().to_owned();
            match code.as_str() {
                "" | "000" | "502" | "503" | "504" => Err(format!("answered {}", if code.is_empty() { "nothing" } else { &code })),
                _ => Ok(()),
            }
        }
        Observed::Absent(_) => Err("probe produced no observation".to_owned()),
        Observed::Unknown(reason) => Err(format!("unobservable: {reason:?}")),
    }
}

/// Retries `attempt` until it succeeds, `deadline` passes or the run stops,
/// reporting each new observation once.
fn observe_until(
    run: &mut TaskRun,
    what: &str,
    deadline: Instant,
    stop: &mut dyn FnMut(&mut TaskRun) -> Option<StopCause>,
    attempt: &mut dyn FnMut(Instant) -> Result<(), String>,
) -> Result<(), NotReady> {
    let started = Instant::now();
    let mut backoff = Duration::from_millis(250);
    let mut last = String::new();
    loop {
        if let Some(cause) = stop(run) {
            return Err(NotReady::Stopped(cause));
        }
        let attempt_deadline = deadline.min(Instant::now() + Duration::from_secs(15));
        match attempt(attempt_deadline) {
            Ok(()) => {
                println!(
                    "[environment] ready: {what} after {}s",
                    started.elapsed().as_secs()
                );
                return Ok(());
            }
            Err(observed) => {
                if observed != last {
                    println!("[environment] waiting: {what}: {observed}");
                    last = observed;
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(NotReady::Failed(format!(
                "{what} was not ready within {}s of the environment coming up; last observed: {last}",
                deadline.saturating_duration_since(started).as_secs()
            )));
        }
        let wake = Instant::now() + backoff;
        while Instant::now() < wake.min(deadline) {
            if let Some(cause) = stop(run) {
                return Err(NotReady::Stopped(cause));
            }
            std::thread::sleep(POLL);
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_owned()
}

fn find_program(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_line_keeps_only_the_headline() {
        assert_eq!(first_line("not ready\nmore"), "not ready");
        assert_eq!(first_line(""), "");
    }
}
