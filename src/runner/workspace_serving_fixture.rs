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

/// Read one request from stdin, initiate it and print the result.
pub fn initiate_from_stdin() -> Result<String, String> {
    use std::io::Read;
    let mut body = String::new();
    std::io::stdin()
        .read_to_string(&mut body)
        .map_err(|error| format!("the request could not be read: {error}"))?;
    initiate(body.trim())
}
