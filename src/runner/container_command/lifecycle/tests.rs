use super::down::lifecycle_cleanup_failed_container_up;
use super::down::run_container_down_command;
use super::integrations::ensure_up_workspace_permissions;
use super::integrations::gateway_port_wait_stage;
use super::integrations::integration_stage;
use super::integrations::ContainerUpRuntimeIntegrations;
use super::integrations::IntegrationOutcomePolicy;
use super::lifecycle_operation_plan;
use super::prepare::prepare_container_up;
use super::report::detached_activation_failure;
use super::report::detached_activation_identity;
use super::report::listener_exposure_failure;
use super::report::sanitized_failure_cause;
use super::report::DetachedOperationState;
use super::up::activation_source;
use crate::runner::container_command::handover::hand_over_conflicting_environments;
use crate::runner::container_command::steps::ProgressSource;
use crate::runner::container_command::support::{
    annotate_left_running_shared_services, annotate_shared_service_notes,
};
use crate::runner::container_command::test_support::temp_repo;
use crate::runner::container_command::RunnerError;
use effigy_containers::annotate_workspace_extra_mount_decisions;
use effigy_containers::policy::restart::RestartPolicyStage;
use effigy_containers::{
    down_report, load_container_policy, load_container_policy_for_creation, up_detached_report,
    ContainerAction, CreationPolicyStage, EffectiveComposeSource, EffectiveContainerPolicy,
    SharedServiceBinding,
};
use effigy_containers::{ContainerConfirmationPolicy, ContainerLifecycleOperation};
use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
    ManifestContainerStartup,
};
use effigy_runtime::write::{run_container_reset, select_generated_service_image_refs};
use effigy_ui::steps::ProgressFrame;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn test_policy(shared_services: Vec<SharedServiceBinding>) -> EffectiveContainerPolicy {
    EffectiveContainerPolicy {
        render: None,
        repo_root: std::path::PathBuf::from("/tmp"),
        name: "web".to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files: vec![PathBuf::from("docker-compose.yml")],
        managed_volumes: vec![],
        borrowed_volumes: Vec::new(),
        data_peers: Vec::new(),
        data_from: None,
        shared_services,
        project_name: "demo-web-dev".to_owned(),
        primary_service: "app".to_owned(),
        dns_domain: None,
        dns_base_domain: None,
        runtime_host_key: None,
        dns_tls: false,
        dns_port: None,
        dns_routes: vec![],
        service_aliases: vec![],
        declared_ports: vec!["8080:80".to_owned()],
        ports_declared_explicitly: true,
        declared_mounts: vec![],
        declared_media_mounts: vec![],
        workspace_source: effigy_containers::artifact_source::WorkspaceSource::Checkout,
        pull_production_hook: None,
        health_check: None,
        health_timeout_secs: 60,
        secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
        secret_runtime_dir: None,
        source_secret_runtime_for_deferrals: false,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: Vec::new(),
    }
}

#[test]
fn up_workspace_permission_preparation_names_container_on_failure() {
    let policy = test_policy(Vec::new());

    let error = ensure_up_workspace_permissions(&policy, || {
        Err(RunnerError::task_invocation(
            "resolved user `dev` (uid=501 gid=20) cannot read/write `/usr/local/cargo/git/db`",
        ))
    })
    .expect_err("preparation failure should surface");
    assert!(matches!(error, RunnerError::TaskInvocation(_)));
    let rendered = error.to_string();
    assert!(
        rendered.contains("container `web` workspace permission preparation failed"),
        "got: {rendered}"
    );
    assert!(
        rendered.contains("/usr/local/cargo/git/db"),
        "got: {rendered}"
    );
}
#[test]
fn detached_json_failure_keeps_the_stage_identity_and_sanitized_cause() {
    let policy = test_policy(Vec::new());
    let operation = DetachedOperationState {
        active: true,
        stage: Some("environment_handover"),
        identity: Some(detached_activation_identity(&policy)),
        ..Default::default()
    };
    let error = RunnerError::task_invocation(
            "gateway domain `api.app.test` is claimed by a container route from checkout `/gone/worktree`\nDB_PASSWORD=hunter2 EFFIGY_TOKEN=abcd1234",
        );

    let failure = detached_activation_failure(&operation, true, error);
    let RunnerError::CommandJsonFailure { rendered } = failure else {
        panic!("detached JSON mode must render a machine failure");
    };
    let document: serde_json::Value = serde_json::from_str(&rendered).expect("json failure");
    assert_eq!(document["phase"], "detached_activation");
    assert_eq!(document["activation_stage"], "environment_handover");
    assert_eq!(document["state"], "unavailable");
    assert_eq!(document["recovery"], "effigy container up --detach");
    assert_eq!(document["failure_kind"], "invocation_refusal");
    assert_eq!(
        document["identity"], "environment `web` at /tmp",
        "the envelope names the environment and checkout"
    );
    let cause = document["cause"].as_str().expect("cause");
    assert!(cause.contains("api.app.test"), "{cause}");
    assert!(cause.contains("/gone/worktree"), "{cause}");
    assert!(cause.contains("DB_PASSWORD=<redacted>"), "{cause}");
    assert!(cause.contains("EFFIGY_TOKEN=<redacted>"), "{cause}");
    assert!(!cause.contains("hunter2"), "{cause}");
    assert!(!cause.contains("abcd1234"), "{cause}");

    // A later failure during the same activation keeps its reconcile advice.
    let failure = detached_activation_failure(
        &operation,
        true,
        RunnerError::GatewayEndpointStore {
            kind: effigy_gateway::EndpointStoreRefusal::Refused,
            reason: "revision changed".to_owned(),
        },
    );
    let RunnerError::CommandJsonFailure { rendered } = failure else {
        panic!("detached JSON mode must render a machine failure");
    };
    let document: serde_json::Value = serde_json::from_str(&rendered).expect("json failure");
    assert_eq!(document["state"], "unavailable");
    assert_eq!(document["recovery"], "effigy container up --detach");
    assert_eq!(document["failure_kind"], "endpoint_store_refusal");
    assert_eq!(
        document["cause"],
        "gateway endpoint store: revision changed"
    );

    // An unobservable store is its own kind: nothing was read or changed.
    let failure = detached_activation_failure(
        &operation,
        true,
        RunnerError::GatewayEndpointStore {
            kind: effigy_gateway::EndpointStoreRefusal::ObservationUnknown,
            reason: "store unreadable".to_owned(),
        },
    );
    let RunnerError::CommandJsonFailure { rendered } = failure else {
        panic!("detached JSON mode must render a machine failure");
    };
    let document: serde_json::Value = serde_json::from_str(&rendered).expect("json failure");
    assert_eq!(document["failure_kind"], "endpoint_observation_unknown");
    assert_eq!(
        document["cause"],
        "gateway endpoint store: observation unknown: store unreadable"
    );

    // A sentence that merely reads like one is not the store's answer: the
    // kind comes from the error's own type, never its wording (effigy#933).
    let failure = detached_activation_failure(
        &operation,
        true,
        RunnerError::task_invocation("gateway endpoint store: revision changed"),
    );
    let RunnerError::CommandJsonFailure { rendered } = failure else {
        panic!("detached JSON mode must render a machine failure");
    };
    let document: serde_json::Value = serde_json::from_str(&rendered).expect("json failure");
    assert_eq!(document["failure_kind"], "invocation_refusal");
}

#[test]
fn lifecycle_progress_contract_cases() {
    concise_container_progress_transcript();
    annotate_shared_service_notes_adds_text_and_json();
    annotate_left_running_shared_services_adds_text_and_json();
}

