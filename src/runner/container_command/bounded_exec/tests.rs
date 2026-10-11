//! Private fake-backend production-path proof for the bounded opt-in surface.
//!
//! These tests drive the public production entry as well as
//! [`run_bounded_command_with_deadline`] — the same plan construction, spawned
//! client, bounded concurrent readers, owned-group termination and
//! workspace-user fallback the CLI uses — against private fake
//! `colima`/`docker` doubles on `PATH`. No live runtime, image, service or
//! Reference resource is touched. The doubles answer from a marker embedded in
//! the container command string, so no process-global env is needed to select
//! behaviour beyond `PATH`.
//!
//! Public-path subprocess cases enter through the normal `run_container`
//! dispatcher under a separate timeout guard and clean up only their own fake
//! client trees. They cover discovery and workspace preparation without
//! touching a live runtime or container.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use effigy_cli::{BoundedExecLimits, ContainerArgs, ContainerSubcommand};

use super::{
    run_bounded_command_with_deadline, run_container_shell_bounded_with_deadline,
    BoundedCommandRequest,
};
use crate::contract_test_support::{lock_test, EnvGuard};
use crate::runner::container_command::run_container;
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
if marker exit-7 || marker SECRET_ARG_MARKER_142; then
    printf 'SECRET_STDOUT_MARKER_142'
    printf 'SECRET_STDERR_MARKER_142' >&2
    exit 7
fi
if marker sleep-30; then
    exec /bin/sleep 30
fi
printf 'ok'
exit 0
"#;

const PUBLIC_FAKE_BACKEND: &str = r#"#!/bin/sh
args="$*"
case "$FAKE_BOUNDED_MODE:$args" in
    runtime-hang:*ps*)
        printf '%s\n' "$$" > "$FAKE_BOUNDED_PID_FILE"
        exec /bin/sleep 30
        ;;
    runtime-mismatch:*ps*)
        printf 'demo-web-old-1\tUp\t\tstale-project\t%s\tapp\tfalse\n' "$FAKE_BOUNDED_REPO"
        exit 0
        ;;
    phase-continuity:*ps*)
        if [ ! -e "$FAKE_BOUNDED_DELAY_MARKER" ]; then
            : > "$FAKE_BOUNDED_DELAY_MARKER"
            exec /bin/sleep 1.4
        fi
        ;;
    runtime-flood:*ps*)
        i=0
        while [ "$i" -lt 4096 ]; do
            printf 'x'
            printf 'y' >&2
            i=$((i + 1))
        done
        exit 0
        ;;
esac
if [ "$FAKE_BOUNDED_MODE" = "remove-before-command" ]; then
    case "$args" in
        *"install -m 0755"*)
            /bin/chmod 000 "$0"
            printf 'installed'
            exit 0
            ;;
    esac
fi
case "$args" in
    *"ps --format"*)
        printf 'demo-web-app-1\tUp\t\tdemo-web\t%s\tapp\tfalse\n' "$FAKE_BOUNDED_REPO"
        exit 0
        ;;
    "ps"|*" ps")
        exit 0
        ;;
    *"status --profile"*)
        printf 'colima running'
        exit 0
        ;;
    *"effigy.install-identity"*)
        printf '%s' "$FAKE_BOUNDED_IDENTITY"
        exit 0
        ;;
    *"uname -m"*)
        if [ "$FAKE_BOUNDED_MODE" = "phase-continuity" ] || [ "$FAKE_BOUNDED_MODE" = "workspace-hang" ]; then
            printf '%s\n' "$$" > "$FAKE_BOUNDED_PID_FILE"
            exec /bin/sleep 30
        fi
        if [ "$FAKE_BOUNDED_MODE" = "workspace-flood" ]; then
            i=0
            while [ "$i" -lt 4096 ]; do
                printf 'x'
                printf 'y' >&2
                i=$((i + 1))
            done
            exit 0
        fi
        if [ "$FAKE_BOUNDED_MODE" = "identity-invalid" ]; then
            printf s390x
            exit 0
        fi
        printf x86_64
        exit 0
        ;;
    *"fallback-budget"*)
        case "$args" in
            *"-u dev"*)
                printf aaa
                printf 'unable to find user dev: no matching entries in passwd file' >&2
                exit 1
                ;;
            *)
                i=0
                while [ "$i" -lt 160 ]; do
                    printf b
                    i=$((i + 1))
                done
                exit 0
                ;;
        esac
        ;;
    *"main-command"*)
        case "$args" in
            *"-w /workspace"*"-u dev"*"HOME=/home/dev"*"app sh -lc"*) ;;
            *)
                printf 'selection-mismatch'
                exit 91
                ;;
        esac
        printf '%s' "command-only"
        : > "$FAKE_BOUNDED_FINAL_MARKER"
        exit 0
        ;;
    *"SECRET_ARG_MARKER_142"*)
        if [ "$FAKE_BOUNDED_MODE" = "inner-nonzero-secret" ]; then
            printf 'SECRET_STDOUT_MARKER_142'
            printf 'SECRET_STDERR_MARKER_142' >&2
            exit 7
        fi
        : > "$FAKE_BOUNDED_FINAL_MARKER"
        exit 0
        ;;
