//! Test-only bridge for the workspace-serving process-lifetime proof
//! (effigy#507; `workspace-serving-fixture` feature only).
//!
//! Two things a disposable process needs from this crate, and nothing else:
//! the initiating front end that starts one detached serving host and returns,
//! and the detached supervisor itself (the binary's `__kernel-supervise` entry
//! routes a serving payload here through the engine dispatch).
//!
//! Neither adds authority. The activation it runs is the disposable one
//! (`DisposableActivation`), which creates a real session run, listener, key and
//! grant book but no container and no prepared guest: it proves process custody,
//! not container-mode preparation, and no product build compiles it.

use std::time::Duration;

use effigy_kernel::ctx::OpenReason;
use effigy_kernel::root::StateRoot;

use super::task_engine;
use super::workspace_client::serving_host::{self, ServingHostRequest};

/// Start one disposable serving host, wait for its committed session, then
/// acknowledge the kernel detach and return that result as JSON. The caller is
/// expected to exit: the host keeps serving in its own process.
pub fn initiate(request_json: &str) -> Result<String, String> {
    initiate_with(request_json, true)
}

/// The same launch, but the front end exits **without** acknowledging the
/// detach: the kernel's owner-loss path. The host must stop serving and settle
/// instead of claiming the session it announced.
pub fn initiate_abandoning(request_json: &str) -> Result<String, String> {
    initiate_with(request_json, false)
}

/// The same launch, but with the caller's own startup budget, reported so a
/// proof can show which deadline actually bounded the wait.
pub fn initiate_with_budget(request_json: &str, budget_ms: u64) -> Result<String, String> {
    initiate_with_budget_parts(request_json, budget_ms)
}

fn initiate_with_budget_parts(request_json: &str, budget_ms: u64) -> Result<String, String> {
    let budget = Duration::from_millis(budget_ms);
    let spent = std::time::Instant::now();
    let request: ServingHostRequest =
        serde_json::from_str(request_json).map_err(|error| format!("the request: {error}"))?;
    // The caller's budget bounds the WHOLE startup, not only the announcement
    // wait: the state root, the launch and the supervisor's admission are all
    // the caller's wait, so a host that cannot be serving within it is a
    // startup that ran out of the caller's deadline.
    let cx = task_engine::effect_context(OpenReason::DevServer).child(budget);
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(&request.project_path),
    };
    let root = StateRoot::checkout_effects(&cx, &request.state_home, &location)
        .map_err(|error| format!("the state root: {error}"))?;
    let executable = task_engine::startup_executable().map_err(|error| error.to_string())?;
    let mut host = serving_host::start(&cx, &root, &request, &executable).map_err(|error| {
        startup_failure(
            spent,
            spent.elapsed(),
            Duration::ZERO,
            budget,
            format!("the launch: {error}"),
        )
    })?;
    // The launch is process startup; the wait for the announcement is the part
    // the caller's budget is allowed to bound. They are reported separately so a
    // proof can assert the mechanism without asserting how fast a process starts.
    let launch = spent.elapsed();
    let remaining = budget.saturating_sub(launch);
    let waiting = std::time::Instant::now();
    let published = host.await_serving(&cx, remaining).map_err(|error| {
        startup_failure(
            spent,
            launch,
            waiting.elapsed(),
            budget,
            format!("the startup result: {error}"),
        )
    })?;
    serde_json::to_string(&serde_json::json!({ "published": published }))
        .map_err(|error| format!("the result: {error}"))
}

/// One startup failure under the caller's own budget. When the budget is what
/// ended the startup, the report says the caller's deadline expired before the
/// host announced its session, whatever stage the launch had reached: that is
/// the caller's answer, and it names the deadline rather than an inner step.
fn startup_failure(
    spent: std::time::Instant,
    launch: Duration,
    wait: Duration,
    budget: Duration,
    detail: String,
) -> String {
    let message = if spent.elapsed() >= budget {
        "the workspace serving host did not announce its session in time".to_owned()
    } else {
        detail
    };
    // `wait` is what the caller's budget bounded, `launch` is process startup and
    // `deadline` is the whole attempt, so a caller can tell them apart.
    format!(
        "wait={}ms launch={}ms deadline={}ms: {message}",
        wait.as_millis(),
        launch.as_millis(),
        spent.elapsed().as_millis()
    )
}

