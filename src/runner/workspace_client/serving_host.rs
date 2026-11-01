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
    /// Container-up intent; the supervisor constructs and retains its plan.
    ContainerUp {
        /// Command intent only. Live preparation never crosses this boundary.
        request: super::container_up::ContainerUpRequest,
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
    /// Test-only expired first cleanup budget. The exact stopped owner must
    /// survive until an explicit stop or owner-loss transition retries cleanup.
    #[serde(default)]
    pub(crate) fail_refusal_cleanup: bool,
    /// Model a real journalled external file whose cleanup observation stays
    /// Unknown until the fixture explicitly permits its removal.
    #[serde(default)]
    pub(crate) unknown_refusal_cleanup: bool,
    /// Attempt a competing second owner for this session inside the same
    /// process, so the kernel's one-serving-owner fence is exercised.
    #[serde(default)]
    pub(crate) contend: bool,
    /// Delay the announcement by this long, so a caller's startup deadline is
    /// what bounds the wait.
    #[serde(default)]
    pub(crate) announce_delay_ms: u64,
}

/// The normal container report produced from completed integration results.
/// It is descriptive output, never activation authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContainerStartupReport {
    pub(crate) json: serde_json::Value,
    pub(crate) text: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) report: Option<ContainerStartupReport>,
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
            report: None,
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
    // The kernel deliberately clears inherited environment. Pass only path
    // routing and the declared credential-free backend identity. Secret values,
    // policies and session credentials never use this lane.
    let mut environment = vec![("HOME".into(), request.state_home.as_os_str().to_owned())];
    if let Some(path) = std::env::var_os("PATH") {
        environment.push(("PATH".into(), path));
    }
    if let Activation::ContainerUp { request: input } = &request.activation {
        if let Some(declaration) = &input.post_prep {
            environment.extend(declaration.backend_environment()?);
        }
    }
    #[cfg(feature = "workspace-serving-fixture")]
    if let Some(config) = std::env::var_os("EFFIGY_WORKSPACE_ACTIVATION_FAKE") {
        environment.push(("EFFIGY_WORKSPACE_ACTIVATION_FAKE".into(), config));
    }
    let launch = EngineLaunch {
        payload,
        cwd: request.project_path.clone(),
        env: environment,
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
            let remaining = until.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(RunError::unavailable(
                    "the workspace serving host did not announce its session in time",
                ));
            }
            // Each read is bounded by what is left of the *requested* budget, so a
            // silent host costs the caller exactly its own deadline and never the
            // effect context's.
            let operation = cx.operation().child(remaining);
            let event = self.client.event(&operation).map_err(|error| {
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
        if self.published.is_none() {
            return Err(RunError::refused(
                "the workspace serving host cannot detach before its committed acknowledgement",
            ));
        }
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
    fn observe(
        &mut self,
        effect: &Effect,
        _: &effigy_kernel::ctx::CleanupContext,
    ) -> Observed<Fence> {
        #[cfg(feature = "workspace-serving-fixture")]
        if let Some(observed) = super::serving_fixture::cleanup_probe_observe(effect) {
            return observed;
        }
        let _ = effect;
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "a workspace serving host has no adapter effects".into(),
        ))
    }
    fn release(&mut self, effect: &Effect, _: &effigy_kernel::ctx::CleanupContext) -> Observed<()> {
        #[cfg(feature = "workspace-serving-fixture")]
        if let Some(observed) = super::serving_fixture::cleanup_probe_release(effect) {
            return observed;
        }
        let _ = effect;
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
pub(super) fn fresh_run_id() -> Result<RunId, RunError> {
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
    if matches!(&request.activation, Activation::ContainerUp { .. }) {
        if let Err(error) = super::container_up::require_private_preparation_output() {
            return Err(refuse(&session, error.category(), error.to_string()));
        }
    }
    let prepared_container = match &request.activation {
        Activation::ContainerUp { request: input } => {
            match super::container_up::preflight(&request, input) {
                Ok(prepared) => Some(prepared),
                Err(error) => return Err(refuse(&session, error.category(), error.to_string())),
            }
        }
        Activation::Disposable(_) => None,
    };
    crate::runner::task_engine::adopt_state_home(request.state_home.clone());
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
    if let Err(error) = session.admitted(&mut run) {
        return settle_refused_host(session, run, error);
    }
    session.active();
    // The activation runs here, in the process that will keep it.
    match request.activation.clone() {
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
            let service = match activated {
                Ok(service) => service,
                Err(error) => {
                    return settle_refused_host(session, run, error);
                }
            };
            serve_activated(
                session,
                run,
                &root,
                service,
                disposable.fail_refusal_cleanup,
                None,
            )
        }
        Activation::ContainerUp { request: input } => {
            let Some(prepared) = prepared_container else {
                return settle_refused_host(
                    session,
                    run,
                    RunError::broken("workspace activation lost its prevalidation"),
                );
            };
            let activated = match super::container_up::activate(
                &request,
                input,
                prepared,
                &mut session,
                &mut run,
            ) {
                Ok(activated) => activated,
                Err(error) => return settle_refused_host(session, run, error),
            };
            let service = activated.service;
            let effects = activated.effects;
            let report = activated.report;
            let result = serve_activated(session, run, &root, service, false, Some(report));
            // Keep the exact integration handles beside the original serving
            // owner until its control loop has ended.
            drop(effects);
            result
        }
    }
}