fn concise_container_progress_transcript() {
    use std::time::Duration;
    let progress = activation_source("docker compose up", "web");
    assert_eq!(
        progress.frame_at(Duration::from_secs(21)),
        "Starting web stack · 21s"
    );
    crate::runner::container_command::steps::restart_policy_stage(
        &progress,
        "the `web` stack",
        RestartPolicyStage::Apply,
        None,
    );
    assert_eq!(
        progress.frame_at(Duration::from_secs(21)),
        "Setting restart policy · 21s"
    );
    let (_, failure) = effigy_ui::steps::capture_lines(|| {
        crate::runner::container_command::steps::during(progress, "routes registered", |_| {
            Err::<(), _>(RunnerError::task_invocation(
                "gateway port 443 is already in use",
            ))
        })
    });
    assert_eq!(failure.len(), 1, "one terminal error line");
    assert!(failure[0].starts_with("✗ gateway port 443 is already in use ("));
}

#[test]
fn activation_frames_name_the_compose_call_and_the_last_runtime_line() {
    use std::time::Duration;

    let progress = activation_source("docker compose up", "web");
    assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(12)),
            "starting the `web` stack; waiting for docker compose up to return for stack `web`; 12s elapsed"
        );
    // The runtime's own line, mapped through the same label production uses.
    let started = effigy_containers::exec::ComposeStepEvent::Starting {
        subject: "demo-web-1".to_owned(),
    };
    progress.event(started.label("demo"));
    // The event never replaces the call still awaited: the frame
    // keeps `docker compose up` and the elapsed wait.
    assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(20)),
            "starting the `web` stack; waiting for docker compose up to return for stack `web`; last: starting web; 20s elapsed"
        );
    // Compose has returned and the restart-policy apply begins: each of its
    // stages names itself as it starts, so a stalled read names that read
    // and never the finished Compose call or its last event (effigy#460,
    // Q-017).
    crate::runner::container_command::steps::restart_policy_stage(
        &progress,
        "the `web` stack",
        RestartPolicyStage::ComposeDeclaration,
        None,
    );
    assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(30)),
            "reading the Compose declaration of the `web` stack for services without a restart policy; waiting for `restart: unless-stopped` on the `web` stack; 30s elapsed"
        );
    // A stalled runtime inventory read keeps saying what it is reading.
    crate::runner::container_command::steps::restart_policy_stage(
        &progress,
        "the `web` stack",
        RestartPolicyStage::RuntimeInventory,
        None,
    );
    assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(44)),
            "reading the running containers of the `web` stack from the runtime; waiting for `restart: unless-stopped` on the `web` stack; 44s elapsed"
        );
    crate::runner::container_command::steps::restart_policy_stage(
        &progress,
        "the `web` stack",
        RestartPolicyStage::Apply,
        None,
    );
    assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(51)),
            "asking the runtime to keep the containers of the `web` stack on `restart: unless-stopped`; waiting for `restart: unless-stopped` on the `web` stack; 51s elapsed"
        );
}

#[test]
fn the_route_stage_names_the_port_read_and_the_blocker_it_reported() {
    use std::time::Duration;

    let progress = ProgressSource::waiting_for("the runtime integrations of this environment");
    integration_stage(&progress, 5, "registering gateway routes");
    assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(2)),
            "registering gateway routes (5/8); waiting for the runtime integrations of this environment; 2s elapsed"
        );
    // The runtime read now in flight, with its attempt and the stage count.
    gateway_port_wait_stage(
        &progress,
        crate::runner::container_command::gateway_registration::GatewayPortWait::Reading {
            attempt: 1,
        },
    );
    assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(6)),
            "reading the runtime's containers for the declared gateway service ports (attempt 1) (5/8); waiting for the runtime integrations of this environment; 6s elapsed"
        );
    // The read answered and its blocker names the service and domain.
    gateway_port_wait_stage(
            &progress,
            crate::runner::container_command::gateway_registration::GatewayPortWait::Blocked {
                attempt: 1,
                detail: "service `db` has not published its declared TCP alias port for gateway domain `db.demo.test`",
            },
        );
    assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(7)),
            "waiting for the declared gateway service ports (attempt 1) (5/8); waiting for the runtime integrations of this environment; last: service `db` has not published its declared TCP alias port for gateway domain `db.demo.test`; 7s elapsed"
        );
    // The next poll keeps that blocker as the last thing this wait saw.
    gateway_port_wait_stage(
        &progress,
        crate::runner::container_command::gateway_registration::GatewayPortWait::Reading {
            attempt: 2,
        },
    );
    assert!(
            progress
                .detailed_frame_at(Duration::from_secs(8))
                .contains("reading the runtime's containers for the declared gateway service ports (attempt 2) (5/8); waiting for the runtime integrations of this environment; last: service `db`"),
            "{}",
            progress.detailed_frame_at(Duration::from_secs(8))
        );
    // The poll then succeeds, and the work after it is its own wait: the
    // resolved blocker is not shown against it.
    for (wait, expected) in [
            (
                crate::runner::container_command::gateway_registration::GatewayPortWait::ConfirmingOwnership,
                "confirming the primary container still owns its binding (5/8); waiting for the runtime integrations of this environment; 9s elapsed",
            ),
            (
                crate::runner::container_command::gateway_registration::GatewayPortWait::ResolvingRoutes,
                "resolving the declared gateway routes against the published ports (5/8); waiting for the runtime integrations of this environment; 9s elapsed",
            ),
            (
                crate::runner::container_command::gateway_registration::GatewayPortWait::ReconcilingRoutes,
                "writing the gateway route table (5/8); waiting for the runtime integrations of this environment; 9s elapsed",
            ),
        ] {
            gateway_port_wait_stage(&progress, wait);
            assert_eq!(progress.detailed_frame_at(Duration::from_secs(9)), expected);
            assert!(
                !progress.detailed_frame_at(Duration::from_secs(9)).contains("last:"),
                "the poll's blocker is cleared for the work that follows it"
            );
        }
    // The timeout diagnostic's own read is named too.
    gateway_port_wait_stage(
            &progress,
            crate::runner::container_command::gateway_registration::GatewayPortWait::ReadingExitedContainers,
        );
    assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(10)),
            "reading the exited gateway service containers for the timeout diagnostic (5/8); waiting for the runtime integrations of this environment; 10s elapsed"
        );
    // A later integration stage clears what the previous stage observed.
    integration_stage(&progress, 6, "reconciling tcp alias hosts");
    assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(11)),
            "reconciling tcp alias hosts (6/8); waiting for the runtime integrations of this environment; 11s elapsed"
        );
}

#[test]
fn a_policy_load_frame_names_the_preparation_stage_now_running() {
    use std::time::Duration;

    let progress = ProgressSource::waiting_for("the container policy, its ports and mount sources");
    // The first frame names the exact wait before any stage is reported.
    assert_eq!(
        progress.detailed_frame_at(Duration::from_secs(1)),
        "waiting for the container policy, its ports and mount sources; 1s elapsed"
    );
    // Each preparation stage names itself as the load reaches it.
    for (stage, expected) in [
        (
            CreationPolicyStage::CheckoutIdentity,
            "recording the checkout identity",
        ),
        (
            CreationPolicyStage::Declaration,
            "resolving the container declaration, ports and loopback",
        ),
        (
            CreationPolicyStage::RenderInputs,
            "preparing render inputs and owned mount directories",
        ),
        (
            CreationPolicyStage::ResolvedDeclaration,
            "resolving the declaration again with the workspace and extra mounts",
        ),
        (CreationPolicyStage::Render, "rendering the Compose stack"),
        (
            CreationPolicyStage::MountSources,
            "preparing the mount sources the stack binds",
        ),
    ] {
        progress.update(|_| ProgressFrame::new(stage.description()));
        assert_eq!(
                progress.detailed_frame_at(Duration::from_secs(9)),
                format!("{expected}; waiting for the container policy, its ports and mount sources; 9s elapsed")
            );
    }
}

