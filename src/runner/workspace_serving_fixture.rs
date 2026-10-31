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
    let started = std::time::Instant::now();
    match initiate_with_budget_parts(request_json, budget_ms) {
        Ok(result) => Ok(result),
        Err(error) => Err(format!(
            "deadline={}ms: {error}",
            started.elapsed().as_millis()
        )),
    }
}

fn initiate_with_budget_parts(request_json: &str, budget_ms: u64) -> Result<String, String> {
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
    let published = host
        .await_serving(&cx, Duration::from_millis(budget_ms))
        .map_err(|error| format!("the startup result: {error}"))?;
    serde_json::to_string(&serde_json::json!({ "published": published }))
        .map_err(|error| format!("the result: {error}"))
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
    let published = host
        .await_serving(&cx, Duration::from_secs(120))
        .map_err(|error| format!("the startup result: {error}"))?;
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
    pub negotiated: bool,
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
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
    let mut transport = crate::runner::workspace_transport::TlsChannelTransport::connect(
        address,
        pin,
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
        return Ok(AuthorityProbe {
            negotiated: false,
            authenticated: false,
            refusal: hello.map(|frame| format!("{frame:?}")),
        });
    }
    write_answer(&mut transport, &Frame::Authenticate(capability))?;
    // An accepted credential is answered with nothing; a refused one answers once.
    let refusal = read_answer(&mut transport)?;
    Ok(AuthorityProbe {
        negotiated: true,
        authenticated: refusal.is_none(),
        refusal: refusal.map(|frame| format!("{frame:?}")),
    })
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
    use effigy_kernel::workspace_channel::Transport;
    let until = std::time::Instant::now() + std::time::Duration::from_millis(600);
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
