//! Behaviour of the published context document: its version lock, its schema
//! routing and the generation fence the guest reads first.

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

fn context(generation: u64) -> WorkspaceContext {
    WorkspaceContext::new(WorkspaceContextInput {
        generation,
        host: WorkspaceHostIdentity {
            version: "0.15.0".to_owned(),
            protocol: CHANNEL_PROTOCOL_VERSION,
        },
        binary: binary(),
        activation: WorkspaceActivationIdentity {
            host_user: "tester".to_owned(),
            checkout_id: "0:00".to_owned(),
            environment: WorkspaceEnvironmentIdentity {
                resource_id: "env:dev".to_owned(),
                generation: "7".to_owned(),
            },
            container: WorkspaceContainerIdentity {
                container_id: "9d1c0b6a7f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4"
                    .to_owned(),
                vm_boot_id: "boot".to_owned(),
                platform: "aarch64".to_owned(),
                libc: "gnu".to_owned(),
            },
        },
        session_run_id: None,
        session_generation: None,
        paths: WorkspacePathMap {
            host_checkout: "/checkout".to_owned(),
            guest_checkout: "/workspace".to_owned(),
            workdir: "/workspace".to_owned(),
            user: "dev".to_owned(),
            home: "/home/dev".to_owned(),
        },
        backend: WorkspaceBackendDescriptor {
            kind: "Docker Engine".to_owned(),
            profile: "default".to_owned(),
            runtime: None,
            vm_type: None,
        },
        sources: Vec::new(),
        routes: Vec::new(),
    })
}

#[test]
fn a_rendered_context_parses_back_to_the_same_generation() {
    let rendered = context(4).render().expect("render");

    let parsed = WorkspaceContext::parse(&rendered).expect("parse");

    assert_eq!(parsed, context(4));
    assert_eq!(parsed.generation, 4);
    assert_eq!(declared_schema(&rendered).expect("schema"), CONTEXT_SCHEMA);
}

#[test]
fn another_schema_is_refused_and_names_the_reprepare_command() {
    let mut document: serde_json::Value =
        serde_json::from_slice(&context(4).render().expect("render")).expect("json");
    document["schema"] = serde_json::Value::from("effigy.workspace-context.v2");

    let refusal =
        WorkspaceContext::parse(&serde_json::to_vec(&document).expect("json")).expect_err("schema");

    assert_eq!(refusal.code, ContextRefusalCode::Unavailable);
    assert!(refusal.message.contains("effigy.workspace-context.v1"));
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
fn an_unknown_field_is_refused_rather_than_ignored() {
    let mut document: serde_json::Value =
        serde_json::from_slice(&context(4).render().expect("render")).expect("json");
    document["future_field"] = serde_json::Value::from(true);

    assert!(WorkspaceContext::parse(&serde_json::to_vec(&document).expect("json")).is_err());
}

#[test]
fn the_published_generation_is_the_one_the_document_carries() {
    let mut guest = FakeGuest::default();
    guest.put_file(CONTEXT_CURRENT_PATH, b"4\n".to_vec(), 0o444);
    guest.put_file(
        &context_path(4),
        context(4).render().expect("render"),
        0o444,
    );

    let published = read_published_context(&mut guest)
        .expect("published context")
        .expect("prepared");

    assert_eq!(published.generation, 4);
    assert_eq!(published.context.generation, 4);
}

#[test]
fn a_generation_directory_carrying_another_number_refuses() {
    let mut guest = FakeGuest::default();
    guest.put_file(CONTEXT_CURRENT_PATH, b"5\n".to_vec(), 0o444);
    guest.put_file(
        &context_path(5),
        context(4).render().expect("render"),
        0o444,
    );

    assert!(read_published_context(&mut guest).is_err());
}

#[test]
fn an_unprepared_guest_publishes_no_generation() {
    let mut guest = FakeGuest::default();

    assert!(read_published_body(&mut guest)
        .expect("body read")
        .is_none());
}