#[test]
fn a_detached_readiness_refusal_json_names_the_waits_it_ended_on() {
    let operation = DetachedOperationState {
        active: true,
        stage: Some("readiness"),
        ..Default::default()
    };
    let error = RunnerError::container_wait_refused(
        "demo",
        "environment `demo` did not become ready within 60s".to_owned(),
        serde_json::json!({
            "failure_kind": "container_readiness",
            "readiness": "unverified",
            "waiting_on": [{
                "kind": "service",
                "service": "web",
                "problem": "web is unhealthy after 4 failed checks",
            }],
        }),
    );
    let RunnerError::CommandJsonFailure { rendered } =
        detached_activation_failure(&operation, true, error)
    else {
        panic!("a detached readiness refusal renders its JSON envelope");
    };
    let document: serde_json::Value = serde_json::from_str(&rendered).expect("json");
    assert_eq!(document["failure_kind"], "container_readiness");
    assert_eq!(document["readiness"], "unverified");
    assert_eq!(document["waiting_on"][0]["service"], "web");
    assert_eq!(
        document["waiting_on"][0]["problem"],
        "web is unhealthy after 4 failed checks"
    );
}

#[test]
fn sanitized_failure_cause_bounds_the_reported_detail() {
    let long = format!("first line\n{}", "detail ".repeat(600));
    let cause = sanitized_failure_cause(&RunnerError::task_invocation(long));
    assert!(cause.starts_with("first line; detail"), "{cause}");
    assert!(cause.chars().count() <= 1201, "{}", cause.chars().count());
    // #364 keeps both ends: the middle is elided and the last line survives.
    assert!(cause.contains('…'), "{cause}");
    assert!(cause.ends_with("detail"), "{cause}");
}
/// The bring-up's integrations, as `complete_container_up_runtime_integrations`
/// hands them to the partial-exposure report: one container route and one
/// healthy listener published, one listener withheld.
fn partial_exposure_fixture() -> (EffectiveContainerPolicy, ContainerUpRuntimeIntegrations) {
    let policy = test_policy(Vec::new());
    let integrations = ContainerUpRuntimeIntegrations {
            gateway_routes: vec![crate::runner::container_command::gateway_registration::RegisteredGatewayRoute {
                domain: "web.test".to_owned(),
                target: Some("127.0.0.1:18080".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                tls: false,
                service: Some("app".to_owned()),
                external_target: false,
            }],
            managed_host_listeners: vec![crate::runner::host_process::ManagedHostListenerResult {
                name: "api".to_owned(),
                status: crate::runner::host_process::HostLifecycleStatus::Ready,
                runtime_generation: "runtime-1".to_owned(),
                generation: "generation-1".to_owned(),
                address: "127.0.0.1:21001".to_owned(),
                internal_url: "http://127.0.0.1:21001".to_owned(),
                public_url: "http://api.test".to_owned(),
                declared_route_domain: "api.test".to_owned(),
                route_domain: "api.test".to_owned(),
                route_tls: false,
            }],
            managed_host_listener_failures: vec![
                crate::runner::host_process::ManagedHostListenerFailure {
                    name: "front".to_owned(),
                    status: crate::runner::host_process::HostLifecycleStatus::Failed,
                    declared_route_domain: "acowtancy.test".to_owned(),
                    route_domain: Some("acowtancy.test".to_owned()),
                    observed_status: Some(500),
                    detail: "managed listener at 127.0.0.1:21002 answered /health with HTTP 500 where 200 is required (status_mismatch)".to_owned(),
                    output_tail: Some("vite: internal server error".to_owned()),
                    log: "/checkout/.effigy/runtime/front.log".to_owned(),
                },
            ],
            tcp_alias_host_notes: Vec::new(),
            warnings: Vec::new(),
        };
    (policy, integrations)
}

/// Partial exposure names the routes that are published and the listener
/// that is not, instead of one blanket unverified-exposure refusal
/// (effigy#455).
#[test]
fn listener_exposure_failure_names_exposed_routes_and_the_withheld_listener() {
    let (policy, integrations) = partial_exposure_fixture();
    let error = listener_exposure_failure(&policy, &integrations);
    let text = error.to_string();
    assert!(
        text.contains("web.test -> 127.0.0.1:18080"),
        "the container's published route is named: {text}"
    );
    assert!(
        text.contains("api.test -> http://api.test"),
        "the ready listener's route is named: {text}"
    );
    assert!(
        text.contains("host listener `front` kept route acowtancy.test withheld"),
        "the withheld route is named with its listener: {text}"
    );
    assert!(
        text.contains("HTTP 500") && text.contains("vite: internal server error"),
        "the refusal carries the observed status and the child output tail: {text}"
    );
    assert!(
        !text.contains("exposure are unverified") && !text.contains("readiness and exposure"),
        "a settled environment with partial exposure is not an unverified activation: {text}"
    );
}

/// The detached JSON envelope for the same state reports the exposure
/// per route: published routes and withheld routes, with the listener's own
/// status and bounded output tail.
#[test]
fn detached_listener_exposure_json_names_exposed_and_withheld_routes() {
    let (policy, integrations) = partial_exposure_fixture();
    let operation = DetachedOperationState {
        active: true,
        stage: Some("persistent_owner"),
        identity: Some(detached_activation_identity(&policy)),
        ..Default::default()
    };
    let failure = detached_activation_failure(
        &operation,
        true,
        listener_exposure_failure(&policy, &integrations),
    );
    let RunnerError::CommandJsonFailure { rendered } = failure else {
        panic!("detached JSON mode must render a machine failure");
    };
    let document: serde_json::Value = serde_json::from_str(&rendered).expect("json failure");
    assert_eq!(document["state"], "partial");
    assert_eq!(document["phase"], "listener_exposure");
    assert_eq!(document["failure_kind"], "managed_listener_exposure");
    assert_eq!(document["exposure"], "partial");
    assert_eq!(document["readiness"], "verified");
    assert_eq!(document["recovery"], "effigy container up --detach");
    assert_eq!(
        document["exposed_routes"],
        json!(["web.test -> 127.0.0.1:18080", "api.test -> http://api.test"])
    );
    let withheld = document["withheld_routes"]
        .as_array()
        .expect("withheld list");
    assert_eq!(withheld.len(), 1, "{document}");
    assert_eq!(withheld[0]["listener"], "front");
    assert_eq!(withheld[0]["route_domain"], "acowtancy.test");
    assert_eq!(withheld[0]["observed_status"], 500);
    assert_eq!(withheld[0]["output_tail"], "vite: internal server error");
    assert!(
        withheld[0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("status_mismatch"),
        "{document}"
    );
    let message = document["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("partial exposure") && !message.contains("unverified"),
        "{message}"
    );
}

#[test]
fn lifecycle_reset_plan_requires_confirmation_only_for_wipe_data() {
    let policy = test_policy(Vec::new());
    let keep_data = lifecycle_operation_plan(
        Path::new("/tmp/repo"),
        &policy,
        ContainerLifecycleOperation::reset(true, false, false),
    );
    assert_eq!(
        keep_data.confirmation,
        ContainerConfirmationPolicy::NoConfirmationRequired
    );
    assert!(!keep_data.consumes_declared_container_secrets());

    let wipe_data = lifecycle_operation_plan(
        Path::new("/tmp/repo"),
        &policy,
        ContainerLifecycleOperation::reset(false, true, false),
    );
    assert_eq!(
        wipe_data.confirmation,
        ContainerConfirmationPolicy::RequireConfirmation {
            reason: "reset removes runtime data",
        }
    );
    assert!(!wipe_data.consumes_declared_container_secrets());
}
pub(super) fn shared_service(name: &str, catalog: &str, host_port: u16) -> SharedServiceBinding {
    SharedServiceBinding {
        service_name: name.to_owned(),
        catalog: catalog.to_owned(),
        domain_label: match catalog {
            "mariadb" => "mysql".to_owned(),
            "postgres" => "postgres".to_owned(),
            "redis" => "redis".to_owned(),
            "memcached" => "memcached".to_owned(),
            other => panic!("unexpected shared catalog {other}"),
        },
        project_name: format!("effigy-shared-{catalog}"),
        compose_file: Path::new("/tmp").join(format!("{name}-{catalog}.yml")),
        host: "host.docker.internal".to_owned(),
        host_port,
        container_port: match catalog {
            "mariadb" => 3306,
            "postgres" => 5432,
            "redis" => 6379,
            "memcached" => 11211,
            other => panic!("unexpected shared catalog {other}"),
        },
        host_env_vars: Vec::new(),
        port_env_vars: Vec::new(),
        render_digest: format!("{name}-{catalog}"),
    }
}

#[test]
fn select_generated_service_image_refs_returns_built_service_images_only() {
    let parsed: serde_yaml::Value = serde_yaml::from_str(
        r#"
services:
  app:
    build:
      context: .
      dockerfile: .effigy/runtime/compose/.effigy-catalog/app/Dockerfile
  web:
    image: nginx:alpine
  worker:
    build:
      context: .
      dockerfile: .effigy/runtime/compose/.effigy-catalog/worker/Dockerfile
    image: custom-worker:dev
"#,
    )
    .expect("parse compose");

    let refs = select_generated_service_image_refs(&parsed, "demo");

    assert_eq!(
        refs,
        vec!["demo-app:latest".to_owned(), "custom-worker:dev".to_owned()]
    );
}

fn annotate_shared_service_notes_adds_text_and_json() {
    let policy = test_policy(vec![]);
    let mut report = up_detached_report(&policy, false, Some("ready"));

    annotate_shared_service_notes(
        &mut report,
        &[
            "db [mariadb] -> host.docker.internal:8106".to_owned(),
            "cache [redis] -> host.docker.internal:8110".to_owned(),
        ],
    );

    assert!(report
        .success_text
        .contains("[shared] ensured db [mariadb] -> host.docker.internal:8106"));
    assert!(report
        .success_text
        .contains("[shared] ensured cache [redis] -> host.docker.internal:8110"));
    assert_eq!(
        report.json["shared_service_actions"],
        json!({
            "action": "ensured",
            "services": [
                "db [mariadb] -> host.docker.internal:8106",
                "cache [redis] -> host.docker.internal:8110"
            ]
        })
    );
}

/// One render per `container up`: preparation publishes exactly one
/// generation for the checkout and runs no apply, and the plan carries the
/// extra-mount decisions that render recorded (effigy#371).
#[test]
fn container_up_preparation_renders_once_and_runs_no_apply() {
    crate::contract_test_support::assert_waits_for_path_mutator(|boundary| {
        boundary();
        let _environment = crate::contract_test_support::clean_fixture_environment();
        let root = temp_repo("container-up-one-render", "one-render");
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir_all(&home).expect("mkdir home");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let log = root.join("docker-calls");
        let docker = bin.join("docker");
        fs::write(
            &docker,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\nexit 0\n",
                log = log.display()
            ),
        )
        .expect("write fake docker");
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&docker).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&docker, permissions).expect("chmod fake docker");
        }
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose");

        let _runtime = crate::contract_test_support::EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap_or_default()
                )),
            ),
            ("HOME", Some(home.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        ]);

        let mut operation = DetachedOperationState::default();
        let prepared = prepare_container_up(&root, Some("web"), false, true, &mut operation);

        let plan = prepared.expect("container up prepares");
        let objects = root.join(".effigy/state/objects");
        let published = fs::read_dir(&objects)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(0);
        assert_eq!(
            published, 1,
            "one `up` publishes one generation: {objects:?}"
        );
        let calls = fs::read_to_string(&log).unwrap_or_default();
        assert!(
            !calls.lines().any(|call| call.contains("compose")),
            "preparation runs no apply: {calls}"
        );
        assert!(plan.workspace_mount_decisions.is_empty());
    });
}

