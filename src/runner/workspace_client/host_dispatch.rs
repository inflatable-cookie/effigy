//! The production `workspace_channel::Dispatcher` seams over the existing host
//! engine (contract 052 §8.11, owner ruling 2026-10-30).
//!
//! This is a narrow adapter, not an engine: resolution goes through the same
//! host resolver a host command uses, admission hands the reserved ticket to
//! the existing task engine, observation runs under the host's own
//! `ObserveContext` and writes nothing, and run ids come from the engine's own
//! entropy. It holds no store, no gate, no grant and no resolution of its own,
//! and it never falls back to guest execution.
//!
//! A run this adapter admits is owned by the returned supervisor client, which
//! the serving owner retains for its whole life: dropping the owner stops those
//! runs as an owner loss, exactly as a terminal closing does.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use effigy_kernel::ctx::{EffectClass, EffectContext, ObserveContext, Placement};
use effigy_kernel::id::{ExecutionTarget, InvocationSpec, ResolvedInvocation, Route, RunId};
use effigy_kernel::root::StateRoot;
use effigy_kernel::workspace_channel::{
    self as channel, InvocationBody, ObserveBody, Refusal, Selector, Target,
};

use crate::runner::error::RunnerError;
use crate::runner::managed_run::Started;

/// The task engine's fresh entropy, requested exactly as an ordinary admission
/// requests it. A dispatcher never invents a run id.
fn fresh_run_id() -> Result<RunId, RunnerError> {
    crate::runner::task_engine::fresh_run_id()
}

/// The production dispatcher: host re-resolution, the existing engine's
/// admission, the host's read-only observation surface and engine entropy.
pub(crate) struct HostDispatch {
    /// The host checkout the guest's relative working directory resolves in.
    host_checkout: PathBuf,
    /// The environment's own cached state root, for the host reads this adapter
    /// makes (never a re-opened store).
    root: StateRoot,
    /// The runs this session admitted, kept as owning clients. Dropping this
    /// list stops them as owner loss, so their authority dies with the owner.
    admitted: Arc<Mutex<Vec<Started>>>,
}

impl std::fmt::Debug for HostDispatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostDispatch")
            .field("host_checkout", &self.host_checkout)
            .finish_non_exhaustive()
    }
}

