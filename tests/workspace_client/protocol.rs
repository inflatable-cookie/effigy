use super::support::*;
use effigy_kernel::ctx::Placement;
use effigy_kernel::id::Digest;
use effigy_kernel::run::RequestId;
use effigy_kernel::workspace_channel::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;

fn reject(edit: impl FnOnce(&mut Pipe, &mut TestServer<'_>), code: RefusalCode) {
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let runs = Rc::new(RefCell::new(vec![]));
    let mut server = server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        session_operations(),
    );
    let mut frames = authenticated(session.session());
    frames.push(Frame::Request(request(
        session.session(),
        1,
        RequestBody::Invoke(body()),
    )));
    let mut pipe = Pipe::new(&frames);
    edit(&mut pipe, &mut server);
    let _result = server.serve(
        &mut pipe,
        &host.cx,
        &host.observe(),
        &host.root,
        &mut NoStreams,
    );
    if pipe.peer.as_ref().is_none_or(|peer| peer.digest != pin()) {
        assert_eq!(pipe.read, 0, "TLS mismatch consumed a credential");
    }
    let rendered = format!("{server:?}{}", String::from_utf8_lossy(&pipe.output));
    for secret in ["ab".repeat(32), "cd".repeat(32)] {
        assert!(
            !rendered.contains(&secret),
            "refusal echoed a capability in logs/JSON"
        );
    }
    refused(&pipe, code);
    ledger.borrow().done();
    assert!(
        runs.borrow().is_empty(),
        "refused request admitted a workload"
    );
    let database = host.database();
    let reservations: i64 = database
        .query_row("SELECT count(*) FROM workspace_admissions", [], |r| {
            r.get(0)
        })
        .expect("admission records");
    assert_eq!(reservations, 0, "refused request left an admission record");
    drop(database);
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("session cleanup");
    host.assert_foreign();
}

#[test]
fn pin_mismatch_never_reads_a_credential_or_dispatches() {
    reject(
        |pipe, _| {
            pipe.peer = Some(ServerIdentity {
                digest: Digest::of(b"foreign server"),
            })
        },
        RefusalCode::IdentityMismatch,
    );
}

#[test]
fn missing_tls_identity_never_reads_a_credential_or_dispatches() {
    reject(|pipe, _| pipe.peer = None, RefusalCode::IdentityMismatch);
}

fn alter_first(pipe: &mut Pipe, edit: impl FnOnce(&mut Value)) {
    let length = u32::from_be_bytes(pipe.input[..4].try_into().expect("prefix")) as usize;
    let mut hello: Value =
        serde_json::from_slice(&pipe.input[4..4 + length]).expect("hello fixture");
    edit(&mut hello);
    let rest = pipe.input[4 + length..].to_vec();
    pipe.input.clear();
    pipe.raw(&serde_json::to_vec(&hello).expect("edited hello"));
    pipe.input.extend(rest);
}

macro_rules! version_case {
    ($name:ident, $key:literal, $value:expr, $code:ident) => {
        #[test]
        fn $name() {
            reject(
                |pipe, _| alter_first(pipe, |hello| hello[$key] = $value),
                RefusalCode::$code,
            );
        }
    };
}
version_case!(
    protocol_has_no_downgrade,
    "protocol_versions",
    json!([2]),
    VersionMismatch
);
version_case!(
    version_mismatch_has_no_effect,
    "effigy_version",
    json!("0.14.1"),
    VersionMismatch
);
version_case!(
    artifact_mismatch_has_no_effect,
    "binary_digest",
    json!(Digest::of(b"foreign artifact")),
    VersionMismatch
);
version_case!(
    stale_hello_has_no_effect,
    "context_generation",
    json!(2),
    StaleContext
);

#[test]
fn invalid_capability_has_no_effect_or_secret_echo() {
    reject(
        |pipe, _| {
            let length =
                u32::from_be_bytes(pipe.input[..4].try_into().expect("hello prefix")) as usize;
            let start = 4 + length;
            let auth_length = u32::from_be_bytes(
                pipe.input[start..start + 4]
                    .try_into()
                    .expect("auth prefix"),
            ) as usize;
            let rest = pipe.input[start + 4 + auth_length..].to_vec();
            pipe.input.truncate(start);
            pipe.raw(
                &encode_frame(&Frame::Authenticate(Secret::new([0xcd; 32])))
                    .expect("invalid capability frame"),
            );
            pipe.input.extend(rest);
        },
        RefusalCode::Unauthenticated,
    );
}

#[test]
fn revoked_capability_cannot_reconnect() {
    reject(
        |_, server| server.grants_mut().revoke_all(),
        RefusalCode::Unauthenticated,
    );
}

#[test]
fn request_before_authentication_has_no_effect() {
    reject(
        |pipe, _| {
            let length =
                u32::from_be_bytes(pipe.input[..4].try_into().expect("hello prefix")) as usize;
            let auth = 4 + length;
            let length =
                u32::from_be_bytes(pipe.input[auth..auth + 4].try_into().expect("auth prefix"))
                    as usize;
            pipe.input.drain(auth..auth + 4 + length);
        },
        RefusalCode::Unauthenticated,
    );
}

fn invalid_request(edit: impl FnOnce(&mut Value), code: RefusalCode) {
    reject(
        |pipe, _| {
            let mut cursor = 0;
            for _ in 0..2 {
                let length =
                    u32::from_be_bytes(pipe.input[cursor..cursor + 4].try_into().expect("prefix"))
                        as usize;
                cursor += 4 + length;
            }
            let mut value: Value =
                serde_json::from_slice(&pipe.input[cursor + 4..]).expect("request fixture");
            edit(&mut value);
            pipe.input.truncate(cursor);
            pipe.raw(&serde_json::to_vec(&value).expect("edited request"));
        },
        code,
    );
}

