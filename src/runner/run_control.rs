//! `effigy runs` and `effigy run status|logs|stop <run>`: generic run
//! control for every run (Q-006), on the kernel run journal.
//!
//! A follower binds to one exact run id and reads each stream's log by byte
//! offset. Losing the follower loses nothing: a new follower resumes at the
//! offsets the first one delivered, and the run itself is never touched or
//! resubmitted. When the run is terminal and its log is gone, the follower
//! still reports the settled outcome from the journal. What cannot be
//! verified — the journal, the run's workload evidence, or one stream's log
//! read — is retried within a bounded budget and then reported as `unknown`
//! with the reason it saw, never as a drained success (R3).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine as _;
use effigy_cli::{RunsAction, RunsArgs};
use effigy_kernel::custody::Stream;
use effigy_kernel::id::RunId;
use effigy_kernel::observe::Observed;

use super::command_context::resolve_active_command_context;
use super::error::RunnerError;
use super::task_engine::{
    checkout_effects_root, effect_context, list_runs, lookup_checkout, request_stop, resolve_in,
    signal_owner, Lookup, RunRecord, StopStarted, RUN_POLL,
};
use effigy_kernel::ctx::OpenReason;

const RUN_SCHEMA: &str = "effigy.run.v1";
const RUNS_SCHEMA: &str = "effigy.runs.v1";
const LOGS_SCHEMA: &str = "effigy.run.logs.v1";
const PREPARE_SCHEMA: &str = "effigy.run.prepare.v1";
/// How long `run prepare` waits for a busy store.
const PREPARE_WAIT: Duration = Duration::from_secs(30);
/// One log read; a follower loops until it reaches the log's end.
const READ_MAX: usize = 256 * 1024;
/// How long a follower keeps retrying an observation it cannot verify — the
/// journal, the run's workload evidence, or one stream's log read — before
/// it reports the outcome unknown with the reason it saw.
const UNKNOWN_BUDGET: Duration = Duration::from_secs(30);
/// How long `run stop` waits for the owner to act on its request before it
/// signals an owner that does not read requests.
const PICKUP: Duration = Duration::from_secs(2);
const STOP_WAIT_DEFAULT: Duration = Duration::from_secs(30);

pub(in crate::runner) fn run_runs(args: RunsArgs) -> Result<String, RunnerError> {
    let json = args.output_json;
    let repo_override = args.repo_override.clone();
    match args.action {
        RunsAction::List { limit, all } => list(repo_override, limit, all, json),
        // A run lookup reads the journal of the checkout this query belongs
        // to and nothing else, resolved once here (effigy#339, Q-011).
        RunsAction::Status { run } => status(&checkout(repo_override)?, &run, json),
        RunsAction::Logs {
            run,
            follow,
            stdout_offset,
            stderr_offset,
        } => logs(
            &checkout(repo_override)?,
            &run,
            follow,
            Offsets {
                stdout: stdout_offset,
                stderr: stderr_offset,
            },
            json,
        ),
        RunsAction::Stop { run, wait_ms } => stop(
            &checkout(repo_override)?,
            &run,
            wait_ms.map_or(STOP_WAIT_DEFAULT, Duration::from_millis),
            json,
        ),
        RunsAction::Prepare { check, task } => {
            prepare(&checkout(repo_override)?, check, task.as_deref(), json)
        }
    }
}