impl HostDispatch {
    pub(crate) fn new(host_checkout: PathBuf, root: StateRoot) -> Self {
        Self {
            host_checkout,
            root,
            admitted: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The owning clients this session's admissions retain, for the owner to
    /// keep alive and drop with itself.
    pub(crate) fn admitted(&self) -> Arc<Mutex<Vec<Started>>> {
        self.admitted.clone()
    }

    fn refuse(message: impl Into<String>) -> Refusal {
        Refusal::forbidden(message.into())
    }
}

impl channel::RunIds for HostDispatch {
    fn next(&mut self) -> Result<RunId, Refusal> {
        fresh_run_id()
            .map_err(|_| Refusal::unavailable("the host engine could not allocate a run identity"))
    }
}

/// Re-resolve one guest `InvocationBody` from host records.
///
/// The host resolves the selector itself: the same `build_execution_preflight`
/// and `resolve_route` a host command uses, then the same `TaskTarget` the
/// engine begins from. Nothing the guest sent is trusted beyond the selector
/// text, its arguments and a checkout-relative working directory that is
/// resolved inside this host checkout.
impl channel::Resolver for HostDispatch {
    fn resolve(
        &mut self,
        cx: &EffectContext,
        body: &InvocationBody,
    ) -> Result<ResolvedInvocation, Refusal> {
        if cx.operation().placement() != Placement::Host {
            return Err(Self::refuse(
                "a guest placement cannot resolve a host invocation",
            ));
        }
        if body.target != Target::Host {
            return Err(Self::refuse(
                "this workspace host dispatches host-target requests only",
            ));
        }
        let Selector::Task(name) = &body.selector else {
            return Err(Self::refuse(
                "this workspace host does not dispatch built-in or operation selectors over the \
                 channel; run the command on the host",
            ));
        };
        let runtime = effigy_tasks::parse_task_runtime_args(&body.args)
            .map_err(|error| Self::refuse(error.to_string()))?;
        if runtime.repo_override.is_some() {
            return Err(Self::refuse(
                "a workspace request cannot override the published checkout",
            ));
        }
        let cwd = self.checkout_directory(&body.cwd)?;
        let invocation = effigy_cli::TaskInvocation {
            name: name.clone(),
            args: body.args.clone(),
        };
        let preflight =
            crate::runner::execute::api::build_execution_preflight(&invocation, cwd.clone())
                .map_err(|error| Self::refuse(error.to_string()))?;
        let route = crate::runner::execute::route::resolve_route(&invocation, &preflight)
            .map_err(|error| Self::refuse(error.to_string()))?;
        let crate::runner::execute::route::Route::Task { selection, .. } = route else {
            return Err(Self::refuse(
                "this selector routes to a host built-in, not a declared task; run it on the host",
            ));
        };
        let target = crate::runner::task_engine::TaskTarget::for_selection(
            &preflight.resolved.resolved_root,
            name,
            &selection,
        );
        let checkout = self.host_checkout.canonicalize().map_err(|error| {
            Self::refuse(format!("the published checkout is unavailable: {error}"))
        })?;
        let resolved_root = target.root.canonicalize().map_err(|error| {
            Self::refuse(format!("the resolved checkout is unavailable: {error}"))
        })?;
        let catalog = target
            .root
            .join(&target.catalog)
            .canonicalize()
            .map_err(|error| {
                Self::refuse(format!("the resolved catalog is unavailable: {error}"))
            })?;
        if resolved_root != checkout || !catalog.starts_with(&checkout) {
            return Err(Self::refuse(
                "the resolved task is outside the published checkout",
            ));
        }
        // A run that would first take a task-declared shared lock owns another
        // run, so it cannot carry this reservation. Refuse before the host
        // records the request.
        if target.shared_lock.is_some() {
            return Err(Self::refuse(format!(
                "task `{}` takes a shared lock, which a reserved workspace request cannot carry; \
                 run it on the host",
                target.label
            )));
        }
        let checkout = crate::runner::task_engine::checkout::mint(&target.root)
            .map_err(|error| Self::refuse(error.to_string()))?;
        let task = target
            .task_id(checkout)
            .map_err(|error| Self::refuse(error.to_string()))?;
        let owner = target
            .owner(checkout)
            .map_err(|error| Self::refuse(error.to_string()))?;
        let argv = std::iter::once(target.label.clone())
            .chain(body.args.iter().cloned())
            .map(std::ffi::OsString::from)
            .collect();
        ResolvedInvocation::new(InvocationSpec {
            task,
            route: Route::Task,
            argv,
            cwd: target.catalog.clone(),
            target: ExecutionTarget::Host,
            env: BTreeMap::new(),
            owner,
            gates: Vec::new(),
            attachments: Vec::new(),
            class: EffectClass::Mutate,
            placement: Placement::Host,
            provenance: BTreeMap::new(),
        })
        .map_err(|error| Self::refuse(error.to_string()))
    }
}

impl HostDispatch {
    /// The host directory a guest's checkout-relative working directory names.
    /// Absolute paths, `..` components and escapes out of the checkout refuse.
    fn checkout_directory(&self, relative: &str) -> Result<PathBuf, Refusal> {
        if relative.starts_with('/')
            || relative.contains(['\\', '\0'])
            || relative.split('/').any(|part| part == "..")
        {
            return Err(Self::refuse(format!(
                "`{relative}` is not a checkout-relative working directory"
            )));
        }
        let current = effigy_kernel::fs::DirFd::open(&self.host_checkout).map_err(|error| {
            Self::refuse(format!("the published checkout is unavailable: {error}"))
        })?;
        let current_identity = current
            .identity()
            .map_err(|error| Self::refuse(error.to_string()))?;
        let published_identity = self
            .root
            .owning_directory()
            .identity()
            .map_err(|error| Self::refuse(error.to_string()))?;
        if current_identity != published_identity {
            return Err(Self::refuse("the published checkout was replaced"));
        }
        let directory = if relative == "." || relative.is_empty() {
            self.host_checkout.clone()
        } else {
            self.host_checkout.join(relative)
        };
        let checkout = self.host_checkout.canonicalize().map_err(|error| {
            Self::refuse(format!("the published checkout is unavailable: {error}"))
        })?;
        let directory = directory.canonicalize().map_err(|error| {
            Self::refuse(format!(
                "the requested working directory is unavailable: {error}"
            ))
        })?;
        if !directory.starts_with(&checkout) {
            return Err(Self::refuse(format!(
                "`{relative}` resolves outside the host checkout"
            )));
        }
        Ok(directory)
    }
}

/// Admit exactly the run the dispatcher reserved.
///
/// The ticket travels unchanged into the existing task engine, which begins the
/// run with `Run::begin_admitted` under the canonical Task gate. This adapter
/// mints no id, holds no grant and records no effect itself; the owning client
/// of the admitted run is retained with the serving owner.
impl channel::Admission for HostDispatch {
    fn admit(
        &mut self,
        cx: &EffectContext,
        ticket: &effigy_kernel::run::AdmissionTicket,
        resolved: &ResolvedInvocation,
    ) -> Result<(), Refusal> {
        if cx.operation().placement() != Placement::Host {
            return Err(Self::refuse("a guest placement cannot admit a host run"));
        }
        if resolved.spec().placement != Placement::Host
            || !matches!(resolved.spec().target, ExecutionTarget::Host)
        {
            return Err(Self::refuse(
                "the resolved invocation is not a host-placed task",
            ));
        }
        let argv = resolved.spec().argv.clone();
        let Some(name) = argv.first().and_then(|arg| arg.to_str()) else {
            return Err(Self::refuse(
                "the resolved invocation names no runnable task",
            ));
        };
        let invocation = effigy_cli::TaskInvocation {
            name: name.to_owned(),
            args: argv
                .iter()
                .skip(1)
                .filter_map(|arg| arg.to_str().map(str::to_owned))
                .collect(),
        };
        // The engine re-resolves from the same host records; the working
        // directory it runs in is the resolved invocation's own catalog.
        let started = crate::runner::managed_run::start_reserved(
            &self.host_checkout,
            &invocation,
            self.host_checkout.join(&resolved.spec().cwd),
            ticket.clone(),
        )
        .map_err(|error| Refusal::unavailable(error.to_string()))?;
        if started.run != ticket.run() {
            // The engine admitted another run than the reservation names. Its
            // client is dropped here, which stops that run as owner loss, and
            // this request refuses instead of reporting a run it did not get.
            drop(started);
            return Err(Refusal::unknown(
                "the host engine admitted a run that is not the reserved one",
            ));
        }
        self.admitted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(started);
        Ok(())
    }
}

/// The host's read surface.
///
/// Observation is a read of host records under the caller's `ObserveContext`:
/// it opens no run, takes no gate and writes nothing. Only the run facts this
/// host keeps are served; an operation this build does not serve refuses with
/// its name rather than guessing an answer or admitting a run.
impl channel::Observer for HostDispatch {
    fn observe(
        &mut self,
        cx: &ObserveContext,
        body: &ObserveBody,
    ) -> Result<serde_json::Value, Refusal> {
        if body
            .target
            .as_ref()
            .is_some_and(|target| *target != Target::Host)
        {
            return Err(Self::refuse(
                "this workspace host answers host-target observations only",
            ));
        }
        match body.op.as_str() {
            "runs:list" => {
                let listing = crate::runner::task_engine::list_owned_runs(cx, &self.root, 64)
                    .map_err(|error| Refusal::unavailable(error.to_string()))?;
                let runs = listing
                    .runs
                    .iter()
                    .map(|run| {
                        serde_json::json!({
                            "run": run.view.run_id,
                            "task": run.task,
                            "state": run.view.state,
                            "phase": run.view.phase,
                            "exit": run.view.exit_code,
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(serde_json::json!({
                    "schema": "effigy.runs.v1",
                    "observed": true,
                    "omitted": listing.omitted.len(),
                    "runs": runs,
                }))
            }
            other => Err(Self::refuse(format!(
                "this workspace host does not serve `{other}` observations over the channel yet; \
                 run the command on the host"
            ))),
        }
    }
}

/// The owning clients of the runs one serving owner admitted. Kept alive by the
/// owner, and dropped with it: owner loss stops those runs.
pub(crate) type RetainedRuns = Arc<Mutex<Vec<Started>>>;

/// The production dispatcher, as the kernel composes it: the same adapter
/// answers all four seams because each reads host records and none holds
/// authority of its own.
pub(crate) type HostDispatcher =
    channel::Dispatcher<HostDispatch, HostDispatch, HostDispatch, HostDispatch>;

/// One connection's host server over that dispatcher.
pub(crate) type HostServer =
    channel::Server<HostDispatch, HostDispatch, HostDispatch, HostDispatch>;

/// One connection's host server, built only from facts the publication
/// validated and the engine-owned seams above.
pub(crate) fn production_server(
    serving: &super::ServingFacts,
    root: StateRoot,
) -> (HostServer, RetainedRuns) {
    let admission = HostDispatch::new(serving.host_checkout.clone(), root.clone());
    let retained = admission.admitted();
    let dispatcher = channel::Dispatcher::new(
        HostDispatch::new(serving.host_checkout.clone(), root.clone()),
        HostDispatch::new(serving.host_checkout.clone(), root.clone()),
        HostDispatch::new(serving.host_checkout.clone(), root),
        admission,
    );
    (
        channel::Server::new(
            serving.negotiation.clone(),
            serving.fence.clone(),
            serving.pin.clone(),
            serving.grant_book(),
            dispatcher,
        ),
        retained,
    )
}

/// The host checkout one serving owner's dispatcher resolves inside.
pub(crate) fn host_checkout(serving: &super::ServingFacts) -> &Path {
    &serving.host_checkout
}

#[cfg(test)]
mod checkout_boundary_tests {
    use super::*;
    use effigy_kernel::workspace_channel::{Observer, OutputMode, Resolver};

    #[test]
    fn observe_refuses_expired_budget_and_status_without_run_identity() {
        let fixture = tempfile::tempdir().expect("fixture");
        let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::Shell);
        let root = effigy_kernel::root::StateRoot::effects(&cx, fixture.path()).expect("state");
        let mut dispatch = HostDispatch::new(fixture.path().to_owned(), root);
        let expired = crate::runner::task_engine::observe_context(std::time::Duration::ZERO);
        assert!(
            dispatch
                .observe(
                    &expired,
                    &ObserveBody {
                        op: "runs:list".into(),
                        target: Some(Target::Host)
                    }
                )
                .is_err(),
            "expired observation must refuse"
        );
        let active = crate::runner::task_engine::observe_context(std::time::Duration::from_secs(5));
        let listing = dispatch
            .observe(
                &active,
                &ObserveBody {
                    op: "runs:list".into(),
                    target: Some(Target::Host),
                },
            )
            .expect("active observation");
        assert_eq!(listing["schema"], "effigy.runs.v1");
        assert!(listing["runs"].is_array());
        assert!(
            dispatch
                .observe(
                    &active,
                    &ObserveBody {
                        op: "runs:status".into(),
                        target: Some(Target::Host)
                    }
                )
                .is_err(),
            "status without a run identity must not return a list"
        );
    }

    fn invoke(cwd: &str, args: Vec<String>) -> InvocationBody {
        InvocationBody {
            selector: Selector::Task("build".into()),
            cwd: cwd.into(),
            target: Target::Host,
            args,
            output: OutputMode::Text,
        }
    }

    #[test]
    fn guest_repo_override_and_symlink_cannot_escape_published_checkout() {
        let fixture = tempfile::tempdir().expect("fixture");
        let checkout = fixture.path().join("checkout");
        let other = fixture.path().join("other");
        std::fs::create_dir_all(&checkout).expect("checkout");
        std::fs::create_dir_all(&other).expect("other checkout");
        for path in [&checkout, &other] {
            std::fs::write(
                path.join("effigy.toml"),
                "[tasks]\nbuild = 'echo fixture'\n",
            )
            .expect("manifest");
        }
        std::os::unix::fs::symlink(&other, checkout.join("outside")).expect("fixture symlink");
        let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::Shell);
        let root = effigy_kernel::root::StateRoot::effects(&cx, &checkout).expect("state");
        let mut dispatch = HostDispatch::new(checkout.clone(), root);
        assert!(
            dispatch.resolve(&cx, &invoke(".", vec![])).is_ok(),
            "ordinary checkout task resolves"
        );
        assert!(
            dispatch.resolve(&cx, &invoke("outside", vec![])).is_err(),
            "symlink escape refuses"
        );
        assert!(
            dispatch
                .resolve(
                    &cx,
                    &invoke(".", vec!["--repo".into(), other.display().to_string()])
                )
                .is_err(),
            "guest repo override refuses"
        );
        std::fs::rename(&checkout, fixture.path().join("old-checkout")).expect("move checkout");
        std::fs::create_dir(&checkout).expect("replace checkout");
        std::fs::write(
            checkout.join("effigy.toml"),
            "[tasks]\nbuild = 'echo replacement'\n",
        )
        .expect("replacement manifest");
        assert!(
            dispatch.resolve(&cx, &invoke(".", vec![])).is_err(),
            "a replacement checkout refuses"
        );
    }
}
