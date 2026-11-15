//! Retained kernel custody for a VM Compose launch that carries values.
//!
//! `colima nerdctl` runs Compose inside the VM, so the values a Compose plan
//! forwards (`secrets.delivery = "compose-env"`) travel in an interpolation
//! file. Tom's #534 ruling is that such an invocation runs in a retained kernel
//! Run: the file moves into kernel custody before the spawn barrier opens, and
//! only a verified group closure lets the kernel remove it. A direct child can
//! never prove that closure, so [`crate::exec::process`] keeps refusing one.
//!
//! This module is the production caller of
//! [`effigy_kernel::custody::Custody::spawn_interpolated`]. The caller supplies
//! the Env Run that already owns the transition ([`RetainedComposeOwner`]), so
//! the spawned group belongs to the same owner `Gate(Env)` serialises:
//!
//! - custody is enabled on that Run the first time a secret-bearing launch
//!   needs it, and never replaced once it exists;
//! - the child's complete environment goes through the custody spec, with the
//!   delivered values classified as secrets so every captured byte is redacted;
//! - an exit is recorded with the Run before the caller returns, and an Unknown
//!   result is never turned into clean output: the file stays in kernel custody
//!   and the caller reports the residue.
//!
//! A launcher that has no Run to borrow still refuses before spawn, naming why
//! ([`ComposeInvocation::require_retained_custody_for_interpolation`]).

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::Write as _IoWrite;
use std::path::Path;
use std::process::Output;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{
    CleanupContext, Deadline, EffectClass, EffectContext, Limits, OperationContext, Placement,
};
use effigy_kernel::custody::Terminal;
use effigy_kernel::custody::{EnvValue, Exit, Lifetime, OwnedProcess, ProcessSpec, SecretValue};
use effigy_kernel::observe::Observed;
use effigy_kernel::run::Run;
use effigy_kernel::supervise::{Executable, LifecyclePlan, Placement as LifecyclePlacement};

use super::implementation::ContainerExecError;
use super::process::{detached_failure, DetachedOutputRecord};
use crate::compose::resolve_host_cli_program;
use crate::policy::render::{compose_invocation_args, ComposeInvocation};

/// The capture budget one retained launch's own context keeps, so a bounded
/// relay can never lose the middle of a build before the caller drains it. The
/// retained backend record keeps its last 64 KiB either way.
const CAPTURE_QUEUE_BYTES: usize = 256 * 1024;
const CAPTURE_TAIL_BYTES: usize = 64 * 1024;
/// How often a detached launch drains and relays its child's output.
const RELAY_INTERVAL: Duration = Duration::from_millis(20);
/// The settlement window a refused or timed-out launch gets to stop its child.
const STOP_BUDGET: Duration = Duration::from_secs(30);

/// The Env Run that owns a Compose transition, borrowed for one launch.
///
/// It is the run `Gate(Env)` already serialises: the child's group, its
/// receipts and the interpolation file's custody all belong to that owner, so
/// dropping the caller cannot leave an unowned file behind.
pub struct RetainedComposeOwner<'a, 'r> {
    run: &'a mut Run<'r>,
    cx: &'a EffectContext,
}

impl<'a, 'r> RetainedComposeOwner<'a, 'r> {
    pub fn new(run: &'a mut Run<'r>, cx: &'a EffectContext) -> Self {
        Self { run, cx }
    }
}

impl std::fmt::Debug for RetainedComposeOwner<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetainedComposeOwner")
            .field("run", &self.run.id())
            .finish_non_exhaustive()
    }
}