/// `effigy run prepare`: prepares this checkout's run journal, fixed
/// strict-mode terminal ring and, when selected, a foreground task's gates.
/// `--check` verifies strict preconditions without migrating the journal or
/// writing the ring and task gates (effigy#981, effigy#985, effigy#991).
///
/// A strict run (`EFFIGY_STRICT_PERSISTENCE=1`) opens no SQLite connection at
/// all: it overwrites one slot of the preallocated ring. Everything a store
/// would have to prepare — schema creation, the additive `ALTER`/backfill, a
/// journal-mode conversion, a bounded prune and recovery — belongs to this step
/// instead, together with creating and fully preallocating the ring. Opening the
/// store without strict persistence is the whole point of the command; the open
/// also ingests any strict records the ring still holds and frees their slots.
fn prepare(
    checkout: &Path,
    check: bool,
    task_selector: Option<&str>,
    json: bool,
) -> Result<String, RunnerError> {
    let root = checkout_effects_root(checkout)?;
    let task_gates = task_selector
        .map(|selector| resolve_prepare_gates(checkout, root, selector))
        .transpose()?;
    let cx = effect_context(OpenReason::Shell);
    let (prepared, schema_version, pages) = if check {
        let observed = effigy_kernel::store::verify_prepared(root, PREPARE_WAIT)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        (false, observed.schema_version, observed.pages)
    } else {
        let store = effigy_kernel::store::Store::open_preparing(&cx, root)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let pages = store
            .page_count()
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        (true, effigy_kernel::store::SCHEMA_VERSION, pages)
    };
    // The fixed ring (effigy#985). `prepare` creates and fully preallocates a
    // missing one; `--check` only verifies it. A full ring is not ready: the
    // next non-strict open (this command, without `--check`) ingests it.
    let ring = if check {
        effigy_kernel::run::terminal_ring_status(root)
    } else {
        effigy_kernel::run::terminal_ring_prepare(root)
    }
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if let Some((owner, ids, _)) = &task_gates {
        if check {
            effigy_kernel::run::verify_prepared_foreground_gates(root, owner, ids)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        } else {
            effigy_kernel::run::prepare_foreground_gates(root, owner, ids)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        }
    }
    if check && !ring.ready() {
        return Err(RunnerError::task_invocation(
            "the strict terminal ring has no free slot, so a strict run would refuse; \
             `effigy run prepare` ingests its records into the checkout journal and frees them"
                .to_owned(),
        ));
    }
    let journal = root.path().join(effigy_kernel::store::DB_NAME);
    let ring_file = root.path().join(effigy_kernel::run::TERMINAL_RING_FILE);
    let allocation_unit_bytes = root
        .allocation_unit_bytes()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if json {
        return Ok(serde_json::json!({
            "schema": PREPARE_SCHEMA,
            "schema_version": 1,
            "journal": journal.display().to_string(),
            "store": { "schema_version": schema_version, "pages": pages },
            "ring": {
                "file": ring_file.display().to_string(),
                "slots": ring.slots,
                "committed": ring.committed,
                "free": ring.free,
                "bytes": ring.bytes,
                "per_write_bytes": ring.per_write_bytes,
            },
            "task_gates": task_gates.as_ref().map(|(_, ids, task)| serde_json::json!({
                "task": task,
                "resources": ids.len(),
            })),
            "allocation_unit_bytes": allocation_unit_bytes,
            "prepared": prepared,
            "ready": true,
        })
        .to_string());
    }
    Ok([
        format!("journal  {}", journal.display()),
        format!("schema   {schema_version}"),
        format!("pages    {pages}"),
        format!(
            "ring     {} ({} slots x {} bytes, {} committed, {} free)",
            ring_file.display(),
            ring.slots,
            ring.per_write_bytes,
            ring.committed,
            ring.free
        ),
        format!("allocation unit  {allocation_unit_bytes} bytes (statfs)"),
        task_gates.as_ref().map_or_else(
            || "task gates       not requested".to_owned(),
            |(_, ids, task)| {
                format!(
                    "task gates       {task} ({} gate files; scope markers prepared)",
                    ids.len()
                )
            },
        ),
        format!(
            "state    {}",
            if prepared {
                "prepared"
            } else {
                "ready (nothing was written)"
            }
        ),
    ]
    .join("\n"))
}

fn resolve_prepare_gates(
    checkout: &Path,
    root: &effigy_kernel::root::StateRoot,
    raw_selector: &str,
) -> Result<
    (
        effigy_kernel::id::ResourceId,
        Vec<effigy_kernel::id::ResourceId>,
        String,
    ),
    RunnerError,
