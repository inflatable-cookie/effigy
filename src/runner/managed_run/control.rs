//! `effigy <task> status|logs|stop [PROFILE]` for a managed run: a pure
//! projection of the run journal, its owner gate and its logs (R2), and the
//! kernel stop path. No session file, marker or PID is read as authority.

use std::io::Write;
use std::time::{Duration, Instant};

use effigy_kernel::custody::Stream;
use effigy_kernel::id::RunId;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{Effect, EffectState};
use effigy_kernel::supervise::OUTPUT_SEQUENCE;
use serde::Serialize;

use super::engine::SETTLE_BUDGET;
use super::settlement::{self, EnvironmentEffect};
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{
    self, lookup_checkout, request_stop, task_status, Lookup, RunView, StopStarted, TaskTarget,
    HISTORY_LIMIT,
};

const STATUS_SCHEMA: &str = "effigy.managed.status.v1";
/// The log name that reads the run's own output: setup, environment,
/// readiness and every workload start and exit.
pub(in crate::runner) const OUTPUT_LOG: &str = "lifecycle";
const LABEL_ENVIRONMENT_UP: &str = "environment up";
const READ_MAX: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct ProcessView {
    pub(in crate::runner) name: String,
    pub(in crate::runner) state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) exit: Option<i32>,
    pub(in crate::runner) starts: usize,
    #[serde(skip)]
    pub(in crate::runner) sequence: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct EnvironmentView {
    pub(in crate::runner) name: String,
    /// `borrowed` environments are left running when the run stops.
    pub(in crate::runner) ownership: &'static str,
    pub(in crate::runner) state: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct ManagedStatus {
    pub(in crate::runner) schema: &'static str,
    pub(in crate::runner) task: String,
    pub(in crate::runner) profile: String,
    /// `running`, `starting`, `stopping`, `interrupted`, a closed outcome,
    /// `never-run` or `unknown`.
    pub(in crate::runner) state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) run: Option<RunView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) environment: Option<EnvironmentView>,
    pub(in crate::runner) processes: Vec<ProcessView>,
    pub(in crate::runner) next: Vec<String>,
    /// Effects the run's settle could not prove released: what `cleanup
    /// incomplete` refers to, as `kind: reason`.
    /// Bundle or catalog sources the Prepare path refreshed before this run
    /// started (contract 020).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::runner) notes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::runner) residue: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::runner) warnings: Vec<String>,
}

/// One managed task and profile, addressed as the user typed it.
pub(in crate::runner) struct Addressed<'a> {
    pub(in crate::runner) target: &'a TaskTarget,
    pub(in crate::runner) task: &'a str,
    pub(in crate::runner) profile: &'a str,
}

impl Addressed<'_> {
    fn command(&self, verb: &str) -> String {
        if self.profile == effigy_managed::DEFAULT_MANAGED_PROFILE {
            format!("effigy {} {verb}", self.task).trim_end().to_owned()
        } else {
            format!("effigy {} {verb} {}", self.task, self.profile)
                .replace("  ", " ")
                .to_owned()
        }
    }
}

/// A managed run is long-lived: a stop it was asked for is its normal end,
/// so a cancelled run reads `stopped` everywhere it is shown.
fn shown_state(state: &str) -> &str {
    match state {
        "cancelled" => "stopped",
        other => other,
    }
}

