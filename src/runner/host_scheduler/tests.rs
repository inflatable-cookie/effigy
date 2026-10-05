use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::PathBuf;

use effigy_core::task_selection::TaskSurface;
use effigy_host_run::{
    BudgetFallback, ClassSource, Priority, RunClass, Settlement, SettlementOutcome, SubmitRequest,
};
use effigy_manifest::{LoadedCatalog, TaskManifest};
use serde_json::{json, Value};

use super::submit::{
    forward_env_from, interpret, launched_exit_code, selector_env_names_for_tasks,
};
use super::{
    is_scheduler_owned_env, parse_override_reason, parse_scheduler_setting, queue_wait_from_status,
    PreLaunch, Settled,
};
use crate::runner::error::RunnerError;

fn code_of(error: &RunnerError) -> i32 {
    error
        .host_scheduler_exit_code()
        .expect("scheduler exit code")
}

fn settlement(outcome: SettlementOutcome, launched: bool, result: Option<Value>) -> Settlement {
    Settlement {
        run_id: "run-1".to_owned(),
        outcome,
        launched,
        settled_at: "2026-10-01T15:00:00Z".to_owned(),
        result,
        containers: Vec::new(),
        raw: Value::Null,
    }
}

fn exited(code: i64) -> Option<Value> {
    Some(json!({"exitCode": code, "signal": null}))
}

fn signalled(signal: Value) -> Option<Value> {
    Some(json!({"exitCode": null, "signal": signal}))
}

fn loaded_catalog(alias: &str, root: &str, manifest: &str, depth: usize) -> LoadedCatalog {
    let catalog_root = PathBuf::from(root);
    LoadedCatalog {
        alias: alias.to_owned(),
        manifest_path: catalog_root.join("effigy.toml"),
        catalog_root,
        bundle_root: None,
        manifest: toml::from_str::<TaskManifest>(manifest).expect("parse catalog fixture"),
        defer_run: None,
        deferred_builtins: BTreeSet::new(),
        depth,
        draft_sources: BTreeMap::new(),
    }
}

#[test]
fn scheduler_setting_accepts_unset_and_one_but_retires_zero() {
    assert!(parse_scheduler_setting(None).is_ok());
    assert!(parse_scheduler_setting(Some(OsString::from("1"))).is_ok());
    let retired = parse_scheduler_setting(Some(OsString::from("0"))).unwrap_err();
    assert_eq!(code_of(&retired), 2);
    assert!(retired
        .to_string()
        .contains("legacy admission backend was retired"));
    for invalid in ["", "yes", "true", "01", " 1", "2"] {
        let error = parse_scheduler_setting(Some(OsString::from(invalid))).unwrap_err();
        assert_eq!(code_of(&error), 2, "{invalid:?}");
        assert!(error.to_string().contains("must be `1` (host scheduler)"));
    }
}

#[test]
fn override_reason_must_be_present_bounded_and_printable() {
    assert_eq!(
        parse_override_reason(&OsString::from("  drill  ")).unwrap(),
        "drill"
    );
    for invalid in [
        String::new(),
        "   ".to_owned(),
        "x".repeat(501),
        "a\nb".to_owned(),
    ] {
        let error = parse_override_reason(&OsString::from(invalid)).unwrap_err();
        assert_eq!(code_of(&error), 2);
    }
}

#[test]
fn scheduler_owned_names_never_cross_boundaries() {
    assert!(is_scheduler_owned_env("HOST_RUN_TOKEN"));
    assert!(is_scheduler_owned_env("HOST_RUN_ID"));
    assert!(!is_scheduler_owned_env("HOST_RUN_OTHER"));
}