/// Move one prepared interpolation file into the owner before the spawn
/// barrier opens, then hand back the child and the context that bounds it.
#[allow(clippy::too_many_arguments)]
fn spawn_retained<'r>(
    invocation: ComposeInvocation,
    run: &mut Run<'r>,
    apply: &EffectContext,
    program: OsString,
    label: &str,
    cwd: &Path,
    env: &[(String, OsString)],
    deadline: Instant,
    terminal: Terminal,
) -> Result<(OwnedProcess, EffectContext), ContainerExecError> {
    enable_custody(run, label)?;
    let cx = spawn_context(apply, deadline);
    let lifetime = deadline
        .saturating_duration_since(effigy_core::run_deadline::now())
        .max(Duration::from_millis(1));
    let spec = ProcessSpec {
        argv: vec![program],
        cwd: cwd.to_path_buf(),
        env: complete_env(env, label)?,
        grace: Duration::from_millis(800),
        lifetime: Lifetime::Finite(lifetime),
        terminal,
        label: label.to_owned(),
    };
    let process = invocation
        .spawn_retained(run, spec, &cx)
        .map_err(|error| launch(label, error))?;
    Ok((process, cx))
}

/// A capture of one secret-bearing VM Compose launch, under retained custody.
pub(super) fn capture(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    deadline: Instant,
    owner: &mut RetainedComposeOwner<'_, '_>,
) -> Result<Output, ContainerExecError> {
    let resolved = resolve_host_cli_program(&program.to_string_lossy());
    let invocation = compose_invocation_args(&resolved, args, env).map_err(|error| {
        launch(
            label,
            format!("cannot prepare the Compose interpolation file: {error}"),
        )
    })?;
    let (mut process, cx) = spawn_retained(
        invocation,
        owner.run,
        owner.cx,
        resolved,
        label,
        repo_root,
        env,
        deadline,
        Terminal::None,
    )?;
    let exit = match process.wait(cx.operation()) {
        Observed::Verified(exit, _) => exit,
        other => return Err(unresolved(process, &cx, label, &other)),
    };
    finish(&process, owner.run, exit, label)?;
    let (stdout, stderr) = process.take_stream_output();
    Ok(Output {
        status: exit_status(exit),
        stdout,
        stderr,
    })
}

/// One detached or streaming VM Compose launch, under retained custody.
///
/// The child's output is relayed into the checkout's retained backend record
/// exactly as a direct child's was, and each complete line reaches `on_line`
/// while the runtime writes it. Nothing about the record's bounded tail or the
/// one-record-per-project rule changes.
#[allow(clippy::too_many_arguments)]
pub(super) fn detached(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
    on_line: &mut dyn FnMut(&str),
    deadline: Instant,
    owner: &mut RetainedComposeOwner<'_, '_>,
) -> Result<Output, ContainerExecError> {
    let resolved = resolve_host_cli_program(&program.to_string_lossy());
    let invocation = compose_invocation_args(&resolved, args, env).map_err(|error| {
        launch(
            label,
            format!("cannot prepare the Compose interpolation file: {error}"),
        )
    })?;
    let record = DetachedOutputRecord::for_invocation(repo_root, &invocation.args)
        .map_err(|error| launch(label, error))?;
    let mut output = record
        .open_file()
        .map_err(|error| launch(label, format!("{} ({})", error, record.path.display())))?;
    let (mut process, cx) = spawn_retained(
        invocation,
        owner.run,
        owner.cx,
        resolved,
        label,
        repo_root,
        env,
        deadline,
        Terminal::None,
    )?;
    let mut offset = 0_u64;
    let exit = loop {
        relay(&process, &mut output, &record, &mut offset, on_line)?;
        if stop_requested.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            let report = process.stop(&cleanup(&cx, label)?);
            return ended(
                process,
                owner.run,
                report.leader,
                label,
                &record,
                on_line,
                &mut offset,
                Some("the stop request ended the Compose invocation"),
            );
        }
        if effigy_core::run_deadline::now() >= deadline {
            let report = process.stop(&cleanup(&cx, label)?);
            return ended(
                process,
                owner.run,
                report.leader,
                label,
                &record,
                on_line,
                &mut offset,
                Some("the activation deadline expired during the Compose invocation"),
            );
        }
        if let Observed::Verified(exit, _) = process.poll() {
            break exit;
        }
        std::thread::sleep(RELAY_INTERVAL);
    };
    relay(&process, &mut output, &record, &mut offset, on_line)?;
    finish(&process, owner.run, exit, label)?;
    let retained = record.retain_tail();
    if exit.code == 0 {
        return Ok(Output {
            status: exit_status(exit),
            stdout: Vec::new(),
            stderr: Vec::new(),
        });
    }
    Err(ContainerExecError::Failure {
        command: label.to_owned(),
        code: Some(exit.code),
        stdout: String::new(),
        stderr: detached_failure(&record, Some(exit.code), retained.err()),
    })
}

