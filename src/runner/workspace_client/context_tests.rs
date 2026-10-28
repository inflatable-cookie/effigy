//! Behaviour of the published context, its version lock and its fences.

use super::*;
use crate::runner::workspace_client::guest_fake::FakeGuest;

const BINARY_DIGEST: &str =
    "sha256:0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";

fn binary() -> GuestBinaryIdentity {
    GuestBinaryIdentity {
        version: "0.15.0".to_owned(),
        digest: BINARY_DIGEST.to_owned(),
        protocol: CHANNEL_PROTOCOL_VERSION,
    }
}

fn container() -> WorkspaceContainerIdentity {
    WorkspaceContainerIdentity {
        container_id: "9d1c0b6a7f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4".to_owned(),
        vm_boot_id: "3f6a1c92-5b7e-4c1d-8a2f-0e9d7c6b5a41".to_owned(),
        platform: "aarch64".to_owned(),
    }
}

fn activation() -> WorkspaceActivationIdentity {
    WorkspaceActivationIdentity {
        host_user: "tom".to_owned(),
        checkout_id: "0:00112233445566778899aabbccddeeff".to_owned(),
        environment: WorkspaceEnvironmentIdentity {
            resource_id: "env:app".to_owned(),
            generation: "sha256:aa11".to_owned(),
        },
        container: container(),
    }
}

fn paths() -> WorkspacePathMap {
    WorkspacePathMap {
        host_checkout: "/Users/tom/src/app".to_owned(),
        guest_checkout: "/workspace".to_owned(),
        workdir: "/workspace".to_owned(),
        user: "vscode".to_owned(),
        home: "/home/vscode".to_owned(),
    }
}

fn channel() -> WorkspaceChannelLocator {
    WorkspaceChannelLocator {
        endpoint: Some("host.docker.internal:47121".to_owned()),
        server_identity: Some("sha256:server".to_owned()),
    }
}

fn context(generation: u64) -> WorkspaceContext {
    WorkspaceContext::new(WorkspaceContextInput {
        generation,
        host: WorkspaceHostIdentity {
            version: "0.15.0".to_owned(),
            protocol: CHANNEL_PROTOCOL_VERSION,
        },
        binary: binary(),
        activation: activation(),
        session_run_id: Some("run-7".to_owned()),
        paths: paths(),
        backend: WorkspaceBackendDescriptor {
            kind: "Docker Desktop".to_owned(),
            profile: "default".to_owned(),
            runtime: None,
            vm_type: None,
        },
        sources: Vec::new(),
        routes: Vec::new(),
        channel: channel(),
    })
}

fn refusal_of(error: ContextReadError) -> ContextRefusal {
    match error {
        ContextReadError::Refused(refusal) => refusal,
        ContextReadError::Guest(error) => panic!("unexpected guest error: {error}"),
    }
}

fn live(generation: u64) -> LiveGuestRun {
    LiveGuestRun {
        binary: binary(),
        container: container(),
        environment_generation: "sha256:aa11".to_owned(),
        session_run_id: Some("run-7".to_owned()),
        context_generation: generation,
    }
}

#[test]
fn an_exact_match_verifies_and_returns_the_host_binding_identity() {
    let verified = context(4).verify(&live(4)).expect("verified");

    assert_eq!(verified.generation, 4);
    assert_eq!(verified.session_run_id, "run-7");
    assert_eq!(verified.binary, binary());
    assert_eq!(verified.protocol, CHANNEL_PROTOCOL_VERSION);
    assert_eq!(verified.activation, activation());
    assert_eq!(verified.paths, paths());
    assert!(verified.channel.is_ready());
}

#[test]
fn a_request_on_an_older_generation_is_stale_and_rereads_the_pointer() {
    let refusal = context(5).verify(&live(4)).expect_err("stale");

    assert_eq!(refusal.code, ContextRefusalCode::StaleContext);
    assert!(!refusal.names_reprepare(), "a stale read rereads `current`");
    assert!(refusal.message.contains(CONTEXT_CURRENT_PATH));
}