/// Both production and disposable dispatchers retain the same custody loop.
fn serve_activated<Ad, Rs, Ob, Id>(
    mut session: EngineSession,
    mut run: Run<'_>,
    root: &StateRoot,
    mut service: ServiceOwner<'_, Ad, Rs, Ob, Id>,
    fail_refusal_cleanup: bool,
    startup_report: Option<ContainerStartupReport>,
) -> Result<(), RunError>
where
    Ad: effigy_kernel::workspace_channel::Admission,
    Rs: effigy_kernel::workspace_channel::Resolver,
    Ob: effigy_kernel::workspace_channel::Observer,
    Id: effigy_kernel::workspace_channel::RunIds,
{
    let observe = observation();
    let watch = session.owner_watch().ok();
    let mut adapter = ServingSettlement;
    // The switch must be a durable fact of the session's own run: the host reads
    // it back from the journal and refuses to serve anything else, so no
    // activation can announce a session it never switched.
    match durable_selection(&observe, root, service.session()) {
        Ok((state, committed)) => {
            let refusal = super::supervisor::committed_state(state)
                .err()
                .map(|refusal| refusal.to_string());
            let lease_mismatch = committed
                .as_ref()
                .is_some_and(|committed| committed != service.lease());
            if let Some(reason) = refusal.or_else(|| {
                lease_mismatch.then(|| {
                    "the serving lease is not the lease the committed selection recorded".to_owned()
                })
            }) {
                let error = RunError::refused(reason);
                let error = retire_refused_service(
                    &mut session,
                    &mut run,
                    service,
                    error,
                    fail_refusal_cleanup,
                    RetirementCause::ListenerFailed,
                    None,
                );
                return settle_refused_host(session, run, error);
            }
        }
        Err(error) => {
            let reported = RunError::reported(error.category(), error.to_string());
            let error = retire_refused_service(
                &mut session,
                &mut run,
                service,
                reported,
                fail_refusal_cleanup,
                RetirementCause::ListenerFailed,
                None,
            );
            return settle_refused_host(session, run, error);
        }
    }

    // Announce only the committed session, after the activation committed.
    let mut published = ServingPublished::of(service.session(), service.lease());
    published.report = startup_report;
    let event = serde_json::to_vec(&HostEvent::Serving(published.clone()))
        .map_err(|error| RunError::broken(format!("the serving result: {error}")))?;
    session.event(event);

    let owner_lost = || watch.as_ref().map(|watch| watch.lost()).unwrap_or(false);
    // The session's terminal custody: one terminal owner and retained reader per
    // admitted run. #452's container-up caller opens an entry for each run it
    // admits; until then every terminal frame refuses rather than dropping
    // input, and the loop already serves the exact value it will hand over
    // (effigy#485).
    //
    // Level 1 forwarding composes around it rather than replacing it: a
    // forwarded run has no terminal, so its owning attachment opens for custody
    // only and its output is read through the session's captured-output
    // observe; terminal input and resize refuse with the named Level 2 message
    // (052 §8.11).
    let mut terminals =
        super::noninteractive_stream::LevelOneStreams::new(service.terminal_custody(&observe));
    // Every way out of the loop retires the custody, so no path can leave the
    // session's listener, key or run behind: a control failure is a listener
    // failure for this purpose, not an early return.
    let end = loop {
        let control = match session.control(&mut run, &[]) {
            Ok(control) => control,
            Err(_) => break RetirementCause::ListenerFailed,
        };
        // The kernel's own stop request can *be* the owner loss (its owning pipe
        // ended), so the cause is read from the request rather than assumed, and
        // a live owner check decides before a plain stop does.
        let reported = control.stop;
        if owner_lost() && !session.detached() {
            break RetirementCause::OwnerLost;
        }
        match reported {
            Some(effigy_kernel::ctx::StopCause::OwnerLost) => break RetirementCause::OwnerLost,
            Some(_) => break RetirementCause::Stopped,
            None => {}
        }
        match serve_turn(&mut service, &mut terminals) {
            Ok(true) => {}
            Ok(false) => std::thread::sleep(TURN),
            Err(error) if error.is_terminal() => break RetirementCause::ListenerFailed,
            // One connection failed its own turn; the session keeps serving.
            Err(_) => {}
        }
    };

    let cleanup = cleanup_budget();
    let retired: Retired = match service.retire_for_recoverable(end, &cleanup, &mut adapter) {
        Ok(retired) => retired,
        Err((error, retained)) => {
            let primary = RunError::unavailable(format!(
                "workspace service ended ({}); session cleanup failed: {error}",
                end.as_str(),
            ));
            let error = retire_refused_service(
                &mut session,
                &mut run,
                *retained,
                primary,
                false,
                end,
                Some(error),
            );
            return settle_failed_host(
                session,
                run,
                error,
                WorkloadOutcome::Cancelled {
                    cause: effigy_kernel::ctx::StopCause::ListenerFailed,
                    signal: None,
                },
                false,
            );
        }
    };
    if !retired.complete() {
        return settle_failed_host(
            session,
            run,
            RunError::unavailable(
                "session cleanup incomplete; re-run down; no complete settlement was acknowledged",
            ),
            WorkloadOutcome::Cancelled {
                cause: effigy_kernel::ctx::StopCause::ListenerFailed,
                signal: None,
            },
            false,
        );
    }
    let terminal = match retired.cause {
        RetirementCause::Stopped | RetirementCause::Down | RetirementCause::Settled => {
            run.finish(WorkloadOutcome::Succeeded)
        }
        RetirementCause::OwnerLost => run.interrupt(
            &cleanup,
            WorkloadOutcome::Cancelled {
                cause: effigy_kernel::ctx::StopCause::OwnerLost,
                signal: None,
            },
        ),
        RetirementCause::ListenerFailed => run.interrupt(
            &cleanup,
            WorkloadOutcome::Cancelled {
                cause: effigy_kernel::ctx::StopCause::ListenerFailed,
                signal: None,
            },
        ),
    };
    let failed = || WorkloadOutcome::Cancelled {
        cause: effigy_kernel::ctx::StopCause::ListenerFailed,
        signal: None,
    };
    if let Err(error) = terminal {
        return settle_failed_host(session, run, error, failed(), false);
    }
    match run.settle_workspace_recoverable(&cleanup, &mut adapter) {
        Ok(settled) => {
            session.closed(&settled);
            Ok(())
        }
        Err((error, retained)) => settle_failed_host(session, *retained, error, failed(), true),
    }
}