esac
printf '%s' "ok"
exit 0
"#;

fn fixture(name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    fixture_in(name, None)
}

fn fixture_in(name: &str, parent: Option<&Path>) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = if let Some(parent) = parent {
        tempfile::Builder::new()
            .prefix("effigy-bounded-exec-")
            .tempdir_in(parent)
            .expect("create fixture under the shared temp root")
    } else {
        tempfile::tempdir().expect("tempdir")
    };
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

fn public_fixture(name: &str, workspace_identity: bool) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let shared_parent = cfg!(target_os = "macos").then_some(Path::new("/Users/Shared"));
    let (dir, bin_dir, repo_root) = fixture_in(name, shared_parent);
    let home = dir.path().join("home");
    fs::create_dir_all(&home).expect("mkdir isolated home");
    let release_cache = home
        .join(".effigy/workspace-bin/linux/x86_64-unknown-linux-gnu")
        .join(format!("v{}", env!("CARGO_PKG_VERSION")))
        .join("effigy-x86_64-unknown-linux-gnu");
    fs::create_dir_all(release_cache.parent().expect("release cache parent"))
        .expect("mkdir isolated release cache");
    fs::write(&release_cache, b"private fake release artifact")
        .expect("seed isolated release cache");
    fs::write(
        repo_root.join("docker-compose.yml"),
        "services:\n  app:\n    image: busybox\n",
    )
    .expect("write compose file");
    let workspace = if workspace_identity {
        r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
user = "dev"
home = "/home/dev"
working_dir = "/workspace"
"#
    } else {
        ""
    };
    fs::write(
        repo_root.join("effigy.toml"),
        format!(
            r#"[containers]
default = "web"

[containers.web]
compose_file = "docker-compose.yml"
project_name = "demo-web"
primary_service = "app"
{workspace}
"#
        ),
    )
    .expect("write manifest");
    for program in ["colima", "docker"] {
        let path = bin_dir.join(program);
        fs::write(&path, PUBLIC_FAKE_BACKEND).expect("write public fake backend");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&path)
                .expect("stat fake backend")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("chmod fake backend");
        }
    }
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
        },
        limits,
        Instant::now() + Duration::from_millis(deadline_ms),
    )
}

#[cfg(unix)]
struct ProcessTreeCleanup {
    pid: u32,
    active: bool,
}

