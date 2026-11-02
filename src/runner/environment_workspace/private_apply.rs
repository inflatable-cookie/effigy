//! The one ordinary backend apply, as an explicit process boundary.
//!
//! The owner resolves the exact frozen command and the guarded Env run hands
//! it its own gate, effect context and absolute deadline. What happens at the
//! other side of that command is external I/O: production runs it through the
//! qualified private invocation, and a proof substitutes a fake backend there
//! so the publication sequence itself is exercised against a real owner.
//!
//! The qualified invocation is not available yet. Recoverable workspace Env
//! admission can retain kernel custody, but this production caller does not yet
//! supply a custody plan or a qualified Discard capability. Until that
//! integration is qualified, `qualified` performs no process work and reports
//! the outcome unsettled, so no caller can mistake it for an executed apply.

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Instant;

use effigy_containers::environment::BackendError;
use effigy_kernel::ctx::EffectContext;
use effigy_kernel::run::Run;

/// Everything the owner resolved for this apply: the frozen command, its
/// working directory, the caller's complete Compose environment and the
/// activation's one absolute deadline.
pub(in crate::runner) struct PrivateApplyCommand<'a> {
    pub(in crate::runner) workdir: &'a Path,
    pub(in crate::runner) program: &'a OsStr,
    pub(in crate::runner) args: &'a [OsString],
    pub(in crate::runner) label: &'a str,
    pub(in crate::runner) env: &'a [(String, OsString)],
    pub(in crate::runner) until: Instant,
}

impl std::fmt::Debug for PrivateApplyCommand<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The command carries the Compose environment; never render it.
        f.debug_struct("PrivateApplyCommand")
            .field("label", &self.label)
            .field("workdir", &self.workdir)
            .finish_non_exhaustive()
    }
}

/// How this owner runs its one apply. Production supplies [`qualified`]; a
/// proof supplies a fake backend at this external process boundary. The guarded
/// Env run is part of the contract either way, so the qualified invocation
/// already receives the exact run it must own its child with.
pub(in crate::runner) type PrivateApply =
    fn(&mut Run<'_>, &EffectContext, &PrivateApplyCommand<'_>) -> Result<(), BackendError>;

/// The qualified production apply.
///
/// It hands the borrowed Env run and the activation's own child effect to the
/// private invocation and never measures a fresh budget: the deadline in
/// `command` is the activation's. Today that invocation refuses by name, so the
/// apply reports `Uncertain` without starting a process and without retaining a
/// backend byte.
pub(in crate::runner) fn qualified(
    _run: &mut Run<'_>,
    _effect: &EffectContext,
    command: &PrivateApplyCommand<'_>,
) -> Result<(), BackendError> {
    effigy_containers::exec::run_backend_invocation_private_until(
        command.workdir,
        command.program,
        command.args,
        command.label,
        command.env,
        command.until,
    )
    .map_err(|_| BackendError::Uncertain("workspace private apply outcome unavailable".into()))
}

/// Test-only candidate requiring review before native activation. The ordinary strategy above is always
/// refused. This entry borrows the actual activation owner and deadline and
/// moves the exact Compose interpolation file into its retained kernel custody.
#[cfg(feature = "workspace-private-qualification")]
pub(in crate::runner) fn qualification_only(
    run: &mut Run<'_>,
    effect: &EffectContext,
    command: &PrivateApplyCommand<'_>,
) -> Result<(), BackendError> {
    use effigy_kernel::custody::{EnvValue, Lifetime, ProcessSpec, SecretValue, Terminal};
    use effigy_kernel::observe::Observed;
    use std::os::unix::ffi::OsStrExt;
    use std::time::Duration;
    let unavailable =
        || BackendError::Uncertain("workspace private apply outcome unavailable".into());
    let bounded = effect.child_until(command.until);
    bounded
        .operation()
        .check("private workspace qualification")
        .map_err(|_| unavailable())?;
    let remaining = command
        .until
        .checked_duration_since(Instant::now())
        .ok_or_else(unavailable)?;
    let program = command.program.to_str().ok_or_else(unavailable)?;
    let program = effigy_containers::compose::resolve_host_cli_program(program);
    if !Path::new(&program).is_absolute() {
        return Err(unavailable());
    }
    let mut env = std::env::vars_os()
        .map(|(key, value)| (key, EnvValue::Plain(value)))
        .collect::<std::collections::BTreeMap<_, _>>();
    for (key, value) in command.env {
        let value = if value.is_empty() {
            EnvValue::Plain(value.clone())
        } else {
            EnvValue::Secret(
                SecretValue::new(value.as_bytes().to_vec()).map_err(|_| unavailable())?,
            )
        };
        env.insert(key.into(), value);
    }
    let invocation = effigy_containers::policy::render::compose_invocation_args(
        command.program,
        command.args,
        command.env,
    )
    .map_err(|_| unavailable())?;
    let mut process = invocation
        .spawn_workspace_discard_qualification(
            run,
            ProcessSpec {
                argv: vec![program],
                cwd: command.workdir.into(),
                env,
                grace: Duration::from_secs(2),
                lifetime: Lifetime::Finite(remaining),
                terminal: Terminal::Discard,
                label: command.label.into(),
            },
            &bounded,
        )
        .map_err(|_| unavailable())?;
    let exit = match process.wait(bounded.operation()) {
        Observed::Verified(exit, _) => exit,
        _ => return Err(unavailable()),
    };
    if !matches!(
        process.qualification_group_closure(bounded.operation()),
        Observed::Verified(..) | Observed::Absent(_)
    ) {
        return Err(unavailable());
    }
    process.complete(run, exit).map_err(|_| unavailable())?;
    if exit.code != 0 {
        return Err(unavailable());
    }
    Ok(())
}