> {
    let (selector, args) = effigy_tasks::parse_task_reference_invocation(raw_selector)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let invocation = effigy_cli::TaskInvocation {
        name: effigy_tasks::render_task_selector(&selector),
        args: args.clone(),
    };
    let preflight = crate::runner::execute::api::build_execution_preflight(
        &invocation,
        checkout.to_path_buf(),
    )?;
    let route = crate::runner::execute::route::resolve_route(&invocation, &preflight)?;
    let crate::runner::execute::route::Route::Task { selection, .. } = route else {
        return Err(RunnerError::task_invocation(
            "`run prepare --task` needs a declared task selector",
        ));
    };
    if !crate::runner::execute::task_tree_is_foreground(&preflight, &selection)? {
        return Err(RunnerError::task_invocation(
            "`run prepare --task` supports foreground task trees only",
        ));
    }
    if crate::runner::execute::task_tree_has_task_invocations(&preflight, &selection)? {
        return Err(RunnerError::task_invocation(
            "`run prepare --task` prepares one task identity at a time; prepare each declared task selector in the tree separately",
        ));
    }
    if selection.task.mode.as_deref() == Some("tui") {
        return Err(RunnerError::task_invocation(
            "`run prepare --task` supports foreground task gates, not managed TUI tasks",
        ));
    }
    let target = crate::runner::task_engine::TaskTarget::for_selection(
        &preflight.resolved.resolved_root,
        &preflight.selector.task_name,
        &selection,
    )
    .with_arguments(&args);
    let checkout_id = crate::runner::task_engine::checkout::read(checkout)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .unwrap_or_else(|| root.checkout_gate_identity());
    let owner = target.owner(checkout_id)?;
    let mut ids = vec![owner.clone()];
    if let Some(shared) = target.shared_lock_owner(checkout_id)? {
        ids.push(shared);
    }
    Ok((owner, ids, target.label))
}

/// The checkout a run query belongs to: its journal is the only one the query
/// reads (effigy#339, Q-011).
fn checkout(repo_override: Option<PathBuf>) -> Result<PathBuf, RunnerError> {
    Ok(resolve_active_command_context(repo_override)?
        .resolved
        .resolved_root)
}

/// A `--run-id` repeat: attach to the named run from its first byte, follow
/// it to settle and exit with its own result. Nothing new starts.
pub(in crate::runner) fn attach(run: &str, json: bool) -> Result<String, RunnerError> {
    if !json {
        eprintln!("effigy: attaching to run {run}; this invocation starts nothing");
    }
    logs(&checkout(None)?, run, true, Offsets::default(), json)
}

fn list(
    repo_override: Option<std::path::PathBuf>,
    limit: usize,
    all: bool,
    json: bool,
) -> Result<String, RunnerError> {
    let root = if all {
        None
    } else {
        Some(checkout(repo_override)?)
    };
    let listing = list_runs(root.as_deref(), limit)?;
    let runs = &listing.runs;
    if json {
        return Ok(serde_json::json!({
            "schema": RUNS_SCHEMA,
            "schema_version": 1,
            "scope": if all { "host" } else { "checkout" },
            "runs": runs.iter().map(run_json).collect::<Vec<_>>(),
            "coverage": {
                "kind": if all { "bounded-discovery" } else { "checkout" },
                "observed_roots": listing.observed_roots,
                "complete": !all && listing.omitted.is_empty(),
                "omitted": listing.omitted,
            },
        })
        .to_string());
    }
    if runs.is_empty() && listing.omitted.is_empty() {
        return Ok("No retained runs or live foreground holders found.".to_owned());
    }
    let mut lines = vec![format!(
        "{:<26}  {:<14}  {:<9}  {}",
        "RUN", "STATE", "EXIT", "TASK"
    )];
    for run in runs {
        lines.push(format!(
            "{:<26}  {:<14}  {:<9}  {}",
            run.view.run_id,
            run.view.state,
            run.view
                .exit_code
                .map_or_else(|| "-".to_owned(), |code| code.to_string()),
            run.task
        ));
    }
    for reason in &listing.omitted {
        lines.push(format!("warning: runtime view omitted {reason}"));
    }
    if all {
        lines.push("Coverage: bounded filesystem discovery; unlisted checkouts may exist.".into());
    }
    Ok(lines.join("\n"))
}

fn status(checkout: &Path, selector: &str, json: bool) -> Result<String, RunnerError> {
    let record = find(checkout, selector)?;
    if json {
        return Ok(serde_json::json!({
            "schema": RUN_SCHEMA,
            "schema_version": 1,
            "run": run_json(&record),
        })
        .to_string());
    }
    let mut lines = vec![
        format!("run      {}", record.view.run_id),
        format!("task     {}", record.task),
        format!("state    {}", record.view.state),
    ];
    if let Some(code) = record.view.exit_code {
        lines.push(format!("exit     {code}"));
    }
    if let Some(cleanup) = record.view.cleanup {
        lines.push(format!("cleanup  {cleanup}"));
    }
    if let Some(pid) = record.view.pid {
        lines.push(format!("owner    pid {pid}"));
    }
    if record.is_terminal_record() {
        lines.push(format!("evidence {}", record.view.evidence_source));
        if let Some(checkout) = &record.view.checkout_id {
            lines.push(format!("checkout {checkout}"));
        }
        if let Some(boot) = &record.view.boot {
            lines.push(format!("boot     {boot}"));
        }
    }
    if record.view.state == "interrupted" {
        if record.foreground() {
            // A foreground run has no replay: its surviving sentinel settles it
            // and records what it can prove (effigy#986).
            lines.push(format!(
                "next     effigy run stop {}  (settle through the run's sentinel; its owner is gone)",
                record.view.run_id
            ));
        } else {
            lines.push(format!(
                "next     effigy {}  (or `effigy run stop {}` to settle it now)",
                record.task, record.view.run_id
            ));
        }
    }
    Ok(lines.join("\n"))
}

