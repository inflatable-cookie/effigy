//! Private, read-only source-qualification utility. Input and output are
//! private pipes; the caller must never persist the registration nonce.
use std::io::{self, Read};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{CancelToken, Deadline, EffectClass, Limits, OperationContext, Placement};
use effigy_kernel::fs::DirectoryIdentity;
use effigy_kernel::id::{RunId, TaskId};
use effigy_kernel::ident::ProcessIdentity;
use effigy_kernel::location::CheckoutLocation;
use effigy_kernel::remote::{qualification_owned_registration, ContainerIdentity};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::Fence;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    checkout: PathBuf,
    checkout_identity: DirectoryIdentity,
    state_identity: DirectoryIdentity,
    run: RunId,
    task: TaskId,
    owner: ProcessIdentity,
    env: Fence,
    container: ContainerIdentity,
}

fn observe() -> Result<(), String> {
    let mut input = String::new();
    io::stdin().take(65537).read_to_string(&mut input).map_err(|_| "request unreadable")?;
    if input.len() > 65536 { return Err("request too large".into()); }
    let request: Request = serde_json::from_str(&input).map_err(|_| "invalid request")?;
    let op = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(20)),
        CancelToken::default(),
        Limits { capture_tail: 4096, log_rotate: 4096, log_keep: 2, line_max: 1024, queue_bytes: 8192, probe_concurrency: 1 },
        EffectClass::Observe,
        Placement::Host,
    );
    let cx = op.observe().map_err(|_| "observation unavailable")?;
    let root = StateRoot::checkout_observe(&cx, &CheckoutLocation { root: request.checkout });
    let root = root.verified().ok_or("checkout unavailable")?;
    let registration = qualification_owned_registration(
        &cx, root, (request.checkout_identity, request.state_identity), request.run,
        &request.task, &request.owner, &request.env, &request.container,
    ).map_err(|_| "registration unavailable")?;
    println!("{}", serde_json::to_string(&registration).map_err(|_| "encoding failed")?);
    Ok(())
}

fn main() {
    if let Err(error) = observe() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
