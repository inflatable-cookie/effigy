//! The detached workspace-serving host supervisor (owner ruling 2026-10-30,
//! contract 052 §8.3 and §8.7; effigy#507).
//!
//! A workspace session's listener, key, grants and admitted clients belong to
//! the session run, so they cannot be prepared in one process and served in
//! another: `SessionLaunch::{launch, serve, publish}` all take the `Run` that
//! owns the session, and `ServingOwner::adopt_production` consumes values only
//! the activating process can build. This module is therefore the *host* of that
//! activation: a detached kernel engine supervisor that starts before the
//! session run exists, performs the activation transaction in its own process,
//! and then keeps the same run, listener, key, grant book and admitted clients
//! through serving and a single settlement.
//!
//! The initiating CLI only launches it and reads a bounded startup result:
//!
//! 1. [`ServingHost::start`] spawns the supervisor through the kernel's own
//!    detached-engine launch path (`start_engine_at`), so the supervisor is the
//!    run's only writer and the CLI is a front end.
//! 2. [`ServingHost::await_serving`] reads engine events until the supervisor
//!    announces the **committed** session it serves, or refuses, or the pipe
//!    ends. Nothing is claimed from a dropped local owner: an absent answer is
//!    an error, never success.
//! 3. [`ServingHost::detach`] acknowledges the kernel's durable detach, after
//!    which the supervisor keeps serving when the CLI exits. Dropping the
//!    front end *without* detaching is the kernel's owner-loss path: the
//!    supervisor stops serving and settles, so a crash before the
//!    acknowledgement leaves at most recoverable authority and never a claim of
//!    success.
//!
//! The activation itself is injected ([`Activation`]): the container-up
//! producer is #452's (`up_prepared`, once #506's planner lands) and this head
//! refuses it by name rather than inventing one. The process-lifetime proof
//! runs the *same* launcher and supervisor with the disposable activation the
//! `workspace-serving-fixture` feature compiles in; that fixture proves process
//! custody, not container-mode preparation or any native backend.

use std::path::PathBuf;
use std::time::Duration;

use effigy_kernel::ctx::{EffectClass, EffectContext};
use effigy_kernel::human::Category;
use effigy_kernel::id::{
    ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId, Route, RunId, Surface, TaskId,
};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::{Invocation, WorkloadOutcome};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{BeginOpts, Effect, Fence, Reconciled, Run, RunError, SettlementAdapter};
use effigy_kernel::store::Store;
use effigy_kernel::supervise::{
    ClientMode, EngineLaunch, EngineSession, Event, Executable, LifecyclePlan, SupervisorClient,
};
use effigy_kernel::workspace_channel::Streams;
use serde::{Deserialize, Serialize};

use super::serving::ServeError;
use super::supervisor::{Retired, RetirementCause, ServiceOwner};

/// The payload schema one serving host accepts.
pub(crate) const SCHEMA: &str = "effigy.workspace-serving.v1";
/// How often the supervisor asks its listener for a connection.
const TURN: Duration = Duration::from_millis(20);

/// What the supervisor's own run records as its own identity.
const SERVING_TASK: &str = "workspace-serving";

/// The effect kind a session's run records when its selection switched. The host
/// reads it back and refuses to serve without it; the container-up producer
/// records the same fact through its own commit (#452).
pub(crate) const SELECTION_KIND: &str = "workspace-selection";

/// One serving host's request: where its state lives, which workspace it
/// serves, and which activation it performs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServingHostRequest {
    pub(crate) schema: String,
    /// The checkout journal root the supervisor writes (never the machine store).
    pub(crate) state_home: PathBuf,
    /// The workspace checkout the supervisor runs in.
    pub(crate) project_path: PathBuf,
    /// The checkout identity the session's environment belongs to.
    pub(crate) checkout: [u8; 16],
    /// The environment name and profile the session borrows.
    pub(crate) environment: String,
    pub(crate) profile: String,
    /// The activation this host performs.
    pub(crate) activation: Activation,
}

