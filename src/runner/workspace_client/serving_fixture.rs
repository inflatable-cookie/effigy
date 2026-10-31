//! The disposable activation the process-lifetime proof runs inside the
//! detached serving host (`workspace-serving-fixture` feature only).
//!
//! It performs the *same* transaction shape the container-up producer performs:
//! the environment is activated at one exact generation, the session run is
//! admitted as a `Detached` run borrowing it, the session's private listener and
//! key are launched under that run's custody, the locator is qualified and
//! published, and the serving owner is built from the published lease and a
//! grant book. Nothing is prepared, no container exists and no guest is
//! involved: this proves process custody, never container-mode preparation or a
//! native backend.
//!
//! It is compiled only when the `workspace-serving-fixture` feature is on, which
//! no product build enables. Its one live dependency is the kernel's own
//! fixture-only active-publication transaction (`effigy-kernel/test-support`),
//! which writes exactly the row an activation commit writes for the same owner
//! fence.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use effigy_kernel::ctx::{EffectContext, ObserveContext};
use effigy_kernel::id::{
    Digest, ExecutionTarget, Generation, InvocationSpec, ResolvedInvocation, Route, RunId, Surface,
    TaskId,
};
use effigy_kernel::observe::{Evidence, Observed, UnknownReason};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{BeginOpts, Fence, Run, RunError, Session};
use effigy_kernel::workspace_channel::{
    self as channel, Dispatcher, GrantBook, HostArtifact, Negotiation, Secret, SessionFence, Target,
};
use effigy_kernel::workspace_session::{
    ChannelListener, ServingProbe, SessionLaunch, SessionRef, SessionSupervisor,
};

use super::host_dispatch::HostDispatch;
use super::serving::ServingOwner;
use super::serving_host::{
    cleanup_budget, recovery_invocation, DisposableActivation, ServingHostRequest,
    ServingSettlement,
};
use super::supervisor::ServiceOwner;
use crate::runner::workspace_transport::listener::SessionTlsListener;
use crate::runner::workspace_transport::TlsChannelTransport;

fn evidence() -> Evidence {
    Evidence::new("disposable workspace serving activation".to_owned(), None)
        .expect("fixed evidence")
}

/// A probe that answers the launch's qualification by dialling the endpoint the
/// launch bound, with the pin the launch holds: the real pinned TLS handshake,
/// against this process's own listener.
struct SelfDial {
    budget: Duration,
}

impl ServingProbe for SelfDial {
    fn pinned_handshake(
        &mut self,
        endpoint: SocketAddr,
        pin: &Digest,
        deadline: Duration,
    ) -> Observed<()> {
        let budget = deadline.min(self.budget);
        match TlsChannelTransport::connect(endpoint, pin.clone(), budget) {
            Ok(transport) => {
                drop(transport);
                Observed::Verified((), evidence())
            }
            Err(error) => Observed::Unknown(UnknownReason::Unparsable(format!(
                "the disposable listener did not answer its own pinned handshake: {error}"
            ))),
        }
    }
}

fn checkout(request: &ServingHostRequest) -> effigy_kernel::id::CheckoutId {
    effigy_kernel::id::CheckoutId::new(
        effigy_kernel::id::CheckoutKind::EphemeralClone,
        request.checkout,
    )
}

fn environment_invocation(request: &ServingHostRequest) -> Result<ResolvedInvocation, RunError> {
    let environment = super::serving_host::environment_resource(request)?;
    let task = TaskId::new(
        checkout(request),
        "",
        Surface::Published,
        "workspace-environment",
        Some(&request.profile),
    )
    .map_err(|error| RunError::refused(format!("the fixture environment task: {error}")))?;
    ResolvedInvocation::new(InvocationSpec {
        owner: environment,
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
        class: effigy_kernel::ctx::EffectClass::Mutate,
        placement: effigy_kernel::ctx::Placement::Host,
        provenance: std::collections::BTreeMap::new(),
    })
    .map_err(|error| RunError::refused(format!("the fixture environment invocation: {error}")))
}

fn fresh_run_id(byte: u8) -> Result<RunId, RunError> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    RunId::new(millis, [byte; 10])
        .map_err(|error| RunError::broken(format!("a fixture run id: {error}")))
}

fn session_invocation(request: &ServingHostRequest) -> Result<ResolvedInvocation, RunError> {
    let task = TaskId::new(
        checkout(request),
        "",
        Surface::Published,
        "workspace",
        Some(&request.profile),
    )
    .map_err(|error| RunError::refused(format!("the fixture session task: {error}")))?;
    ResolvedInvocation::new(InvocationSpec {
        owner: task.resource(),
        task,
        route: Route::Task,
        argv: vec![
            std::ffi::OsString::from("effigy"),
            std::ffi::OsString::from("workspace"),
        ],
        cwd: String::new(),
        target: ExecutionTarget::Host,
        env: std::collections::BTreeMap::new(),
        gates: Vec::new(),
        attachments: Vec::new(),
        class: effigy_kernel::ctx::EffectClass::Mutate,
        placement: effigy_kernel::ctx::Placement::Host,
        provenance: std::collections::BTreeMap::new(),
    })
    .map_err(|error| RunError::refused(format!("the fixture session invocation: {error}")))
}