/// The same launch, but the front end crashes *before* it reads any startup
/// result: an interrupted handoff, where the owner never learns a session and the
/// host must settle without claiming one.
pub fn initiate_and_drop_before_result(request_json: &str) -> Result<String, String> {
    let request: ServingHostRequest =
        serde_json::from_str(request_json).map_err(|error| format!("the request: {error}"))?;
    let cx = task_engine::effect_context(OpenReason::DevServer);
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(&request.project_path),
    };
    let root = StateRoot::checkout_effects(&cx, &request.state_home, &location)
        .map_err(|error| format!("the state root: {error}"))?;
    let executable = task_engine::startup_executable().map_err(|error| error.to_string())?;
    let host = serving_host::start(&cx, &root, &request, &executable)
        .map_err(|error| format!("the launch: {error}"))?;
    // Name the supervisor run before the handoff is lost. The claim is still
    // nothing: no session was read, so stdout stays empty. The owner goes away
    // with the handoff in flight.
    eprintln!("supervisor_run={}", host.run());
    drop(host);
    std::process::abort();
}

/// The same launch, but the front end crashes *after* acknowledging the detach:
/// the acknowledgement is durable, so the host must keep serving a crashed
/// client's session.
pub fn initiate_then_crash(request_json: &str) -> Result<String, String> {
    use std::io::Write;
    let result = initiate_with(request_json, true)?;
    // The front end prints the committed session it acknowledged, then crashes:
    // a dead client, not a clean exit, is what the host must survive.
    println!("{result}");
    let _ = std::io::stdout().flush();
    std::process::abort();
}

fn initiate_with(request_json: &str, detach: bool) -> Result<String, String> {
    let request: ServingHostRequest =
        serde_json::from_str(request_json).map_err(|error| format!("the request: {error}"))?;
    let cx = task_engine::effect_context(OpenReason::DevServer);
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(&request.project_path),
    };
    let root = StateRoot::checkout_effects(&cx, &request.state_home, &location)
        .map_err(|error| format!("the state root: {error}"))?;
    let executable = task_engine::startup_executable().map_err(|error| error.to_string())?;
    let mut host = serving_host::start(&cx, &root, &request, &executable)
        .map_err(|error| format!("the launch: {error}"))?;
    // A refused front end still leaves its supervisor settling in its own
    // process; name that run so a proof can wait for the settle.
    let supervisor_run = host.run();
    let published = host
        .await_serving(&cx, Duration::from_secs(120))
        .map_err(|error| {
            format!("the startup result: {error} (supervisor_run={supervisor_run})")
        })?;
    let supervisor_run = if detach {
        host.detach(&cx)
            .map_err(|error| format!("the detach: {error}"))?
    } else {
        // No acknowledgement: dropping the front end here is owner loss.
        let run = host.run();
        drop(host);
        run
    };
    serde_json::to_string(&serde_json::json!({
        "supervisor_run": supervisor_run.to_string(),
        "published": published,
        "detached": detach,
    }))
    .map_err(|error| format!("the result: {error}"))
}

/// Read one request from stdin, launch it without acknowledging the detach and
/// print the result.
pub fn initiate_abandoning_from_stdin() -> Result<String, String> {
    use std::io::Read;
    let mut body = String::new();
    std::io::stdin()
        .read_to_string(&mut body)
        .map_err(|error| format!("the request could not be read: {error}"))?;
    initiate_abandoning(body.trim())
}

/// Read `{ "request": <request>, "budget_ms": <n> }` from stdin and start it
/// under that budget.
pub fn initiate_deadline_from_stdin() -> Result<String, String> {
    use std::io::Read;
    let mut body = String::new();
    std::io::stdin()
        .read_to_string(&mut body)
        .map_err(|error| format!("the request could not be read: {error}"))?;
    let value: serde_json::Value =
        serde_json::from_str(body.trim()).map_err(|error| format!("the request: {error}"))?;
    let budget = value
        .get("budget_ms")
        .and_then(|budget| budget.as_u64())
        .ok_or_else(|| "the request names no budget".to_owned())?;
    let request = value
        .get("request")
        .ok_or_else(|| "the request names no serving request".to_owned())?;
    initiate_with_budget(&request.to_string(), budget)
}

/// Read one request from stdin and drop the handoff before reading its result.
pub fn initiate_and_drop_stdin() -> Result<String, String> {
    use std::io::Read;
    let mut body = String::new();
    std::io::stdin()
        .read_to_string(&mut body)
        .map_err(|error| format!("the request could not be read: {error}"))?;
    initiate_and_drop_before_result(body.trim())
}