/// The activation a serving host performs, as the typed boundary #452 fills.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(crate) enum Activation {
    /// The container-up producer: the real `up_prepared` transaction, which #452
    /// supplies once #506's planner lands.
    ContainerUp {
        /// The producer's own frozen input, opaque to this module.
        request: Vec<u8>,
    },
    /// The disposable activation the process-lifetime proof runs: a real
    /// session run, listener, key, grant book and leased session with no
    /// container and no prepared guest.
    Disposable(DisposableActivation),
}

/// The disposable activation's own inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DisposableActivation {
    /// The supervisor id recorded on the session's serving lease.
    pub(crate) supervisor_id: String,
    /// The capability this fixture grants, as 64 lowercase hex characters, so
    /// the proof can authenticate against the retained grant book.
    pub(crate) capability: String,
    /// The TLS accept budget for the session's private listener.
    pub(crate) tls_budget_ms: u64,
    /// Stop the listener immediately after announcing it, so the supervisor's
    /// own loop meets the terminal refusal and must retire the session.
    #[serde(default)]
    pub(crate) fail_listener: bool,
    /// A distinct environment name for a disposable host that shares a journal
    /// with an earlier one, so each host activates its own generation.
    #[serde(default)]
    pub(crate) environment_suffix: String,
    /// Fault injection: announce a session whose switch was never recorded, so
    /// the host must refuse to serve it.
    #[serde(default)]
    pub(crate) omit_selection_record: bool,
}

/// What one serving host announced to its front end: the committed session it
/// keeps and the lease its listener published.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServingPublished {
    /// The session run's identity, as the lease names it.
    pub(crate) session_run: String,
    /// The session's own generation.
    pub(crate) session_generation: u64,
    /// The environment generation the session borrows.
    pub(crate) environment_generation: u64,
    /// The endpoint the guest dials, as the published lease names it.
    pub(crate) endpoint: String,
    /// The TLS pin the lease published.
    pub(crate) pin: String,
    /// The `supervisor_id` the lease recorded.
    pub(crate) supervisor_id: String,
}

impl ServingPublished {
    fn of(
        session: &effigy_kernel::workspace_session::SessionRef,
        lease: &effigy_kernel::workspace_session::ServingLease,
    ) -> Self {
        Self {
            session_run: lease.session().run.to_string(),
            session_generation: lease.session().generation.get(),
            environment_generation: session.environment_generation().get(),
            endpoint: lease.endpoint().to_owned(),
            pin: lease.pin().hex(),
            supervisor_id: lease.supervisor_id().to_owned(),
        }
    }
}

/// One engine event from a serving host, as its front end reads it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", deny_unknown_fields)]
pub(crate) enum HostEvent {
    /// The committed session this host serves. Sent only after the activation
    /// committed, and only for a selection that switched.
    Serving(ServingPublished),
}

/// Whether this payload is a workspace-serving host's.
pub(crate) fn owns_payload(payload: &[u8]) -> bool {
    serde_json::from_slice::<ServingHostRequest>(payload).is_ok()
}

/// A refusal this supervisor reports to its client and returns, with the
/// kernel's category crossing the event pipe unchanged.
fn refuse(session: &EngineSession, category: Category, detail: impl std::fmt::Display) -> RunError {
    let detail = detail.to_string();
    session.refused(category, &detail);
    RunError::reported(category, detail)
}

/// Begin one serving host: the kernel's detached engine supervisor, from this
/// process, with the request as its payload.
pub(crate) fn start(
    cx: &EffectContext,
    root: &StateRoot,
    request: &ServingHostRequest,
    executable: &Executable,
) -> Result<ServingHost, RunError> {
    let payload = serde_json::to_vec(request)?;
    let launch = EngineLaunch {
        payload,
        cwd: request.project_path.clone(),
        env: Vec::new(),
        handoff: None,
        admission: None,
    };
    let client = effigy_kernel::supervise::start_engine_at(
        cx,
        root,
        launch,
        executable,
        ClientMode::Owning,
    )?;
    Ok(ServingHost {
        client,
        published: None,
    })
}