/// Report an ending that a stop or a deadline produced: the same failure shape
/// a direct child's ending had, with the root cause named.
#[allow(clippy::too_many_arguments)]
fn ended(
    process: OwnedProcess,
    run: &mut Run<'_>,
    leader: Observed<Exit>,
    label: &str,
    record: &DetachedOutputRecord,
    on_line: &mut dyn FnMut(&str),
    offset: &mut u64,
    cause: Option<&str>,
) -> Result<Output, ContainerExecError> {
    let _ = relay(&process, &mut std::io::sink(), record, offset, on_line);
    if let Observed::Verified(exit, _) = leader {
        finish(&process, run, exit, label)?;
        let retained = record.retain_tail();
        return Err(ContainerExecError::Failure {
            command: label.to_owned(),
            code: Some(exit.code),
            stdout: String::new(),
            stderr: detached_failure(record, Some(exit.code), retained.err()),
        });
    }
    let message = cause.unwrap_or("the Compose invocation did not report an exit");
    Err(ContainerExecError::Failure {
        command: label.to_owned(),
        code: None,
        stdout: String::new(),
        stderr: format!(
            "[effigy] {message}; the retry removed nothing and its retained output is at {}. The interpolation file stays in kernel custody until a verified group closure",
            record.path.display()
        ),
    })
}

/// Copy everything the child has written since the last turn into the retained
/// record, then report its complete lines.
fn relay(
    process: &OwnedProcess,
    output: &mut dyn _IoWrite,
    record: &DetachedOutputRecord,
    offset: &mut u64,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), ContainerExecError> {
    let bytes = process.take_output();
    if !bytes.is_empty() {
        output
            .write_all(&bytes)
            .and_then(|()| output.flush())
            .map_err(|error| ContainerExecError::Failure {
                command: record.path.display().to_string(),
                code: None,
                stdout: String::new(),
                stderr: format!(
                    "[effigy] writing the retained backend record failed: {error}; its path is {}",
                    record.path.display()
                ),
            })?;
    }
    record.report_new_lines(offset, on_line);
    Ok(())
}

/// Record a verified exit with the run, so settlement can authorize cleanup.
fn finish(
    process: &OwnedProcess,
    run: &mut Run<'_>,
    exit: Exit,
    label: &str,
) -> Result<(), ContainerExecError> {
    process
        .complete(run, exit)
        .map_err(|error| launch(label, error))
}

/// An unproven ending: stop the child, keep the file in custody and say why.
fn unresolved(
    mut process: OwnedProcess,
    cx: &EffectContext,
    label: &str,
    observed: &Observed<Exit>,
) -> ContainerExecError {
    let reason = match observed {
        Observed::Verified(exit, _) => format!("the Compose invocation exited with {}", exit.code),
        Observed::Unknown(reason) => reason.to_string(),
        Observed::Absent(proof) => proof.to_string(),
    };
    let stopped = match cleanup(cx, label) {
        Ok(cleanup) => process.stop(&cleanup),
        Err(error) => return error,
    };
    ContainerExecError::Failure {
        command: label.to_owned(),
        code: None,
        stdout: String::new(),
        stderr: format!(
            "[effigy] the Compose invocation's end is unproven ({reason}); the stop observed {}. Its interpolation file stays in kernel custody and is removed only after a verified group closure",
            match &stopped.group {
                Observed::Verified(_, _) => "a closed group".to_owned(),
                Observed::Absent(proof) => proof.to_string(),
                Observed::Unknown(reason) => format!("an unproven group ({reason})"),
            }
        ),
    }
}