#[test]
fn submitted_environment_projects_composed_release_and_group_needs_without_ambient_credentials() {
    let catalogs = vec![
        loaded_catalog(
            "root",
            "/workspace/project",
            r#"
[env]
MANIFEST_VALUE = "catalog value"

[tasks.release]
run = [
  "task:builder/package",
  { run = "echo $TASK_MODE", env = "TASK_MODE" },
  { run = "echo $TASK_API_TOKEN", env = "TASK_API_TOKEN" },
  { run = "echo configured value", env = "MANIFEST_VALUE" },
  { run = "echo configured cross-catalog value", env = "catalog:root:MANIFEST_VALUE" },
]

[tasks.cycle_a]
run = [{ task = "cycle_b" }]

[tasks.cycle_b]
run = [
  { task = "cycle_a" },
  { run = "echo $CYCLE_CONTROL_TOKEN", env = "CYCLE_CONTROL_TOKEN" },
]

[tasks.unselected_release]
run = [{ run = "echo $UNSELECTED_RELEASE_TOKEN", env = "UNSELECTED_RELEASE_TOKEN" }]

[drafts.release_candidate]
created = "2026-09-15"
purpose = "Composed release draft fixture"
run = [{ draft = "builder/release_candidate" }]
"#,
            0,
        ),
        loaded_catalog(
            "builder",
            "/workspace/project/build",
            r#"
[env]
CHILD_MANIFEST_VALUE = "builder catalog value"

[tasks.package]
run = [{ task = "package_inner" }]

[tasks.package_inner]
run = [
  { run = "echo $RELEASE_SIGNING_TOKEN", env = "RELEASE_SIGNING_TOKEN" },
  { run = "echo configured child value", env = "CHILD_MANIFEST_VALUE" },
  { run = "echo configured qualified child value", env = "catalog:builder:CHILD_MANIFEST_VALUE" },
]

[drafts.release_candidate]
created = "2026-09-15"
purpose = "Composed release draft fixture"
run = [
  { task = "package_inner" },
  { run = "echo $DRAFT_RELEASE_TOKEN", env = "DRAFT_RELEASE_TOKEN" },
]
"#,
            1,
        ),
    ];
    // Each tuple is one resolved QA-group member (catalog alias, task name,
    // surface). The release task, draft member, and cyclic member must all
    // contribute only their reachable selector-declared environment names.
    let selector_env_names = selector_env_names_for_tasks(
        [
            ("root", "release", TaskSurface::Published),
            ("root", "release_candidate", TaskSurface::Draft),
            ("root", "cycle_a", TaskSurface::Published),
        ],
        &catalogs,
        &PathBuf::from("/workspace/project"),
    )
    .expect("resolve composed selector environment");
    for name in [
        "TASK_MODE",
        "TASK_API_TOKEN",
        "RELEASE_SIGNING_TOKEN",
        "DRAFT_RELEASE_TOKEN",
        "CYCLE_CONTROL_TOKEN",
    ] {
        assert!(selector_env_names.contains(name));
    }
    for name in [
        "MANIFEST_VALUE",
        "CHILD_MANIFEST_VALUE",
        "UNSELECTED_RELEASE_TOKEN",
    ] {
        assert!(!selector_env_names.contains(name));
    }

    let vars = vec![
        ("PATH", "caller-path"),
        ("HOME", "/caller/home"),
        ("HOST_RUN_TOKEN", "host-run-token-canary"),
        ("HOST_RUN_ID", "host-run-id-canary"),
        ("EFFIGY_ADMISSION_LEASE_ID", "retired-lease-canary"),
        ("EFFIGY_HOST_SCHEDULER", "1"),
        ("EFFIGY_HOST_RUN_ROOT", "/state/host-run"),
        ("EFFIGY_APP_SECRET", "effigy-app-secret-canary"),
        (
            "EFFIGY_RELEASE_SIGNING_SECRET",
            "effigy-release-secret-canary",
        ),
        ("EFFIGY_NUCLEUS_KEY", "effigy-nucleus-key-canary"),
        ("CARGO_HOME", "/cache/cargo"),
        ("CARGO_TARGET_DIR", "/cache/target"),
        ("CARGO_BUILD_JOBS", "7"),
        ("CARGO_INCREMENTAL", "0"),
        ("CARGO_NET_OFFLINE", "true"),
        ("CARGO_REGISTRIES_PRIVATE_TOKEN", "registry-token-canary"),
        (
            "CARGO_REGISTRIES_RELEASE_TOKEN",
            "cargo-release-token-canary",
        ),
        ("CARGO_RELEASE_TOKEN", "cargo-release-prefix-canary"),
        ("CARGO_HTTP_TOKEN", "cargo-http-token-canary"),
        ("CARGO_NETRC_PASSWORD", "cargo-netrc-password-canary"),
        ("EFFIGY_RELEASE_TOKEN", "effigy-release-prefix-canary"),
        ("RUSTFLAGS", "-D warnings"),
        ("CI", "true"),
        ("TASK_MODE", "release"),
        ("TASK_API_TOKEN", "selector-api-token-canary"),
        ("MANIFEST_VALUE", "manifest-shadow-canary"),
        ("CHILD_MANIFEST_VALUE", "child-manifest-shadow-canary"),
        ("RELEASE_SIGNING_TOKEN", "release-signing-canary"),
        ("DRAFT_RELEASE_TOKEN", "draft-release-canary"),
        ("CYCLE_CONTROL_TOKEN", "cycle-control-canary"),
        ("UNSELECTED_RELEASE_TOKEN", "unselected-release-canary"),
        ("APPLE_PASSWORD", "apple-password-canary"),
        ("RELEASE_GITHUB_TOKEN", "release-github-token-canary"),
        (
            "UPDATE_SIGNING_SECRET_KEY_FILE",
            "update-signing-key-file-canary",
        ),
        ("NUCLEUS_KEY", "nucleus-key-canary"),
        ("MESSAGING_TOKEN", "messaging-token-canary"),
        ("MESSAGING_SESSION_TOKEN", "messaging-session-canary"),
        ("SESSION_TOKEN", "session-token-canary"),
        ("SESSION_COOKIE", "session-cookie-canary"),
        ("NUCLEUS_PLANE_KEY", "nucleus-plane-key-canary"),
        ("GITHUB_TOKEN", "github-token-canary"),
        ("SSH_AUTH_SOCK", "/tmp/ssh-agent.sock"),
        ("SHELL", "/bin/zsh"),
        ("TERM", "xterm-256color"),
        ("PWD", "/caller/worktree"),
        ("OLDPWD", "/caller/previous"),
        ("A=B", "invalid-name-canary"),
        ("", "empty-name-canary"),
    ]
    .into_iter()
    .map(|(key, value)| (OsString::from(key), OsString::from(value)))
    .collect::<Vec<_>>();
    let projected = forward_env_from(vars.into_iter(), &selector_env_names);
    let request = SubmitRequest {
        client_request_id: "request-1".to_owned(),
        caller: "test".to_owned(),
        repository: "/workspace/project".to_owned(),
        cwd: "/workspace/project".into(),
        selector: "qa-group:root/release".to_owned(),
        argv: vec![
            "/usr/bin/effigy".to_owned(),
            "tasks".to_owned(),
            "qa-group".to_owned(),
            "run".to_owned(),
            "root/release".to_owned(),
        ],
        class: RunClass::Heavy,
        class_source: ClassSource::Manifest,
        priority: Priority::Validation,
        budget_fallback: BudgetFallback {
            cpu: 4,
            memory_bytes: 4 * 1024 * 1024 * 1024,
        },
        capacity_deadline_ms: 30_000,
        run_timeout_ms: 60_000,
        env: projected,
        cancel_on_disconnect: false,
    };
    let serialized = serde_json::to_string(&request).expect("serialize request");
    let wire = serde_json::from_str::<Value>(&serialized).expect("parse serialized request");
    let env = wire["env"].as_object().expect("serialized env map");
    let actual_env_names = env.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected_env_names = [
        "EFFIGY_HOST_SCHEDULER",
        "EFFIGY_HOST_RUN_ROOT",
        "CI",
        "CARGO_HOME",
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_JOBS",
        "CARGO_INCREMENTAL",
        "CARGO_NET_OFFLINE",
        "RUSTFLAGS",
        "TASK_MODE",
        "TASK_API_TOKEN",
        "RELEASE_SIGNING_TOKEN",
        "DRAFT_RELEASE_TOKEN",
        "CYCLE_CONTROL_TOKEN",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(actual_env_names, expected_env_names);

    for canary in [
        "host-run-token-canary",
        "host-run-id-canary",
        "retired-lease-canary",
        "effigy-app-secret-canary",
        "effigy-release-secret-canary",
        "effigy-nucleus-key-canary",
        "registry-token-canary",
        "cargo-release-token-canary",
        "cargo-release-prefix-canary",
        "cargo-http-token-canary",
        "cargo-netrc-password-canary",
        "effigy-release-prefix-canary",
        "apple-password-canary",
        "release-github-token-canary",
        "update-signing-key-file-canary",
        "nucleus-key-canary",
        "messaging-token-canary",
        "messaging-session-canary",
        "session-token-canary",
        "session-cookie-canary",
        "nucleus-plane-key-canary",
        "github-token-canary",
        "manifest-shadow-canary",
        "child-manifest-shadow-canary",
        "unselected-release-canary",
        "caller-path",
        "/caller/home",
        "/caller/worktree",
        "/caller/previous",
        "invalid-name-canary",
        "empty-name-canary",
    ] {
        assert!(!serialized.contains(canary));
    }
    assert!(!env.contains_key("PATH"));
    assert!(!env.contains_key("HOME"));
    assert!(!env.contains_key("HOST_RUN_TOKEN"));
    assert!(!env.contains_key("HOST_RUN_ID"));
    assert!(!env.contains_key("EFFIGY_ADMISSION_LEASE_ID"));
    assert!(!env.contains_key("CARGO_REGISTRIES_PRIVATE_TOKEN"));
    assert!(!env.contains_key("CARGO_REGISTRIES_RELEASE_TOKEN"));
    assert!(!env.contains_key("CARGO_RELEASE_TOKEN"));
    assert!(!env.contains_key("CARGO_HTTP_TOKEN"));
    assert!(!env.contains_key("CARGO_NETRC_PASSWORD"));
    assert!(!env.contains_key("EFFIGY_APP_SECRET"));
    assert!(!env.contains_key("EFFIGY_RELEASE_SIGNING_SECRET"));
    assert!(!env.contains_key("EFFIGY_RELEASE_TOKEN"));
    assert!(!env.contains_key("EFFIGY_NUCLEUS_KEY"));
    assert!(!env.contains_key("APPLE_PASSWORD"));
    assert!(!env.contains_key("RELEASE_GITHUB_TOKEN"));
    assert!(!env.contains_key("UPDATE_SIGNING_SECRET_KEY_FILE"));
    assert!(!env.contains_key("NUCLEUS_KEY"));
    assert!(!env.contains_key("MESSAGING_TOKEN"));
    assert!(!env.contains_key("MESSAGING_SESSION_TOKEN"));
    assert!(!env.contains_key("SESSION_TOKEN"));
    assert!(!env.contains_key("SESSION_COOKIE"));
    assert!(!env.contains_key("NUCLEUS_PLANE_KEY"));
    assert!(!env.contains_key("SHELL"));
    assert!(!env.contains_key("TERM"));
    assert!(!env.contains_key("PWD"));
    assert!(!env.contains_key("OLDPWD"));
    assert!(matches!(
        env.get("EFFIGY_HOST_SCHEDULER").and_then(Value::as_str),
        Some("1")
    ));
    assert!(matches!(
        env.get("EFFIGY_HOST_RUN_ROOT").and_then(Value::as_str),
        Some("/state/host-run")
    ));
    assert!(matches!(
        env.get("CARGO_HOME").and_then(Value::as_str),
        Some("/cache/cargo")
    ));
    assert!(matches!(
        env.get("CARGO_TARGET_DIR").and_then(Value::as_str),
        Some("/cache/target")
    ));
    assert!(matches!(
        env.get("CARGO_BUILD_JOBS").and_then(Value::as_str),
        Some("7")
    ));
    assert!(matches!(
        env.get("CARGO_INCREMENTAL").and_then(Value::as_str),
        Some("0")
    ));
    assert!(matches!(
        env.get("CARGO_NET_OFFLINE").and_then(Value::as_str),
        Some("true")
    ));
    assert!(matches!(
        env.get("RUSTFLAGS").and_then(Value::as_str),
        Some("-D warnings")
    ));
    assert!(matches!(
        env.get("CI").and_then(Value::as_str),
        Some("true")
    ));
    assert!(matches!(
        env.get("TASK_MODE").and_then(Value::as_str),
        Some("release")
    ));
    assert!(env.contains_key("TASK_API_TOKEN"));
    assert!(env.contains_key("RELEASE_SIGNING_TOKEN"));
    assert!(env.contains_key("DRAFT_RELEASE_TOKEN"));
    assert!(env.contains_key("CYCLE_CONTROL_TOKEN"));
    assert!(serialized.contains("selector-api-token-canary"));
    assert!(serialized.contains("release-signing-canary"));
    assert!(serialized.contains("draft-release-canary"));
    assert!(serialized.contains("cycle-control-canary"));
    assert_eq!(wire["selector"], "qa-group:root/release");
    assert_eq!(
        wire["argv"],
        json!([
            "/usr/bin/effigy",
            "tasks",
            "qa-group",
            "run",
            "root/release"
        ])
    );
    assert_eq!(wire["class"], "heavy");
    assert_eq!(wire["classSource"], "manifest");
}

#[cfg(unix)]
#[test]
fn submitted_environment_drops_invalid_utf8_and_nul_entries() {
    use std::os::unix::ffi::OsStringExt;

    let declared = ["MALFORMED_VALUE".to_owned(), "NUL_VALUE".to_owned()]
        .into_iter()
        .collect();
    let vars = vec![
        (
            OsString::from_vec(vec![0xff]),
            OsString::from("invalid-key-canary"),
        ),
        (
            OsString::from("MALFORMED_VALUE"),
            OsString::from_vec(vec![0xfe]),
        ),
        (OsString::from("NUL_VALUE"), OsString::from("nul\0value")),
        (
            OsString::from("NUL_NAME\0"),
            OsString::from("nul-name-canary"),
        ),
    ];
    let projected = forward_env_from(vars.into_iter(), &declared);
    assert!(projected.is_empty());
}

#[test]
fn child_status_is_preserved_and_a_non_pass_never_reports_zero() {
    let exit = |outcome, result| launched_exit_code(&settlement(outcome, true, result));
    assert_eq!(exit(SettlementOutcome::Passed, exited(0)), 0);
    assert_eq!(exit(SettlementOutcome::Failed, exited(7)), 7);
    assert_eq!(
        exit(SettlementOutcome::Failed, signalled(json!("SIGKILL"))),
        137
    );
    assert_eq!(exit(SettlementOutcome::Failed, signalled(json!(15))), 143);
    assert_eq!(
        exit(SettlementOutcome::Cancelled, signalled(json!("SIGTERM"))),
        143
    );
    // An inconsistent outcome with exit 0 is not a pass.
    assert_eq!(exit(SettlementOutcome::Failed, exited(0)), 1);
    assert_eq!(exit(SettlementOutcome::Cancelled, exited(0)), 1);
    assert_eq!(exit(SettlementOutcome::TimedOut, exited(0)), 124);
    assert_eq!(
        exit(SettlementOutcome::TimedOut, signalled(json!("SIGTERM"))),
        124
    );
    assert_eq!(exit(SettlementOutcome::Lost, None), 70);
}

#[test]
fn unlaunched_settlements_are_typed_prelaunch_outcomes_with_measured_wait() {
    let capacity = interpret(
        &settlement(SettlementOutcome::CapacityTimeout, false, None),
        3,
        Some(5),
        1_200,
        None,
    );
    assert_eq!(
        capacity,
        Settled::NotLaunched {
            run_id: "run-1".to_owned(),
            epoch: 3,
            reason: PreLaunch::CapacityTimeout,
            queue_wait_ms: Some(1_200),
            interrupt_signal: None,
        }
    );
    let cancelled = interpret(
        &settlement(SettlementOutcome::Cancelled, false, None),
        3,
        None,
        40,
        Some(15),
    );
    let error = cancelled.into_error();
    assert_eq!(code_of(&error), 143);
    assert!(error.to_string().contains("cancelled before launch"));
    let timeout_error = capacity.into_error();
    assert_eq!(code_of(&timeout_error), 1);
    assert!(timeout_error.to_string().contains("capacity_timeout"));
    assert!(timeout_error
        .to_string()
        .contains("not a validation failure"));
}

#[test]
fn launched_settlements_keep_unknown_queue_wait_unknown() {
    let settled = interpret(
        &settlement(SettlementOutcome::Passed, true, exited(0)),
        2,
        None,
        900,
        None,
    );
    assert_eq!(
        settled,
        Settled::Launched {
            run_id: "run-1".to_owned(),
            epoch: 2,
            exit_code: 0,
            outcome: SettlementOutcome::Passed,
            queue_wait_ms: None,
        }
    );
    // The settled child's status is relayed by the entrypoint, never rendered.
    assert!(matches!(
        settled.into_error(),
        RunnerError::HostRunSettled { code: 0 }
    ));
}

#[test]
fn nested_queue_wait_comes_from_the_status_record_and_is_null_when_absent() {
    let status = json!({
        "submittedAt": "2026-10-01T15:00:00.000Z",
        "admittedAt": "2026-10-01T15:00:02.250Z",
    });
    assert_eq!(queue_wait_from_status(&status), Some(2_250));
    assert_eq!(
        queue_wait_from_status(&json!({"submittedAt": "2026-10-01T15:00:00Z"})),
        None
    );
    assert_eq!(queue_wait_from_status(&json!({})), None);
    let inverted = json!({
        "submittedAt": "2026-10-01T15:00:05Z",
        "admittedAt": "2026-10-01T15:00:00Z",
    });
    assert_eq!(queue_wait_from_status(&inverted), None);
}

/// Serializes the termination-proof tests that install the process-wide
/// signal-forwarding scope with timeout-descendant interrupt proofs.
#[cfg(unix)]
fn owned_children_test_serial() -> std::sync::MutexGuard<'static, ()> {
    crate::runner::owned_children::hold_group_cleanup_test_lock()
}