/// Read one request from stdin, initiate it and crash after the detach.
pub fn initiate_then_crash_stdin() -> Result<String, String> {
    use std::io::Read;
    let mut body = String::new();
    std::io::stdin()
        .read_to_string(&mut body)
        .map_err(|error| format!("the request could not be read: {error}"))?;
    initiate_then_crash(body.trim())
}

/// Read one request from stdin, initiate it and print the result.
pub fn initiate_from_stdin() -> Result<String, String> {
    use std::io::Read;
    let mut body = String::new();
    std::io::stdin()
        .read_to_string(&mut body)
        .map_err(|error| format!("the request could not be read: {error}"))?;
    initiate(body.trim())
}

/// What a third process observed about a host it does not own: the pinned TLS
/// identity it verified, and whether the retained grant book accepted the
/// capability the host issued.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthorityProbe {
    /// The retained listener answered this process's pinned TLS handshake and
    /// protocol greeting.
    pub negotiated: bool,
    /// The retained grant book ANSWERED the fixture's probe request with the
    /// exact expected body and this request's id. Silence, EOF or a timeout fails.
    pub answered: bool,
    /// The unissued capability was REFUSED on its own connection with a typed
    /// refusal frame. Silence, EOF or a timeout fails.
    pub refused_unissued: bool,
    /// The fixture dispatcher recorded whether its observe seam was reached, so a
    /// missing reply is traced rather than guessed.
    pub trace: Vec<String>,
}