#[test]
fn the_creation_load_captures_a_mount_created_during_preparation() {
    let _environment = crate::contract_test_support::clean_fixture_environment();
    let root = temp_repo("workspace-mount-creation-plan", "direct");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace-root/app"
mounts = ["prepared:/workspace-root/prepared:ro"]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write compose");

    // A mount source the run prepares does not exist yet when the manifest
    // is read: the one creation load is the render that captures it, so an
    // `up` never renders twice (effigy#371).
    let source = root.join("prepared");
    fs::create_dir(&source).expect("preparation creates source");
    let (policy, decisions) = load_container_policy_for_creation(&root, Some("web"))
        .expect("resolve creation-time mounts");

    let backend = effigy_containers::ContainerManager::defaults();
    let request = effigy_containers::ContainerManagerRequest::new(&root)
        .backend_override(effigy_containers::BackendId::docker_compose());
    let plan = backend
        .compose_invocation_plan(
            &request,
            &effigy_containers::ContainerBackendDetection::new(false),
            &policy.profile,
            &effigy_containers::compose::compose_up_args(&policy),
            ContainerAction::Activate,
            "docker compose up",
        )
        .expect("compose backend plan");
    let compose_path = plan
        .args
        .windows(2)
        .find(|args| args[0] == "-f")
        .map(|args| PathBuf::from(&args[1]))
        .expect("backend plan captures compose file");
    let captured_compose = policy
        .read_compose(&compose_path)
        .expect("read backend compose input");
    let canonical_source = source.canonicalize().expect("canonical source");

    assert_eq!(decisions.len(), 1);
    assert_eq!(
        decisions[0].decision,
        effigy_containers::WorkspaceExtraMountDecisionKind::IncludedInComposeSpec
    );
    assert!(captured_compose.contains(&format!(
        "{}:/workspace-root/prepared:ro",
        canonical_source.display()
    )));

    let mut report = up_detached_report(&policy, false, None);
    annotate_workspace_extra_mount_decisions(&mut report, &decisions);
    assert_eq!(
        report.json["workspace_extra_mounts"]["compose_up_dispatch"],
        "succeeded"
    );
    assert_eq!(
        report.json["workspace_extra_mounts"]["runtime_mount_verification"],
        "not_performed"
    );
    assert!(report.json["workspace_extra_mounts"]
        .get("applied")
        .is_none());
}

fn annotate_left_running_shared_services_adds_text_and_json() {
    let policy = test_policy(vec![
        shared_service("db", "mariadb", 8106),
        shared_service("cache", "redis", 8110),
    ]);
    let mut report = down_report(&policy, true);

    annotate_left_running_shared_services(&mut report, &policy);

    assert!(report
        .success_text
        .contains("[shared] left running db [mariadb] -> host.docker.internal:8106"));
    assert!(report
        .success_text
        .contains("[shared] left running cache [redis] -> host.docker.internal:8110"));
    assert_eq!(
        report.json["shared_service_actions"],
        json!({
            "action": "left-running",
            "services": [
                "db [mariadb] -> host.docker.internal:8106",
                "cache [redis] -> host.docker.internal:8110"
            ]
        })
    );
}