fn stop(
    checkout: &Path,
    selector: &str,
    wait: Duration,
    json: bool,
) -> Result<String, RunnerError> {
    let record = find(checkout, selector)?;
    if record.foreground() {
        return stop_foreground(&record, wait, json);
    }
    if record.is_terminal_record() {
        return Err(RunnerError::task_invocation(format!(
            "run {} is settled ({}) from its foreground terminal record; there is no live owner to stop and its reported identity grants no signal",
            record.id, record.view.state
        )));
    }
    let started = Instant::now();
    let how = request_stop(&record)?;
    let mut signalled = false;
    let settled = loop {
        match record.refresh() {
            Lookup::Found(now) if now.closed() => break *now,
            Lookup::Found(now) => {
                // An owner that reads no requests (a sequence, managed or
                // container run) gets TERM through its verified gate holder.
                if how == StopStarted::Requested
                    && !signalled
                    && started.elapsed() >= PICKUP
                    && now.view.state == "running"
                {
                    signalled = signal_owner(&now);
                }
                if now.view.state == "interrupted" {
                    let _ = request_stop(&now);
                }
            }
            Lookup::NotFound => {
                return Err(not_found(selector));
            }
            Lookup::Unknown(_) => {}
        }
        if started.elapsed() >= wait {
            return Err(RunnerError::RunControl {
                code: 1,
                detail: format!(
                    "run {} did not settle within {} ms after the stop request; inspect it with \
                     `effigy run status {}`",
                    record.view.run_id,
                    wait.as_millis(),
                    record.view.run_id
                ),
                details_json: serde_json::json!({
                    "schema": RUN_SCHEMA,
                    "schema_version": 1,
                    "run": run_json(&record),
                    "stop": "pending",
                })
                .to_string(),
            });
        }
        std::thread::sleep(RUN_POLL);
    };
    let how = match how {
        StopStarted::AlreadyClosed => "already-closed",
        StopStarted::Requested if signalled => "signalled-owner",
        StopStarted::Requested => "requested",
        StopStarted::Settled => "settled-interrupted",
    };
    if json {
        return Ok(serde_json::json!({
            "schema": RUN_SCHEMA,
            "schema_version": 1,
            "run": run_json(&settled),
            "stop": how,
        })
        .to_string());
    }
    Ok(format!(
        "run {} is {} (stop: {how})",
        settled.view.run_id, settled.view.state
    ))
}