/// The initiating side of one serving host.
#[derive(Debug)]
pub(crate) struct ServingHost {
    client: SupervisorClient,
    published: Option<ServingPublished>,
}

impl ServingHost {
    /// The supervisor run this host owns, once the kernel admitted it.
    pub(crate) fn run(&self) -> RunId {
        self.client.run
    }

    /// The committed session, once it has been announced.
    pub(crate) fn published(&self) -> Option<&ServingPublished> {
        self.published.as_ref()
    }

    /// Read the bounded startup result. The supervisor announces its committed
    /// session only after its activation committed and its selection switched;
    /// a refusal, a closed event stream or an end of pipe is an error, so a
    /// dropped front end never reads as success.
    pub(crate) fn await_serving(
        &mut self,
        cx: &EffectContext,
        budget: Duration,
    ) -> Result<ServingPublished, RunError> {
        let until = std::time::Instant::now() + budget;
        loop {
            if std::time::Instant::now() > until {
                return Err(RunError::unavailable(
                    "the workspace serving host did not announce its session in time",
                ));
            }
            let event = self.client.event(cx.operation()).map_err(|error| {
                RunError::unavailable(format!("the serving host event stream ended: {error}"))
            })?;
            match event {
                Event::Engine(bytes) => {
                    let event: HostEvent = serde_json::from_slice(&bytes).map_err(|_| {
                        RunError::broken("the serving host announced an unreadable result")
                    })?;
                    match event {
                        HostEvent::Serving(published) => {
                            self.published = Some(published.clone());
                            return Ok(published);
                        }
                    }
                }
                Event::Refused { category, detail } => {
                    return Err(RunError::reported(category, detail))
                }
                Event::Closed { .. } => {
                    return Err(RunError::unavailable(
                        "the workspace serving host closed before announcing a session",
                    ))
                }
                Event::Admitted { .. } | Event::Active | Event::Log { .. } => {}
            }
        }
    }

    /// Acknowledge the kernel's durable detach: the supervisor keeps serving
    /// after this front end exits.
    pub(crate) fn detach(self, cx: &EffectContext) -> Result<RunId, RunError> {
        let id = self.client.run;
        self.client.detach(cx, id)
    }

    /// Ask the supervisor to stop through the kernel's own control path. It
    /// settles once and reports its outcome.
    pub(crate) fn stop(self, cx: &EffectContext) -> Result<RunId, RunError> {
        let id = self.client.run;
        self.client.stop(cx, id)?;
        Ok(id)
    }
}

/// The kernel's settlement surface for a serving host's own run: its effects
/// are the session's (which the session run owns), so it has none of its own.
pub(crate) struct ServingSettlement;

impl SettlementAdapter for ServingSettlement {
    fn observe(&mut self, _: &Effect, _: &effigy_kernel::ctx::CleanupContext) -> Observed<Fence> {
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "a workspace serving host has no adapter effects".into(),
        ))
    }
    fn release(&mut self, _: &Effect, _: &effigy_kernel::ctx::CleanupContext) -> Observed<()> {
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "a workspace serving host has no adapter effects".into(),
        ))
    }
    fn reconcile(
        &mut self,
        _: &Effect,
        _: &effigy_kernel::ctx::CleanupContext,
    ) -> Observed<Reconciled> {
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "a workspace serving host has no adapter effects".into(),
        ))
    }
}

pub(crate) fn cleanup_budget() -> effigy_kernel::ctx::CleanupContext {
    crate::runner::task_engine::cleanup_context(Duration::from_secs(120))
}

pub(crate) fn observation() -> effigy_kernel::ctx::ObserveContext {
    crate::runner::task_engine::observe_context(Duration::from_secs(10))
}

