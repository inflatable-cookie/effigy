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
use effigy_kernel::id::{
    Digest, ExecutionTarget, InvocationSpec, ResolvedInvocation, Route, RunId,
};
use effigy_kernel::root::StateRoot;
use effigy_kernel::workspace_channel::{
    self as channel, InvocationBody, ObserveBody, Refusal, Selector, Target,
};

use crate::runner::error::RunnerError;
use crate::runner::managed_run::Started;

/// Named Level 1 adapters. These discriminants select an owning host engine;
/// they never grant authority or imply that its adapter is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceOperation {
    DeclaredTask,
    Test,
    Qa,
    QaStatus,
    QaStop,
    Doctor,
    Resolve,
    TaskStatus,
    ContainerStatus,
    ContainerLogs,
    ContainerDown,
    ContainerRestart,
    RunsList,
    RunsStatus,
    RunsStop,
    WorkspaceAttach,
}

impl WorkspaceOperation {
    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::DeclaredTask => "task",
            Self::Test => "test",
            Self::Qa => "tasks:qa-group",
            Self::QaStatus => "tasks:qa-group:status",
            Self::QaStop => "tasks:qa-group:stop",
            Self::Doctor => "doctor",
            Self::Resolve => "tasks:show",
            Self::TaskStatus => "tasks:status",
            Self::ContainerStatus => "container:status",
            Self::ContainerLogs => "container:logs",
            Self::ContainerDown => "container:down",
            Self::ContainerRestart => "container:restart",
            Self::RunsList => "runs:list",
            Self::RunsStatus => "runs:status",
            Self::RunsStop => "runs:stop",
            Self::WorkspaceAttach => "workspace:attach",
        }
    }

    pub(crate) fn from_invocation(body: &InvocationBody) -> Result<Self, Refusal> {
        if body.target != Target::Host {
            return Err(Refusal::forbidden(
                "workspace Level 1 requires host placement",
            ));
        }
        match &body.selector {
            Selector::Task(_) => Ok(Self::DeclaredTask),
            Selector::Operation(name) => match name.as_str() {
                "test" => Ok(Self::Test),
                "tasks:qa-group" => Ok(Self::Qa),
                "tasks:qa-group:stop" => Ok(Self::QaStop),
                "container:down" => Ok(Self::ContainerDown),
                "container:restart" => Ok(Self::ContainerRestart),
                "runs:stop" => Ok(Self::RunsStop),
                "workspace:attach" => Ok(Self::WorkspaceAttach),
                _ => Err(unsupported_operation()),
            },
            Selector::Shell(_) => Err(unsupported_operation()),
        }
    }

    pub(crate) fn from_observation(body: &ObserveBody) -> Result<Self, Refusal> {
        if body
            .target
            .as_ref()
            .is_some_and(|target| *target != Target::Host)
        {
            return Err(Refusal::forbidden(
                "workspace Level 1 requires host placement",
            ));
        }
        match body.op.as_str() {
            "doctor" => Ok(Self::Doctor),
            "tasks:qa-group:status" => Ok(Self::QaStatus),
            "tasks:show" => Ok(Self::Resolve),
            "tasks:status" => Ok(Self::TaskStatus),
            "container:status" => Ok(Self::ContainerStatus),
            "container:logs" => Ok(Self::ContainerLogs),
            "runs:list" => Ok(Self::RunsList),
            "runs:status" => Ok(Self::RunsStatus),
            _ => Err(unsupported_operation()),
        }
    }
}

fn unsupported_operation() -> Refusal {
    Refusal::forbidden("workspace Level 2 unsupported operation; use an independent host CLI")
}

/// The exact borrowed handoff from the authenticated kernel dispatcher to an
/// owning engine. The ticket is rechecked durably by `Run::begin_admitted`;
/// this view cannot mint, change or stand in for that authority.
#[derive(Debug)]
pub(crate) struct ReservedWorkspaceInvocation<'a> {
    ticket: &'a effigy_kernel::run::AdmissionTicket,
    resolved: &'a ResolvedInvocation,
    checkout: &'a Path,
}