/// Authenticate against a running host from this process: the pinned TLS
/// handshake, the protocol greeting, then the capability. Nothing here is
/// fixture-owned state — the transport verifies the pin and the host answers from
/// its own retained grant book, so a `true` proves the key and the grants are
/// still the ones the activation published.
pub fn probe_authority(
    endpoint: &str,
    pin: &str,
    capability: &str,
    session_run: &str,
    trace_path: &std::path::Path,
) -> Result<AuthorityProbe, String> {
    use effigy_kernel::id::{Digest, Generation, RunId};
    use effigy_kernel::workspace_channel::{Frame, Hello, Secret, PROTOCOL_V1, SCHEMA};

    let address = match endpoint
        .strip_prefix("effigy-host:")
        .unwrap_or(endpoint)
        .parse::<std::net::SocketAddr>()
    {
        Ok(address) => address,
        Err(_) => {
            let port = endpoint
                .strip_prefix("effigy-host:")
                .ok_or_else(|| format!("`{endpoint}` is not a published workspace endpoint"))?
                .parse::<u16>()
                .map_err(|_| format!("`{endpoint}` names no port"))?;
            std::net::SocketAddr::from(([127, 0, 0, 1], port))
        }
    };
    let pin =
        Digest::parse_hex(pin).ok_or_else(|| "the published pin is not a digest".to_owned())?;
    let capability = Secret::parse(capability).map_err(|error| error.to_string())?;
    let session: RunId = session_run
        .parse()
        .map_err(|_| "the published session run is not a run id".to_owned())?;
    let trace_path = trace_path.to_path_buf();
    let mut transport = crate::runner::workspace_transport::TlsChannelTransport::connect(
        address,
        pin.clone(),
        std::time::Duration::from_secs(5),
    )
    .map_err(|error| format!("the pinned connection failed: {error}"))?;
    write_answer(
        &mut transport,
        &Frame::Hello(Hello {
            schema: SCHEMA.to_owned(),
            protocol_versions: vec![PROTOCOL_V1],
            effigy_version: "fixture".to_owned(),
            binary_digest: Digest([0; 32]),
            context_generation: Generation::new(1),
            session_run_id: session,
        }),
    )?;
    let hello = read_answer(&mut transport)?;
    let negotiated = matches!(hello, Some(Frame::HelloOk(_)));
    if !negotiated {
        let _ = hello;
        return Ok(AuthorityProbe {
            negotiated: false,
            answered: false,
            refused_unissued: false,
            trace: Vec::new(),
        });
    }
    // The capability is presented, then the fixture's probe operation is sent.
    // The reply must be THIS request's answer: silence, EOF, a timeout, a read
    // error, a refusal or another request's answer all fail.
    write_answer(&mut transport, &Frame::Authenticate(capability))?;
    let _ = read_answer(&mut transport)?;
    let request_id = effigy_kernel::run::RequestId::new([0x5a; 16]);
    write_answer(
        &mut transport,
        &Frame::Request(effigy_kernel::workspace_channel::Request {
            id: request_id,
            context_generation: effigy_kernel::id::Generation::new(1),
            environment_generation: effigy_kernel::id::Generation::new(1),
            session_run_id: session,
            container_id: "fixture".to_owned(),
            vm_boot_id: "fixture".to_owned(),
            body: effigy_kernel::workspace_channel::RequestBody::Observe(
                effigy_kernel::workspace_channel::ObserveBody {
                    run_id: None,
                    op: crate::runner::workspace_client::serving_fixture::PROBE_OPERATION
                        .to_owned(),
                    target: Some(effigy_kernel::workspace_channel::Target::Host),
                },
            ),
        }),
    )?;
    let answered = match read_answer_within(&mut transport, std::time::Duration::from_secs(10))? {
        Some(effigy_kernel::workspace_channel::Frame::Response(response))
            if response.id == request_id =>
        {
            response.outcome
                == effigy_kernel::workspace_channel::ResponseOutcome::Observed(
                    serde_json::json!({"answer": "retained-grant-authorised"}),
                )
        }
        _ => false,
    };
    drop(transport);

    // The unissued control, on its own connection: a capability the host never
    // issued must be refused with a typed refusal. Silence fails the proof.
    let refused_unissued = unissued_is_refused(address, &pin, session)?;

    // The dispatcher's own trace, so a missing reply says where it stopped.
    let trace = std::fs::read_to_string(&trace_path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    Ok(AuthorityProbe {
        negotiated,
        answered,
        refused_unissued,
        trace,
    })
}

/// Present a capability the host never issued and require a typed refusal.
fn unissued_is_refused(
    address: std::net::SocketAddr,
    pin: &effigy_kernel::id::Digest,
    session: effigy_kernel::id::RunId,
) -> Result<bool, String> {
    use effigy_kernel::workspace_channel::{Frame, Hello, PROTOCOL_V1, SCHEMA};
    let mut transport = crate::runner::workspace_transport::TlsChannelTransport::connect(
        address,
        pin.clone(),
        std::time::Duration::from_secs(5),
    )
    .map_err(|error| format!("the control connection failed: {error}"))?;
    write_answer(
        &mut transport,
        &Frame::Hello(Hello {
            schema: SCHEMA.to_owned(),
            protocol_versions: vec![PROTOCOL_V1],
            effigy_version: "fixture".to_owned(),
            binary_digest: effigy_kernel::id::Digest([0; 32]),
            context_generation: effigy_kernel::id::Generation::new(1),
            session_run_id: session,
        }),
    )?;
    if !matches!(read_answer(&mut transport)?, Some(Frame::HelloOk(_))) {
        return Err("the control connection did not negotiate".to_owned());
    }
    write_answer(
        &mut transport,
        &Frame::Authenticate(effigy_kernel::workspace_channel::Secret::new([0x00; 32])),
    )?;
    match read_answer_within(&mut transport, std::time::Duration::from_secs(10))? {
        Some(Frame::Refuse(_)) => Ok(true),
        _ => Ok(false),
    }
}

/// One competing process's report: whether it was refused, and why.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CompetingOwner {
    pub refused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A competing **detached serving host**: a second process starts the production
/// launcher against the same checkout and environment identity, and the
/// activation must refuse. A successful start would be a second owner, so it is
/// stopped immediately and reported as failed contention.
pub fn compete_detached(request_json: &str) -> Result<CompetingOwner, String> {
    let request: ServingHostRequest =
        serde_json::from_str(request_json).map_err(|error| format!("the request: {error}"))?;
    let cx = task_engine::effect_context(OpenReason::DevServer);
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(&request.project_path),
    };
    let root = StateRoot::checkout_effects(&cx, &request.state_home, &location)
        .map_err(|error| format!("the state root: {error}"))?;
    let executable = task_engine::startup_executable().map_err(|error| error.to_string())?;
    // The kernel's own owner gate is the fencing: a second owner for the same
    // workspace is refused there, before it can launch anything. Only a typed
    // refusal counts; an unavailable or broken launcher proves nothing.
    let mut host = match serving_host::start(&cx, &root, &request, &executable) {
        Ok(host) => host,
        Err(error) => {
            // Only the kernel's own owner-gate refusal is a second-owner refusal.
            // Any other launcher error proves nothing and must FAIL the proof.
            return Ok(CompetingOwner {
                refused: is_owner_gate_refusal(&error),
                reason: Some(error.to_string()),
            });
        }
    };
    match host.await_serving(&cx, Duration::from_secs(20)) {
        // Only a typed refusal of the second owner counts. An unavailable or
        // timed-out contender proves nothing, so it is reported as not-refused.
        Err(error) => Ok(CompetingOwner {
            refused: is_owner_gate_refusal(&error),
            reason: Some(error.to_string()),
        }),
        Ok(published) => {
            let _ = host.stop(&cx);
            Ok(CompetingOwner {
                refused: false,
                reason: Some(format!(
                    "a second detached owner announced session {}",
                    published.session_run
                )),
            })
        }
    }
}

