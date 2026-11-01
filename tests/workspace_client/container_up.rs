//! Detached production-entry refusal control. This does not prove positive
//! container orchestration or qualify any native backend.
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn container_up_detached_preflight_refuses_before_any_journal_token_or_mount_effect() {
    for (case, budget, checkout, reason) in [
        (
            "empty-budget",
            0,
            [1u8; 16],
            "workspace activation budget is empty",
        ),
        ("wrong-checkout", 1000, [2u8; 16], "checkout differs"),
        (
            "absent-backend",
            1000,
            [1u8; 16],
            "identity observation unavailable",
        ),
    ] {
        let fixture = effigy_test_support::Fixture::new(case).expect("fixture");
        let state = fixture.path().join("state");
        let project = fixture.path().join("project");
        std::fs::create_dir_all(&state).expect("state home");
        std::fs::create_dir_all(project.join(".git")).expect("project identity");
        let token = project.join(".git/effigy-checkout-id");
        let original_token = "01010101010101010101010101010101";
        std::fs::write(&token, original_token).expect("preexisting identity");
        let request = serde_json::json!({
            "schema": "effigy.workspace-serving.v1", "state_home": state,
            "project_path": project, "checkout": checkout, "environment": "dev",
            "profile": "default", "activation": { "kind": "ContainerUp",
                "request": {"environment":"dev", "retention":"persistent", "budget_ms":budget}},
        });
        let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("__workspace-serving-fixture")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("real front end and detached host");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(request.to_string().as_bytes())
            .expect("intent");
        let output = child.wait_with_output().expect("front end exit");
        assert!(!output.status.success(), "{case}: startup acknowledged");
        assert!(
            output.stdout.is_empty(),
            "{case}: no committed announcement"
        );
        let error = String::from_utf8(output.stderr).expect("diagnostic");
        assert!(error.contains(reason), "{case}: {error}");
        assert!(
            !error.contains("supervisor_run="),
            "{case}: a run was admitted: {error}"
        );
        assert!(
            !project.join(".effigy/state/kernel.db").exists(),
            "{case}: checkout journal was created"
        );
        assert!(
            !state.join(".effigy/state/kernel.db").exists(),
            "{case}: home journal was created"
        );
        assert!(
            !state.join(".effigy/state/machine.db").exists(),
            "{case}: machine journal was created"
        );
        assert_eq!(
            std::fs::read_to_string(&token).expect("identity remains"),
            original_token
        );
        assert!(
            !project.join("prepared").exists(),
            "{case}: mount source was prepared"
        );
    }
}

#[test]
fn container_mode_cli_refuses_before_mount_preparation_or_backend_effects() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = effigy_test_support::Fixture::new("container-up-cli-refusal").expect("fixture");
    let project = fixture.path().join("project");
    let home = fixture.path().join("home");
    let bin = fixture.path().join("fake-bin");
    for directory in [&project, &home, &bin] {
        std::fs::create_dir_all(directory).expect("fixture directory");
    }
    std::fs::write(
        project.join("effigy.toml"),
        r#"
[containers]
default = "web"
[containers.web]
compose_file = "compose.yml"
primary_service = "app"
[systems]
default = "dev"
[systems.dev]
default_workspace = "app"
[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace"
user = "1000:1000"
home = "/home/operator"
mounts = ["prepared:/workspace/prepared:ro"]
"#,
    )
    .expect("declaration");
    std::fs::write(
        project.join("compose.yml"),
        "services:\n  app:\n    image: example:test\n",
    )
    .expect("compose");
    for name in ["docker", "colima", "nerdctl"] {
        let executable = bin.join(name);
        std::fs::write(
            &executable,
            "#!/bin/sh\nprintf called >> \"$EFFIGY_TEST_RUNTIME_MARKER\"\nexit 93\n",
        )
        .expect("fake runtime");
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))
            .expect("fake executable");
    }
    let marker = fixture.path().join("runtime-called");
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&project)
        .args(["container", "up", "--detach", "--json"])
        .env("HOME", &home)
        .env("PATH", std::env::join_paths(paths).expect("fixture path"))
        .env("EFFIGY_TEST_RUNTIME_MARKER", &marker)
        .output()
        .expect("real CLI process");
    assert!(
        !output.status.success(),
        "unsupported activation acknowledged"
    );
    let result = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        result.contains("bounded workspace Phase-A preparation is unavailable"),
        "{result}"
    );
    assert!(!marker.exists(), "runtime was contacted");
    assert!(
        !project.join("prepared").exists(),
        "bind source was prepared"
    );
    assert!(
        !project.join(".effigy").exists(),
        "checkout state was materialized"
    );
}

fn cleanup_request(fixture: &effigy_test_support::Fixture, unknown: bool) -> serde_json::Value {
    let state = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state).expect("state");
    std::fs::create_dir_all(&project).expect("project");
    let digest = effigy_kernel::id::Digest::of(project.to_string_lossy().as_bytes());
    let checkout: Vec<u8> = digest.0.iter().copied().take(16).collect();
    serde_json::json!({
        "schema":"effigy.workspace-serving.v1", "state_home":state,
        "project_path":project, "checkout":checkout, "environment":"cleanup-proof", "profile":"default",
        "activation":{"kind":"Disposable", "supervisor_id":"cleanup-proof", "capability":super::CAPABILITY,
            "tls_budget_ms":1000, "omit_selection_record":true,
            "fail_refusal_cleanup":!unknown, "unknown_refusal_cleanup":unknown},
    })
}