/// The newest run of this task and profile, projected from the journal.
pub(in crate::runner) fn status_view(addressed: &Addressed<'_>) -> ManagedStatus {
    let view = task_status(addressed.target, HISTORY_LIMIT);
    let state = if view.state == "not-recorded" {
        view.latest
            .as_ref()
            .map_or(view.state.as_str(), |run| run.state.as_str())
    } else {
        &view.state
    };
    let state = shown_state(state).to_owned();
    let mut status = ManagedStatus {
        schema: STATUS_SCHEMA,
        task: addressed.task.to_owned(),
        profile: addressed.profile.to_owned(),
        state,
        run: view
            .current
            .clone()
            .or_else(|| view.history.first().cloned()),
        environment: None,
        processes: Vec::new(),
        next: Vec::new(),
        notes: effigy_manifest::bundle_update_notes(),
        residue: Vec::new(),
        warnings: view.warnings.clone(),
    };
    if let Some(run) = &status.run {
        match run.run_id.parse::<RunId>() {
            Ok(id) => match lookup_checkout(&addressed.target.root, id) {
                Lookup::Found(record) => match record.effects() {
                    Observed::Verified(effects, _) => {
                        status.processes = processes(&effects, run.state == "running");
                        status.environment = environment(&effects);
                        status.residue = effects
                            .iter()
                            .filter(|effect| {
                                matches!(effect.state, EffectState::Residue | EffectState::Unknown)
                            })
                            .map(|effect| match &effect.reason {
                                Some(reason) => format!("{}: {reason}", effect.spec.kind),
                                None => effect.spec.kind.clone(),
                            })
                            .collect();
                    }
                    Observed::Absent(_) => {}
                    Observed::Unknown(reason) => status
                        .warnings
                        .push(format!("run effects are unreadable: {reason}")),
                },
                Lookup::NotFound => status
                    .warnings
                    .push("run is absent from its owning checkout".into()),
                Lookup::Unknown(reason) => status
                    .warnings
                    .push(format!("run effects are unreadable: {reason}")),
            },
            Err(_) => status.warnings.push("run id is unreadable".to_owned()),
        }
    }
    status.next = match status.state.as_str() {
        "running" | "stopping" => vec![addressed.command("logs"), addressed.command("stop")],
        "interrupted" => vec![addressed.command("stop")],
        "not-recorded" => vec![format!("effigy {}", addressed.task)],
        _ => vec![format!("effigy {} --headless", addressed.task)],
    };
    status
}

/// Workloads by name, newest start of each, in first-start order.
pub(in crate::runner) fn processes(effects: &[Effect], live: bool) -> Vec<ProcessView> {
    let mut views: Vec<ProcessView> = Vec::new();
    for effect in effects
        .iter()
        .filter(|effect| effect.spec.kind == "kernel:process")
    {
        let Some(label) = effect.spec.payload.get("label").and_then(|v| v.as_str()) else {
            continue;
        };
        if label == LABEL_ENVIRONMENT_UP {
            continue;
        }
        let receipt = effect.receipt.as_ref();
        let exit = receipt
            .and_then(|receipt| receipt.get("exit"))
            .and_then(|exit| exit.as_i64())
            .map(|exit| exit as i32);
        let pid = receipt
            .and_then(|receipt| receipt.pointer("/identity/pid"))
            .and_then(|pid| pid.as_u64())
            .map(|pid| pid as u32);
        let state = match (effect.state, exit) {
            (EffectState::Intended, _) => {
                if live {
                    "starting"
                } else {
                    "unknown"
                }
            }
            (EffectState::Applied, None) if live => "running",
            (EffectState::Applied, None) => "unknown",
            (_, Some(_)) => "exited",
            (EffectState::Released, None) => "stopped",
            (EffectState::Failed, None) => "not-started",
            (EffectState::Residue | EffectState::Unknown, None) => "residue",
            (EffectState::Retained | EffectState::Irreversible, None) => "stopped",
        };
        let view = ProcessView {
            name: label.to_owned(),
            state,
            pid: (state == "running" || state == "starting")
                .then_some(pid)
                .flatten(),
            exit,
            starts: 1,
            sequence: effect.sequence,
        };
        match views.iter_mut().find(|known| known.name == label) {
            Some(known) => {
                let starts = known.starts + 1;
                *known = ProcessView { starts, ..view };
            }
            None => views.push(view),
        }
    }
    views
}

