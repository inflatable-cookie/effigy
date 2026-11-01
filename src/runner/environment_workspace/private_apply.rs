//! The one ordinary backend apply, as an explicit process boundary.
//!
//! The owner resolves the exact frozen command and the guarded Env run hands
//! it its own gate, effect context and absolute deadline. What happens at the
//! other side of that command is external I/O: production runs it through the
//! qualified private invocation, and a proof substitutes a fake backend there
//! so the publication sequence itself is exercised against a real owner.
//!
//! The qualified invocation is not available yet. A same-Env apply needs the
//! Env run to own kernel custody, and the workspace activation admits that run
//! through `Run::begin_claimed_recoverable`, which refuses an activation that
//! would carry a sentinel. Until resident Env custody lands, `qualified`
//! performs no process work and reports the outcome unsettled, so no caller can
//! mistake it for an executed apply.

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