/// Owns the two children a termination proof creates. `cleanup` reaps them
/// explicitly and reports the outcome; `Drop` is the unconditional backstop so
/// an assertion failure or panic still kills and reaps both children.
#[cfg(unix)]
struct TerminationProofChildren {
    owned: Option<std::process::Child>,
    foreign: Option<std::process::Child>,
}

#[cfg(unix)]
impl TerminationProofChildren {
    fn spawn_pair() -> Self {
        // Put the first child under the RAII fixture *before* spawning the
        // second, so a panic in the second spawn still unwinds through `Drop`
        // and reaps the first child instead of leaking it.
        let mut children = Self {
            owned: Some(Self::spawn_one()),
            foreign: None,
        };
        children.foreign = Some(Self::spawn_one());
        children
    }

    fn spawn_one() -> std::process::Child {
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .expect("spawn sleep")
    }

    fn owned_mut(&mut self) -> &mut std::process::Child {
        self.owned.as_mut().expect("owned child present")
    }

    fn foreign_mut(&mut self) -> &mut std::process::Child {
        self.foreign.as_mut().expect("foreign child present")
    }

    /// Kill and reap both children exactly once. Returns whether reaping
    /// succeeded for both, which is the cleanup proof.
    fn cleanup(&mut self) -> bool {
        let owned = Self::reap_exact(self.owned.take());
        let foreign = Self::reap_exact(self.foreign.take());
        owned && foreign
    }