#[test]
fn refused_service_retains_exact_cleanup_custody_until_explicit_stop() {
    use effigy_kernel::run::Phase;
    use std::time::{Duration, Instant};
    let fixture = effigy_test_support::Fixture::new("workspace-refusal-cleanup").expect("fixture");
    let state = fixture.path().join("state");
    let project = fixture.path().join("project");
    let request = cleanup_request(&fixture, false);
    let mut probe = effigy::workspace_serving_fixture::start_refused_host_probe(
        &request.to_string(),
        std::path::Path::new(env!("CARGO_BIN_EXE_effigy")),
    )
    .expect("original detached owner");
    let host_run = probe.run();
    let error = probe.await_refusal().expect("refusal with retained pipe");
    assert!(
        error.contains("no committed selection"),
        "primary error lost: {error}"
    );
    assert!(
        error.contains("cleanup failed"),
        "cleanup error lost: {error}"
    );
    assert!(error.contains("exact owner retained"), "{error}");
    assert!(
        error.contains("key_held=false serving=false"),
        "listener/key remained servable: {error}"
    );
    let session_run = error
        .split("session_run=")
        .nth(1)
        .and_then(|value| value.split(')').next())
        .expect("exact session run")
        .parse()
        .expect("session identity");
    let endpoint = error
        .split("listener=")
        .nth(1)
        .and_then(|value| value.split_whitespace().next())
        .expect("retained lease endpoint");
    let address = super::endpoint_of(endpoint);
    assert!(
        std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err(),
        "stopped listener still accepts grants"
    );
    let effect = super::effects();
    let observe = super::observation();
    let root = super::state_root(&effect, &state, &project);
    for run in [host_run, session_run] {
        assert_ne!(
            super::read_summary(&observe, &root, run)
                .expect("exact run retained")
                .phase,
            Phase::Closed
        );
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_ne!(
        super::read_summary(&observe, &root, session_run)
            .expect("no automatic retry")
            .phase,
        Phase::Closed
    );
    probe.stop().expect("explicit exact-host stop");
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let closed = [host_run, session_run].into_iter().all(|run| {
            super::read_summary(&observe, &root, run).is_some_and(|summary| {
                summary.phase == Phase::Closed && summary.cleanup_complete == Some(true)
            })
        });
        if closed {
            break;
        }
        assert!(
            Instant::now() < until,
            "original owners did not recover cleanup"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    super::settle_fixture(&fixture);
}

#[test]
fn unknown_cleanup_retains_same_session_until_verified_explicit_recovery() {
    use effigy_kernel::observe::Observed;
    use effigy_kernel::run::Phase;
    use std::time::{Duration, Instant};
    let fixture = effigy_test_support::Fixture::new("workspace-unknown-cleanup").expect("fixture");
    let request = cleanup_request(&fixture, true);
    let state = fixture.path().join("state");
    let project = fixture.path().join("project");
    let resource = state.join("cleanup-probe");
    let release = state.join("cleanup-probe-release");
    let mut probe = effigy::workspace_serving_fixture::start_refused_host_probe(
        &request.to_string(),
        std::path::Path::new(env!("CARGO_BIN_EXE_effigy")),
    )
    .expect("same detached host");
    let host_run = probe.run();
    let error = probe.await_refusal().expect("Unknown cleanup refusal");
    let session_run = error
        .split("session_run=")
        .nth(1)
        .and_then(|value| value.split(')').next())
        .expect("exact session run")
        .parse()
        .expect("session identity");
    let effect = super::effects();
    let observe = super::observation();
    let root = super::state_root(&effect, &state, &project);
    let summary =
        super::read_summary(&observe, &root, session_run).expect("original session journal");
    if summary.phase == Phase::Closed {
        // Failing-first cleanup of this test-owned fake file is not a release
        // receipt and is never represented as successful session cleanup.
        drop(probe);
        std::fs::remove_file(&resource).expect("remove fake residue after reproduced defect");
        super::settle_fixture(&fixture);
        panic!("Unknown adapter cleanup incorrectly closed original session {session_run} (cleanup={:?}) and discarded exact custody", summary.cleanup_complete);
    }
    assert!(
        error.contains("cleanup failed") && error.contains("exact owner retained"),
        "{error}"
    );
    assert!(
        !error.contains("private-cleanup-bytes-6f82c9"),
        "untrusted cleanup diagnostic escaped"
    );
    assert!(error.contains("key_held=false serving=false"), "{error}");
    assert!(resource.is_file(), "Unknown resource silently removed");
    let reader = match effigy_kernel::store::Store::read(&observe, &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("journal unavailable: {other:?}"),
    };
    if let Observed::Verified(effects, _) = effigy_kernel::run::effects(&reader, session_run) {
        assert!(
            effects.iter().all(|effect| !effect
                .reason
                .as_deref()
                .unwrap_or_default()
                .contains("private-cleanup-bytes-6f82c9")),
            "untrusted diagnostic recorded as evidence"
        );
    }
    drop(reader);
    std::fs::write(&release, b"explicit fake backend release permitted")
        .expect("allow actual fake-file cleanup");
    assert!(
        resource.is_file(),
        "recovery must wait for the original owner's control request"
    );
    probe.stop().expect("stop original exact host");
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if [host_run, session_run].into_iter().all(|run| {
            super::read_summary(&observe, &root, run).is_some_and(|summary| {
                summary.phase == Phase::Closed && summary.cleanup_complete == Some(true)
            })
        }) {
            break;
        }
        assert!(
            Instant::now() < until,
            "Unknown cleanup did not recover exact original IDs"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !resource.exists(),
        "fake backend resource not actually cleaned"
    );
    super::settle_fixture(&fixture);
}