/// Wait while retaining the exact owners. A persistent lost pipe is one
/// transition, not permission to retry cleanup indefinitely.
pub(super) fn await_cleanup_recovery(
    session: &mut EngineSession,
    run: &mut Run<'_>,
    owner_loss_handled: &mut bool,
) {
    loop {
        // A broken control observation grants no cleanup authority. Keep
        // both owners; a later exact request can still be observed.
        if let Ok(control) = session.control(run, &[]) {
            match control.stop {
                Some(effigy_kernel::ctx::StopCause::OwnerLost) if !*owner_loss_handled => {
                    *owner_loss_handled = true;
                    return;
                }
                Some(effigy_kernel::ctx::StopCause::OwnerLost) | None => {}
                Some(_) => return,
            }
        }
        std::thread::sleep(TURN);
    }
}

fn retire_refused_service<Ad, Rs, Ob, Id>(
    session: &mut EngineSession,
    run: &mut Run<'_>,
    mut service: ServiceOwner<'_, Ad, Rs, Ob, Id>,
    primary: RunError,
    fail_first_cleanup: bool,
    cause: RetirementCause,
    initial_failure: Option<RunError>,
) -> RunError
where
    Ad: effigy_kernel::workspace_channel::Admission,
    Rs: effigy_kernel::workspace_channel::Resolver,
    Ob: effigy_kernel::workspace_channel::Observer,
    Id: effigy_kernel::workspace_channel::RunIds,
{
    let mut cleanup = if fail_first_cleanup {
        crate::runner::task_engine::cleanup_context(Duration::ZERO)
    } else {
        cleanup_budget()
    };
    let mut owner_loss_handled = false;
    let mut previous_failure = initial_failure.as_ref().map(ToString::to_string);
    if let Some(error) = initial_failure {
        session.refused_message(error.category(), format!(
            "{primary}; cleanup failed: {error}; exact owner retained (supervisor_run={}, session_run={}); request stop for that supervisor run to retry cleanup",
            run.id(), service.session().run(),
        ));
        await_cleanup_recovery(session, run, &mut owner_loss_handled);
        cleanup = cleanup_budget();
    }
    loop {
        let session_run = service.session().run();
        match service.retire_for_recoverable(cause, &cleanup, &mut ServingSettlement) {
            Ok(retired) => {
                if !retired.complete() {
                    return RunError::unavailable(format!(
                        "{primary}; session cleanup incomplete (session_run={session_run}); re-run down",
                    ));
                }
                return match previous_failure {
                    Some(failure) => RunError::unavailable(format!(
                        "{primary}; initial cleanup failed: {failure}; exact session cleanup recovered",
                    )),
                    None => primary,
                };
            }
            Err((error, retained)) => {
                previous_failure = Some(error.to_string());
                service = *retained;
                session.refused_message(error.category(), format!(
                    "{primary}; cleanup failed: {error}; exact owner retained (supervisor_run={}, session_run={session_run}); listener={} key_held={} serving={}; request stop for that supervisor run to retry cleanup",
                    run.id(), service.lease().endpoint(), service.key_held(), service.serving(),
                ));
                await_cleanup_recovery(session, run, &mut owner_loss_handled);
                cleanup = cleanup_budget();
            }
        }
    }
}