fn stop_foreground(record: &RunRecord, wait: Duration, json: bool) -> Result<String, RunnerError> {
    // The only foreground signal target is a freshly verified live owner.
    if record.view.state == "running" {
        if !signal_owner(record) {
            return Err(unknown(
                &record.id.to_string(),
                "foreground owner is no longer verified; no signal was sent",
            ));
        }
        let started = Instant::now();
        while !record.foreground_released() {
            if started.elapsed() >= wait {
                return Err(unknown(
                    &record.id.to_string(),
                    "the verified foreground owner was signalled but custody has not released its \
                     gate",
                ));
            }
            std::thread::sleep(RUN_POLL);
        }
        if json {
            let mut run = run_json(record);
            run["state"] = "not-recorded".into();
            run["pid"] = serde_json::Value::Null;
            return Ok(serde_json::json!({
                "schema": RUN_SCHEMA, "schema_version": 1,
                "run": run, "stop": "signalled-owner", "custody": "released",
                "outcome": "not-recorded",
            })
            .to_string());
        }
        return Ok(format!(
            "run {}: owner signalled, custody released; outcome not recorded (foreground)",
            record.id
        ));
    }
    if record.view.state != "interrupted" {
        // Nothing was verified about the owner, so nothing may be signalled and
        // no settlement can be claimed.
        return Err(unknown(
            &record.id.to_string(),
            "foreground owner is no longer verified; no signal was sent",
        ));
    }
    // The recorded owner is verifiably gone: there is no owner to signal. The
    // run's own memory sentinel settles it, and this command waits for that
    // settlement and reports the record it wrote — never a signal, never an
    // inferred success (effigy#986).
    let started = Instant::now();
    while !record.foreground_released() {
        if started.elapsed() >= wait {
            return Err(unknown(
                &record.id.to_string(),
                "the run's owner is gone and its sentinel has not released custody; no signal was \
                 sent",
            ));
        }
        std::thread::sleep(RUN_POLL);
    }
    let settled = loop {
        match record.refresh() {
            Lookup::Found(now) if now.is_terminal_record() => break Some(*now),
            _ => {}
        }
        if started.elapsed() >= wait {
            break None;
        }
        std::thread::sleep(RUN_POLL);
    };
    let Some(settled) = settled else {
        if json {
            return Ok(serde_json::json!({
                "schema": RUN_SCHEMA, "schema_version": 1,
                "run": run_json(record), "stop": "settled-by-sentinel",
                "custody": "released", "settlement": "not-recorded",
            })
            .to_string());
        }
        return Ok(format!(
            "run {}: custody released by its sentinel; no outcome was recorded",
            record.id
        ));
    };
    if json {
        return Ok(serde_json::json!({
            "schema": RUN_SCHEMA, "schema_version": 1,
            "run": run_json(&settled), "stop": "settled-by-sentinel",
            "custody": "released", "settlement": "recorded",
        })
        .to_string());
    }
    Ok(format!(
        "run {} settled by its sentinel: {} (cleanup {})",
        settled.view.run_id,
        settled.view.state,
        settled.view.cleanup.unwrap_or("unknown")
    ))
}

/// Byte offsets already delivered for each stream of the run's workload.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::runner) struct Offsets {
    pub(in crate::runner) stdout: u64,
    pub(in crate::runner) stderr: u64,
}

impl Offsets {
    fn get(&self, stream: Stream) -> u64 {
        match stream {
            Stream::Stdout => self.stdout,
            Stream::Stderr => self.stderr,
        }
    }
    fn set(&mut self, stream: Stream, value: u64) {
        match stream {
            Stream::Stdout => self.stdout = value,
            Stream::Stderr => self.stderr = value,
        }
    }
}

/// What a follower hands its sink.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::runner) enum Event<'a> {
    Bytes {
        stream: Stream,
        offset: u64,
        bytes: &'a [u8],
    },
    /// Bytes `from..to` were rotated or retained away before they were read.
    Expired { stream: Stream, from: u64, to: u64 },
    /// A log write failed; output after this offset was not kept.
    Truncated { stream: Stream, at: u64 },
    /// The terminal run kept no log, or retention removed it.
    LogsGone,
}

/// How a follow ended.
#[derive(Debug)]
pub(in crate::runner) enum Ending {
    /// Terminal: closed, or interrupted with its owner gone.
    Settled(Box<RunRecord>),
    /// Not following, and the run is still open.
    Open(Box<RunRecord>),
}

#[derive(Debug)]
pub(in crate::runner) enum FollowError {
    /// The sink, or the follower's own stream, was lost. `Offsets` holds
    /// exactly what was delivered; a new follower resumes there.
    Lost(std::io::Error),
    NotFound,
    /// An observation — the journal, the run's workload evidence, or one
    /// stream's log read — could not be verified within the budget (R3).
    Unknown(String),
}

/// Which observation a retry deadline tracks. Every observation owns its
/// deadline: one stage's success retires only its own deadline, so a follow
/// cannot hang on an observation that stays unverifiable while a cheaper one
/// keeps answering on every poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Observation {
    /// The journal lookup.
    Journal,
    /// The run's workload process, read from its own journal rows.
    Workload,
    /// One stream's log read.
    Log(Stream),
}

impl Observation {
    fn slot(self) -> usize {
        match self {
            Self::Journal => 0,
            Self::Workload => 1,
            Self::Log(stream) => 2 + stream as usize,
        }
    }
}

/// Per-observation retry deadlines. An observation that keeps coming back
/// `Unknown` spends one `UNKNOWN_BUDGET` and then ends the follow with the
/// reason it saw; a completed observation — a value or a proven absence —
/// retires only its own deadline.
#[derive(Default)]
struct RetryDeadlines([Option<Instant>; 4]);

