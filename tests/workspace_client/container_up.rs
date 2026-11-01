//! Detached production-entry refusal control. This does not prove positive
//! container orchestration or qualify any native backend.
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{CancelToken, Deadline, EffectClass, Limits, OperationContext, Placement};
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{self, Phase};

#[test]
fn container_up_empty_budget_refuses_in_detached_supervisor_and_settles() {
    let fixture =
        effigy_test_support::Fixture::new("container-up-startup-refusal").expect("fixture");
    let state = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state).expect("state home");
    std::fs::create_dir_all(&project).expect("project");
    let digest = effigy_kernel::id::Digest::of(project.to_string_lossy().as_bytes());
    let checkout: Vec<u8> = digest.0.iter().copied().take(16).collect();
    let request = serde_json::json!({
        "schema": "effigy.workspace-serving.v1",
        "state_home": state,
        "project_path": project,
        "checkout": checkout,
        "environment": "dev",
        "profile": "default",
        "activation": {
            "kind": "ContainerUp",
            "request": { "environment": "dev", "retention": "persistent", "budget_ms": 0 },
        },
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("__workspace-serving-fixture")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("front end");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(request.to_string().as_bytes())
        .expect("intent");
    let output = child.wait_with_output().expect("front end exit");
    assert!(!output.status.success(), "startup was acknowledged");
    assert!(output.stdout.is_empty(), "no committed announcement");
    let error = String::from_utf8(output.stderr).expect("diagnostic");
    assert!(
        error.contains("workspace activation budget is empty"),
        "{error}"
    );
    let host_run = error
        .split("supervisor_run=")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .expect("host run")
        .parse()
        .expect("run identity");
    let observe = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(60)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        EffectClass::Observe,
        Placement::Host,
    )
    .observe()
    .expect("observe capability");
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(&project),
    };
    let root =
        match effigy_kernel::root::StateRoot::checkout_observe_in(&observe, &state, &location) {
            Observed::Verified(root, _) => root,
            other => panic!("host journal unavailable: {other:?}"),
        };
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Observed::Verified(reader, _) = effigy_kernel::store::Store::read(&observe, &root) {
            if let Observed::Verified(summary, _) = run::summary(&reader, host_run) {
                if summary.phase == Phase::Closed {
                    assert_eq!(summary.cleanup_complete, Some(true));
                    break;
                }
            }
        }
        assert!(Instant::now() < until, "refused supervisor did not settle");
        std::thread::sleep(Duration::from_millis(20));
    }
    // Refusal occurred before loading a declaration or preparing a backend.
    assert!(!project.join("effigy.toml").exists());
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