/// Enable the helper-backed custody a secret-bearing spawn needs, once.
fn enable_custody(run: &mut Run<'_>, label: &str) -> Result<(), ContainerExecError> {
    if run.has_sentinel() {
        return Ok(());
    }
    let executable = custody_executable().map_err(|error| launch(label, error))?;
    run.enable_custody(&LifecyclePlan {
        executable,
        placement: LifecyclePlacement::Detached,
    })
    .map_err(|error| launch(label, error))
}

/// The binary this run's sentinel and checkout backup re-exec: this one.
/// A fixture names its own helper, because a test harness binary is not an
/// Effigy binary and cannot answer the helper handshake.
fn custody_executable() -> std::io::Result<Executable> {
    #[cfg(test)]
    if let Some(path) = test_executable::get() {
        return Executable::capture(&path);
    }
    Executable::current()
}

/// The bounded context one retained spawn and its wait run under: the apply's
/// own deadline and cancellation, with a relay budget large enough that a
/// drained queue never loses the bytes a record keeps.
fn spawn_context(apply: &EffectContext, deadline: Instant) -> EffectContext {
    let operation = apply.operation();
    let deadline = match operation.deadline() {
        Deadline::At(at) => at.min(deadline),
        Deadline::Open(_) => deadline,
    };
    OperationContext::new(
        Deadline::At(deadline),
        operation.cancel_token().clone(),
        Limits {
            capture_tail: CAPTURE_TAIL_BYTES,
            log_rotate: operation.limits().log_rotate,
            log_keep: operation.limits().log_keep,
            line_max: operation.limits().line_max,
            queue_bytes: CAPTURE_QUEUE_BYTES,
            probe_concurrency: operation.limits().probe_concurrency,
        },
        EffectClass::Mutate,
        Placement::Host,
    )
    .effects()
    .unwrap_or_else(|_| apply.clone())
}

fn cleanup(cx: &EffectContext, label: &str) -> Result<CleanupContext, ContainerExecError> {
    cx.operation()
        .cleanup(STOP_BUDGET)
        .map_err(|error| launch(label, format!("no settlement window: {error}")))
}

/// The complete environment a custody child execs with: the parent's own
/// variables as plain values, and the delivered Compose values classified as
/// secrets so custody redacts every copy from captured output.
fn complete_env(
    env: &[(String, OsString)],
    label: &str,
) -> Result<BTreeMap<OsString, EnvValue>, ContainerExecError> {
    let mut complete = BTreeMap::new();
    for (key, value) in std::env::vars_os() {
        if is_reserved(&key) {
            continue;
        }
        complete.insert(key, EnvValue::Plain(value));
    }
    for (key, value) in env {
        let secret = SecretValue::new(value.as_encoded_bytes().to_vec()).map_err(|error| {
            launch(
                label,
                format!("Compose interpolation value `{key}` cannot travel: {error}"),
            )
        })?;
        complete.insert(OsString::from(key), EnvValue::Secret(secret));
    }
    Ok(complete)
}

fn is_reserved(key: &OsStr) -> bool {
    key == OsStr::new("EFFIGY_CUSTODY_NONCE") || key == OsStr::new("EFFIGY_PROBE_NONCE")
}

fn launch(label: &str, error: impl std::fmt::Display) -> ContainerExecError {
    ContainerExecError::Launch {
        command: label.to_owned(),
        error: std::io::Error::other(error.to_string()),
    }
}

/// The status a reported exit code stands for. The kernel reports the code the
/// leader exited with, or `128 + signal` when a signal ended it.
fn exit_status(exit: Exit) -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    std::process::ExitStatus::from_raw((exit.code & 0xff) << 8)
}

#[cfg(test)]
pub(super) mod test_executable {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    thread_local! {
        static PATH: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }
    pub(in crate::exec) fn get() -> Option<PathBuf> {
        PATH.with(|slot| slot.borrow().clone())
    }
    /// Point this fixture's custody helpers at a binary that speaks the helper
    /// protocol. Callers restore with [`clear`].
    pub(in crate::exec) fn set(path: &Path) {
        PATH.with(|slot| *slot.borrow_mut() = Some(path.to_path_buf()));
    }
    pub(in crate::exec) fn clear() {
        PATH.with(|slot| *slot.borrow_mut() = None);
    }
}