impl<'a> ReservedWorkspaceInvocation<'a> {
    pub(crate) fn validate(
        cx: &EffectContext,
        ticket: &'a effigy_kernel::run::AdmissionTicket,
        resolved: &'a ResolvedInvocation,
        checkout: &'a Path,
    ) -> Result<Self, Refusal> {
        if cx.operation().placement() != Placement::Host
            || resolved.spec().placement != Placement::Host
            || !matches!(resolved.spec().target, ExecutionTarget::Host)
        {
            return Err(Refusal::forbidden(
                "a guest placement cannot admit a host run",
            ));
        }
        if !checkout.is_absolute() {
            return Err(Refusal::forbidden("the published checkout is not absolute"));
        }
        channel::check_relative_path(&resolved.spec().cwd)?;
        if resolved.spec().argv.is_empty()
            || resolved
                .spec()
                .argv
                .iter()
                .any(|argument| argument.to_str().is_none())
        {
            return Err(Refusal::invalid(
                "the frozen invocation requires exact UTF-8 arguments",
            ));
        }
        Ok(Self {
            ticket,
            resolved,
            checkout,
        })
    }

    pub(crate) fn ticket(&self) -> &effigy_kernel::run::AdmissionTicket {
        self.ticket
    }
    pub(crate) fn resolved(&self) -> &ResolvedInvocation {
        self.resolved
    }
    pub(crate) fn checkout(&self) -> &Path {
        self.checkout
    }
}

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
    checkout_id: Option<String>,
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

/// Resolve the same profile identity the managed planner will use.
pub(in crate::runner) fn workspace_task_target(
    root: &std::path::Path,
    task_name: &str,
    runtime_args: &effigy_tasks::TaskRuntimeArgs,
    selection: &effigy_manifest::TaskSelection<'_>,
) -> Result<crate::runner::task_engine::TaskTarget, crate::runner::error::RunnerError> {
    let target = crate::runner::task_engine::TaskTarget::for_selection(root, task_name, selection);
    if selection.task.mode.as_deref() != Some("tui") {
        return Ok(target);
    }
    let invocation = effigy_managed::parse_managed_invocation(runtime_args)?;
    if !matches!(
        invocation.action,
        effigy_managed::ManagedInvocation::Run { .. }
    ) {
        return Ok(target);
    }
    Ok(
        target.with_profile(effigy_managed::profiles::requested_profile_name(
            &invocation.runtime_args,
        )),
    )
}