impl RetryDeadlines {
    /// Records a failed observation: starts its deadline on the first
    /// failure, waits one poll toward it, and past the budget ends the
    /// follow with that observation's reason.
    fn retrying(&mut self, stage: Observation, reason: String) -> Result<(), FollowError> {
        let since = self.0[stage.slot()].get_or_insert_with(Instant::now);
        if since.elapsed() >= unknown_budget() {
            return Err(FollowError::Unknown(reason));
        }
        std::thread::sleep(RUN_POLL);
        Ok(())
    }

    /// A completed observation retires its own deadline and nothing else's.
    fn observed(&mut self, stage: Observation) {
        self.0[stage.slot()] = None;
    }
}

/// The follow's retry budget. A proof may shrink only its own thread's budget;
/// the shipped budget stays `UNKNOWN_BUDGET` of real clock.
fn unknown_budget() -> Duration {
    #[cfg(test)]
    if let Some(budget) = TEST_UNKNOWN_BUDGET.with(std::cell::Cell::get) {
        return budget;
    }
    UNKNOWN_BUDGET
}

#[cfg(test)]
std::thread_local! {
    static TEST_UNKNOWN_BUDGET: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

/// Reads the run's workload logs from `offsets`, handing every byte to
/// `sink` once and in order, until the log end (or, when following, until
/// the run is terminal and its logs are drained). `offsets` always holds
/// what the sink accepted. An observation that stays unverifiable — journal,
/// workload evidence, or one stream's read — ends the follow as `Unknown`
/// within the retry budget; a stream is never reported as drained when its
/// read could not be verified.
pub(in crate::runner) fn follow(
    checkout: &Path,
    id: RunId,
    offsets: &mut Offsets,
    keep_following: bool,
    sink: &mut dyn FnMut(Event<'_>) -> std::io::Result<()>,
) -> Result<Ending, FollowError> {
    let mut deadlines = RetryDeadlines::default();
    let mut truncated = [false, false];
    loop {
        let record = match lookup_checkout(checkout, id) {
            Lookup::Found(record) => {
                deadlines.observed(Observation::Journal);
                record
            }
            Lookup::NotFound => return Err(FollowError::NotFound),
            Lookup::Unknown(reason) => {
                deadlines.retrying(Observation::Journal, reason)?;
                continue;
            }
        };
        if record.foreground() {
            sink(Event::LogsGone).map_err(FollowError::Lost)?;
            return Ok(Ending::Open(record));
        }
        // Observed before the drain: a run terminal now has all of its
        // output in its logs, so the drain below is the last one.
        let terminal = record.terminal();
        if terminal {
            fault_point("FollowSettled").map_err(FollowError::Lost)?;
        }
        let process = match record.workload_process() {
            Observed::Verified(process, _) => {
                deadlines.observed(Observation::Workload);
                process
            }
            Observed::Absent(_) => {
                deadlines.observed(Observation::Workload);
                None
            }
            Observed::Unknown(reason) => {
                // The lookup above can keep answering on every poll; this
                // deadline belongs to the workload observation itself, so a
                // persistent failure still ends the follow within the budget
                // instead of polling forever. The run is unaffected.
                deadlines.retrying(Observation::Workload, reason.to_string())?;
                continue;
            }
        };
        let mut moved = false;
        let mut gone = false;
        if let Some(sequence) = process {
            for (index, stream) in [Stream::Stdout, Stream::Stderr].into_iter().enumerate() {
                loop {
                    let offset = offsets.get(stream);
                    let read = match record.read_log(sequence, stream, offset, READ_MAX) {
                        Observed::Verified(read, _) => {
                            deadlines.observed(Observation::Log(stream));
                            read
                        }
                        Observed::Absent(_) => {
                            deadlines.observed(Observation::Log(stream));
                            gone = true;
                            break;
                        }
                        Observed::Unknown(reason) => {
                            // An unverifiable read is not a drained stream:
                            // retry the exact offset, and end the follow with
                            // the reason rather than report the bytes that
                            // could not be read as delivered.
                            deadlines.retrying(Observation::Log(stream), reason.to_string())?;
                            continue;
                        }
                    };
                    if read.from > offset {
                        sink(Event::Expired {
                            stream,
                            from: offset,
                            to: read.from,
                        })
                        .map_err(FollowError::Lost)?;
                        offsets.set(stream, read.from);
                    }
                    if read.truncated
                        && !truncated[index]
                        && read.from + read.bytes.len() as u64 >= read.end
                    {
                        truncated[index] = true;
                        sink(Event::Truncated {
                            stream,
                            at: read.end,
                        })
                        .map_err(FollowError::Lost)?;
                    }
                    if read.bytes.is_empty() {
                        break;
                    }
                    fault_point("FollowRead").map_err(FollowError::Lost)?;
                    sink(Event::Bytes {
                        stream,
                        offset: read.from,
                        bytes: &read.bytes,
                    })
                    .map_err(FollowError::Lost)?;
                    offsets.set(stream, read.from + read.bytes.len() as u64);
                    moved = true;
                }
            }
        } else if terminal {
            gone = true;
        }
        if terminal {
            if gone && offsets.stdout == 0 && offsets.stderr == 0 {
                sink(Event::LogsGone).map_err(FollowError::Lost)?;
            }
            fault_point("FollowDone").map_err(FollowError::Lost)?;
            return Ok(Ending::Settled(record));
        }
        if !keep_following {
            return Ok(Ending::Open(record));
        }
        if !moved {
            std::thread::sleep(RUN_POLL);
        }
    }
}

/// Engine fault points for stream-loss proofs; inert without the
/// `fault-injection` feature, and no environment variable arms them.
fn fault_point(name: &'static str) -> std::io::Result<()> {
    #[cfg(feature = "fault-injection")]
    {
        effigy_kernel::fault_injection::point(name)
    }
    #[cfg(not(feature = "fault-injection"))]
    {
        let _ = name;
        Ok(())
    }
}

fn logs(
    checkout: &Path,
    selector: &str,
    keep_following: bool,
    from: Offsets,
    json: bool,
) -> Result<String, RunnerError> {
    let record = find(checkout, selector)?;
    if record.foreground() || record.is_terminal_record() {
        return Ok(if json {
            serde_json::json!({
                "schema": LOGS_SCHEMA, "schema_version": 1, "run": run_json(&record),
                "settled": record.closed(), "logs_available": false,
                "reason": "foreground-no-retained-log",
                "stdout": StreamJson::new(from.stdout).json(from.stdout),
                "stderr": StreamJson::new(from.stderr).json(from.stderr),
            })
            .to_string()
        } else if record.is_terminal_record() {
            format!(
                "run {} settled as {} (foreground terminal record); no retained log",
                record.id, record.view.state
            )
        } else {
            format!("run {} is foreground; no retained log", record.id)
        });
    }
    let id = record.id;
    let mut offsets = from;
    let ending = if json {
        let mut streams = [StreamJson::new(from.stdout), StreamJson::new(from.stderr)];
        let mut logs_gone = false;
        let ending = follow(checkout, id, &mut offsets, keep_following, &mut |event| {
            match event {
                Event::Bytes { stream, bytes, .. } => {
                    streams[stream as usize].bytes.extend_from_slice(bytes)
                }
                Event::Expired { stream, from, to } => {
                    streams[stream as usize].expired.push((from, to))
                }
                Event::Truncated { stream, at } => streams[stream as usize].truncated_at = Some(at),
                Event::LogsGone => logs_gone = true,
            }
            Ok(())
        });
        let ending = ending.map_err(|error| follow_error(selector, error))?;
        let record = match &ending {
            Ending::Settled(record) | Ending::Open(record) => record,
        };
        let payload = serde_json::json!({
            "schema": LOGS_SCHEMA,
            "schema_version": 1,
            "run": run_json(record),
            "settled": matches!(ending, Ending::Settled(_)),
            "logs_available": !logs_gone,
            "stdout": streams[0].json(offsets.stdout),
            "stderr": streams[1].json(offsets.stderr),
        });
        return match ending {
            Ending::Settled(record) if keep_following && record.exit_code() != 0 => {
                Err(RunnerError::RunControl {
                    code: record.exit_code(),
                    detail: format!(
                        "run {} settled as {} (exit {})",
                        record.view.run_id,
                        record.view.state,
                        record.exit_code()
                    ),
                    details_json: payload.to_string(),
                })
            }
            _ => Ok(payload.to_string()),
        };
    } else {
        let mut stdout = std::io::stdout();
        let mut stderr = std::io::stderr();
        follow(
            checkout,
            id,
            &mut offsets,
            keep_following,
            &mut |event| match event {
                Event::Bytes { stream, bytes, .. } => {
                    let out: &mut dyn Write = match stream {
                        Stream::Stdout => &mut stdout,
                        Stream::Stderr => &mut stderr,
                    };
                    out.write_all(bytes).and_then(|()| out.flush())
                }
                Event::Expired { stream, from, to } => {
                    eprintln!(
                        "effigy: run {id} {} bytes {from}..{to} expired before they were read",
                        stream.name()
                    );
                    Ok(())
                }
                Event::Truncated { stream, at } => {
                    eprintln!(
                        "effigy: run {id} {} log is truncated at byte {at}: a log write failed",
                        stream.name()
                    );
                    Ok(())
                }
                Event::LogsGone => {
                    eprintln!(
                        "effigy: run {id} kept no output log (it ran without `--run-id`, its \
                     output went to a terminal, or retention removed it); its outcome is below"
                    );
                    Ok(())
                }
            },
        )
        .map_err(|error| follow_error(selector, error))?
    };
    match ending {
        Ending::Settled(record) if keep_following => {
            let code = record.exit_code();
            if code != 0 {
                eprintln!(
                    "effigy: run {} settled as {} (exit {code})",
                    record.view.run_id, record.view.state
                );
                return Err(RunnerError::RunSettled { code });
            }
            Ok(String::new())
        }
        _ => Ok(String::new()),
    }
}

struct StreamJson {
    from: u64,
    bytes: Vec<u8>,
    expired: Vec<(u64, u64)>,
    truncated_at: Option<u64>,
}

impl StreamJson {
    fn new(from: u64) -> Self {
        Self {
            from,
            bytes: Vec::new(),
            expired: Vec::new(),
            truncated_at: None,
        }
    }

    fn json(&self, next: u64) -> serde_json::Value {
        serde_json::json!({
            "from": self.from,
            "next_offset": next,
            "data_base64": base64::engine::general_purpose::STANDARD.encode(&self.bytes),
            "expired": self.expired.iter().map(|(from, to)| serde_json::json!({"from": from, "to": to})).collect::<Vec<_>>(),
            "truncated_at": self.truncated_at,
        })
    }
}

fn find(checkout: &Path, selector: &str) -> Result<RunRecord, RunnerError> {
    match resolve_in(checkout, selector)? {
        Lookup::Found(record) => Ok(*record),
        Lookup::NotFound => Err(not_found(selector)),
        Lookup::Unknown(reason) => Err(unknown(selector, &reason)),
    }
}

fn follow_error(selector: &str, error: FollowError) -> RunnerError {
    match error {
        FollowError::NotFound => not_found(selector),
        FollowError::Unknown(reason) => unknown(selector, &reason),
        FollowError::Lost(error) => RunnerError::RunControl {
            code: 1,
            detail: format!(
                "the output stream for run `{selector}` closed ({error}); the run is unaffected. \
                 Resume with `effigy run logs {selector} --follow` and the offsets you received"
            ),
            details_json: String::new(),
        },
    }
}

fn not_found(selector: &str) -> RunnerError {
    RunnerError::RunControl {
        code: 1,
        detail: format!(
            "run `{selector}`: not recorded (foreground or expired); no live holder or retained journal record was found"
        ),
        details_json: serde_json::json!({
            "schema": RUN_SCHEMA,
            "schema_version": 1,
            "selector": selector,
            "state": "not-found",
            "recording": "not-recorded",
        })
        .to_string(),
    }
}

fn unknown(selector: &str, reason: &str) -> RunnerError {
    RunnerError::RunControl {
        code: 1,
        detail: format!(
            "run `{selector}`: outcome unknown, because runtime evidence cannot be verified ({reason}). \
             The run itself is unaffected; check it with `effigy run status {selector}`"
        ),
        details_json: serde_json::json!({
            "schema": RUN_SCHEMA,
            "schema_version": 1,
            "selector": selector,
            "state": "unknown",
            "reason": reason,
        })
        .to_string(),
    }
}

fn run_json(record: &RunRecord) -> serde_json::Value {
    let mut value = serde_json::to_value(&record.view).unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        map.insert("task".to_owned(), record.task.clone().into());
        map.insert(
            "exit_code".to_owned(),
            if record.is_terminal_record() {
                record.view.exit_code.into()
            } else if record.terminal() {
                record.exit_code().into()
            } else {
                serde_json::Value::Null
            },
        );
    }
    value
}

#[cfg(test)]
mod tests;