/// The disposable grant book: one capability bound to this session, scoped to the
/// session's own operations on the host target.
fn serving_server(
    request: &ServingHostRequest,
    disposable: &DisposableActivation,
    session: &SessionRef,
    pin: Digest,
    root: &StateRoot,
) -> Result<channel::Server<HostDispatch, HostDispatch, HostDispatch, HostDispatch>, RunError> {
    let capability = Secret::parse(&disposable.capability)
        .map_err(|_| RunError::refused("the disposable capability is not a capability"))?;
    let kernel_session = Session {
        run: session.run(),
        generation: session.generation(),
    };
    let mut grants = GrantBook::new();
    grants.issue(
        capability,
        channel::GrantBinding {
            host_user: "fixture".to_owned(),
            checkout: request.environment.clone(),
            session: kernel_session,
            environment_generation: session.environment_generation(),
            container_id: "fixture".to_owned(),
            vm_boot_id: "fixture".to_owned(),
        },
        channel::session_operations(),
        [Target::Host].into_iter().collect(),
    );
    let context_generation = Generation::new(1);
    let negotiation = Negotiation {
        context_generation,
        session_run_id: session.run(),
        artifact: HostArtifact {
            effigy_version: "fixture".to_owned(),
            binary_digest: Digest([0; 32]),
        },
    };
    let fence = SessionFence {
        context_generation,
        environment_generation: session.environment_generation(),
        session: kernel_session,
        container_id: "fixture".to_owned(),
        vm_boot_id: "fixture".to_owned(),
    };
    let host_checkout = request.project_path.clone();
    let dispatcher = Dispatcher::new(
        HostDispatch::new(host_checkout.clone(), root.clone()),
        HostDispatch::new(host_checkout.clone(), root.clone()),
        HostDispatch::new(host_checkout.clone(), root.clone()),
        HostDispatch::new(host_checkout, root.clone()),
    );
    Ok(channel::Server::new(
        negotiation,
        fence,
        pin,
        grants,
        dispatcher,
    ))
}

/// Perform the disposable activation and return the serving owner that keeps its
/// session custody.
pub(crate) fn activate<'r>(
    cx: &EffectContext,
    observe: &ObserveContext,
    root: &'r StateRoot,
    request: &ServingHostRequest,
    disposable: &DisposableActivation,
) -> Result<ServiceOwner<'r>, RunError> {
    let cleanup = cleanup_budget();
    let mut adapter = ServingSettlement;
    let environment = super::serving_host::environment_resource(request)?;
    let fence = Fence {
        resource: environment,
        generation: Generation::new(1),
    };
    // The environment's own run creates its row; the fixture then records the
    // same `active` publication an activation commit writes for this fence.
    let environment_invocation = environment_invocation(request)?;
    let environment_run = Run::begin(
        cx,
        root,
        &environment_invocation,
        BeginOpts {
            id: fresh_run_id(0x11)?,
            wait: effigy_kernel::gate::Wait::FailFast,
            acknowledge: Vec::new(),
            next: recovery_invocation()?,
            custody: None,
        },
        &cleanup,
        &mut adapter,
    )?;
    environment_run.settle(&cleanup, &mut adapter)?;
    effigy_kernel::run::publish_active_for_fixture(cx, root, &fence)?;

    // The session run borrows that environment and is a detached run, exactly as
    // a workspace session is.
    let session_invocation = session_invocation(request)?;
    let (session_run, session) = SessionRef::admit(
        cx,
        root,
        &session_invocation,
        BeginOpts {
            id: fresh_run_id(0x22)?,
            wait: effigy_kernel::gate::Wait::FailFast,
            acknowledge: Vec::new(),
            next: recovery_invocation()?,
            custody: Some(effigy_kernel::supervise::LifecyclePlan {
                executable: crate::runner::task_engine::startup_executable()
                    .map_err(|error| RunError::step_failed("the fixture startup binary", error))?,
                placement: effigy_kernel::supervise::Placement::Detached,
            }),
        },
        &cleanup,
        &mut adapter,
        &fence,
        effigy_kernel::supervise::Placement::Detached,
    )?;
    let mut supervisor = SessionSupervisor::adopt(session_run, &disposable.supervisor_id)?;

    // The session's own listener and key, launched under that run's custody.
    let budget = Duration::from_millis(disposable.tls_budget_ms.clamp(1, 5_000));
    let (listener, key, acceptor) =
        SessionTlsListener::bind(SocketAddr::from(([127u8, 0, 0, 1], 0)), budget)
            .map_err(|error| RunError::unavailable(format!("the fixture listener: {error}")))?;
    let endpoint = ChannelListener::endpoint(&listener)
        .ok_or_else(|| RunError::unavailable("the fixture listener bound no endpoint"))?;
    let pin = key.pin();
    let mut launch = SessionLaunch::launch(
        &mut supervisor,
        &session,
        &disposable.supervisor_id,
        key,
        Box::new(listener),
    )
    .map_err(|error| RunError::refused(error.to_string()))?;
    launch
        .qualify(&mut SelfDial { budget }, budget)
        .map_err(|error| RunError::unavailable(format!("the fixture qualification: {error}")))?;
    let bound = launch
        .publish(&supervisor)
        .map_err(|error| RunError::refused(format!("the fixture publication: {error}")))?;
    let lease = bound.lease().clone();
    let _ = endpoint;
    let server = serving_server(request, disposable, &session, pin, root)?;
    let owner = ServingOwner::new(
        supervisor,
        launch,
        acceptor,
        server,
        Arc::new(cx.clone()),
        Arc::new(observe.clone()),
        root,
    );
    ServiceOwner::retained(owner, session, lease)
        .map_err(|error| RunError::refused(format!("the fixture custody: {error:?}")))
}