#[test]
fn run_container_reset_rejects_keep_data_with_wipe_data() {
    let root = temp_repo("container-eject", "reset-conflicting-data-flags");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let error = run_container_reset(
        &root,
        None,
        true,
        true,
        false,
        |_| panic!("conflicting flags must not reconcile shared services"),
        |_, _, _| panic!("conflicting flags must not apply"),
        |_, _, _| panic!("conflicting flags must not enter a transition"),
    )
    .expect_err("should fail");
    assert!(error
        .to_string()
        .contains("does not accept both `--keep-data` and `--wipe-data`"));
}

/// Isolates the partial-activation mark in a private lease home. The guard
/// keeps the environment override for the life of the test.
fn isolated_lease_home() -> (tempfile::TempDir, crate::contract_test_support::EnvGuard) {
    let temp = tempfile::tempdir().expect("temporary fixture root");
    fs::write(temp.path().join("effigy.toml"), "[containers]\n").expect("private marker input");
    fs::write(temp.path().join("docker-compose.yml"), "services: {}\n")
        .expect("private compose input");
    let lease_home = temp.path().join("lease-home");
    fs::create_dir_all(&lease_home).expect("temporary lease home");
    let guard = crate::contract_test_support::EnvGuard::set_many(&[(
        "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
        Some(lease_home.display().to_string()),
    )]);
    (temp, guard)
}

fn prepared_marker_policy(root: &Path) -> EffectiveContainerPolicy {
    fs::write(root.join("effigy.toml"),"[containers]\ndefault='web'\n[containers.web]\ncompose_file='docker-compose.yml'\nproject_name='demo-web-dev'\nprimary_service='app'\n").unwrap();
    fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .unwrap();
    effigy_containers::prepare_container_policy(root, None).unwrap()
}

fn running_row(service: &str, oneoff: bool) -> effigy_containers::exec::RunningComposeContainer {
    effigy_containers::exec::RunningComposeContainer {
        container_name: format!("web-{service}"),
        status: "Up 2 minutes".into(),
        ports: Vec::new(),
        project_name: Some("demo-web-dev".to_owned()),
        working_dir: None,
        service: Some(service.to_owned()),
        oneoff,
    }
}

#[test]
fn detached_gateway_registration_failure_keeps_started_scope() {
    let (temp, _lease_home) = isolated_lease_home();
    let policy = prepared_marker_policy(temp.path());
    let cleaned = std::cell::Cell::new(false);
    let error = IntegrationOutcomePolicy::Detached.failure(
        &policy,
        RunnerError::task_invocation("timed out waiting for declared gateway services"),
        || {
            cleaned.set(true);
            Ok(())
        },
    );

    assert!(
        !cleaned.get(),
        "detached bring-up must not tear the scope down"
    );
    let rendered = error.to_string();
    assert!(rendered.starts_with("timed out waiting for declared gateway services"));
    assert!(rendered.contains("container `web` stays as Compose left it"));
    assert!(rendered.contains("gateway exposure is unverified"));
    assert!(rendered.contains("no teardown ran"));
    assert!(rendered.contains("effigy container up --detach"));
    assert!(rendered.contains("reconcile from what is running"));
}

#[test]
fn workspace_integration_failure_retains_owner_without_lifecycle_cleanup() {
    let policy = test_policy(Vec::new());
    let error = IntegrationOutcomePolicy::Workspace.failure(
        &policy,
        RunnerError::task_invocation("integration effects unsettled; re-run up"),
        || panic!("workspace recovery owns cleanup"),
    );
    assert_eq!(
        error.to_string(),
        "integration effects unsettled; re-run up"
    );
}

#[test]
fn attached_gateway_registration_failure_still_tears_scope_down() {
    let (temp, _lease_home) = isolated_lease_home();
    let policy = prepared_marker_policy(temp.path());
    for cleanup_fails in [false, true] {
        let cleaned = std::cell::Cell::new(0);
        let error = IntegrationOutcomePolicy::Attached.failure(
            &policy,
            RunnerError::task_invocation("gateway registration failed"),
            || {
                cleaned.set(cleaned.get() + 1);
                if cleanup_fails {
                    Err(RunnerError::task_invocation("docker compose down failed"))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(cleaned.get(), 1);
        assert_eq!(
            error.to_string(),
            if cleanup_fails {
                "gateway registration failed
container up cleanup also failed: docker compose down failed"
            } else {
                "gateway registration failed"
            }
        );
    }
}

#[test]
fn running_primary_does_not_hide_missing_or_stopped_siblings() {
    let private = tempfile::tempdir().expect("private partial stack");
    let compose = private.path().join("compose.yml");
    fs::write(
        &compose,
        "services:\n  app:\n    image: app\n  db:\n    image: db\n",
    )
    .expect("compose");
    let mut policy = test_policy(Vec::new());
    policy.compose_files = vec![compose];
    let primary = running_row("app", false);
    assert!(!effigy_containers::exec::compose_stack_is_running(
        &policy,
        std::slice::from_ref(&primary)
    )
    .expect("missing sibling"));
    let mut sibling = running_row("db", false);
    sibling.status = "Exited (1)".into();
    assert!(!effigy_containers::exec::compose_stack_is_running(
        &policy,
        &[primary.clone(), sibling.clone()]
    )
    .expect("stopped sibling"));
    sibling.status = "Up 10 seconds".into();
    assert!(
        effigy_containers::exec::compose_stack_is_running(&policy, &[primary, sibling])
            .expect("complete stack")
    );
}

/// A repo whose only running environment is a Compose stack that a fake
/// `docker` reports as up, so a real `container down` path (discovery,
/// runtime check, compose down) runs without touching a live runtime.
struct FakeDownFixture {
    _temp: tempfile::TempDir,
    repo_root: PathBuf,
    parent: PathBuf,
    project: String,
    _env: crate::contract_test_support::EnvGuard,
}

impl FakeDownFixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temporary fixture root");
        let parent = temp.path().to_path_buf();
        let repo_root = parent.join("repo");
        fs::create_dir_all(&repo_root).expect("mkdir repo");
        let project = "effigy-down-fixture".to_owned();
        fs::write(
                repo_root.join("effigy.toml"),
                format!(
                    "[containers]\ndefault = \"web\"\n\n[containers.web]\ncompose_file = \"docker-compose.yml\"\nprimary_service = \"app\"\nproject_name = \"{project}\"\n"
                ),
            )
            .expect("write manifest");
        fs::write(
            repo_root.join("docker-compose.yml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose");

        let bin = parent.join("bin");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let docker = bin.join("docker");
        fs::write(
                &docker,
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$EFFIGY_TEST_DOCKER_LOG\"\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \"--format\" ]; then\n    printf 'web-app\\tUp 1 minute\\t\\t%s\\t%s\\tapp\\tFalse\\n' \"$EFFIGY_TEST_PROJECT\" \"$EFFIGY_TEST_WORKDIR\"\n    exit 0\n  fi\ndone\nexit 0\n",
            )
            .expect("write fake docker");
        let mut perms = fs::metadata(&docker)
            .expect("stat fake docker")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&docker, perms).expect("chmod fake docker");
        // `command_exists("colima")` also probes well-known install
        // directories, so shadow the host CLI with a fake that reports no
        // profiles instead of listing the developer's real VMs.
        let colima = bin.join("colima");
        fs::write(&colima, "#!/bin/sh\nexit 0\n").expect("write fake colima");
        let mut perms = fs::metadata(&colima)
            .expect("stat fake colima")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&colima, perms).expect("chmod fake colima");

        let lease_home = parent.join("lease-home");
        fs::create_dir_all(&lease_home).expect("mkdir lease home");
        let home = parent.join("home");
        fs::create_dir_all(&home).expect("mkdir fake home");
        let env = crate::contract_test_support::EnvGuard::set_many(&[
            ("PATH", Some(bin.display().to_string())),
            ("HOME", Some(home.display().to_string())),
            ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(lease_home.display().to_string()),
            ),
            (
                "EFFIGY_TEST_DOCKER_LOG",
                Some(parent.join("docker.log").display().to_string()),
            ),
            ("EFFIGY_TEST_PROJECT", Some(project.clone())),
            ("EFFIGY_TEST_WORKDIR", Some(repo_root.display().to_string())),
        ]);
        FakeDownFixture {
            _temp: temp,
            repo_root,
            parent,
            project,
            _env: env,
        }
    }

    fn policy(&self) -> EffectiveContainerPolicy {
        load_container_policy(&self.repo_root, None).expect("fixture policy")
    }

    /// The environment record after a stop: stopped and no longer
    /// persistent, so a later task-started stack idle-stops again.
    fn assert_stopped(&self, policy: &EffectiveContainerPolicy) {
        let record = crate::runner::environment::record(policy)
            .unwrap_or_else(|| panic!("down records the environment: {}", self.docker_log()));
        assert_eq!(
            record.state,
            effigy_containers::environment::EnvState::Stopped,
            "{}",
            self.docker_log()
        );
    }

    fn docker_log(&self) -> String {
        fs::read_to_string(self.parent.join("docker.log")).unwrap_or_default()
    }
}