/// One fresh run identity for the host supervisor.
fn fresh_run_id() -> Result<RunId, RunError> {
    use std::io::Read;
    let mut bytes = [0u8; 10];
    let read = std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_ok();
    if !read {
        return Err(RunError::unavailable(
            "the workspace serving host has no run entropy",
        ));
    }
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    RunId::new(millis, bytes).map_err(|error| RunError::broken(format!("a fresh run id: {error}")))
}

/// The supervisor's own invocation: a detached run owned by the workspace
/// session's task, so the kernel gate and the journal name the same owner a
/// host command would.
fn serving_invocation(request: &ServingHostRequest) -> Result<ResolvedInvocation, RunError> {
    let checkout = effigy_kernel::id::CheckoutId::new(
        effigy_kernel::id::CheckoutKind::EphemeralClone,
        request.checkout,
    );
    let task = TaskId::new(
        checkout,
        "",
        Surface::Published,
        SERVING_TASK,
        Some(&request.profile),
    )
    .map_err(|error| RunError::refused(format!("the serving invocation's task: {error}")))?;
    ResolvedInvocation::new(InvocationSpec {
        owner: task.resource(),
        task,
        route: Route::Task,
        argv: vec![
            std::ffi::OsString::from("effigy"),
            std::ffi::OsString::from("container"),
            std::ffi::OsString::from("up"),
        ],
        cwd: String::new(),
        target: ExecutionTarget::Host,
        env: std::collections::BTreeMap::new(),
        gates: Vec::new(),
        attachments: Vec::new(),
        class: EffectClass::Mutate,
        placement: effigy_kernel::ctx::Placement::Host,
        provenance: std::collections::BTreeMap::new(),
    })
    .map_err(|error| RunError::refused(format!("the serving invocation: {error}")))
}

/// The recovery invocation a settled serving host names.
pub(crate) fn recovery_invocation() -> Result<Invocation, RunError> {
    Invocation::validated(
        vec![
            std::ffi::OsString::from("effigy"),
            std::ffi::OsString::from("container"),
            std::ffi::OsString::from("down"),
        ],
        |argv| argv.len() == 3 && argv[0] == "effigy" && argv[1] == "container",
    )
    .map_err(|_| RunError::broken("the serving recovery invocation"))
}