#[test]
fn a_replaced_container_refuses_and_names_the_host_reprepare() {
    let mut replaced = live(4);
    replaced.container.container_id = "ffffffff".to_owned();

    let refusal = context(4).verify(&replaced).expect_err("identity");

    assert_eq!(refusal.code, ContextRefusalCode::IdentityMismatch);
    assert!(refusal.names_reprepare());
    assert!(refusal.message.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn a_changed_vm_boot_refuses_even_when_the_container_id_still_matches() {
    let mut rebooted = live(4);
    rebooted.container.vm_boot_id = "00000000-0000-0000-0000-000000000000".to_owned();

    let refusal = context(4).verify(&rebooted).expect_err("identity");

    assert_eq!(refusal.code, ContextRefusalCode::IdentityMismatch);
    assert!(refusal
        .message
        .contains("00000000-0000-0000-0000-000000000000"));
}

#[test]
fn another_environment_generation_refuses_before_any_effect() {
    let mut moved = live(4);
    moved.environment_generation = "sha256:bb22".to_owned();

    let refusal = context(4).verify(&moved).expect_err("identity");

    assert_eq!(refusal.code, ContextRefusalCode::IdentityMismatch);
    assert!(refusal.message.contains("sha256:bb22"));
}

#[test]
fn a_client_version_mismatch_names_the_host_reprepare() {
    let mut other_build = live(4);
    other_build.binary.version = "0.14.1".to_owned();

    let refusal = context(4).verify(&other_build).expect_err("version");

    assert_eq!(refusal.code, ContextRefusalCode::VersionMismatch);
    assert!(refusal.message.contains("0.14.1"));
    assert!(refusal.message.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn a_binary_digest_mismatch_refuses_even_at_the_same_version() {
    let mut repacked = live(4);
    repacked.binary.digest = "sha256:ff".to_owned();

    let refusal = context(4).verify(&repacked).expect_err("version");

    assert_eq!(refusal.code, ContextRefusalCode::VersionMismatch);
    assert!(refusal.message.contains("sha256:ff"));
}

#[test]
fn a_context_without_an_admitted_session_leaves_host_access_unavailable() {
    let mut prepared = context(4);
    prepared.session_run_id = None;

    let refusal = prepared.verify(&live(4)).expect_err("unavailable");

    assert_eq!(refusal.code, ContextRefusalCode::Unavailable);
    assert!(refusal.message.contains("no workspace session run"));
    assert!(refusal.message.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn a_context_without_a_qualified_channel_reports_host_access_unavailable() {
    let mut unqualified = context(4);
    unqualified.channel = WorkspaceChannelLocator::default();

    let refusal = unqualified.verify(&live(4)).expect_err("unavailable");

    assert_eq!(refusal.code, ContextRefusalCode::Unavailable);
    assert!(refusal.message.contains("no host authority channel"));
}

#[test]
fn a_session_named_by_another_context_refuses() {
    let mut enrolled_elsewhere = live(4);
    enrolled_elsewhere.session_run_id = Some("run-9".to_owned());

    let refusal = context(4)
        .verify(&enrolled_elsewhere)
        .expect_err("identity");

    assert_eq!(refusal.code, ContextRefusalCode::IdentityMismatch);
    assert!(refusal.message.contains("run-7"));
}

#[test]
fn published_context_round_trips_through_the_pointer() {
    let body = context(4).render().expect("render");
    let mut guest = FakeGuest::new();
    guest.put_file(CONTEXT_CURRENT_PATH, "4\n", 0o644);
    guest.put_file(&context_path(4), body, 0o444);

    let published = read_published_context(&mut guest)
        .expect("read")
        .expect("prepared");

    assert_eq!(published.generation, 4);
    assert_eq!(published.context, context(4));
}

#[test]
fn an_unprepared_workspace_reads_as_absent_not_as_a_refusal() {
    let mut guest = FakeGuest::new();

    assert!(read_published_context(&mut guest).expect("read").is_none());
}

#[test]
fn a_pointer_without_a_document_refuses_and_names_the_reprepare() {
    let mut guest = FakeGuest::new();
    guest.put_file(CONTEXT_CURRENT_PATH, "4\n", 0o644);

    let refusal = refusal_of(read_published_context(&mut guest).expect_err("unprepared"));

    assert_eq!(refusal.code, ContextRefusalCode::Unavailable);
    assert!(refusal.message.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn a_document_published_under_another_schema_is_refused() {
    let mut document: serde_json::Value =
        serde_json::from_slice(&context(4).render().expect("render")).expect("json");
    document["schema"] = serde_json::Value::String("effigy.workspace-context.v2".to_owned());

    let refusal =
        WorkspaceContext::parse(&serde_json::to_vec(&document).expect("json")).expect_err("schema");

    assert_eq!(refusal.code, ContextRefusalCode::Unavailable);
    assert!(refusal.message.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn a_document_with_unknown_fields_is_refused_before_dispatch() {
    let mut document: serde_json::Value =
        serde_json::from_slice(&context(4).render().expect("render")).expect("json");
    document["grant"] = serde_json::Value::String("not-a-field".to_owned());

    let refusal = WorkspaceContext::parse(&serde_json::to_vec(&document).expect("json"))
        .expect_err("unknown field");

    assert_eq!(refusal.code, ContextRefusalCode::Unavailable);
}

#[test]
fn a_context_pinning_another_protocol_refuses_without_a_downgrade() {
    let mut document: serde_json::Value =
        serde_json::from_slice(&context(4).render().expect("render")).expect("json");
    document["binary"]["protocol"] = serde_json::Value::from(2);

    let refusal = WorkspaceContext::parse(&serde_json::to_vec(&document).expect("json"))
        .expect_err("protocol");

    assert_eq!(refusal.code, ContextRefusalCode::VersionMismatch);
    assert!(refusal.message.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn only_an_authority_bearing_operation_enrols_a_direct_terminal() {
    assert!(!GuestOperationClass::Static.requires_enrolment());
    assert!(!GuestOperationClass::Observe.requires_enrolment());
    assert!(GuestOperationClass::Mutate.requires_enrolment());
}

#[test]
fn an_enrolment_credential_never_prints_its_secret() {
    let credential = WorkspaceEnrolmentCredential::new("sha256:aa11", "9d1c", "s3cret-token");

    let rendered = format!("{credential:?}");

    assert!(rendered.contains("<redacted>"));
    assert!(!rendered.contains("s3cret-token"));
    assert!(credential.file_body().contains("s3cret-token"));
}
