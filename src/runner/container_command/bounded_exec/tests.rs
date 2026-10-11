//! Private fake-backend production-path proof for the bounded opt-in surface.
//!
//! These tests drive [`run_bounded_command_with_deadline`] — the same plan
//! construction, spawned client, bounded concurrent readers, owned-group
//! termination and workspace-user fallback the CLI uses — against private fake
//! `colima`/`docker` doubles on `PATH`. No live runtime, image, service or
//! Reference resource is touched. The doubles answer from a marker embedded in
//! the container command string, so no process-global env is needed to select
//! behaviour beyond `PATH`.
//!
//! Limits disclosed: because session resolution needs a live runtime, these
//! fixtures enter below it. That resolution is covered by the shared container
//! command tests; the bounded operation's own accounting, selection arguments,
//! exit distinctions and owned cleanup are covered here with real processes.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_cli::BoundedExecLimits;

use super::{run_bounded_command_with_deadline, BoundedCommandRequest};
use crate::contract_test_support::{lock_test, EnvGuard};
use crate::runner::error::RunnerError;
use crate::runner::test_support::effective_container_policy;

const FAKE_BACKEND: &str = r#"#!/bin/sh
# Private bounded-exec backend double. Answers from a marker in the wrapped
# container command string, never from a live runtime.
if [ -n "$FAKE_BOUNDED_MARKER" ]; then
    : > "$FAKE_BOUNDED_MARKER"
fi
args="$*"
marker() {
    case "$args" in
        *"$1"*) return 0 ;;
        *) return 1 ;;
    esac
}
if marker fallback-budget; then
    case "$args" in
        *"-u dev"*)
            printf 'aaa'
            printf 'unable to find user dev: no matching entries in passwd file' >&2
            exit 1
            ;;
        *)
            printf 'bb'
            exit 0
            ;;
    esac
fi
if marker fallback-root; then
    case "$args" in
        *"-u dev"*)
            printf 'unable to find user dev: no matching entries in passwd file' >&2
            exit 1
            ;;
        *)
            printf 'root-out'
            exit 0
            ;;
    esac
fi
if marker big-stdout; then
    i=0
    while [ $i -lt 4096 ]; do
        printf 'x'
        i=$((i+1))
    done
    exit 0
fi
if marker big-stderr; then
    i=0
    while [ $i -lt 4096 ]; do
        printf 'y' >&2
        i=$((i+1))
    done
    exit 0
fi
if marker exit-7; then
    printf 'inner-out'
    printf 'inner-err' >&2
    exit 7
fi
if marker sleep-30; then
    exec /bin/sleep 30
fi
printf 'ok'
exit 0
"#;

fn fixture(name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let bin_dir = dir.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    for program in ["colima", "docker"] {
        let path = bin_dir.join(program);
        fs::write(&path, FAKE_BACKEND).expect("write fake backend");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&path).expect("stat").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("chmod");
        }
    }
    let repo_root = dir.path().join(format!("repo-{name}"));
    fs::create_dir_all(&repo_root).expect("mkdir repo");
    (dir, bin_dir, repo_root)
}

fn policy() -> effigy_containers::EffectiveContainerPolicy {
    let mut policy = effective_container_policy("web", "demo-web", "app", "docker-compose.yml");
    policy.workspace_user = Some("dev".to_owned());
    policy.workspace_home = Some("/home/dev".to_owned());
    policy
}

fn limits(deadline_ms: u64, stdout_bytes: u64, stderr_bytes: u64) -> BoundedExecLimits {
    BoundedExecLimits {
        deadline_ms,
        stdout_limit_bytes: stdout_bytes,
        stderr_limit_bytes: stderr_bytes,
    }
}

fn run_json(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    command: &str,
    limits: &BoundedExecLimits,
    deadline_ms: u64,
) -> Result<String, RunnerError> {
    run_bounded_command_with_deadline(
        BoundedCommandRequest {
            repo_root,
            policy,
            service: "app",
            working_dir: Some(Path::new("/workspace")),
            command,
            output_json: true,
            deadline: Instant::now() + Duration::from_millis(deadline_ms),
        },
        limits,
    )
}

fn failure_details(error: &RunnerError) -> serde_json::Value {
    let details = error
        .json_error_details()
        .expect("bounded failure must carry json details");
    serde_json::from_str(details).expect("parse bounded failure details")
}

fn with_fake_backend<T>(bin_dir: &Path, backend: &str, f: impl FnOnce() -> T) -> T {
    let _lock = lock_test();
    let current = std::env::var("PATH").unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), current);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("EFFIGY_COMPOSE_BACKEND", Some(backend.to_owned())),
        ("FAKE_BOUNDED_MARKER", None),
    ]);
    f()
}

#[test]
fn complete_command_returns_bounded_json_success() {
    let (_dir, bin_dir, repo) = fixture("complete");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        let output = run_json(&repo, &policy, "plain", &limits(30_000, 64, 64), 30_000)
            .expect("bounded command succeeds");
        let value: serde_json::Value = serde_json::from_str(&output).expect("json");
        assert_eq!(value["schema"], "effigy.container.bounded-exec.v1");
        assert_eq!(value["ok"], true);
        assert_eq!(value["code"], 0);
        assert_eq!(value["stdout"], "ok");
        assert_eq!(value["stdout_bytes"], 2);
        assert_eq!(value["stderr_bytes"], 0);
        assert_eq!(value["attempts"], 1);
        assert_eq!(value["workspace_user_fallback"], false);
        assert_eq!(value["remote_cancellation"], "unverified");
    });
}