fn environment(effects: &[Effect]) -> Option<EnvironmentView> {
    let effect = effects
        .iter()
        .rev()
        .find(|effect| effect.spec.kind == settlement::ENVIRONMENT)?;
    let payload: EnvironmentEffect = serde_json::from_value(effect.spec.payload.clone()).ok()?;
    Some(EnvironmentView {
        name: payload.environment,
        ownership: if payload.borrowed {
            "borrowed"
        } else {
            "owned"
        },
        state: match effect.state {
            EffectState::Intended => "starting",
            EffectState::Applied => "up",
            EffectState::Released => "down",
            EffectState::Retained => "left running",
            EffectState::Residue | EffectState::Unknown => "residue",
            EffectState::Failed => "not started",
            EffectState::Irreversible => "irreversible",
        },
    })
}

pub(in crate::runner) fn status(
    addressed: &Addressed<'_>,
    json: bool,
) -> Result<String, RunnerError> {
    let status = status_view(addressed);
    if json {
        return serde_json::to_string(&status)
            .map_err(|error| RunnerError::task_invocation(format!("status encoding: {error}")));
    }
    Ok(render(&status))
}

pub(in crate::runner) fn render(status: &ManagedStatus) -> String {
    let mut lines = vec![format!(
        "managed task `{}` (profile `{}`): {}",
        status.task, status.profile, status.state
    )];
    if let Some(run) = &status.run {
        let mut line = format!("  run          {}", run.run_id);
        if let Some(pid) = run.pid {
            line.push_str(&format!("  supervisor pid {pid}"));
        }
        if let Some(cleanup) = run.cleanup {
            line.push_str(&format!("  cleanup {cleanup}"));
        }
        lines.push(line);
    }
    if let Some(environment) = &status.environment {
        lines.push(format!(
            "  environment  `{}`: {} ({})",
            environment.name, environment.state, environment.ownership
        ));
    }
    for process in &status.processes {
        let mut line = format!("  {:<12} {}", process.name, process.state);
        if let Some(pid) = process.pid {
            line.push_str(&format!("  pid {pid}"));
        }
        if let Some(exit) = process.exit {
            line.push_str(&format!("  exit={exit}"));
        }
        if process.starts > 1 {
            line.push_str(&format!("  starts {}", process.starts));
        }
        lines.push(line);
    }
    for residue in &status.residue {
        lines.push(format!("  residue: {residue}"));
    }
    for warning in &status.warnings {
        lines.push(format!("  warning: {warning}"));
    }
    if !status.next.is_empty() {
        lines.push(format!("  next: {}", status.next.join(" | ")));
    }
    lines.join("\n")
}