#[cfg(unix)]
impl ProcessTreeCleanup {
    fn new(pid: u32) -> Self {
        Self { pid, active: true }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

#[cfg(unix)]
impl Drop for ProcessTreeCleanup {
    fn drop(&mut self) {
        if self.active {
            effigy_process::terminate_process_tree(self.pid, true);
        }
    }
}

#[cfg(unix)]
struct RecordedClientCleanup(PathBuf);

#[cfg(unix)]
impl Drop for RecordedClientCleanup {
    fn drop(&mut self) {
        if let Ok(raw) = fs::read_to_string(&self.0) {
            if let Ok(pid) = raw.trim().parse::<u32>() {
                effigy_process::terminate_process_tree(pid, true);
            }
        }
    }
}

#[cfg(unix)]
struct GuardedPublicCase<'a> {
    mode: &'a str,
    backend: &'a str,
    identity: &'a str,
    deadline_ms: u64,
    stdout_limit: u64,
}

#[cfg(unix)]
struct GuardedPublicOutcome {
    elapsed: Duration,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[cfg(unix)]
fn run_guarded_public_case(
    dir: &tempfile::TempDir,
    bin_dir: &Path,
    repo_root: &Path,
    case: GuardedPublicCase<'_>,
) -> GuardedPublicOutcome {
    use std::os::unix::process::CommandExt;

    let GuardedPublicCase {
        mode,
        backend,
        identity,
        deadline_ms,
        stdout_limit,
    } = case;
    let pid_file = dir.path().join("fake-client-pid");
    let home = dir.path().join("home");
    let delay_marker = dir.path().join(format!("runtime-delay-{mode}"));
    let final_marker = dir.path().join(format!("final-command-ran-{mode}"));
    let child_stdout_path = dir.path().join(format!("public-child-{mode}.stdout"));
    let child_stderr_path = dir.path().join(format!("public-child-{mode}.stderr"));
    let child_stdout = fs::File::create(&child_stdout_path).expect("create child stdout capture");
    let child_stderr = fs::File::create(&child_stderr_path).expect("create child stderr capture");
    let _client_cleanup = RecordedClientCleanup(pid_file.clone());
    let test_name = std::thread::current()
        .name()
        .expect("parent test name")
        .rsplit_once("::")
        .map(|(module, _)| format!("{module}::guarded_public_entry_child"))
        .expect("parent test is module-qualified");
    let current_exe = std::env::current_exe().expect("test executable path");
    let path = bin_dir.display().to_string();
    let mut command = Command::new(current_exe);
    command
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("EFFIGY_BOUNDED_PUBLIC_CHILD", "1")
        .env("FAKE_BOUNDED_MODE", mode)
        .env("FAKE_BOUNDED_BACKEND", backend)
        .env("FAKE_BOUNDED_REPO", repo_root)
        .env("FAKE_BOUNDED_IDENTITY", identity)
        .env("FAKE_BOUNDED_PID_FILE", &pid_file)
        .env("FAKE_BOUNDED_DELAY_MARKER", &delay_marker)
        .env("FAKE_BOUNDED_FINAL_MARKER", &final_marker)
        .env("FAKE_BOUNDED_STDOUT_LIMIT", stdout_limit.to_string())
        .env("FAKE_BOUNDED_DEADLINE_MS", deadline_ms.to_string())
        .env("PATH", path)
        .env("HOME", home)
        .env("EFFIGY_COMPOSE_BACKEND", backend)
        .env("EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE", "download")
        .stdin(Stdio::null())
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::from(child_stderr));
    command.process_group(0);
    let mut child = command.spawn().expect("spawn guarded public-path test");
    let mut cleanup = ProcessTreeCleanup::new(child.id());
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("poll public-path test") {
            cleanup.disarm();
            if status.success() {
                let _ = fs::remove_file(&pid_file);
            }
            let stdout = fs::read(&child_stdout_path).expect("read child stdout capture");
            let stderr = fs::read(&child_stderr_path).expect("read child stderr capture");
            assert!(
                status.success(),
                "public-path child failed for {mode}; stdout={:?}, stderr={:?}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
            return GuardedPublicOutcome {
                elapsed: started.elapsed(),
                stdout,
                stderr,
            };
        }
        if started.elapsed() >= Duration::from_secs(12) {
            effigy_process::terminate_process_tree(child.id(), true);
            let _ = child.wait();
            cleanup.disarm();
            panic!("public-path child exceeded the independent 12s harness guard for {mode}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn guarded_public_entry_child() {
    if std::env::var_os("EFFIGY_BOUNDED_PUBLIC_CHILD").is_none() {
        return;
    }
    let mode = std::env::var("FAKE_BOUNDED_MODE").expect("fake mode");
    let backend = std::env::var("FAKE_BOUNDED_BACKEND").expect("fake backend");
    let repo_root = PathBuf::from(std::env::var_os("FAKE_BOUNDED_REPO").expect("repo root"));
    let final_marker =
        PathBuf::from(std::env::var_os("FAKE_BOUNDED_FINAL_MARKER").expect("final marker"));
    let stdout_limit = std::env::var("FAKE_BOUNDED_STDOUT_LIMIT")
        .expect("stdout limit")
        .parse::<u64>()
        .expect("integer stdout limit");
    let deadline_ms = std::env::var("FAKE_BOUNDED_DEADLINE_MS")
        .expect("deadline")
        .parse::<u64>()
        .expect("integer deadline");
    if mode == "valid-identity" || mode == "valid-colima" {
        let expected_identity = format!("release:v{}", effigy_core::build_info::active_version());
        assert_eq!(
            std::env::var("FAKE_BOUNDED_IDENTITY").expect("fake identity"),
            expected_identity,
            "public fixture identity must match the child runtime identity"
        );
    }
    let limits = limits(deadline_ms, stdout_limit, 256);
    let command = if mode == "remove-before-command" {
        "SECRET_ARG_MARKER_142"
    } else if mode == "fallback-budget" {
        "fallback-budget"
    } else if mode == "inner-nonzero-secret" {
        "SECRET_ARG_MARKER_142"
    } else {
        "main-command"
    };
    let result = run_container(ContainerArgs {
        subcommand: ContainerSubcommand::Shell {
            name: Some("web".to_owned()),
            service: None,
            command: Some(command.to_owned()),
            bounded: Some(limits),
        },
        repo_override: Some(repo_root),
        output_json: mode != "inner-nonzero-secret",
    });

    match mode.as_str() {
        "valid-identity" | "valid-colima" => {
            let output = result.expect("bounded public command succeeds");
            let value: serde_json::Value = serde_json::from_str(&output).expect("json output");
            assert_eq!(value["ok"], true);
            assert_eq!(value["stdout"], "command-only");
            assert_eq!(value["stdout_bytes"], "command-only".len());
            assert_eq!(value["attempts"], 1);
            assert_eq!(
                value["preparation"]["captures"], 3,
                "public command must include the bounded workspace identity probe"
            );
            let preparation_stdout = value["preparation"]["retained"]["stdout_bytes"]
                .as_u64()
                .unwrap();
            let minimum_preparation_stdout = std::env::var("FAKE_BOUNDED_IDENTITY").unwrap().len()
                as u64
                + if backend == "colima" {
                    "colima running".len() as u64
                } else {
                    0
                };
            assert!(preparation_stdout >= minimum_preparation_stdout);
            assert!(
                value["retained"]["stdout_bytes"].as_u64().unwrap()
                    == preparation_stdout + value["stdout_bytes"].as_u64().unwrap()
            );
            assert!(
                final_marker.exists(),
                "bounded command should execute after preparation"
            );
        }
        "runtime-mismatch" => {
            let error = result.expect_err("stale runtime identity must refuse");
            let details = failure_details(&error);
            assert_eq!(details["failure"], "runtime_identity_mismatch");
            assert_eq!(details["phase"], "runtime_discovery");
            assert!(!final_marker.exists());
        }
        "identity-invalid" => {
            let error = result.expect_err("stale workspace identity must require refresh");
            let details = failure_details(&error);
            assert_eq!(details["failure"], "workspace_preparation_failed");
            assert_eq!(details["phase"], "workspace_preparation");
            assert_eq!(details["preparation"]["captures"], 4);
            assert!(!final_marker.exists());
        }
        "phase-continuity" => {
            let error = result.expect_err("workspace phase must share the discovery deadline");
            let details = failure_details(&error);
            assert_eq!(details["failure"], "deadline");
            assert_eq!(details["phase"], "workspace_preparation");
            assert!(!final_marker.exists());
        }
        "fallback-budget" => {
            let error = result.expect_err("fallback must consume the preparation budget");
            let details = failure_details(&error);
            assert_eq!(details["failure"], "stdout_overflow");
            assert_eq!(details["phase"], "command_execution");
            assert_eq!(details["attempts"], 2);
            assert_eq!(details["workspace_user_fallback"], true);
            assert_eq!(details["preparation"]["captures"], 3);
            assert!(!final_marker.exists());
        }
        "remove-before-command" => {
            let error = result.expect_err("missing final backend must be a launch failure");
            let human = error.to_string();
            let json = error
                .json_error_details()
                .unwrap_or_else(|| panic!("expected bounded launch failure, got: {human}"));
            assert!(
                human.contains("launch"),
                "human error is safe and useful: {human}"
            );
            assert!(!human.contains("SECRET_ARG_MARKER_142"));
            assert!(!json.contains("SECRET_ARG_MARKER_142"));
            assert!(!json.contains("docker compose"));
            let details: serde_json::Value =
                serde_json::from_str(json).expect("json details parse");
            assert_eq!(details["failure"], "launch_failed");
            assert_eq!(details["phase"], "command_execution");
            assert_eq!(details["termination_scope"], "owned_host_client_group");
            assert_eq!(details["container"], "[redacted]");
            assert_eq!(details["service"], "[redacted]");
            assert!(!final_marker.exists());
        }
        "inner-nonzero-secret" => {
            let error = result.expect_err("nonzero inner command must fail");
            let human = error.to_string();
            let json = error.json_error_details().expect("json error details");
            assert!(human.contains("exited with status"));
            assert!(json.contains("inner_command_failed"));
            for marker in [
                "SECRET_ARG_MARKER_142",
                "SECRET_STDOUT_MARKER_142",
                "SECRET_STDERR_MARKER_142",
            ] {
                assert!(!human.contains(marker), "human error leaked {marker}");
                assert!(!json.contains(marker), "json error leaked {marker}");
            }
        }
        _ => {
            let error = result.expect_err("bounded preparation must refuse");
            let details = failure_details(&error);
            let expected_phase = if mode.starts_with("runtime-") {
                "runtime_discovery"
            } else {
                "workspace_preparation"
            };
            assert_eq!(details["phase"], expected_phase);
            if mode.ends_with("hang") {
                assert_eq!(details["failure"], "deadline");
            } else {
                assert!(
                    details["failure"] == "stdout_overflow"
                        || details["failure"] == "stderr_overflow",
                    "unexpected bounded failure: {details}"
                );
            }
            assert!(
                !final_marker.exists(),
                "failed preparation must not spawn the command"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn public_entry_bounds_discovery_preparation_and_fallback_on_the_full_path() {
    let (dir, bin_dir, repo) = public_fixture("public-path", true);
    let version = effigy_core::build_info::package_version();
    let identity = format!("release:v{version}");
    let stale_identity = "release:vstale";
    let _valid_docker = run_guarded_public_case(
        &dir,
        &bin_dir,
        &repo,
        GuardedPublicCase {
            mode: "valid-identity",
            backend: "docker",
            identity: &identity,
            deadline_ms: 8_000,
            stdout_limit: 256,
        },
    );
    let _valid_colima = run_guarded_public_case(
        &dir,
        &bin_dir,
        &repo,
        GuardedPublicCase {
            mode: "valid-colima",
            backend: "colima",
            identity: &identity,
            deadline_ms: 8_000,
            stdout_limit: 256,
        },
    );
    let continuity = run_guarded_public_case(
        &dir,
        &bin_dir,
        &repo,
        GuardedPublicCase {
            mode: "phase-continuity",
            backend: "docker",
            identity: stale_identity,
            deadline_ms: 2_000,
            stdout_limit: 8_192,
        },
    );
    assert!(
        continuity.elapsed < Duration::from_millis(3200),
        "phase-continuity reset the deadline across discovery: {:?}",
        continuity.elapsed
    );
    let fallback_stdout_limit = format!(
        "demo-web-app-1\tUp\t\tdemo-web\t{}\tapp\tfalse\n",
        repo.display()
    )
    .len() as u64
        + identity.len() as u64
        + "aaa".len() as u64
        + 80;
    for (mode, backend) in [
        ("runtime-hang", "docker"),
        ("runtime-flood", "docker"),
        ("runtime-mismatch", "docker"),
        ("identity-invalid", "docker"),
        ("workspace-hang", "docker"),
        ("workspace-flood", "docker"),
        ("fallback-budget", "docker"),
    ] {
        let _outcome = run_guarded_public_case(
            &dir,
            &bin_dir,
            &repo,
            GuardedPublicCase {
                mode,
                backend,
                identity: if matches!(
                    mode,
                    "identity-invalid" | "workspace-hang" | "workspace-flood"
                ) {
                    stale_identity
                } else {
                    &identity
                },
                deadline_ms: 2_000,
                stdout_limit: match mode {
                    "fallback-budget" => fallback_stdout_limit,
                    "runtime-mismatch" | "identity-invalid" | "workspace-hang" => 512,
                    "workspace-flood" => 512,
                    _ => 64,
                },
            },
        );
    }

    let (launch_dir, launch_bin, launch_repo) = public_fixture("launch-secret", true);
    let _launch = run_guarded_public_case(
        &launch_dir,
        &launch_bin,
        &launch_repo,
        GuardedPublicCase {
            mode: "remove-before-command",
            backend: "docker",
            identity: stale_identity,
            deadline_ms: 2_000,
            stdout_limit: 256,
        },
    );
    let human_nonzero = run_guarded_public_case(
        &dir,
        &bin_dir,
        &repo,
        GuardedPublicCase {
            mode: "inner-nonzero-secret",
            backend: "docker",
            identity: &identity,
            deadline_ms: 2_000,
            stdout_limit: 512,
        },
    );
    for marker in [
        "SECRET_ARG_MARKER_142",
        "SECRET_STDOUT_MARKER_142",
        "SECRET_STDERR_MARKER_142",
    ] {
        assert!(
            !String::from_utf8_lossy(&human_nonzero.stdout).contains(marker),
            "human mode wrote captured stdout marker {marker}"
        );
        assert!(
            !String::from_utf8_lossy(&human_nonzero.stderr).contains(marker),
            "human mode wrote captured stderr marker {marker}"
        );
    }
}

#[cfg(unix)]
#[test]
fn expired_public_entry_refuses_before_the_first_discovery_spawn() {
    let (dir, bin_dir, repo) = public_fixture("expired-public-path", true);
    let marker = dir.path().join("unexpected-client-spawn");
    let _lock = lock_test();
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(bin_dir.display().to_string())),
        ("HOME", Some(dir.path().join("home").display().to_string())),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        (
            "EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE",
            Some("download".to_owned()),
        ),
        ("FAKE_BOUNDED_MODE", Some("valid-identity".to_owned())),
        ("FAKE_BOUNDED_IDENTITY", Some("release:vexpired".to_owned())),
        ("FAKE_BOUNDED_PID_FILE", Some(marker.display().to_string())),
        (
            "FAKE_BOUNDED_FINAL_MARKER",
            Some(marker.display().to_string()),
        ),
    ]);
    let deadline = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("expired deadline");
    let error = run_container_shell_bounded_with_deadline(
        &repo,
        Some("web"),
        None,
        "main-command",
        limits(30_000, 64, 64),
        true,
        deadline,
    )
    .expect_err("expired operation must refuse before discovery");
    let details = failure_details(&error);
    assert_eq!(details["failure"], "deadline");
    assert_eq!(details["phase"], "runtime_discovery");
    assert_eq!(details["preparation"]["captures"], 0);
    assert!(
        !marker.exists(),
        "expired operation must not start the fake backend"
    );
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
        let error = run_json(
            &repo,
            &policy,
            "SECRET_ARG_MARKER_142",
            &limits(30_000, 64, 64),
            30_000,
        )
        .expect_err("inner nonzero exit must fail");
        assert!(matches!(
            error,
            RunnerError::BoundedExecFailure { code: 7, .. }
        ));
        let details = failure_details(&error);
        assert_eq!(details["failure"], "inner_command_failed");
        assert_eq!(details["inner_code"], 7);
        for marker in [
            "SECRET_ARG_MARKER_142",
            "SECRET_STDOUT_MARKER_142",
            "SECRET_STDERR_MARKER_142",
        ] {
            assert!(!error.to_string().contains(marker));
            assert!(!details.to_string().contains(marker));
        }
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
            },
            &limits(30_000, 64, 64),
            started
                .checked_sub(Duration::from_secs(1))
                .expect("expired deadline"),
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