#[test]
fn global_container_down_is_one_stop_transition_per_environment() {
    for global in [true, false] {
        let fixture = FakeDownFixture::new();
        let policy = fixture.policy();
        crate::contract_test_support::with_cwd(&fixture.parent, || {
            run_container_down_command(None, None, global, false)
        })
        .expect("down succeeds");
        fixture.assert_stopped(&policy);
        let log = fixture.docker_log();
        let down = log
            .lines()
            .filter(|line| line.split_whitespace().any(|arg| arg == "down"))
            .collect::<Vec<_>>();
        assert_eq!(down.len(), 1, "global={global}: {log}");
        assert!(
            down[0].contains(&policy.project_name),
            "global={global}: {log}"
        );
        assert_eq!(policy.project_name, fixture.project);
    }
}

/// Two catalog-driven environments, `dev` and `hybrid` (which declares
/// `data_from = "dev"`), with a fake `docker` on PATH so the `data_from`
/// handover runs end to end without touching a live runtime. The fake
/// reports one running (or exited) `db` container for the peer project
/// named by `EFFIGY_TEST_PEER_PROJECT`.
struct DataFromFixture {
    _temp: tempfile::TempDir,
    repo_root: PathBuf,
    parent: PathBuf,
    _env: crate::contract_test_support::EnvGuard,
}

impl DataFromFixture {
    fn new(peer_project: &str, peer_status: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temporary fixture root");
        let parent = temp.path().to_path_buf();
        let repo_root = parent.join("repo");
        fs::create_dir_all(&repo_root).expect("mkdir repo");
        fs::write(
            repo_root.join("effigy.toml"),
            r#"
[containers]
default = "dev"

[containers.dev]
primary_service = "db"
project_name = "acowtancy-dev"

[containers.dev.services.db]
catalog = "postgres"

[containers.hybrid]
primary_service = "db"
project_name = "acowtancy-hybrid"
data_from = "dev"

[containers.hybrid.services.db]
catalog = "postgres"
"#,
        )
        .expect("write manifest");

        let bin = parent.join("bin");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let docker = bin.join("docker");
        fs::write(
                &docker,
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$EFFIGY_TEST_DOCKER_LOG\"\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \"--format\" ]; then\n    printf 'dev-db\\t%s\\t\\t%s\\t%s\\tdb\\tFalse\\n' \"$EFFIGY_TEST_PEER_STATUS\" \"$EFFIGY_TEST_PEER_PROJECT\" \"$EFFIGY_TEST_WORKDIR\"\n    exit 0\n  fi\ndone\ncase \"$*\" in\n  *\" stop \"*|*\" down \"*)\n    if [ \"$EFFIGY_TEST_STOP_FAILS\" = \"1\" ]; then\n      echo teardown refused >&2\n      exit 3\n    fi\n    ;;\nesac\nexit 0\n",
            )
            .expect("write fake docker");
        let mut perms = fs::metadata(&docker)
            .expect("stat fake docker")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&docker, perms).expect("chmod fake docker");
        let colima = bin.join("colima");
        fs::write(&colima, "#!/bin/sh\nexit 0\n").expect("write fake colima");
        let mut perms = fs::metadata(&colima)
            .expect("stat fake colima")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&colima, perms).expect("chmod fake colima");

        let home = parent.join("home");
        fs::create_dir_all(&home).expect("mkdir fake home");
        let env = crate::contract_test_support::EnvGuard::set_many(&[
            ("PATH", Some(bin.display().to_string())),
            ("HOME", Some(home.display().to_string())),
            ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            (
                "EFFIGY_TEST_DOCKER_LOG",
                Some(parent.join("docker.log").display().to_string()),
            ),
            ("EFFIGY_TEST_PEER_PROJECT", Some(peer_project.to_owned())),
            ("EFFIGY_TEST_PEER_STATUS", Some(peer_status.to_owned())),
            ("EFFIGY_TEST_STOP_FAILS", Some("0".to_owned())),
            ("EFFIGY_TEST_WORKDIR", Some(repo_root.display().to_string())),
        ]);
        DataFromFixture {
            _temp: temp,
            repo_root,
            parent,
            _env: env,
        }
    }

    fn policy(&self, name: &str) -> EffectiveContainerPolicy {
        load_container_policy(&self.repo_root, Some(name)).expect("fixture policy")
    }

    fn docker_log(&self) -> String {
        fs::read_to_string(self.parent.join("docker.log")).unwrap_or_default()
    }
}

#[test]
fn up_of_borrower_hands_over_whole_owner_environment() {
    let fixture = DataFromFixture::new("acowtancy-dev", "Up 1 minute");
    let hybrid = fixture.policy("hybrid");

    let notes = hand_over_conflicting_environments(&fixture.repo_root, &hybrid, |_| Ok(Vec::new()))
        .expect("borrower handover succeeds");

    let log = fixture.docker_log();
    assert_eq!(notes.len(), 1, "one line for one handover: {log}");
    assert!(
        notes[0].contains("stopping environment `dev`")
            && notes[0].contains("shared data services (data_from)"),
        "{}",
        notes[0]
    );
    assert!(
        log.contains("volume create"),
        "the borrowed volume is ensured before compose up: {log}"
    );
    assert!(
        log.contains("acowtancy-dev-db-data"),
        "the owner's volume name is ensured: {log}"
    );
    assert!(
        log.contains("-p acowtancy-dev") && log.contains(" down"),
        "the owner environment comes down whole in its own project: {log}"
    );
    assert!(
        !log.contains(" down -v"),
        "the handover never removes volumes: {log}"
    );
    let downs = log
        .lines()
        .filter(|line| line.split_whitespace().any(|arg| arg == "down"))
        .collect::<Vec<_>>();
    assert_eq!(downs.len(), 1, "one peer environment transition: {log}");
    assert!(
        !downs[0].split_whitespace().any(|arg| arg == "-v"),
        "peer volumes stay: {log}"
    );
}