/// The run's logs: one workload, the run's own output (`lifecycle`), or
/// every workload. `follow` streams until the run closes.
pub(in crate::runner) fn logs(
    addressed: &Addressed<'_>,
    process: Option<&str>,
    follow: bool,
) -> Result<String, RunnerError> {
    let status = status_view(addressed);
    let Some(run) = &status.run else {
        return Err(not_running(addressed, &status));
    };
    let id = run
        .run_id
        .parse::<RunId>()
        .map_err(|_| RunnerError::task_invocation("run id is unreadable"))?;
    let record = match lookup_checkout(&addressed.target.root, id) {
        Lookup::Found(record) => record,
        _ => return Err(not_running(addressed, &status)),
    };
    let mut sources: Vec<(String, i64)> = match process {
        Some(name) if name == OUTPUT_LOG => vec![(name.to_owned(), OUTPUT_SEQUENCE)],
        Some(name) => {
            let found = status
                .processes
                .iter()
                .filter(|view| view.name == name)
                .map(|view| (view.name.clone(), view.sequence))
                .collect::<Vec<_>>();
            if found.is_empty() {
                let known = status
                    .processes
                    .iter()
                    .map(|view| view.name.as_str())
                    .chain(std::iter::once(OUTPUT_LOG))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(RunnerError::task_invocation(format!(
                    "managed task `{}` (profile `{}`) has no process `{name}` in run {}; known: {known}",
                    addressed.task, addressed.profile, run.run_id
                )));
            }
            found
        }
        None => status
            .processes
            .iter()
            .map(|view| (view.name.clone(), view.sequence))
            .collect(),
    };
    if sources.is_empty() {
        sources.push((OUTPUT_LOG.to_owned(), OUTPUT_SEQUENCE));
    }
    let prefix = sources.len() > 1;
    let mut offsets = vec![[0u64; 2]; sources.len()];
    let mut truncated = vec![[false; 2]; sources.len()];
    let mut partials: Vec<[super::client::FollowedLines; 2]> =
        sources.iter().map(|_| Default::default()).collect();
    let mut text = String::new();
    loop {
        let mut moved = false;
        for (index, (name, sequence)) in sources.iter().enumerate() {
            for (slot, stream) in [Stream::Stdout, Stream::Stderr].into_iter().enumerate() {
                let Observed::Verified(read, _) =
                    record.read_log(*sequence, stream, offsets[index][slot], READ_MAX)
                else {
                    continue;
                };
                let gap = read.from != offsets[index][slot];
                let failed = read.truncated && !truncated[index][slot];
                truncated[index][slot] |= read.truncated;
                offsets[index][slot] = read.from + read.bytes.len() as u64;
                if read.bytes.is_empty() && !gap && !failed {
                    continue;
                }
                moved = true;
                let chunk = std::str::from_utf8(&read.bytes).ok().filter(|text|
                    !text.chars().any(|ch| ch.is_control() && ch != '\n'))
                    .ok_or_else(|| RunnerError::task_invocation("managed output contains invalid plain text; inspect the run before reopening"))?;
                let mut rendered = if gap && !prefix {
                    "\n[effigy] output gap: older canonical bytes expired\n".to_owned()
                } else {
                    String::new()
                };
                if failed && !prefix {
                    rendered.push_str(
                        "\n[effigy] capture truncated: inspect the run before restarting\n",
                    );
                }
                rendered.push_str(&if prefix {
                    partials[index][slot].render(name, Some(&read), false)
                } else {
                    chunk.to_owned()
                });
                if follow {
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(rendered.as_bytes());
                    let _ = out.flush();
                } else {
                    text.push_str(&rendered);
                }
            }
        }
        if !follow {
            if prefix {
                for (index, (name, _)) in sources.iter().enumerate() {
                    for partial in &mut partials[index] {
                        text.push_str(&partial.render(name, None, true));
                    }
                }
            }
            return Ok(text);
        }
        let closed = matches!(record.refresh(), Lookup::Found(record) if record.closed() || record.view.state == "interrupted");
        if closed && !moved {
            if prefix {
                let mut out = std::io::stdout().lock();
                for (index, (name, _)) in sources.iter().enumerate() {
                    for partial in &mut partials[index] {
                        out.write_all(partial.render(name, None, true).as_bytes())
                            .map_err(|e| {
                                RunnerError::task_invocation(format!("managed log output: {e}"))
                            })?;
                    }
                }
                out.flush().map_err(|e| {
                    RunnerError::task_invocation(format!("managed log output: {e}"))
                })?;
            }
            return Ok(String::new());
        }
        if !moved {
            std::thread::sleep(task_engine::RUN_POLL);
        }
    }
}

