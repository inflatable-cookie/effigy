//! `effigy runs` and `effigy run status|logs|stop <run>`: generic run
//! control for every run (Q-006), on the kernel run journal.
//!
//! A follower binds to one exact run id and reads each stream's log by byte
//! offset. Losing the follower loses nothing: a new follower resumes at the
//! offsets the first one delivered, and the run itself is never touched or
//! resubmitted. When the run is terminal and its log is gone, the follower
//! still reports the settled outcome from the journal; only an unobservable
//! journal reads `unknown` (R3).

use std::io::{IsTerminal, Write};
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
    list_runs, lookup_checkout, request_stop, resolve_in, signal_owner, Lookup, RunRecord,
    StopStarted, RUN_POLL,
};
use effigy_core::terminal_projection::PlainStream;

const RUN_SCHEMA: &str = "effigy.run.v1";
const RUNS_SCHEMA: &str = "effigy.runs.v1";
const LOGS_SCHEMA: &str = "effigy.run.logs.v1";
/// One log read; a follower loops until it reaches the log's end.
const READ_MAX: usize = 256 * 1024;
/// How long a follower keeps retrying an unreadable journal before it
/// reports the outcome unknown.
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
    }
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
        lines.push(format!(
            "next     effigy {}  (or `effigy run stop {}` to settle it now)",
            record.task, record.view.run_id
        ));
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
                "the verified foreground owner was signalled but custody has not released its gate",
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
    Ok(format!(
        "run {}: owner signalled, custody released; outcome not recorded (foreground)",
        record.id
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
    /// The journal could not be observed within the budget (R3).
    Unknown(String),
}

/// Reads the run's workload logs from `offsets`, handing every byte to
/// `sink` once and in order, until the log end (or, when following, until
/// the run is terminal and its logs are drained). `offsets` always holds
/// what the sink accepted.
pub(in crate::runner) fn follow(
    checkout: &Path,
    id: RunId,
    offsets: &mut Offsets,
    keep_following: bool,
    sink: &mut dyn FnMut(Event<'_>) -> std::io::Result<()>,
) -> Result<Ending, FollowError> {
    let mut unknown_since: Option<Instant> = None;
    let mut truncated = [false, false];
    loop {
        let record = match lookup_checkout(checkout, id) {
            Lookup::Found(record) => {
                unknown_since = None;
                record
            }
            Lookup::NotFound => return Err(FollowError::NotFound),
            Lookup::Unknown(reason) => {
                let since = *unknown_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= UNKNOWN_BUDGET {
                    return Err(FollowError::Unknown(reason));
                }
                std::thread::sleep(RUN_POLL);
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
            Observed::Verified(process, _) => process,
            Observed::Absent(_) => None,
            Observed::Unknown(_) => {
                std::thread::sleep(RUN_POLL);
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
                        Observed::Verified(read, _) => read,
                        Observed::Absent(_) => {
                            gone = true;
                            break;
                        }
                        Observed::Unknown(_) => break,
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
    // A fresh offset does not carry the parser state of its previous reader.
    // It may lie inside an escape or past a rotated prefix, so reconstruct
    // from byte zero or remain fail-closed.
    let mut projections = [
        resume_projection(&record, Stream::Stdout, from.stdout),
        resume_projection(&record, Stream::Stderr, from.stderr),
    ];
    let ending = if json {
        let mut streams = [StreamJson::new(from.stdout), StreamJson::new(from.stderr)];
        let mut logs_gone = false;
        let ending = follow(checkout, id, &mut offsets, keep_following, &mut |event| {
            match event {
                Event::Bytes { stream, bytes, .. } => {
                    let slot = stream as usize;
                    streams[slot]
                        .bytes
                        .extend_from_slice(&projections[slot].project(bytes));
                }
                Event::Expired { stream, from, to } => {
                    projections[stream as usize] = PlainStream::unknown();
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
        let terminal = [stdout.is_terminal(), stderr.is_terminal()];
        follow(
            checkout,
            id,
            &mut offsets,
            keep_following,
            &mut |event| match event {
                Event::Bytes { stream, bytes, .. } => {
                    let projected;
                    let bytes = if terminal[stream as usize] {
                        bytes
                    } else {
                        projected = projections[stream as usize].project(bytes);
                        projected.as_slice()
                    };
                    let out: &mut dyn Write = match stream {
                        Stream::Stdout => &mut stdout,
                        Stream::Stderr => &mut stderr,
                    };
                    out.write_all(bytes).and_then(|()| out.flush())
                }
                Event::Expired { stream, from, to } => {
                    projections[stream as usize] = PlainStream::unknown();
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

/// Reconstruct a cold cursor only from retained byte zero. The ceiling bounds
/// work for large logs; unknown or expired prefixes remain fail-closed.
fn resume_projection(record: &RunRecord, stream: Stream, offset: u64) -> PlainStream {
    if offset == 0 {
        return PlainStream::default();
    }
    let Observed::Verified(Some(sequence), _) = record.workload_process() else {
        return PlainStream::unknown();
    };
    resume_with(offset, |start, length| {
        match record.read_log(sequence, stream, start, length) {
            Observed::Verified(read, _) => Some(read),
            _ => None,
        }
    })
}

fn resume_with(
    offset: u64,
    read: impl FnOnce(u64, usize) -> Option<effigy_kernel::run::LogRead>,
) -> PlainStream {
    if offset == 0 {
        return PlainStream::default();
    }
    if offset > effigy_core::terminal_projection::PREFIX_SCAN_LIMIT {
        return PlainStream::unknown();
    }
    let Some(read) = read(0, offset as usize) else {
        return PlainStream::unknown();
    };
    if read.retained_from != 0 || read.from != 0 || read.bytes.len() as u64 != offset {
        return PlainStream::unknown();
    }
    let mut projection = PlainStream::default();
    projection.push(&read.bytes);
    projection
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