/// The detached supervisor's whole job: admit its own run, perform the
/// activation in this process, keep the committed session, serve it until it
/// ends, and settle once.
///
/// This is the entry the binary's engine dispatch calls; `activation` performs
/// the injected activation and returns the custody this host retains.
pub(crate) fn supervise(mut session: EngineSession) -> Result<(), RunError> {
    let request: ServingHostRequest = match serde_json::from_slice(session.payload()) {
        Ok(request) => request,
        Err(_) => {
            return Err(refuse(
                &session,
                Category::Refused,
                "the workspace serving payload is unreadable",
            ))
        }
    };
    if request.schema != SCHEMA {
        return Err(refuse(
            &session,
            Category::Refused,
            "the workspace serving payload is unsupported",
        ));
    }
    let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::DevServer);
    #[cfg_attr(not(feature = "workspace-serving-fixture"), allow(unused_variables))]
    let observe = observation();
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(&request.project_path),
    };
    let root = match StateRoot::checkout_effects(&cx, &request.state_home, &location) {
        Ok(root) => root,
        Err(error) => {
            return Err(refuse(
                &session,
                Category::Unavailable,
                format!("its state root is unusable ({error})"),
            ))
        }
    };
    let invocation = serving_invocation(&request).map_err(|error| {
        refuse(
            &session,
            error.category(),
            format!("its invocation is unusable ({error})"),
        )
    })?;
    let next = recovery_invocation()?;
    let executable = crate::runner::task_engine::startup_executable()
        .map_err(|error| RunError::step_failed("workspace serving startup binary", error))?;
    let options = BeginOpts {
        id: fresh_run_id()?,
        wait: effigy_kernel::gate::Wait::FailFast,
        acknowledge: Vec::new(),
        next,
        custody: Some(LifecyclePlan {
            executable,
            placement: effigy_kernel::supervise::Placement::Detached,
        }),
    };
    let mut adapter = ServingSettlement;
    let mut run = match Run::begin(
        &cx,
        &root,
        &invocation,
        options,
        &cleanup_budget(),
        &mut adapter,
    ) {
        Ok(run) => run,
        Err(error) => {
            session.refused_message(error.category(), error.to_string());
            return Err(error);
        }
    };
    session.admitted(&mut run)?;
    session.active();
    let watch = session.owner_watch().ok();

    // The activation runs here, in the process that will keep it.
    let mut service = match request.activation.clone() {
        Activation::Disposable(disposable) => {
            #[cfg(not(feature = "workspace-serving-fixture"))]
            let activated: Result<ServiceOwner<'_>, RunError> = {
                let _ = &disposable;
                Err(refuse(
                    &session,
                    Category::Refused,
                    "this build has no disposable workspace serving activation",
                ))
            };
            #[cfg(feature = "workspace-serving-fixture")]
            let activated =
                super::serving_fixture::activate(&cx, &observe, &root, &request, &disposable);
            match activated {
                Ok(service) => service,
                Err(error) => {
                    let _ = run.interrupt(&cleanup_budget(), WorkloadOutcome::Failed { code: 1 });
                    let _ = run.settle(&cleanup_budget(), &mut adapter);
                    session.refused_message(error.category(), error.to_string());
                    return Err(error);
                }
            }
        }
        Activation::ContainerUp { .. } => {
            let error = refuse(
                &session,
                Category::Refused,
                "the container-up activation producer is not wired yet; #452 supplies it",
            );
            let _ = run.interrupt(&cleanup_budget(), WorkloadOutcome::Failed { code: 1 });
            let _ = run.settle(&cleanup_budget(), &mut adapter);
            return Err(error);
        }
    };

    // The switch must be a durable fact of the session's own run: the host reads
    // it back from the journal and refuses to serve anything else, so no
    // activation can announce a session it never switched.
    match durable_selection(&observe, &root, service.session()) {
        Ok(state) => {
            if let Err(refusal) = super::supervisor::committed_state(state) {
                let error = refuse(&session, Category::Refused, refusal.to_string());
                let _ = service.abandon();
                let _ = run.interrupt(&cleanup_budget(), WorkloadOutcome::Failed { code: 1 });
                let _ = run.settle(&cleanup_budget(), &mut adapter);
                return Err(error);
            }
        }
        Err(error) => {
            let reported = refuse(&session, error.category(), error.to_string());
            let _ = service.abandon();
            let _ = run.interrupt(&cleanup_budget(), WorkloadOutcome::Failed { code: 1 });
            let _ = run.settle(&cleanup_budget(), &mut adapter);
            return Err(reported);
        }
    }

    // Announce only the committed session, after the activation committed.
    let published = ServingPublished::of(service.session(), service.lease());
    let event = serde_json::to_vec(&HostEvent::Serving(published.clone()))
        .map_err(|error| RunError::broken(format!("the serving result: {error}")))?;
    session.event(event);

    let owner_lost = || watch.as_ref().map(|watch| watch.lost()).unwrap_or(false);
    // Every way out of the loop retires the custody, so no path can leave the
    // session's listener, key or run behind: a control failure is a listener
    // failure for this purpose, not an early return.
    let end = loop {
        let control = match session.control(&mut run, &[]) {
            Ok(control) => control,
            Err(_) => break RetirementCause::ListenerFailed,
        };
        if control.stop.is_some() {
            break RetirementCause::Stopped;
        }
        if owner_lost() && !session.detached() {
            break RetirementCause::OwnerLost;
        }
        match serve_turn(
            &mut service,
            &mut effigy_kernel::workspace_channel::NoStreams,
        ) {
            Ok(true) => {}
            Ok(false) => std::thread::sleep(TURN),
            Err(error) if error.is_terminal() => break RetirementCause::ListenerFailed,
            Err(_) => {}
        }
    };

    let cleanup = cleanup_budget();
    let retired: Retired = match service.retire_for(end, &cleanup, &mut adapter) {
        Ok(retired) => retired,
        Err(error) => {
            // The release is already made (the retire stops the listener and
            // wipes the key first); report the kernel's settle failure instead
            // of closing this run as if the session had settled.
            let _ = run.interrupt(
                &cleanup,
                WorkloadOutcome::Cancelled {
                    cause: effigy_kernel::ctx::StopCause::OwnerLost,
                    signal: None,
                },
            );
            let _ = run.settle(&cleanup, &mut adapter);
            session.refused_message(error.category(), error.to_string());
            return Err(error);
        }
    };
    match retired.cause {
        RetirementCause::Stopped | RetirementCause::Down | RetirementCause::Settled => {
            run.finish(WorkloadOutcome::Succeeded)?
        }
        RetirementCause::OwnerLost | RetirementCause::ListenerFailed => {
            run.interrupt(
                &cleanup,
                WorkloadOutcome::Cancelled {
                    cause: effigy_kernel::ctx::StopCause::OwnerLost,
                    signal: None,
                },
            )?;
        }
    }
    let settled = run.settle(&cleanup, &mut adapter)?;
    session.closed(&settled);
    Ok(())
}

