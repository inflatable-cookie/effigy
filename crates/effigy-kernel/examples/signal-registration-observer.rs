//! Private, read-only source-qualification utility. Input and output are
//! private pipes; the caller must never persist the registration nonce.
use std::io::{self, Read};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{CancelToken, Deadline, EffectClass, Limits, OperationContext, Placement};
use effigy_kernel::fs::DirectoryIdentity;
use effigy_kernel::id::RunId;
use effigy_kernel::ident::ProcessIdentity;
use effigy_kernel::location::CheckoutLocation;
use effigy_kernel::remote::{qualification_owned_registration, ContainerIdentity, QualificationSubject};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::EffectState;
use effigy_kernel::store::Store;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    checkout: PathBuf,
    checkout_identity: DirectoryIdentity,
    state_identity: DirectoryIdentity,
    run: String,
    task: String,
    owner_pid: u32,
    expected_owner: Option<ProcessIdentity>,
    container: ContainerIdentity,
}

fn observe() -> Result<(), String> {
    let mut input = String::new();
    io::stdin().take(65537).read_to_string(&mut input).map_err(|_| "request unreadable")?;
    if input.len() > 65536 { return Err("request too large".into()); }
    let request: Request = serde_json::from_str(&input).map_err(|_| "invalid request")?;
    let run: RunId = request.run.parse().map_err(|_| "invalid run")?;
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
    let reader = Store::read(&cx, root);
    let reader = reader.verified().ok_or("store unavailable")?;
    let summary = effigy_kernel::run::summary(reader, run);
    let summary = summary.verified().ok_or("run unavailable")?;
    let invocation = summary.invocation.as_ref().ok_or("invocation unavailable")?;
    let task = &invocation.spec().task;
    if task.catalog() != "" || task.task() != request.task || task.profile().is_some() {
        return Err("task differs".into());
    }
    let metadata = effigy_kernel::run::metadata(&cx, root, run);
    let metadata = metadata.verified().ok_or("owner unavailable")?;
    let owner = metadata.owner.as_ref().filter(|owner| owner.pid == request.owner_pid)
        .ok_or("owner differs")?;
    if request.expected_owner.as_ref().is_some_and(|expected| expected != owner) {
        return Err("owner moved".into());
    }
    let until = Instant::now() + Duration::from_secs(19);
    let env = loop {
        let effects = effigy_kernel::run::effects(reader, run);
        let effects = effects.verified().ok_or("effects unavailable")?;
        let mut remote = effects.iter().filter(|effect| effect.spec.kind == "kernel:remote-process");
        match (remote.next(), remote.next()) {
            (Some(effect), None) if effect.state == EffectState::Applied => break effect.spec.target.clone(),
            (None, None) | (Some(effigy_kernel::run::Effect { state: EffectState::Intended, .. }), None) => {},
            _ => return Err("remote intent ambiguous or failed".into()),
        }
        if Instant::now() >= until { return Err("remote registration unavailable".into()); }
        std::thread::sleep(Duration::from_millis(20));
    };
    let registration = qualification_owned_registration(
        &cx, root, (request.checkout_identity, request.state_identity),
        &QualificationSubject { run, task, owner, env: &env, container: &request.container },
    ).map_err(|_| "registration unavailable")?;
    let response = serde_json::json!({"registration": registration, "owner": owner});
    println!("{}", serde_json::to_string(&response).map_err(|_| "encoding failed")?);
    Ok(())
}

fn main() {
    if let Err(error) = observe() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
