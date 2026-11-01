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