#[test]
fn docker_and_colima_backend_transport_paths_both_execute() {
    let (_dir, bin_dir, repo) = fixture("both-backends");
    let policy = policy();
    for backend in ["docker", "colima"] {
        with_fake_backend(&bin_dir, backend, || {
            let output = run_json(&repo, &policy, "plain", &limits(30_000, 64, 64), 30_000)
                .unwrap_or_else(|error| panic!("{backend} backend failed: {error}"));
            let value: serde_json::Value = serde_json::from_str(&output).expect("json");
            assert_eq!(value["ok"], true, "{backend} backend");
        });
    }
}

#[test]
fn one_byte_over_stdout_budget_refuses_with_safe_diagnostics() {
    let (_dir, bin_dir, repo) = fixture("stdout-over");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        let error = run_json(&repo, &policy, "big-stdout", &limits(30_000, 4, 64), 30_000)
            .expect_err("stdout overflow must refuse");
        assert!(matches!(
            error,
            RunnerError::BoundedExecFailure { code: 1, .. }
        ));
        let details = failure_details(&error);
        assert_eq!(details["failure"], "stdout_overflow");
        assert_eq!(details["stream"], "stdout");
        assert_eq!(details["limit_bytes"], 4);
        // No captured bytes may leak into the diagnostic.
        assert!(!details.to_string().contains("xxxx"));
        assert!(
            !error.to_string().contains("xxxx"),
            "diagnostics must not leak captured output"
        );
    });
}

#[test]
fn stderr_overflow_names_the_stderr_stream() {
    let (_dir, bin_dir, repo) = fixture("stderr-over");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        let error = run_json(&repo, &policy, "big-stderr", &limits(30_000, 64, 4), 30_000)
            .expect_err("stderr overflow must refuse");
        let details = failure_details(&error);
        assert_eq!(details["failure"], "stderr_overflow");
        assert_eq!(details["stream"], "stderr");
        assert_eq!(details["limit_bytes"], 4);
        assert!(!details.to_string().contains("yyyy"));
    });
}

#[test]
fn missing_workspace_user_falls_back_without_resetting_the_budget() {
    let (_dir, bin_dir, repo) = fixture("fallback");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        // First attempt (workspace user) retains 3 stdout bytes and fails; the
        // eligible fallback may only retain 1 more of the 4-byte total.
        let error = run_json(
            &repo,
            &policy,
            "fallback-budget",
            &limits(30_000, 4, 1024),
            30_000,
        )
        .expect_err("the fallback must not reset the cumulative budget");
        let details = failure_details(&error);
        assert_eq!(details["failure"], "stdout_overflow");
        assert_eq!(details["attempts"], 2);
        assert_eq!(details["workspace_user_fallback"], true);
    });
}

#[test]
fn missing_workspace_user_falls_back_when_the_total_budget_holds() {
    let (_dir, bin_dir, repo) = fixture("fallback-ok");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        let output = run_json(
            &repo,
            &policy,
            "fallback-root",
            &limits(30_000, 1024, 1024),
            30_000,
        )
        .expect("fallback within budget succeeds");
        let value: serde_json::Value = serde_json::from_str(&output).expect("json");
        assert_eq!(value["ok"], true);
        assert_eq!(value["stdout"], "root-out");
        assert_eq!(value["attempts"], 2);
        assert_eq!(value["workspace_user_fallback"], true);
        // The first attempt's retained stderr still counts against the total.
        assert!(value["retained"]["stderr_bytes"].as_u64().unwrap_or(0) > 0);
    });
}

#[test]
fn inner_nonzero_exit_is_distinct_from_a_bounds_failure() {
    let (_dir, bin_dir, repo) = fixture("inner-exit");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        let error = run_json(&repo, &policy, "exit-7", &limits(30_000, 64, 64), 30_000)
            .expect_err("inner nonzero exit must fail");
        assert!(matches!(
            error,
            RunnerError::BoundedExecFailure { code: 7, .. }
        ));
        let details = failure_details(&error);
        assert_eq!(details["failure"], "inner_command_failed");
        assert_eq!(details["inner_code"], 7);
    });
}

#[test]
fn expired_deadline_refuses_without_spawning_the_backend() {
    let (_dir, bin_dir, repo) = fixture("expired");
    let policy = policy();
    let marker = repo.join("spawned");
    {
        let _lock = lock_test();
        let current = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{}", bin_dir.display(), current);
        let _env = EnvGuard::set_many(&[
            ("PATH", Some(path)),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            ("FAKE_BOUNDED_MARKER", Some(marker.display().to_string())),
        ]);
        let started = Instant::now();
        let error = run_bounded_command_with_deadline(
            BoundedCommandRequest {
                repo_root: &repo,
                policy: &policy,
                service: "app",
                working_dir: Some(Path::new("/workspace")),
                command: "plain",
                output_json: true,
                deadline: started
                    .checked_sub(Duration::from_secs(1))
                    .expect("expired deadline"),
            },
            &limits(30_000, 64, 64),
        )
        .expect_err("expired deadline must refuse before spawn");
        let details = failure_details(&error);
        assert_eq!(details["failure"], "deadline");
        assert_eq!(details["attempts"], 0);
    }
    assert!(
        !marker.exists(),
        "an expired deadline must not spawn the backend"
    );
}

#[test]
fn slow_command_hits_the_deadline_without_partial_success() {
    let (_dir, bin_dir, repo) = fixture("slow");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        let started = Instant::now();
        let error = run_json(&repo, &policy, "sleep-30", &limits(300, 64, 64), 300)
            .expect_err("slow command must hit the deadline");
        assert!(started.elapsed() < Duration::from_secs(10));
        let details = failure_details(&error);
        assert_eq!(details["failure"], "deadline");
    });
}