/// The kernel stop path, then a wait for the run's own settle: workloads,
/// remote groups, and the environment the run started. Idempotent: a closed
/// run reports its outcome; an interrupted one is settled here.
pub(in crate::runner) fn stop(addressed: &Addressed<'_>) -> Result<String, RunnerError> {
    let status = status_view(addressed);
    let Some(run) = &status.run else {
        return Ok(format!(
            "managed task `{}` (profile `{}`) is not running",
            addressed.task, addressed.profile
        ));
    };
    let id = run
        .run_id
        .parse::<RunId>()
        .map_err(|_| RunnerError::task_invocation("run id is unreadable"))?;
    let record = match lookup_checkout(&addressed.target.root, id) {
        Lookup::Found(record) => *record,
        Lookup::NotFound => return Err(not_running(addressed, &status)),
        Lookup::Unknown(reason) => {
            return Err(RunnerError::task_invocation(format!(
                "the run journal is unreadable ({reason}); nothing was stopped"
            )))
        }
    };
    let started = Instant::now();
    let how = request_stop(&record)?;
    let deadline = SETTLE_BUDGET + Duration::from_secs(30);
    let settled = loop {
        match record.refresh() {
            Lookup::Found(now) if now.closed() => break *now,
            Lookup::Found(now) if now.view.state == "interrupted" => {
                // The supervisor died with the stop in flight; settle here.
                let _ = request_stop(&now);
            }
            _ => {}
        }
        if started.elapsed() >= deadline {
            let now = status_view(addressed);
            return Err(RunnerError::task_invocation(format!(
                "managed task `{}` (profile `{}`) is still settling after {}s: {}. The stop \
                 continues on its own; watch it with `{}`",
                addressed.task,
                addressed.profile,
                deadline.as_secs(),
                now.state,
                addressed.command("status")
            )));
        }
        std::thread::sleep(task_engine::RUN_POLL);
    };
    let now = status_view(addressed);
    let mut summary = format!(
        "settled managed task `{}` (profile `{}`): {}{}",
        addressed.task,
        addressed.profile,
        shown_state(&settled.view.state),
        match settled.view.cleanup {
            Some("incomplete") => ", cleanup incomplete (residue is listed by status)",
            _ => "",
        }
    );
    if how == StopStarted::AlreadyClosed {
        summary.push_str(" (it had already stopped)");
    }
    if let Some(environment) = &now.environment {
        summary.push_str(&format!(
            "; environment `{}` {}",
            environment.name, environment.state
        ));
    }
    // Residue is not a clean stop: the exit status says so (R5).
    if settled.view.cleanup == Some("incomplete") {
        return Err(RunnerError::task_invocation(format!(
            "{summary}. Run `{}` to retry its settle",
            addressed.command("stop")
        )));
    }
    Ok(summary)
}

/// Stops one managed run by id and waits for its settle: the handover and
/// `container down` path for a run bound to the environment they change.
pub(in crate::runner) fn stop_run(
    checkout: &std::path::Path,
    id: RunId,
) -> Result<String, RunnerError> {
    let record = match lookup_checkout(checkout, id) {
        Lookup::Found(record) => *record,
        Lookup::NotFound => {
            return Err(RunnerError::task_invocation(format!(
                "managed holder {id} is not readable in its owning checkout; nothing was stopped"
            )))
        }
        Lookup::Unknown(reason) => {
            return Err(RunnerError::task_invocation(format!(
                "run {id} is unreadable ({reason}); nothing was stopped"
            )))
        }
    };
    request_stop(&record)?;
    let budget = SETTLE_BUDGET + Duration::from_secs(30);
    let deadline = Instant::now() + budget;
    loop {
        match record.refresh() {
            Lookup::Found(now) if now.closed() => {
                if now.view.cleanup == Some("incomplete") {
                    return Err(RunnerError::task_invocation(format!(
                        "run {id} settled ({}) with cleanup incomplete; its residue is listed by its status",
                        shown_state(&now.view.state)
                    )));
                }
                return Ok(format!(
                    "run {id} settled: {}",
                    shown_state(&now.view.state)
                ));
            }
            Lookup::Found(now) if now.view.state == "interrupted" => {
                let _ = request_stop(&now);
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            return Err(RunnerError::task_invocation(format!(
                "run {id} did not settle within {}s of its stop request",
                budget.as_secs()
            )));
        }
        std::thread::sleep(task_engine::RUN_POLL);
    }
}

fn not_running(addressed: &Addressed<'_>, status: &ManagedStatus) -> RunnerError {
    RunnerError::task_invocation(format!(
        "managed task `{}` (profile `{}`) has no run ({}). Start it with `effigy {}` (TUI) or \
         `effigy {} --headless`",
        addressed.task, addressed.profile, status.state, addressed.task, addressed.task
    ))
}