/// The selection state a session's own run durably recorded, read back from the
/// journal. Absent means the activation never recorded a switch, which is never
/// servable.
fn durable_selection(
    observe: &effigy_kernel::ctx::ObserveContext,
    root: &StateRoot,
    session: &effigy_kernel::workspace_session::SessionRef,
) -> Result<effigy_kernel::workspace_session::SelectionState, RunError> {
    use effigy_kernel::workspace_session::SelectionState;
    let reader = match Store::read(observe, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Unknown(reason) => {
            return Err(RunError::unavailable(format!(
                "the session's selection record is unreadable: {reason}"
            )))
        }
        Observed::Absent(_) => {
            return Err(RunError::unavailable(
                "the session's journal is not recorded",
            ))
        }
    };
    let effects = match effigy_kernel::run::effects(&reader, session.run()) {
        Observed::Verified(effects, _) => effects,
        Observed::Unknown(reason) => {
            return Err(RunError::unavailable(format!(
                "the session's selection record is unreadable: {reason}"
            )))
        }
        Observed::Absent(_) => {
            return Err(RunError::unavailable(
                "the session's selection record is not recorded",
            ))
        }
    };
    for effect in effects.iter().rev() {
        if effect.spec.kind != SELECTION_KIND {
            continue;
        }
        let recorded = effect
            .receipt
            .as_ref()
            .or(Some(&effect.spec.payload))
            .and_then(|value| value.get("state"))
            .and_then(|state| state.as_str());
        return match recorded {
            Some("switched") => Ok(SelectionState::Switched),
            Some("pending") => Ok(SelectionState::Pending),
            Some("abandoned") => Ok(SelectionState::Abandoned),
            _ => Err(RunError::unavailable(
                "the session's selection record names no state",
            )),
        };
    }
    Err(RunError::refused(
        "the session's run recorded no committed selection; nothing is served",
    ))
}

/// One serving turn over the retained custody.
fn serve_turn<'a>(
    service: &mut ServiceOwner<'a>,
    streams: &mut impl Streams,
) -> Result<bool, ServeError> {
    service.serve_pending(streams)
}

/// The environment's resource this host's session borrows.
pub(crate) fn environment_resource(request: &ServingHostRequest) -> Result<ResourceId, RunError> {
    let checkout = effigy_kernel::id::CheckoutId::new(
        effigy_kernel::id::CheckoutKind::EphemeralClone,
        request.checkout,
    );
    ResourceId::environment(checkout, &request.environment)
        .map_err(|error| RunError::refused(format!("the serving environment: {error}")))
}