    fn reap_exact(child: Option<std::process::Child>) -> bool {
        match child {
            Some(mut child) => {
                let _ = child.kill();
                child.wait().is_ok()
            }
            None => false,
        }
    }
}

#[cfg(unix)]
impl Drop for TerminationProofChildren {
    fn drop(&mut self) {
        let _ = Self::reap_exact(self.owned.take());
        let _ = Self::reap_exact(self.foreign.take());
    }
}

#[cfg(unix)]
struct TerminationObservation {
    /// True only when the registered child was observed terminated before any
    /// cleanup ran. A fallback kill is never counted as evidence.
    owned_terminated_before_cleanup: bool,
    /// True only when the unregistered child was still alive before cleanup.
    foreign_alive_before_cleanup: bool,
    /// True when unconditional exact-owned cleanup reaped both children.
    cleanup_reaped_both: bool,
}

/// Enter the real owned-children scope, raise SIGTERM to this process, and
/// report what the signal forwarder did before cleanup. Delivery is governed by
/// the production scope; the test-only seam in `owned_children` can disable
/// delivery for the negative proof.
#[cfg(unix)]
fn observe_registered_child_termination(wait: std::time::Duration) -> TerminationObservation {
    use std::time::Instant;

    let mut children = TerminationProofChildren::spawn_pair();
    let mut owned_terminated_before_cleanup = false;
    let foreign_alive_before_cleanup;
    {
        let _scope =
            crate::runner::owned_children::OwnedChildrenScope::enter().expect("enter scope");
        assert!(crate::runner::owned_children::signal_scope_active());
        crate::runner::owned_children::register_process_group(children.owned_mut().id());
        unsafe {
            libc::raise(libc::SIGTERM);
        }
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            match children.owned_mut().try_wait().expect("poll owned") {
                Some(status) => {
                    // The observed status must be a termination, never success.
                    owned_terminated_before_cleanup = !status.success();
                    break;
                }
                None => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        foreign_alive_before_cleanup = children
            .foreign_mut()
            .try_wait()
            .expect("poll foreign")
            .is_none();
    }
    let cleanup_reaped_both = children.cleanup();
    TerminationObservation {
        owned_terminated_before_cleanup,
        foreign_alive_before_cleanup,
        cleanup_reaped_both,
    }
}

#[cfg(unix)]
#[test]
fn owned_children_scope_forwards_termination_only_to_registered_groups() {
    let _serial = owned_children_test_serial();
    let observation = observe_registered_child_termination(std::time::Duration::from_secs(10));
    assert!(
        observation.owned_terminated_before_cleanup,
        "the registered child was terminated by the scope before any cleanup"
    );
    assert!(
        observation.foreign_alive_before_cleanup,
        "an unregistered process group is never signalled"
    );
    assert!(
        observation.cleanup_reaped_both,
        "unconditional exact-owned cleanup reaps both test-created children"
    );
}

#[cfg(unix)]
#[test]
fn owned_children_termination_oracle_fails_when_forwarding_is_disabled() {
    let _serial = owned_children_test_serial();
    let _seam = crate::runner::owned_children::disable_forwarding_for_test();
    let observation = observe_registered_child_termination(std::time::Duration::from_secs(1));
    assert!(
        !observation.owned_terminated_before_cleanup,
        "with forwarding disabled the termination oracle must fail: no pre-cleanup termination"
    );
    assert!(
        observation.foreign_alive_before_cleanup,
        "a disabled forwarder still never signals the unregistered group"
    );
    assert!(
        observation.cleanup_reaped_both,
        "cleanup still reaps both test-created children when the oracle fails"
    );
}