/// Preserve host-run custody and both errors rather than dropping a failed
/// settlement or reporting its primary refusal as successful cleanup.
fn settle_refused_host(
    session: EngineSession,
    run: Run<'_>,
    primary: RunError,
) -> Result<(), RunError> {
    settle_failed_host(session, run, primary, WorkloadOutcome::NotStarted, false)
}

fn settle_failed_host(
    mut session: EngineSession,
    mut run: Run<'_>,
    primary: RunError,
    workload: WorkloadOutcome,
    defer_first_attempt: bool,
) -> Result<(), RunError> {
    let mut owner_loss_handled = false;
    if defer_first_attempt {
        session.refused_message(primary.category(), format!(
            "cleanup failed: {primary}; exact supervisor owner retained (supervisor_run={}); request stop for that run to retry cleanup", run.id(),
        ));
        await_cleanup_recovery(&mut session, &mut run, &mut owner_loss_handled);
    }
    loop {
        let cleanup = cleanup_budget();
        let mark_error = match run.phase() {
            Ok(effigy_kernel::run::Phase::Stopping) => None,
            Ok(_) => run.interrupt(&cleanup, workload.clone()).err(),
            Err(error) => Some(error),
        };
        let run_id = run.id();
        match run.settle_workspace_recoverable(&cleanup, &mut ServingSettlement) {
            Ok(outcome) => {
                let error = if let Some(mark_error) = mark_error {
                    RunError::unavailable(format!(
                        "{primary}; recording refusal failed: {mark_error}"
                    ))
                } else if outcome.cleanup() == effigy_kernel::outcome::CleanupOutcome::Incomplete {
                    RunError::unavailable(format!(
                        "{primary}; supervisor {run_id} cleanup incomplete; re-run down"
                    ))
                } else {
                    primary
                };
                session.refused_message(error.category(), error.to_string());
                session.closed(&outcome);
                return Err(error);
            }
            Err((error, retained)) => {
                run = *retained;
                let mark_error = mark_error
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "none".into());
                session.refused_message(error.category(), format!(
                    "{primary}; recording refusal error: {mark_error}; cleanup failed: {error}; exact supervisor owner retained (supervisor_run={run_id}); request stop for that run to retry cleanup",
                ));
                await_cleanup_recovery(&mut session, &mut run, &mut owner_loss_handled);
            }
        }
    }
}

