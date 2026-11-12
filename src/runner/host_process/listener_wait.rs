//! Activation and readiness observations for lifecycle callers.

use super::*;

/// Wait until the supervisor publishes the activation record for this spec.
/// That record is the barrier between supervision admission and readiness, so
/// the wait below starts from a record that belongs to this activation. A
/// previous activation of the same process keeps its own
/// `runtime_generation`, so a stale record never satisfies the wait.
pub(super) fn wait_for_listener_activation(
    source: Option<&kernel_run::HostLifecycleSource>,
    path: &Path,
    process_name: &str,
    runtime_generation: &str,
    timeout: Duration,
) -> Result<(), RunnerError> {
    let deadline = Instant::now() + timeout;
    loop {
        if kernel_run::read_lifecycle(source, path)?
            .is_some_and(|state| state.runtime_generation == runtime_generation)
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(RunnerError::task_invocation(format!(
                "timed out after {}s waiting for managed host listener `{process_name}` to publish its activation",
                timeout.as_secs()
            )));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Bytes of a child's log this wait reads before bounding the tail it names.
const LISTENER_OUTPUT_TAIL_READ_BYTES: u64 = 8 * 1024;
/// Maximum UTF-8 bytes retained after redacting secret-shaped values. Readers
/// select a contiguous suffix of one child
/// block and never interpret ANSI or join text across supervisor messages.
const LISTENER_OUTPUT_TAIL_BYTES: usize = 320;
/// Characters of a listener's failure detail a report keeps.
pub(super) const MAX_LISTENER_FAILURE_DETAIL_BYTES: usize = 900;

/// The last of a listener child's own output, redacted and bounded, for a
/// progress line or a refusal.
///
/// The supervisor appends its own `[effigy host-process]` lines to the same
/// log, so those are dropped here: an operator reading a failure wants the
/// child's words. A log that is missing, a symlink, or empty contributes
/// nothing.
///
/// Read only an identity-proven canonical file. Excerpts are a contiguous
/// bounded suffix of one child block; no ANSI interpreter transforms persisted
/// bytes or joins text across supervisor messages. Capture already redacted
/// the log's classified secret values; the tail then replaces values of
/// secret-shaped `key=value` words too, because a child can print a
/// credential Effigy never classified (effigy#548).
pub(super) fn listener_output_tail(log_path: &Path) -> Option<String> {
    use std::io::{Read as _, Seek, SeekFrom};
    use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
    let proof = effigy_kernel::custody::MergedLog::identity(log_path).ok()?;
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(log_path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file()
        || metadata.dev() != proof.identity.device
        || metadata.ino() != proof.identity.inode
    {
        return None;
    }
    let len = proof.length;
    if metadata.len() < len {
        return None;
    }
    let start = len.saturating_sub(LISTENER_OUTPUT_TAIL_READ_BYTES);
    let read_from = start.saturating_sub(1);
    file.seek(SeekFrom::Start(read_from)).ok()?;
    let mut raw = vec![0; (len - read_from) as usize];
    file.read_exact(&mut raw).ok()?;
    if !effigy_kernel::custody::MergedLog::identity(log_path)
        .is_ok_and(|after| after.identity == proof.identity)
    {
        return None;
    }
    if start > 0 {
        let begins_line = raw.first() == Some(&b'\n');
        raw.remove(0);
        if !begins_line {
            let newline = raw.iter().position(|byte| *byte == b'\n')?;
            raw.drain(..=newline);
        }
    }
    let text = std::str::from_utf8(&raw).ok()?;
    if text.chars().any(|ch| ch.is_control() && ch != '\n') {
        return None;
    }
    listener_child_block(&effigy_ui::human::redact(text))
}

pub(super) fn listener_child_block(text: &str) -> Option<String> {
    let mut block_start = 0;
    let mut last_child_block = None;
    let mut position = 0;
    for line in text.split_inclusive('\n') {
        if line.starts_with("[effigy host-process]") {
            let block = &text[block_start..position];
            if !block.trim().is_empty() {
                last_child_block = Some(block);
            }
            block_start = position + line.len();
        }
        position += line.len();
    }
    let block = if !text[block_start..position].trim().is_empty() {
        &text[block_start..position]
    } else {
        last_child_block?
    };
    let mut begin = block.len().saturating_sub(LISTENER_OUTPUT_TAIL_BYTES);
    while !block.is_char_boundary(begin) {
        begin += 1;
    }
    Some(block[begin..].to_owned())
}

/// How often a listener whose port is still closed reports progress. The
/// production interval is thirty seconds; a fixture may shorten it so a proof
/// can observe the emitted line without waiting one out.
pub(super) fn listener_progress_interval() -> Duration {
    #[cfg(debug_assertions)]
    if let Some(millis) = std::env::var(START_PROGRESS_INTERVAL_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
    {
        return Duration::from_millis(millis.clamp(50, 30_000));
    }
    LISTENER_START_PROGRESS_INTERVAL
}

/// The one place a listener wait publishes a progress line: the console keeps
/// it, and a test in this process can assert the line the actual wait emitted.
fn emit_listener_progress(line: &str) {
    #[cfg(test)]
    LISTENER_PROGRESS.with(|lines| lines.borrow_mut().push(line.to_owned()));
    eprintln!("{line}");
}

#[cfg(test)]
thread_local! {
    /// Progress lines emitted in this process. The listener wait runs on the
    /// caller's thread, so a fixture asserts exactly what it printed.
    static LISTENER_PROGRESS: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The progress lines this process emitted, drained for one assertion.
#[cfg(test)]
pub(in crate::runner) fn take_listener_progress() -> Vec<String> {
    LISTENER_PROGRESS.with(|lines| std::mem::take(&mut *lines.borrow_mut()))
}

/// A supervisor's last probe and whether this generation has ever connected.
/// A later connection failure must not reset the readiness clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ListenerProbeState {
    pub(super) outcome: HttpReadinessOutcome,
    pub(super) readiness_started: bool,
}

pub(super) fn listener_probe_detail(probe: Option<ListenerProbeState>) -> String {
    match probe {
        None => "no readiness probe observation is available".to_owned(),
        Some(ListenerProbeState { outcome: HttpReadinessOutcome::Ready, .. }) =>
            "the declared HTTP status was verified; waiting for route publication".to_owned(),
        Some(probe) if !probe.readiness_started => format!(
            "the last probe could not connect ({}); its readiness timeout has not started",
            probe.outcome.as_str()
        ),
        Some(probe) => format!(
            "the port has accepted a connection; waiting for the declared HTTP status (last probe: {}); its readiness timeout is running",
            probe.outcome.as_str()
        ),
    }
}

/// The caller uses the supervisor's observed probe phase, never inferring a
/// closed port from an unpublished route. Unknown observations stay unknown.
pub(super) fn listener_start_progress(
    process_name: &str,
    elapsed: Duration,
    log_path: &Path,
    probe: Option<ListenerProbeState>,
) -> String {
    let mut line = format!(
        "[host process] listener `{process_name}` is still starting after {}s: {}; `effigy container <env> down` stops it",
        elapsed.as_secs(),
        listener_probe_detail(probe)
    );
    if let Some(tail) = listener_output_tail(log_path) {
        line.push_str(&format!("; last child output: {tail}"));
    }
    line
}

/// What a caller's wait for one listener's published state concluded.
pub(super) enum ListenerWait {
    /// The listener published this generation's verified state.
    Ready(Box<HostLifecycleObservation>),
    /// Nothing verified was published, so only this listener's own route is
    /// withheld. `budget_expired` says the enclosing activation's observation
    /// budget ended the wait rather than the listener's own readiness.
    Withheld {
        diagnostic: String,
        observed_status: Option<u16>,
        budget_expired: bool,
    },
}

/// Wait until the listener's supervisor publishes this generation's declared
/// state: `ready`, or the failure it observed.
///
/// The listener's own readiness is never the caller's deadline: a child that is
/// still building has not opened its port, and the supervisor's readiness
/// budget starts when it does. The caller's `deadline` is the enclosing
/// activation's observation budget, the same one the bring-up's other runtime
/// observations share (effigy#411). A listener still starting when that budget
/// is spent withholds only its own route: the caller gets `Withheld` instead of
/// waiting for a build that outlasts the activation (effigy#455). A supervisor
/// that exited without publishing is still the failure this wait names.
///
/// `on_wait` runs about once a second while the wait continues, so the caller
/// can publish what it waits for: the entry's own tab names the probe and its
/// elapsed time instead of a generic spinner (effigy#458).
pub(super) fn wait_for_listener_state(
    source: Option<&kernel_run::HostLifecycleSource>,
    path: &Path,
    process_name: &str,
    owner: &str,
    route_table_path: &Path,
    deadline: Option<Instant>,
    mut on_wait: impl FnMut(Instant),
) -> Result<ListenerWait, RunnerError> {
    let mut last_probe = None;
    let started = Instant::now();
    let mut next_progress = started + listener_progress_interval();
    let mut next_wait_report = started;
    loop {
        let now = Instant::now();
        if now >= next_wait_report {
            next_wait_report = now + Duration::from_secs(1);
            on_wait(now);
        }
        if let Some(state) = kernel_run::read_lifecycle(source, path)? {
            if state.owner != owner {
                return Err(RunnerError::task_invocation(format!(
                    "managed host listener state for `{process_name}` belongs to a different checkout/profile owner"
                )));
            }
            last_probe = state.probe;
            match state.status {
                HostLifecycleStatus::Ready => {
                    let route_owner = state.route_owner.as_ref().ok_or_else(|| {
                        RunnerError::task_invocation(format!(
                            "managed host listener state for `{process_name}` is ready without a published route"
                        ))
                    })?;
                    let public_scheme = if route_owner.route_tls {
                        "https"
                    } else {
                        "http"
                    };
                    let expected_public = state
                        .route_domain
                        .as_deref()
                        .map(|domain| format!("{public_scheme}://{domain}"));
                    if state.internal_url.as_deref()
                        != Some(format!("http://{}", route_owner.address).as_str())
                        || state.address.as_deref() != Some(route_owner.address.as_str())
                        || state.runtime_generation != route_owner.runtime_generation
                        || route_owner.generation != state.generation
                        || state.public_url != expected_public
                    {
                        return Err(RunnerError::task_invocation(format!(
                            "managed host listener `{process_name}` has inconsistent endpoint URLs"
                        )));
                    }
                    if managed_route_matches(route_table_path, &state, owner)? {
                        return Ok(ListenerWait::Ready(Box::new(state)));
                    }
                }
                HostLifecycleStatus::Stopped => {
                    return Err(RunnerError::task_invocation(format!(
                        "managed host listener `{process_name}` was stopped before readiness"
                    )));
                }
                HostLifecycleStatus::OwnerGone => {
                    return Err(RunnerError::task_invocation(format!(
                        "managed host listener `{process_name}` stopped starting: {}; its log names what it did",
                        state.diagnostic.as_deref().unwrap_or("its supervisor exited before publishing readiness")
                    )));
                }
                HostLifecycleStatus::Failed => {
                    return Ok(ListenerWait::Withheld {
                        diagnostic: state.diagnostic.clone().unwrap_or_default(),
                        observed_status: state.observed_status,
                        budget_expired: false,
                    });
                }
                _ => {
                    // The supervisor's own progress line goes to the process
                    // log; this one goes to the console of whoever is waiting,
                    // so a slow start is never a silent prompt (effigy#370).
                    if Instant::now() >= next_progress {
                        next_progress = Instant::now() + listener_progress_interval();
                        emit_listener_progress(&listener_start_progress(
                            process_name,
                            started.elapsed(),
                            &path.with_file_name(format!("{}.log", sanitize(process_name))),
                            last_probe,
                        ));
                    }
                }
            }
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Ok(ListenerWait::Withheld {
                diagnostic: format!(
                    "managed host listener `{process_name}` had not published verified readiness after {}s, when the activation's observation budget expired; {}",
                    started.elapsed().as_secs(),
                    listener_probe_detail(last_probe)
                ),
                observed_status: None,
                budget_expired: true,
            });
        }
        thread::sleep(Duration::from_millis(50));
    }
}