/// Whether one contender outcome is the kernel's own owner-gate refusal: the
/// workspace task is held by the live session, so a second owner is refused
/// before it can launch anything. Nothing else counts as a second-owner refusal.
pub(crate) fn is_owner_gate_refusal(error: &effigy_kernel::run::RunError) -> bool {
    error.category() == effigy_kernel::human::Category::Refused
        && error.to_string().contains("is held by")
}

/// The client identity this process resolves for capability storage. Two commands
/// in one terminal share it; another terminal does not.
pub fn client_identity() -> Result<String, String> {
    crate::runner::workspace_client::client_id().map_err(|error| error.to_string())
}

/// The identity one controlling terminal (name plus device numbers) or one
/// session-only client resolves. Kept behind the fixture feature so the terminal
/// boundary is provable without a pty.
pub fn terminal_identity(device: Option<u64>, session: i64) -> Result<String, String> {
    crate::runner::workspace_client::terminal_client_identity(device, session as libc::pid_t)
        .map_err(|error| error.to_string())
}

/// Print this process's client identity.
pub fn client_identity_stdin() -> Result<String, String> {
    client_identity()
}

/// Read one request from stdin and start a competing detached host for it.
pub fn compete_detached_stdin() -> Result<String, String> {
    use std::io::Read;
    let mut body = String::new();
    std::io::stdin()
        .read_to_string(&mut body)
        .map_err(|error| format!("the request could not be read: {error}"))?;
    compete_detached(body.trim())
        .and_then(|competing| serde_json::to_string(&competing).map_err(|error| error.to_string()))
}

fn write_answer(
    transport: &mut crate::runner::workspace_transport::TlsChannelTransport,
    frame: &effigy_kernel::workspace_channel::Frame,
) -> Result<(), String> {
    use effigy_kernel::workspace_channel::Transport;
    let body = effigy_kernel::workspace_channel::encode_frame(frame)
        .map_err(|refusal| refusal.to_string())?;
    let mut framed = Vec::with_capacity(body.len() + 4);
    framed.extend_from_slice(&(body.len() as u32).to_be_bytes());
    framed.extend_from_slice(&body);
    transport
        .write_all(&framed)
        .map_err(|error| format!("the frame could not be written: {error}"))
}

fn read_answer(
    transport: &mut crate::runner::workspace_transport::TlsChannelTransport,
) -> Result<Option<effigy_kernel::workspace_channel::Frame>, String> {
    read_answer_within(transport, std::time::Duration::from_millis(600))
}

fn read_answer_within(
    transport: &mut crate::runner::workspace_transport::TlsChannelTransport,
    budget: std::time::Duration,
) -> Result<Option<effigy_kernel::workspace_channel::Frame>, String> {
    use effigy_kernel::workspace_channel::Transport;
    let until = std::time::Instant::now() + budget;
    let mut buffer = Vec::new();
    loop {
        if buffer.len() >= 4 {
            let size = u32::from_be_bytes(buffer[..4].try_into().expect("four bytes")) as usize;
            if buffer.len() >= size + 4 {
                return effigy_kernel::workspace_channel::decode_frame(&buffer[4..size + 4])
                    .map(Some)
                    .map_err(|refusal| refusal.to_string());
            }
        }
        if std::time::Instant::now() > until {
            return Ok(None);
        }
        let mut chunk = [0u8; 4096];
        match transport.read(&mut chunk) {
            Ok(0) => return Ok(None),
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return Ok(None),
        }
    }
}