macro_rules! request_case {
    ($name:ident, $edit:expr, $code:ident) => {
        #[test]
        fn $name() {
            invalid_request($edit, RefusalCode::$code);
        }
    };
}
request_case!(
    context_generation_is_fenced,
    |r| r["context_generation"] = json!(2),
    StaleContext
);
request_case!(
    environment_generation_is_fenced,
    |r| r["environment_generation"] = json!(3),
    StaleContext
);
request_case!(
    container_identity_is_full_and_fenced,
    |r| r["container_id"] = json!("full"),
    IdentityMismatch
);
request_case!(
    vm_boot_is_fenced,
    |r| r["vm_boot_id"] = json!("boot-2"),
    IdentityMismatch
);
request_case!(
    session_identity_is_fenced,
    |r| r["session_run_id"] = json!(id(201)),
    IdentityMismatch
);
request_case!(
    unknown_method_cannot_reach_host,
    |r| r["method"] = json!("internal"),
    InvalidRequest
);
request_case!(
    resolved_plan_cannot_be_smuggled,
    |r| r["body"]["executable"] = json!("/bin/sh"),
    InvalidRequest
);
request_case!(
    guest_state_root_cannot_be_smuggled,
    |r| r["body"]["state_root"] = json!("/host/.effigy"),
    InvalidRequest
);
request_case!(
    cwd_traversal_cannot_reach_host,
    |r| r["body"]["cwd"] = json!("../../host"),
    InvalidRequest
);
request_case!(
    absolute_cwd_cannot_reach_host,
    |r| r["body"]["cwd"] = json!("/host"),
    InvalidRequest
);
request_case!(
    argument_count_is_bounded,
    |r| r["body"]["args"] = json!(vec!["arg"; 257]),
    InvalidRequest
);
request_case!(
    argument_bytes_are_bounded,
    |r| r["body"]["args"] = json!(["a".repeat(4097)]),
    InvalidRequest
);
request_case!(
    body_bytes_are_bounded,
    |r| r["body"]["args"] = json!(vec!["a".repeat(4096); 17]),
    InvalidRequest
);
request_case!(
    target_scope_is_host_owned,
    |r| r["body"]["target"] = json!({"container":{"service":"foreign"}}),
    Forbidden
);

#[test]
fn malformed_duplicate_utf8_and_oversized_frames_close_before_host_effects() {
    for bytes in [
        b"{".as_slice(),
        b"{\"frame\":\"hello\",\"frame\":\"request\"}",
        &[0xff],
    ] {
        reject(
            |pipe, _| {
                pipe.input.clear();
                pipe.raw(bytes);
            },
            RefusalCode::InvalidRequest,
        );
    }
    // read_frame must reject on the four-byte prefix alone, before reading or
    // allocating the claimed body. This is a separate allocation-bound proof.
    let mut pipe = Pipe::new(&[]);
    pipe.input
        .extend_from_slice(&((MAX_FRAME_BYTES + 1) as u32).to_be_bytes());
    assert_eq!(
        read_frame(&mut pipe).expect_err("oversized prefix").code,
        RefusalCode::InvalidRequest
    );
    assert_eq!(pipe.read, 4, "oversized frame read its body");
}

#[test]
fn observe_retries_reobserve_without_admission_and_preserve_host_json() {
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let runs = Rc::new(RefCell::new(vec![]));
    let body = ObserveBody {
        op: "status".into(),
        target: Some(Target::Host),
    };
    ledger
        .borrow_mut()
        .expect([Call::Observe(body.clone()), Call::Observe(body.clone())]);
    let mut server = server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        [Method::Observe].into_iter().collect(),
    );
    let request = request(session.session(), 1, RequestBody::Observe(body));
    let mut frames = authenticated(session.session());
    frames.extend([Frame::Request(request.clone()), Frame::Request(request)]);
    let mut pipe = Pipe::new(&frames);
    server
        .serve(
            &mut pipe,
            &host.cx,
            &host.observe(),
            &host.root,
            &mut NoStreams,
        )
        .expect("observe");
    let responses = pipe
        .frames()
        .into_iter()
        .filter_map(|f| match f {
            Frame::Response(Response {
                outcome: ResponseOutcome::Observed(value),
                ..
            }) => Some(value),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 2);
    assert!(responses.iter().all(|r| r["schema"] == "effigy.command.v1"));
    assert!(runs.borrow().is_empty());
    let rendered = format!(
        "{:?}{:?}{}",
        server,
        ledger.borrow().calls,
        String::from_utf8_lossy(&pipe.output)
    );
    for secret in ["ab".repeat(32), "cd".repeat(32)] {
        assert!(
            !rendered.contains(&secret),
            "credential entered logs, arguments or JSON"
        );
    }
    ledger.borrow().done();
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("session cleanup");
    host.assert_foreign();
}

#[test]
fn guest_placement_never_falls_back_to_local_effects() {
    let guest = context(Placement::Guest);
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let mut dispatcher = Dispatcher::new(
        Engine {
            host: &host,
            ledger: ledger.clone(),
            runs: Rc::new(RefCell::new(vec![])),
        },
        Resolve(ledger.clone()),
        Observe(ledger.clone()),
        Ids(ledger.clone()),
    );
    let result = dispatcher.invoke(
        &guest,
        &host.observe(),
        &host.root,
        session.session(),
        RequestId::new([1; 16]),
        Digest::of(b"guest"),
        &body(),
    );
    assert_eq!(
        result.expect_err("no guest effect context").code,
        RefusalCode::Forbidden
    );
    ledger.borrow().done();
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("cleanup");
    host.assert_foreign();
}