/// The selection state a session's own run durably recorded, read back from the
/// journal. Absent means the activation never recorded a switch, which is never
/// servable.
fn durable_selection(
    observe: &effigy_kernel::ctx::ObserveContext,
    root: &StateRoot,
    session: &effigy_kernel::workspace_session::SessionRef,
) -> Result<
    (
        effigy_kernel::workspace_session::SelectionState,
        Option<effigy_kernel::workspace_session::ServingLease>,
    ),
    RunError,
> {
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
    // The kernel's own committed selection is the fence. It exists once an
    // activation commit wrote it; a disposable activation has none by
    // construction, so only in a fixture build does the host fall back to the
    // record that activation wrote on the session's own run.
    match effigy_kernel::workspace_session::committed_selection(&reader, session.environment()) {
        Observed::Verified(Some(obligation), _) => {
            if obligation.session_run() != session.run() {
                return Err(RunError::refused(
                    "the committed selection names another session; nothing is served",
                ));
            }
            // The durable receipt's serving lease is what may be served: the
            // committed lease and the live lease must be one fact, so a custody
            // that published another lease for the same session is refused.
            return Ok((
                obligation.state(),
                Some(obligation.receipt().serving().clone()),
            ));
        }
        Observed::Verified(None, _) => {}
        Observed::Unknown(reason) => {
            return Err(RunError::unavailable(format!(
                "the committed workspace selection is unreadable: {reason}"
            )))
        }
        Observed::Absent(_) => {}
    }
    #[cfg(not(feature = "workspace-serving-fixture"))]
    {
        let _ = (reader, session);
        Err(RunError::refused(
            "the workspace has no committed selection; nothing is served",
        ))
    }
    #[cfg(feature = "workspace-serving-fixture")]
    {
        fixture_selection(&reader, session).map(|state| (state, None))
    }
}

/// The record a disposable activation writes on the session's own run. Only a
/// fixture build reads it, and only when the kernel has no committed selection
/// for the environment: it is what a disposable activation can record without a
/// container activation commit.
#[cfg(feature = "workspace-serving-fixture")]
fn fixture_selection(
    reader: &effigy_kernel::store::StoreReader,
    session: &effigy_kernel::workspace_session::SessionRef,
) -> Result<effigy_kernel::workspace_session::SelectionState, RunError> {
    use effigy_kernel::workspace_session::SelectionState;
    let effects = match effigy_kernel::run::effects(reader, session.run()) {
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
fn serve_turn<'a, Ad, Rs, Ob, Id>(
    service: &mut ServiceOwner<'a, Ad, Rs, Ob, Id>,
    streams: &mut impl Streams,
) -> Result<bool, ServeError>
where
    Ad: effigy_kernel::workspace_channel::Admission,
    Rs: effigy_kernel::workspace_channel::Resolver,
    Ob: effigy_kernel::workspace_channel::Observer,
    Id: effigy_kernel::workspace_channel::RunIds,
{
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