#[test]
fn up_of_owner_takes_back_whole_borrower_environment() {
    let fixture = DataFromFixture::new("acowtancy-hybrid", "Up 1 minute");
    let dev = fixture.policy("dev");

    let notes = hand_over_conflicting_environments(&fixture.repo_root, &dev, |_| Ok(Vec::new()))
        .expect("owner handover");

    let log = fixture.docker_log();
    assert_eq!(notes.len(), 1, "one line for one handover: {log}");
    assert!(
        notes[0].contains("stopping environment `hybrid`")
            && notes[0].contains("shared data services (data_from)"),
        "{}",
        notes[0]
    );
    assert!(
        !log.contains("volume create"),
        "the owner borrows nothing and creates no volumes: {log}"
    );
    assert!(
        log.contains("-p acowtancy-hybrid") && log.contains(" down"),
        "the borrower environment comes down whole in its own project: {log}"
    );
    let downs = log
        .lines()
        .filter(|line| line.split_whitespace().any(|arg| arg == "down"))
        .collect::<Vec<_>>();
    assert_eq!(downs.len(), 1, "one peer environment transition: {log}");
    assert!(
        !downs[0].split_whitespace().any(|arg| arg == "-v"),
        "peer volumes stay: {log}"
    );
}

#[test]
fn up_refuses_when_peer_environment_stop_fails_and_stays_recoverable() {
    let fixture = DataFromFixture::new("acowtancy-dev", "Up 1 minute");
    let _fail = crate::contract_test_support::EnvGuard::set_many(&[(
        "EFFIGY_TEST_STOP_FAILS",
        Some("1".to_owned()),
    )]);
    let hybrid = fixture.policy("hybrid");
    let dev = fixture.policy("dev");

    let error = hand_over_conflicting_environments(&fixture.repo_root, &hybrid, |_| Ok(Vec::new()))
        .expect_err("a failed stop refuses the activation");

    let rendered = error.to_string();
    assert!(
        rendered.contains("stopping environment `dev`"),
        "{rendered}"
    );
    assert!(rendered.contains("failed"), "{rendered}");
    assert!(rendered.contains("recovers from"), "{rendered}");
    // The failed stop is journalled as an unknown backend effect; the
    // peer's record is not published stopped, and its own `up` reconciles.
    let record = crate::runner::environment::record(&dev).expect("peer record");
    assert_ne!(
        record.state,
        effigy_containers::environment::EnvState::Stopped
    );
}

#[test]
fn up_skips_handover_when_peer_data_services_are_stopped() {
    let fixture = DataFromFixture::new("acowtancy-dev", "Exited (0) 2 minutes ago");
    let hybrid = fixture.policy("hybrid");

    let notes = hand_over_conflicting_environments(&fixture.repo_root, &hybrid, |_| Ok(Vec::new()))
        .expect("handover succeeds without a running peer");

    let log = fixture.docker_log();
    assert!(notes.is_empty(), "no handover line without a running peer");
    assert!(
        !log.lines()
            .any(|line| line.split_whitespace().any(|arg| arg == "down")),
        "no teardown runs for an already-stopped peer: {log}"
    );
    assert!(
        log.contains("volume create"),
        "borrowed volumes are still ensured: {log}"
    );
}

/// A checkout whose container environment has never been prepared, plus a
/// fake `docker` that records every invocation.
struct UnpreparedDownFixture {
    _temp: tempfile::TempDir,
    repo_root: PathBuf,
    parent: PathBuf,
    project: String,
    _env: crate::contract_test_support::EnvGuard,
}

impl UnpreparedDownFixture {
    /// A generated (catalog) environment: loaded without Prepare, its
    /// published port is still unassigned.
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temporary fixture root");
        let parent = temp.path().to_path_buf();
        let repo_root = parent.join("repo");
        fs::create_dir_all(&repo_root).expect("mkdir repo");
        let project = "effigy-286-fixture".to_owned();
        fs::write(
                repo_root.join("effigy.toml"),
                format!(
                    "[containers]\ndefault = \"web\"\n\n[containers.web]\nprimary_service = \"app\"\nproject_name = \"{project}\"\n\n[containers.web.services.app]\ncatalog = \"nginx\"\nvariant = \"default\"\n"
                ),
            )
            .expect("write manifest");

        let bin = parent.join("bin");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let docker = bin.join("docker");
        fs::write(
            &docker,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$EFFIGY_TEST_DOCKER_LOG\"\nexit 0\n",
        )
        .expect("write fake docker");
        let mut perms = fs::metadata(&docker)
            .expect("stat fake docker")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&docker, perms).expect("chmod fake docker");
        // `command_exists("colima")` also probes well-known install
        // directories, so shadow the host CLI with a fake that reports no
        // profiles.
        let colima = bin.join("colima");
        fs::write(&colima, "#!/bin/sh\nexit 0\n").expect("write fake colima");
        let mut perms = fs::metadata(&colima)
            .expect("stat fake colima")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&colima, perms).expect("chmod fake colima");

        let home = parent.join("home");
        fs::create_dir_all(&home).expect("mkdir fake home");
        let lease_home = parent.join("lease-home");
        fs::create_dir_all(&lease_home).expect("mkdir lease home");
        let env = crate::contract_test_support::EnvGuard::set_many(&[
            ("PATH", Some(bin.display().to_string())),
            ("HOME", Some(home.display().to_string())),
            ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(lease_home.display().to_string()),
            ),
            (
                "EFFIGY_TEST_DOCKER_LOG",
                Some(parent.join("docker.log").display().to_string()),
            ),
        ]);
        Self {
            _temp: temp,
            repo_root,
            parent,
            project,
            _env: env,
        }
    }

    fn policy(&self) -> EffectiveContainerPolicy {
        load_container_policy(&self.repo_root, None).expect("fixture policy")
    }

    fn docker_log(&self) -> String {
        fs::read_to_string(self.parent.join("docker.log")).unwrap_or_default()
    }

    /// Every Compose invocation the fake backend served. `compose` is the
    /// first argument of one, so an empty list proves no Compose ran.
    fn compose_invocations(&self) -> Vec<String> {
        self.docker_log()
            .lines()
            .filter(|line| line.starts_with("compose"))
            .map(str::to_owned)
            .collect()
    }

    /// The first start ran and failed: its record never published `active`
    /// and its ports were never assigned.
    fn record_never_activated_environment(&self, policy: &EffectiveContainerPolicy) {
        let env = crate::runner::environment::env_ref(policy).expect("fixture environment");
        let ctx = crate::runner::environment::context(&self.repo_root).expect("fixture context");
        effigy_containers::environment::reconcile(
            &ctx,
            &env,
            effigy_containers::environment::Up {
                render: policy.render.as_ref().map(|render| &render.closure),
                project: &policy.project_name,
                backend: &crate::runner::environment::backend(&self.repo_root, policy),
                retention: effigy_containers::environment::Retention::Persistent,
                workdir: &self.repo_root,
                primary_service: &policy.primary_service,
                compose_files: &policy.compose_files,
            },
            || Ok(()),
        )
        .expect("the never-activated environment is recorded");
        assert_eq!(
            crate::runner::environment::record(policy)
                .expect("fixture record")
                .state,
            effigy_containers::environment::EnvState::Pending,
            "the fixture's record never published active"
        );
    }
}