impl HostDispatch {
    pub(crate) fn new(host_checkout: PathBuf, root: StateRoot) -> Self {
        Self {
            host_checkout,
            root,
            checkout_id: None,
            admitted: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(super) fn for_serving(host_checkout: PathBuf, root: StateRoot, checkout_id: &str) -> Self {
        let mut dispatch = Self::new(host_checkout, root);
        dispatch.checkout_id = Some(checkout_id.to_owned());
        dispatch
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

/// Fingerprint the composed values actually used by routing, not a later
/// filesystem reread. An unknown source cannot qualify a reserved request.
pub(in crate::runner) fn catalog_snapshot(
    catalogs: &[effigy_manifest::LoadedCatalog],
) -> Result<effigy_kernel::id::Digest, Refusal> {
    let entries = catalogs
        .iter()
        .map(|catalog| {
            let digest = catalog
                .effective_digest
                .as_ref()
                .ok_or_else(|| Refusal::unavailable("workspace catalog snapshot is unavailable"))?;
            Ok((
                &catalog.alias,
                &catalog.catalog_root,
                &catalog.manifest_path,
                &catalog.bundle_root,
                catalog.depth,
                digest,
            ))
        })
        .collect::<Result<Vec<_>, Refusal>>()?;
    let bytes = serde_json::to_vec(&entries)
        .map_err(|_| Refusal::unavailable("workspace catalog snapshot cannot be encoded"))?;
    let mut domain = b"effigy.workspace.catalog-snapshot\0\x01".to_vec();
    domain.extend(bytes);
    Ok(effigy_kernel::id::Digest::of(&domain))
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
    fn admission_invocation(
        &mut self,
        body: &InvocationBody,
        resolved: &ResolvedInvocation,
        run: RunId,
    ) -> Result<ResolvedInvocation, Refusal> {
        let runtime = effigy_execution::ExecutionRuntimeArgsPlan::from_args(&body.args)
            .map_err(|error| Self::refuse(error.to_string()))?
            .exec_task_runtime_args();
        let mut spec = resolved.spec().clone();
        spec.argv.truncate(1);
        spec.argv.extend(
            runtime
                .passthrough
                .into_iter()
                .map(std::ffi::OsString::from),
        );
        let token = effigy_kernel::run::workspace_managed_token(run);
        spec.provenance.insert(
            crate::runner::task_engine::MANAGED_RUN_PROVENANCE.to_owned(),
            Digest::of(token.as_bytes()),
        );
        ResolvedInvocation::new(spec).map_err(|error| Self::refuse(error.to_string()))
    }

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
        WorkspaceOperation::from_invocation(body)?;
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
        let target = workspace_task_target(
            &preflight.resolved.resolved_root,
            &preflight.selector.task_name,
            &preflight.runtime_args_exec,
            &selection,
        )
        .map_err(|error| Self::refuse(error.to_string()))?;
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
        let policy = if target.shared_lock.is_some() {
            super::coverage_policy::ResolvedTaskPolicy::SharedLock
        } else {
            super::coverage_policy::ResolvedTaskPolicy::LevelOne
        };
        if let Some(message) = super::coverage_policy::unsupported_resolved_task(policy) {
            return Err(Self::refuse(message));
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
        let provenance = BTreeMap::from([(
            "workspace:catalog-snapshot".to_owned(),
            catalog_snapshot(&preflight.catalogs)?,
        )]);
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
            provenance,
        })
        .map_err(|error| Self::refuse(error.to_string()))
    }
}

impl HostDispatch {
    pub(crate) fn state_root(&self) -> &StateRoot {
        &self.root
    }

    /// Reparse a typed read using host CLI grammar and verify its claimed
    /// operation against that parsed command before any owning observer runs.
    pub(crate) fn observation_command(
        &self,
        body: &ObserveBody,
    ) -> Result<Option<(PathBuf, effigy_cli::Command)>, Refusal> {
        WorkspaceOperation::from_observation(body)?;
        let Some(query) = &body.query else {
            return Ok(None);
        };
        let cwd = self.checkout_directory(&query.cwd).map_err(|_| {
            Refusal::forbidden("the Observe cwd is outside the pinned checkout or unavailable")
        })?;
        let command = effigy_cli::parse_command(query.args.clone()).map_err(|_| {
            Refusal::invalid("the Observe query does not name a valid host command")
        })?;
        use effigy_cli::{Command, ContainerSubcommand, RunsAction};
        let forbidden = match &command {
            Command::Doctor(args) => args.repo_override.is_some() || args.fix || args.refresh,
            Command::Tasks(args) => args.repo_override.is_some(),
            Command::Container(args) => {
                args.repo_override.is_some()
                    || matches!(
                        args.subcommand,
                        ContainerSubcommand::Status { global: true, .. }
                            | ContainerSubcommand::Logs { follow: true, .. }
                    )
            }
            Command::Runs(args) => {
                args.repo_override.is_some()
                    || matches!(args.action, RunsAction::List { all: true, .. })
            }
            _ => false,
        };
        if forbidden {
            return Err(Refusal::forbidden(
                "workspace Observe scope or mutation flags unsupported; use an independent host CLI",
            ));
        }
        match super::classify(&command) {
            super::GuestOperation::Observe {
                op,
                target: Target::Host,
                run_id,
            } if op == body.op && run_id == body.run_id => Ok(Some((cwd, command))),
            _ => Err(Refusal::invalid(
                "the Observe query does not match its read-only operation",
            )),
        }
    }

    /// The host directory a guest's checkout-relative working directory names.
    /// Absolute paths, `..` components and escapes out of the checkout refuse.
    pub(crate) fn checkout_directory(&self, relative: &str) -> Result<PathBuf, Refusal> {
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
        let handoff =
            ReservedWorkspaceInvocation::validate(cx, ticket, resolved, &self.host_checkout)?;
        let argv = handoff.resolved().spec().argv.clone();
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
                .map(|arg| {
                    arg.to_str().map(str::to_owned).ok_or_else(|| {
                        Refusal::invalid("the frozen invocation requires exact UTF-8 arguments")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        // Recheck the published directory immediately before launching its
        // supervisor, and carry the exact resolved facts through its private
        // owner channel for a second comparison before any fallback or run.
        self.checkout_directory(".")?;
        let identity = self
            .root
            .owning_directory()
            .identity()
            .map_err(|error| Self::refuse(error.to_string()))?;
        let resolution = crate::runner::managed_run::WorkspaceResolution {
            invocation: serde_json::to_vec(resolved)
                .map_err(|_| Refusal::unavailable("workspace resolution cannot be encoded"))?,
            checkout: self.host_checkout.clone(),
            identity,
        };
        // The engine re-resolves from the same host records; the working
        // directory it runs in is the resolved invocation's own catalog.
        let started = crate::runner::managed_run::start_reserved(
            &self.host_checkout,
            &invocation,
            self.host_checkout.join(&resolved.spec().cwd),
            ticket.clone(),
            resolution,
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
        cx.check("workspace observation")
            .map_err(|error| Refusal::unavailable(error.to_string()))?;
        if body.op != "runs:status" && body.run_id.is_some() {
            return Err(Refusal::invalid(
                "run identity is not valid for this observation",
            ));
        }
        if body
            .target
            .as_ref()
            .is_some_and(|target| *target != Target::Host)
        {
            return Err(Self::refuse(
                "this workspace host answers host-target observations only",
            ));
        }
        self.observation_command(body)?;
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
            "runs:status" => {
                let id = body
                    .run_id
                    .as_deref()
                    .ok_or_else(|| Refusal::invalid("runs:status requires a run identity"))?
                    .parse::<RunId>()
                    .map_err(|_| Refusal::invalid("invalid run identity"))?;
                let checkout = self.checkout_id.as_deref().ok_or_else(|| {
                    Refusal::unavailable("published checkout identity is unavailable")
                })?;
                crate::runner::task_engine::status_owned_run(cx, &self.root, checkout, id)
                    .map_err(|error| Refusal::unavailable(error.to_string()))
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
    let admission = HostDispatch::for_serving(
        serving.host_checkout.clone(),
        root.clone(),
        &serving.enrolment.checkout,
    );
    let retained = admission.admitted();
    let dispatcher = channel::Dispatcher::new(
        HostDispatch::for_serving(
            serving.host_checkout.clone(),
            root.clone(),
            &serving.enrolment.checkout,
        ),
        HostDispatch::for_serving(
            serving.host_checkout.clone(),
            root.clone(),
            &serving.enrolment.checkout,
        ),
        HostDispatch::for_serving(
            serving.host_checkout.clone(),
            root,
            &serving.enrolment.checkout,
        ),
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
                        query: None,
                        run_id: None,
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
                    query: None,
                    run_id: None,
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
                        query: None,
                        run_id: None,
                        op: "runs:status".into(),
                        target: Some(Target::Host)
                    }
                )
                .is_err(),
            "status without a run identity must not return a list"
        );
    }

    #[test]
    fn run_status_reads_exact_scoped_journal_identity() {
        use effigy_kernel::id::{CheckoutId, CheckoutKind, Surface, TaskId};
        use effigy_kernel::observe::{Observed, UnknownReason};
        use effigy_kernel::run::{BeginOpts, Effect, Fence, Reconciled, SettlementAdapter};
        struct NoEffects;
        impl SettlementAdapter for NoEffects {
            fn observe(
                &mut self,
                _: &Effect,
                _: &effigy_kernel::ctx::CleanupContext,
            ) -> Observed<Fence> {
                Observed::Unknown(UnknownReason::GuardUnavailable)
            }
            fn release(
                &mut self,
                _: &Effect,
                _: &effigy_kernel::ctx::CleanupContext,
            ) -> Observed<()> {
                Observed::Unknown(UnknownReason::GuardUnavailable)
            }
            fn reconcile(
                &mut self,
                _: &Effect,
                _: &effigy_kernel::ctx::CleanupContext,
            ) -> Observed<Reconciled> {
                Observed::Unknown(UnknownReason::GuardUnavailable)
            }
        }
        let fixture = tempfile::tempdir().unwrap();
        let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::Shell);
        let root = StateRoot::effects(&cx, fixture.path()).unwrap();
        let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, [11; 16]);
        let task = TaskId::new(checkout, "", Surface::Published, "sample", None).unwrap();
        let id = RunId::new(42, [7; 10]).unwrap();
        let invocation = ResolvedInvocation::new(InvocationSpec {
            owner: task.resource(),
            task,
            route: Route::Task,
            argv: vec!["sample".into()],
            cwd: "".into(),
            target: ExecutionTarget::Host,
            env: BTreeMap::new(),
            gates: vec![],
            attachments: vec![],
            class: EffectClass::Mutate,
            placement: Placement::Host,
            provenance: BTreeMap::new(),
        })
        .unwrap();
        let next = effigy_kernel::outcome::Invocation::validated(vec!["recover".into()], |_| true)
            .unwrap();
        let cleanup = cx
            .operation()
            .cleanup(std::time::Duration::from_secs(5))
            .unwrap();
        let run = effigy_kernel::run::Run::begin(
            &cx,
            &root,
            &invocation,
            BeginOpts {
                id,
                wait: effigy_kernel::gate::Wait::FailFast,
                acknowledge: vec![],
                next,
                custody: None,
            },
            &cleanup,
            &mut NoEffects,
        )
        .unwrap();
        let mut host =
            HostDispatch::for_serving(fixture.path().to_owned(), root.clone(), &checkout.text());
        let observe =
            crate::runner::task_engine::observe_context(std::time::Duration::from_secs(5));
        let body = |run_id| ObserveBody {
            query: None,
            op: "runs:status".into(),
            target: Some(Target::Host),
            run_id,
        };
        let status = host
            .observe(&observe, &body(Some(id.to_string())))
            .expect("exact status");
        assert_eq!(status["run"], id.to_string());
        assert_eq!(status["status"]["state"], "running");
        assert!(host.observe(&observe, &body(None)).is_err());
        assert!(host
            .observe(&observe, &body(Some("invalid".into())))
            .is_err());
        assert!(host
            .observe(
                &observe,
                &body(Some(RunId::new(42, [8; 10]).unwrap().to_string()))
            )
            .is_err());
        let mut foreign = HostDispatch::for_serving(
            fixture.path().to_owned(),
            root.clone(),
            &CheckoutId::new(CheckoutKind::EphemeralClone, [12; 16]).text(),
        );
        assert!(foreign
            .observe(&observe, &body(Some(id.to_string())))
            .is_err());
        drop(run);
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
    fn changed_task_definition_changes_the_reserved_resolution() {
        let fixture = tempfile::tempdir().expect("fixture");
        let checkout = fixture.path().join("checkout");
        std::fs::create_dir(&checkout).expect("checkout");
        let manifest = checkout.join("effigy.toml");
        std::fs::write(&manifest, "[tasks]\nbuild = 'echo original'\n").expect("manifest");
        let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::Shell);
        let root = StateRoot::effects(&cx, &checkout).expect("state");
        let mut dispatch = HostDispatch::new(checkout, root);
        let before = dispatch
            .resolve(&cx, &invoke(".", vec![]))
            .expect("first resolution");
        std::fs::write(&manifest, "[tasks]\nbuild = 'echo replacement'\n")
            .expect("changed manifest");
        let after = dispatch
            .resolve(&cx, &invoke(".", vec![]))
            .expect("second resolution");
        assert_ne!(
            before.digest(),
            after.digest(),
            "a changed command must not retain the reserved resolution"
        );
    }

    #[test]
    fn reserved_managed_identity_includes_default_and_named_profile() {
        let fixture = tempfile::tempdir().unwrap();
        let checkout = fixture.path().join("checkout");
        std::fs::create_dir(&checkout).unwrap();
        std::fs::write(
            checkout.join("effigy.toml"),
            "[tasks.build]\nmode = 'tui'\nconcurrent = [{ name = 'one', run = 'echo one' }]\n[tasks.build.profiles.admin]\nconcurrent = [{ name = 'two', run = 'echo two' }]\n",
        ).unwrap();
        let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::Shell);
        let root = StateRoot::effects(&cx, &checkout).unwrap();
        let mut dispatch = HostDispatch::new(checkout, root);
        for (args, expected) in [(vec![], "default"), (vec!["admin".into()], "admin")] {
            let resolved = dispatch.resolve(&cx, &invoke(".", args)).unwrap();
            assert_eq!(resolved.spec().task.profile(), Some(expected));
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

#[cfg(test)]
mod level_one_routing_tests {
    use super::*;
    use effigy_kernel::workspace_channel::Resolver;

    fn host() -> (tempfile::TempDir, HostDispatch) {
        let fixture = tempfile::tempdir().unwrap();
        let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::Shell);
        let root = StateRoot::effects(&cx, fixture.path()).unwrap();
        let dispatch = HostDispatch::new(fixture.path().to_owned(), root);
        (fixture, dispatch)
    }

    #[test]
    fn level_one_observation_reparses_exact_argv_and_refuses_effects_and_escapes() {
        let (fixture, dispatch) = host();
        std::fs::create_dir(fixture.path().join("nested")).unwrap();
        let body = |op: &str, args: &[&str], cwd: &str| ObserveBody {
            query: Some(channel::ObserveQuery {
                cwd: cwd.into(),
                args: args.iter().map(|s| (*s).to_owned()).collect(),
                output: channel::OutputMode::Json,
            }),
            op: op.into(),
            target: Some(Target::Host),
            run_id: None,
        };
        let (cwd, command) = dispatch
            .observation_command(&body(
                "doctor",
                &["doctor", "build", "--", "two words"],
                "nested",
            ))
            .unwrap()
            .unwrap();
        assert_eq!(cwd, fixture.path().join("nested").canonicalize().unwrap());
        let effigy_cli::Command::Doctor(args) = command else {
            panic!("doctor");
        };
        assert_eq!(args.explain.unwrap().args, vec!["--", "two words"]);
        for query in [
            body("doctor", &["doctor", "--fix"], "."),
            body("doctor", &["doctor", "--refresh"], "."),
            body("doctor", &["doctor", "--repo", "/private-cwd"], "."),
            body("doctor", &["release", "run"], "."),
            body(
                "container:status",
                &["container", "status", "--global"],
                ".",
            ),
            body("container:logs", &["container", "logs", "--follow"], "."),
            body("runs:list", &["runs", "--all"], "."),
            body("doctor", &["doctor"], "../private-cwd"),
        ] {
            let refusal = dispatch.observation_command(&query).unwrap_err();
            assert!(!refusal.diagnostic.contains("private-cwd"));
        }
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), fixture.path().join("outside")).unwrap();
        assert!(dispatch
            .observation_command(&body("doctor", &["doctor"], "outside"))
            .is_err());
    }

    #[test]
    fn level_one_direct_host_request_cannot_admit_a_shared_lock_task() {
        let (fixture, mut dispatch) = host();
        std::fs::write(
            fixture.path().join("effigy.toml"),
            "[tasks.build]\nrun = 'echo forbidden'\nlock = 'shared'\n",
        )
        .unwrap();
        let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::Shell);
        let body = InvocationBody {
            selector: Selector::Task("build".into()),
            cwd: ".".into(),
            target: Target::Host,
            args: vec![],
            output: channel::OutputMode::Text,
        };
        let refusal = dispatch.resolve(&cx, &body).unwrap_err();
        assert!(refusal.diagnostic.contains("shared-lock"), "{refusal}");
        assert!(dispatch.admitted().lock().unwrap().is_empty());
        for op in [
            "release",
            "gateway",
            "config",
            "install",
            "pause",
            "not-a-route",
        ] {
            let body = InvocationBody {
                selector: Selector::Operation(op.into()),
                ..body.clone()
            };
            assert_eq!(
                WorkspaceOperation::from_invocation(&body).unwrap_err().code,
                channel::RefusalCode::Forbidden
            );
        }
    }
}