/// effigy#286: a `down` on an environment this checkout never prepared has
/// nothing to stop. It succeeds as one plain line, never rendering and
/// never calling Compose, whose interpolation would refuse the unassigned
/// ports of a render that was never prepared.
#[test]
fn container_down_of_a_never_prepared_environment_is_a_noop() {
    let fixture = UnpreparedDownFixture::new();

    let text = crate::contract_test_support::with_cwd(&fixture.repo_root, || {
        run_container_down_command(None, None, false, false)
    })
    .expect("down of a never-prepared environment succeeds");
    assert!(
        text.contains("nothing to stop for `web`"),
        "one plain line says nothing was running: {text}"
    );
    assert!(
        text.contains(&fixture.project),
        "the line names the Compose project it proved empty: {text}"
    );
    assert_eq!(
        fixture.compose_invocations(),
        Vec::<String>::new(),
        "no Compose ran: {}",
        fixture.docker_log()
    );

    let json = crate::contract_test_support::with_cwd(&fixture.repo_root, || {
        run_container_down_command(None, None, false, true)
    })
    .expect("the JSON no-op succeeds");
    let payload: serde_json::Value = serde_json::from_str(&json).expect("JSON payload");
    assert_eq!(payload["schema"], "effigy.container.down.v1");
    assert_eq!(payload["ok"], true);
    assert!(
        payload["noop"]
            .as_str()
            .is_some_and(|reason| reason.contains("never prepared")),
        "the no-op states its reason: {json}"
    );
    assert_eq!(
        fixture.compose_invocations(),
        Vec::<String>::new(),
        "no Compose ran: {}",
        fixture.docker_log()
    );
}

/// A failed first start cleans up: its own closeout stops the partial
/// stack.
#[test]
fn a_failed_first_start_cleans_up_its_partial_stack() {
    let fixture = UnpreparedDownFixture::new();
    let policy = fixture.policy();
    fixture.record_never_activated_environment(&policy);

    lifecycle_cleanup_failed_container_up(&fixture.repo_root, &policy)
        .expect("the failed start's cleanup succeeds");

    let log = fixture.docker_log();
    let calls = fixture.compose_invocations();
    assert_eq!(
        calls
            .iter()
            .filter(|line| line.split_whitespace().any(|arg| arg == "down"))
            .count(),
        1,
        "{log}"
    );
    assert!(
        calls
            .iter()
            .all(|line| !line.split_whitespace().any(|arg| arg == "up")),
        "{log}"
    );
}

const BINDING_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A fake runtime that lists one running owned service. `ps` reports the
/// row, `inspect` returns that container's identity, and `exec` fails the
/// way docker does for a `working_dir` the image does not have.
struct UpBindingFixture {
    _temp: tempfile::TempDir,
    repo_root: PathBuf,
    parent: PathBuf,
    project: String,
    _env: crate::contract_test_support::EnvGuard,
}

impl UpBindingFixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temporary fixture root");
        let parent = temp.path().to_path_buf();
        let repo_root = parent.join("repo");
        fs::create_dir_all(&repo_root).expect("mkdir repo");
        fs::write(
            repo_root.join("effigy.toml"),
            r#"
[containers]
default = "dev"

[containers.dev]
compose_file = "docker-compose.yml"
primary_service = "app"
project_name = "up-binding-fixture"
"#,
        )
        .expect("write manifest");
        fs::write(
            repo_root.join("docker-compose.yml"),
            "services:\n  app:\n    image: example:one\n    working_dir: /workspace-root/app\n",
        )
        .expect("write compose");

        let bin = parent.join("bin");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let docker = bin.join("docker");
        fs::write(
                &docker,
                r#"#!/bin/sh
printf '%s\n' "$*" >> "$EFFIGY_TEST_DOCKER_LOG"
for arg in "$@"; do
  if [ "$arg" = "--format" ]; then
    printf '%s\tUp 1 minute\t\t%s\t%s\tapp\tFalse\n' "$EFFIGY_TEST_CONTAINER" "$EFFIGY_TEST_PROJECT" "$EFFIGY_TEST_WORKDIR"
    exit 0
  fi
done
case "$1" in
  inspect)
    printf '[{"Id":"%s","Name":"/%s","State":{"Status":"running","Running":true,"ExitCode":0},"Config":{"Labels":{"com.docker.compose.project":"%s","com.docker.compose.service":"app","com.docker.compose.oneoff":"False","com.docker.compose.project.working_dir":"%s"}},"Mounts":[],"NetworkSettings":{"Ports":{"80/tcp":[{"HostIp":"127.0.0.1","HostPort":"18080"}]}}}]\n' "$EFFIGY_TEST_ID" "$EFFIGY_TEST_CONTAINER" "$EFFIGY_TEST_PROJECT" "$EFFIGY_TEST_WORKDIR"
    exit 0
    ;;
  exec)
    echo 'OCI runtime exec failed: exec: "/workspace-root/app": stat /workspace-root/app: no such file or directory: unknown' >&2
    exit 126
    ;;
esac
exit 0
"#,
            )
            .expect("write fake docker");
        let mut perms = fs::metadata(&docker)
            .expect("stat fake docker")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&docker, perms).expect("chmod fake docker");
        let colima = bin.join("colima");
        fs::write(&colima, "#!/bin/sh\nexit 0\n").expect("write fake colima");
        let mut perms = fs::metadata(&colima)
            .expect("stat fake colima")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&colima, perms).expect("chmod fake colima");

        let home = parent.join("home");
        fs::create_dir_all(&home).expect("mkdir fake home");
        let env = crate::contract_test_support::EnvGuard::set_many(&[
            ("PATH", Some(bin.display().to_string())),
            ("HOME", Some(home.display().to_string())),
            ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            (
                "EFFIGY_TEST_DOCKER_LOG",
                Some(parent.join("docker.log").display().to_string()),
            ),
            ("EFFIGY_TEST_ID", Some(BINDING_ID.to_owned())),
            (
                "EFFIGY_TEST_CONTAINER",
                Some("up-binding-fixture-app-1".to_owned()),
            ),
            ("EFFIGY_TEST_PROJECT", Some("up-binding-fixture".to_owned())),
            ("EFFIGY_TEST_WORKDIR", Some(repo_root.display().to_string())),
        ]);
        UpBindingFixture {
            _temp: temp,
            repo_root,
            parent,
            project: "up-binding-fixture".to_owned(),
            _env: env,
        }
    }

    fn policy(&self) -> EffectiveContainerPolicy {
        load_container_policy(&self.repo_root, None).expect("fixture policy")
    }

    fn docker_log(&self) -> String {
        fs::read_to_string(self.parent.join("docker.log")).unwrap_or_default()
    }
}

/// `up` no longer probes exec readiness, so a `working_dir` the image does
/// not have is not a bring-up failure: the owned binding is proved from the
/// running inventory, and a user's own `effigy exec` reports its own error.
#[test]
fn container_up_binding_check_does_not_depend_on_the_image_working_dir() {
    let fixture = UpBindingFixture::new();
    let policy = fixture.policy();
    assert_eq!(policy.project_name, fixture.project);

    let binding = crate::runner::container_runtime_prep::ensure_primary_service_binding(
        &fixture.repo_root,
        &policy,
        None,
    )
    .expect("a missing image working_dir does not fail up");

    assert_eq!(binding.id(), BINDING_ID);
    let log = fixture.docker_log();
    assert!(
        !log.lines().any(|line| line.starts_with("exec ")),
        "up runs no exec readiness probe: {log}"
    );
}
