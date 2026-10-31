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
use std::process::{Child, Command, Stdio};
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
# Optional late progress marker for refusal-accounting fixtures: written at
# the fixture's critical point, never at startup.
bounded_progress() {
    if [ -n "$FAKE_BOUNDED_PROGRESS_MARKER" ]; then
        : > "$FAKE_BOUNDED_PROGRESS_MARKER"
    fi
}
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
if marker hang-secrets; then
    # Refused-transport privacy fixture: private partial bytes on both
    # channels, then a leader that never exits inside the budget.
    printf 'SECRET_STDOUT_MARKER_142'
    printf 'SECRET_STDERR_MARKER_142' >&2
    exec /bin/sleep 120
fi
if marker pending-flood; then
    # Pending-reader progress fixture: a flood that cannot fit in any pipe,
    # then a sleeping leader that keeps both pipes open without EOF.
    i=0
    while [ "$i" -lt 100000 ]; do
        printf 'x'
        i=$((i + 1))
    done
    exec /bin/sleep 30
fi
if marker partial-both-hold; then
    # The admitted refusal shape: the leader exits fast after private partial
    # output on both channels while an inherited writer keeps the pipes open,
    # so no EOF and no completed capture ever arrives. The progress marker
    # proves the fixture reached its write point, keeping the proof
    # load-independent.
    printf 'abc'
    printf 'xyz' 1>&2
    (sleep 30) &
    bounded_progress
    exit 0
fi
if marker eof-hold-stderr; then
    # Asymmetric snapshot fixture: stdout reaches a real EOF with zero bytes
    # while the sleeping leader keeps stderr pending. The progress marker is
    # written only after stdout is closed.
    exec 1>&-
    bounded_progress
    exec /bin/sleep 30
fi
if marker sleep-30; then
    exec /bin/sleep 30
fi
printf 'ok'
exit 0
"#;

const PUBLIC_FAKE_BACKEND: &str = r#"#!/bin/sh
args="$*"
FAKE_ID=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
# The executable store is addressed by the digest's hex part; the context's
# `current` pointer carries the full `sha256:<hex>` value.
FAKE_DIGEST_HEX=${FAKE_BOUNDED_DIGEST#sha256:}
case "$FAKE_BOUNDED_MODE:$args" in
    runtime-hang:*ps*|launch-blocked-discovery:*ps*)
        printf '%s\n' "$$" > "$FAKE_BOUNDED_PID_FILE"
        exec /bin/sleep 30
        ;;
    runtime-mismatch:*ps*)
        printf 'demo-web-old-1\tUp\t\tstale-project\t%s\tapp\tfalse\n' "$FAKE_BOUNDED_REPO"
        exit 0
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
        *"/run/effigy/context/current"*)
            /bin/chmod 000 "$0" "$FAKE_BOUNDED_BIN_DIR/docker" "$FAKE_BOUNDED_BIN_DIR/colima"
            printf '1'
            exit 0
            ;;
    esac
fi
# The bounded entry verifies the prepared guest context instead of installing
# one (contract 052 §8.9). These answers are the published context the fixture
# wrote: one generation, one pinned executable, nothing refreshed.
case "$args" in
    *"/run/effigy/context/current"*)
        if [ "$FAKE_BOUNDED_MODE" = "workspace-hang" ]; then
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
        printf '1'
        exit 0
        ;;
    *"/run/effigy/context/1/context.json"*)
        # `read` is a shell builtin: the child's PATH holds only the fixture
        # backend, so a `cat` here would fail and read as an empty document.
        while IFS= read -r line || [ -n "$line" ]; do
            printf '%s\n' "$line"
        done < "$FAKE_BOUNDED_CONTEXT_FILE"
        exit 0
        ;;
    *"/run/effigy/bin/current"*)
        printf '%s' "$FAKE_BOUNDED_DIGEST"
        exit 0
        ;;
    *"/proc/sys/kernel/random/boot_id"*)
        # The entry compares the live container, boot, platform and image libc
        # to the published context before it runs anything.
        printf '%s' "$FAKE_BOUNDED_PROBE"
        exit 0
        ;;
    *"readlink"*"/usr/local/bin/effigy"*)
        case "$FAKE_BOUNDED_MODE" in
            entry-missing) exit 3 ;;
            entry-wrong) printf '/run/effigy/other\n'; exit 0 ;;
        esac
        printf '/run/effigy/bin/%s/effigy\n' "$FAKE_DIGEST_HEX"
        exit 0
        ;;
    *"/usr/local/bin/effigy"*)
        if [ "$FAKE_BOUNDED_MODE" = "entry-wrong-bytes" ]; then
            printf '%064d\n' 0
        else
            printf '%s\n' "$FAKE_DIGEST_HEX"
        fi
        exit 0
        ;;
    *"$FAKE_DIGEST_HEX"*)
        # The entry authenticates the installed executable with a digest probe,
        # never by reading its bytes into the caller's capture budget.
        printf '%s\n' "$FAKE_DIGEST_HEX"
        exit 0
        ;;
esac
case "$args" in
    *"ps --no-trunc"*)
        printf '%s\tUp 1 minute\tFalse\t%s\t\t\n' "$FAKE_ID" "$FAKE_BOUNDED_REPO"
        exit 0
        ;;
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
            *"-w /workspace"*"-u dev"*"HOME=/home/dev"*"$FAKE_ID sh -lc"*) ;;
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

fn fixture(name: &str) -> (effigy_test_support::Fixture, PathBuf, PathBuf) {
    fixture_in(name, None)
}

fn fixture_in(
    name: &str,
    parent: Option<&Path>,
) -> (effigy_test_support::Fixture, PathBuf, PathBuf) {
    let dir = if let Some(parent) = parent {
        effigy_test_support::Fixture::in_parent("effigy-bounded-exec-", parent)
            .expect("create fixture under the shared temp root")
    } else {
        effigy_test_support::Fixture::new("effigy-bounded-exec-").expect("tempdir")
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

fn public_fixture(
    name: &str,
    workspace_identity: bool,
) -> (effigy_test_support::Fixture, PathBuf, PathBuf) {
    // A fresh directory in the run's own scratch, never a hardcoded shared
    // directory. The child's own `<fixture>/tmp` (set below) is its temporary
    // directory, so the checkout is not under it and the Colima host-path guard
    // is satisfied without moving the fixture out of the run's scratch.
    let (dir, bin_dir, repo_root) = fixture_in(name, None);
    let home = dir.path().join("home");
    fs::create_dir_all(&home).expect("mkdir isolated home");
    // The child's own temp directory stays inside this fixture, so the repo is
    // not under it and every temporary file the run makes is confined here.
    let child_temp = dir.path().join("tmp");
    fs::create_dir_all(&child_temp).expect("mkdir child temp");
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
            target_id: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
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
    child: Child,
    active: bool,
}

#[cfg(unix)]
impl Drop for ProcessTreeCleanup {
    fn drop(&mut self) {
        if self.active {
            // Child remains unreaped while this exact tree is signaled. Never
            // recover signal authority from a PID in a stale fixture file.
            effigy_process::terminate_process_tree(self.child.id(), true);
            let _ = self.child.wait();
        }
    }
}

#[cfg(unix)]
#[test]
fn public_child_guard_reaps_on_unwind_and_preserves_foreign_child() {
    use std::os::unix::process::CommandExt;
    let spawn = || {
        let mut command = Command::new("/bin/sleep");
        command.arg("30").process_group(0);
        ProcessTreeCleanup {
            child: command.spawn().expect("owned guard child"),
            active: true,
        }
    };
    let mut foreign = spawn();
    let owned = spawn();
    let owned_pid = owned.child.id();
    let result = std::panic::catch_unwind(move || {
        let _owned = owned;
        panic!("controlled assertion failure");
    });
    assert!(result.is_err());
    assert!(
        !effigy_process::process_is_running(owned_pid),
        "guard must kill and reap"
    );
    assert!(foreign.child.try_wait().expect("foreign wait").is_none());
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
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// The digest the fixture's pinned guest executable is published under.
fn public_guest_digest() -> String {
    format!("sha256:{}", "ab".repeat(32))
}

/// Guest image facts; the full container id comes from the verified exec target.
fn public_guest_identity_probe(mode: &str) -> String {
    // The spoofed guest claims the old context's id, but the verified runtime
    // target still names the replacement. Both full and short claims are false.
    let full_id = if mode == "hostname-spoof" {
        "ab".repeat(32)
    } else {
        "0123456789abcdef".repeat(4)
    };
    format!(
        "cgroup=0::/docker/{full_id} \nboot=boot\nhostname={short_id}\nplatform=x86_64\n\
         libc_musl=\nlibc_glibc=/lib/x86_64-linux-gnu/libc.so.6\n",
        short_id = &full_id[..12],
    )
}

/// Establish the environment activation fence the entry check reads.
///
/// The fixture's own state store is activated exactly as `container up` would
/// activate it, so `environment_generation` finds the record and the published
/// context can name the generation it is bound to. Only the fixture store is
/// touched: HOME and `EFFIGY_TEST_CONTAINERS_HOME` point at it while the
/// activation runs.
fn public_environment_identity(
    home: &Path,
    repo_root: &Path,
) -> crate::runner::workspace_client::context::WorkspaceEnvironmentIdentity {
    let _lock = lock_test();
    let _env = EnvGuard::set_many(&[
        ("HOME", Some(home.display().to_string())),
        (
            "EFFIGY_TEST_CONTAINERS_HOME",
            Some(home.join(".effigy").display().to_string()),
        ),
    ]);
    let policy = effigy_containers::load_container_policy(repo_root, Some("web"))
        .expect("fixture container policy");
    let env = crate::runner::environment::env_ref(&policy).expect("fixture environment ref");
    let context = crate::runner::environment::context(repo_root).expect("fixture environment ctx");
    effigy_containers::environment::activate(&context, &env, &[])
        .expect("fixture environment activation");
    let record = crate::runner::environment::record(&policy).expect("fixture activation record");
    crate::runner::workspace_client::context::WorkspaceEnvironmentIdentity {
        // The host derives this identity from the policy name at Prepare and
        // again at entry; the generation is the record's activation generation.
        resource_id: format!("env:{}", policy.name),
        generation: record.generation.get().to_string(),
    }
}

/// Assert the fixture context pins the activation fence this run will read, so
/// a fixture that drifted fails loudly instead of as a product refusal.
fn assert_public_activation_fence(repo_root: &Path, context_path: &Path, mode: &str) {
    let policy = effigy_containers::load_container_policy(repo_root, Some("web"))
        .expect("fixture container policy");
    let record = crate::runner::environment::record(&policy)
        .expect("fixture activation record before entry");
    let published = crate::runner::workspace_client::context::WorkspaceContext::parse(
        &fs::read(context_path).expect("read published fixture context"),
    )
    .expect("parse published fixture context");
    assert_eq!(
        published.activation.environment.generation,
        record.generation.get().to_string(),
        "fixture mode {mode} must publish the activation generation the entry reads"
    );
    assert_eq!(
        published.activation.environment.resource_id,
        format!("env:{}", policy.name),
        "fixture mode {mode} must publish the environment resource the entry reads"
    );
}

/// Write the guest context the fixture pretends Prepare published.
///
/// The document is the real one: `identity-invalid` pins a version this host
/// cannot speak to, and every other mode pins this build.
fn write_public_guest_context(dir: &Path, repo_root: &Path, mode: &str) -> PathBuf {
    use crate::runner::workspace_client::context::{
        GuestBinaryIdentity, WorkspaceActivationIdentity, WorkspaceBackendDescriptor,
        WorkspaceChannelLocator, WorkspaceContainerIdentity, WorkspaceContext,
        WorkspaceContextInput, WorkspaceHostIdentity, WorkspacePathMap, CHANNEL_PROTOCOL_VERSION,
    };
    let environment = public_environment_identity(&dir.join("home"), repo_root);
    let host_version = effigy_core::build_info::active_version();
    let guest_version = if mode == "identity-invalid" {
        "9.9.9".to_owned()
    } else {
        host_version.clone()
    };
    let context = WorkspaceContext::new(WorkspaceContextInput {
        generation: 1,
        host: WorkspaceHostIdentity {
            version: host_version,
            protocol: CHANNEL_PROTOCOL_VERSION,
        },
        binary: GuestBinaryIdentity {
            version: guest_version,
            digest: public_guest_digest(),
            protocol: CHANNEL_PROTOCOL_VERSION,
        },
        activation: WorkspaceActivationIdentity {
            host_user: "tester".to_owned(),
            checkout_id: "0:00".to_owned(),
            environment,
            container: WorkspaceContainerIdentity {
                container_id: if mode == "hostname-spoof" {
                    "ab".repeat(32)
                } else {
                    "0123456789abcdef".repeat(4)
                },
                vm_boot_id: "boot".to_owned(),
                platform: "x86_64".to_owned(),
                libc: "gnu".to_owned(),
            },
        },
        session_run_id: Some("run-1".to_owned()),
        session_generation: Some(1),
        paths: WorkspacePathMap {
            host_checkout: "/workspace".to_owned(),
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
        channel: WorkspaceChannelLocator {
            endpoint: Some("host:1".to_owned()),
            server_identity: Some("sha256:s".to_owned()),
        },
    });
    let path = dir.join(format!("guest-context-{mode}.json"));
    fs::write(&path, context.render().expect("render guest context")).expect("write guest context");
    path
}

/// Bytes the bounded entry reads during workspace preparation: the generation
/// number, the context document, the digest pointer and the executable.
fn public_guest_preparation_stdout(dir: &Path, mode: &str) -> u64 {
    let context = fs::read(dir.join(format!("guest-context-{mode}.json")))
        .map(|bytes| bytes.len() as u64)
        .unwrap_or(0);
    // The generation number, the document, the identity probe document, the
    // digest pointer and the digest probe's one line: never the executable's
    // own bytes.
    1 + context
        + public_guest_identity_probe(mode).len() as u64
        + public_guest_digest().len() as u64
        + 65
        + "/run/effigy/bin/".len() as u64
        + 64
        + "/effigy\n".len() as u64
        + 65
}

#[cfg(unix)]
fn run_guarded_public_case(
    dir: &effigy_test_support::Fixture,
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
    let final_marker = dir.path().join(format!("final-command-ran-{mode}"));
    let child_stdout_path = dir.path().join(format!("public-child-{mode}.stdout"));
    let child_stderr_path = dir.path().join(format!("public-child-{mode}.stderr"));
    let child_stdout = fs::File::create(&child_stdout_path).expect("create child stdout capture");
    let child_stderr = fs::File::create(&child_stderr_path).expect("create child stderr capture");
    let test_name = std::thread::current()
        .name()
        .expect("parent test name")
        .rsplit_once("::")
        .map(|(module, _)| format!("{module}::guarded_public_entry_child"))
        .expect("parent test is module-qualified");
    let current_exe = std::env::current_exe().expect("test executable path");
    let listed = Command::new(&current_exe)
        .args(["--list", "--exact", &test_name])
        .output()
        .expect("list required public child");
    assert!(listed.status.success());
    assert!(
        String::from_utf8_lossy(&listed.stdout)
            .lines()
            .any(|line| line == format!("{test_name}: test")),
        "required public child is absent: {test_name}"
    );
    let path = bin_dir.display().to_string();
    let mut command = Command::new(current_exe);
    command
        .arg("--exact")
        .arg(&test_name)
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("EFFIGY_BOUNDED_PUBLIC_CHILD", "1")
        .env("FAKE_BOUNDED_MODE", mode)
        .env("FAKE_BOUNDED_BACKEND", backend)
        .env("FAKE_BOUNDED_REPO", repo_root)
        .env("FAKE_BOUNDED_IDENTITY", identity)
        .env("FAKE_BOUNDED_PID_FILE", &pid_file)
        .env("FAKE_BOUNDED_FINAL_MARKER", &final_marker)
        .env("FAKE_BOUNDED_STDOUT_LIMIT", stdout_limit.to_string())
        .env("FAKE_BOUNDED_DEADLINE_MS", deadline_ms.to_string())
        .env("PATH", path)
        .env("HOME", &home)
        .env("TMPDIR", dir.path().join("tmp"))
        .env("TMP", dir.path().join("tmp"))
        .env("TEMP", dir.path().join("tmp"))
        .env("EFFIGY_TEST_CONTAINERS_HOME", home.join(".effigy"))
        .env("EFFIGY_COMPOSE_BACKEND", backend)
        .env(
            "EFFIGY_ACTIVE_VERSION",
            effigy_core::build_info::package_version(),
        )
        .env("EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE", "download")
        .env(
            "FAKE_BOUNDED_CONTEXT_FILE",
            write_public_guest_context(dir.path(), repo_root, mode),
        )
        .env("FAKE_BOUNDED_DIGEST", public_guest_digest())
        .env("FAKE_BOUNDED_PROBE", public_guest_identity_probe(mode))
        .env("FAKE_BOUNDED_BIN_DIR", bin_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::from(child_stderr));
    command.process_group(0);
    let child = spawn_public_child(&mut command);
    let mut cleanup = ProcessTreeCleanup {
        child,
        active: true,
    };
    let started = Instant::now();
    loop {
        if let Some(status) = cleanup.child.try_wait().expect("poll public-path test") {
            cleanup.active = false;
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
            return GuardedPublicOutcome { stdout, stderr };
        }
        if started.elapsed() >= Duration::from_millis(deadline_ms) + Duration::from_secs(10) {
            effigy_process::terminate_process_tree(cleanup.child.id(), true);
            let _ = cleanup.child.wait();
            cleanup.active = false;
            panic!("public-path child exceeded its deadline plus 10s cleanup guard for {mode}");
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
    assert_public_activation_fence(
        &repo_root,
        &PathBuf::from(std::env::var_os("FAKE_BOUNDED_CONTEXT_FILE").expect("fake context file")),
        &mode,
    );
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
    let (result, trace) =
        crate::runner::exec_command::BoundedCommandBudget::with_capture_limit_trace(|| {
            run_container(ContainerArgs {
                subcommand: ContainerSubcommand::Shell {
                    name: Some("web".to_owned()),
                    service: None,
                    command: Some(command.to_owned()),
                    bounded: Some(limits),
                },
                repo_override: Some(repo_root),
                output_json: mode != "inner-nonzero-secret",
            })
        });
    assert!(
        !trace.is_empty(),
        "public proof must reach a real capture: {result:?}"
    );
    let deadline = trace[0].1.deadline;
    let origin = trace[0].1.origin;
    assert!(
        origin < deadline,
        "the public entry origin must precede its deadline"
    );
    for pair in trace.windows(2) {
        assert_eq!(
            pair[1].1.deadline, deadline,
            "phase reset the absolute deadline"
        );
        assert_eq!(
            pair[1].1.origin, origin,
            "phase rebased the original operation origin"
        );
        assert!(pair[1].1.stdout_bytes <= pair[0].1.stdout_bytes);
        assert!(pair[1].1.stderr_bytes <= pair[0].1.stderr_bytes);
    }

    match mode.as_str() {
        "valid-identity" | "valid-colima" => {
            let output = result.expect("bounded public command succeeds");
            let value: serde_json::Value = serde_json::from_str(&output).expect("json output");
            assert_eq!(value["ok"], true);
            assert_eq!(value["stdout"], "command-only");
            assert_eq!(value["stdout_bytes"], "command-only".len());
            assert_eq!(value["attempts"], 1);
            // Three owned-service listings plus the published generation, its
            // document, the identity probe, the digest pointer and the pinned
            // executable, stable entry target and bytes launched through it.
            assert_eq!(
                value["preparation"]["captures"], 10,
                "public command must verify the published guest context and its pinned executable"
            );
            let preparation_stdout = value["preparation"]["retained"]["stdout_bytes"]
                .as_u64()
                .unwrap();
            let minimum_preparation_stdout =
                1 + std::env::var("FAKE_BOUNDED_DIGEST").unwrap().len() as u64 + 64;
            assert!(
                preparation_stdout >= minimum_preparation_stdout,
                "preparation must read the context, the pointer and the executable: {preparation_stdout}"
            );
            assert!(
                value["retained"]["stdout_bytes"].as_u64().unwrap()
                    == preparation_stdout + value["stdout_bytes"].as_u64().unwrap()
            );
            assert!(
                final_marker.exists(),
                "bounded command should execute after preparation"
            );
            let final_limit = &trace.last().expect("command capture").1;
            assert_eq!(final_limit.stdout_bytes, stdout_limit - preparation_stdout);
            assert_eq!(
                trace.len() as u64,
                value["preparation"]["captures"].as_u64().unwrap() + 1
            );
            assert!(trace
                .iter()
                .any(|(phase, _)| phase.label() == "workspace_preparation"));
            assert_eq!(trace.last().unwrap().0.label(), "command_execution");
        }
        "runtime-mismatch" => {
            let error = result.expect_err("stale runtime identity must refuse");
            let details = failure_details(&error);
            assert_eq!(details["failure"], "runtime_identity_mismatch");
            assert_eq!(details["phase"], "runtime_discovery");
            assert!(!final_marker.exists());
        }
        "hostname-spoof" => {
            let error = result.expect_err("guest claims cannot override the verified backend id");
            let details = failure_details(&error);
            assert_eq!(details["failure"], "workspace_preparation_failed");
            assert_eq!(details["phase"], "workspace_preparation");
            assert!(
                !final_marker.exists(),
                "the stale guest must never run the workload"
            );
        }
        "entry-missing" | "entry-wrong" | "entry-wrong-bytes" => {
            let error = result.expect_err("untrusted stable entry must refuse");
            let details = failure_details(&error);
            assert_eq!(details["failure"], "workspace_preparation_failed");
            assert_eq!(details["phase"], "workspace_preparation");
            assert!(!final_marker.exists(), "unverified entry must never launch");
        }
        "identity-invalid" => {
            let error = result.expect_err("stale workspace identity must require refresh");
            let details = failure_details(&error);
            assert_eq!(details["failure"], "workspace_preparation_failed");
            assert_eq!(details["phase"], "workspace_preparation");
            // Three owned-service listings, then the generation pointer and
            // its document: enough to refuse a stale version before any effect.
            assert_eq!(details["preparation"]["captures"], 5);
            // This phase failure is decided about five spawned-and-completed
            // captures; it is not a transport refusal, so it must not claim
            // any refusal observations (not_attempted would be false here).
            assert!(
                details.get("refusal_observations").is_none(),
                "a phase failure about completed captures carries no refusal block: {details}"
            );
            assert!(!final_marker.exists());
        }
        "launch-blocked-discovery" => {
            let error =
                result.expect_err("a blocked discovery cannot prove a later launch failure");
            let details = failure_details(&error);
            assert_eq!(details["failure"], "deadline");
            assert_eq!(details["phase"], "runtime_discovery");
            assert_eq!(details["attempts"], 0);
            assert!(!final_marker.exists());
        }
        "fallback-budget" => {
            let error = result.expect_err("fallback must consume the preparation budget");
            let details = failure_details(&error);
            assert_eq!(details["failure"], "stdout_overflow");
            assert_eq!(details["phase"], "command_execution");
            assert_eq!(details["attempts"], 2);
            assert_eq!(details["workspace_user_fallback"], true);
            assert_eq!(details["preparation"]["captures"], 10);
            assert!(!final_marker.exists());
        }
        "remove-before-command" => {
            // The fixture removes the backend at the first preparation read, so
            // the next client spawn is a launch failure and the command never
            // runs. Its arguments and both streams stay out of the human and
            // JSON diagnostics.
            let error = result.expect_err("a vanished backend cannot launch the command");
            let human = error.to_string();
            let json = error
                .json_error_details()
                .unwrap_or_else(|| panic!("expected bounded command failure, got: {human}"));
            assert!(!human.contains("SECRET_ARG_MARKER_142"));
            assert!(!json.contains("SECRET_ARG_MARKER_142"));
            assert!(!json.contains("docker compose"));
            let details: serde_json::Value =
                serde_json::from_str(json).expect("json details parse");
            assert_eq!(details["failure"], "launch_failed");
            assert_eq!(details["phase"], "workspace_preparation");
            assert_eq!(details["termination_scope"], "owned_host_client_group");
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

// Only spawn snapshots inherited variables. Release the environment lock before
// waiting for the private child: its environment can no longer be mutated by
// another test, and unrelated fixture setup need not wait for its whole run.
#[cfg(unix)]
fn spawn_public_child(command: &mut Command) -> Child {
    let _environment = lock_test();
    command.spawn().expect("spawn guarded public child")
}

#[cfg(unix)]
#[test]
fn public_child_environment_snapshot_waits_for_handoff_mutator() {
    use std::os::unix::process::CommandExt;
    let dir = tempfile::tempdir().expect("private snapshot cwd");
    let lock = lock_test();
    let marker = crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_NAME;
    let original = std::env::var_os(marker).unwrap_or_default();
    let env = EnvGuard::set_many(&[(marker, Some("1".to_owned()))]);
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "test \"${EFFIGY_INTERNAL_CONTAINER_HANDOFF-}\" = \"$1\"",
            "snapshot",
        ])
        .arg(original)
        .current_dir(dir.path())
        .process_group(0);
    let reader = thread::spawn(move || {
        let mut child = ProcessTreeCleanup {
            child: spawn_public_child(&mut command),
            active: true,
        };
        let status = child.child.wait().expect("wait for snapshot child");
        child.active = false;
        assert!(
            status.success(),
            "child inherited the temporary handoff marker"
        );
    });
    let queued = crate::contract_test_support::test_lock_waiter_position(
        reader.thread().id(),
        Duration::from_secs(30),
    );
    // Release before asserting/joining so a failed control cannot strand a child.
    drop(env);
    drop(lock);
    let result = reader.join();
    assert!(
        queued.is_some(),
        "child spawn must wait for the handoff mutator"
    );
    result.expect("child observes the restored environment");
}

// Semantic public-path cases make up to six real kernel-custodied captures.
// They test phase/byte/exit accounting, not an 8s host-startup performance SLA.
// Dedicated hang/expired cases below retain their short absolute deadlines.
const PUBLIC_SEMANTIC_BUDGET_MS: u64 = 60_000;

#[cfg(unix)]
#[test]
fn public_entry_bounds_discovery_preparation_and_fallback_on_the_full_path() {
    let (dir, bin_dir, repo) = public_fixture("public-path", true);
    let version = effigy_core::build_info::package_version();
    let identity = format!("release:v{version}");
    let _valid_docker = run_guarded_public_case(
        &dir,
        &bin_dir,
        &repo,
        GuardedPublicCase {
            mode: "valid-identity",
            backend: "docker",
            identity: &identity,
            deadline_ms: PUBLIC_SEMANTIC_BUDGET_MS,
            stdout_limit: 4096,
        },
    );
    // The public Colima path guard requires a repo path outside the temporary
    // directory. The fixture is a fresh directory in the Effigy scratch
    // namespace with its own temp child, so the repo is not under TMPDIR on any
    // platform; Linux still covers Colima through the private transport-path
    // case above.
    #[cfg(target_os = "macos")]
    let _valid_colima = run_guarded_public_case(
        &dir,
        &bin_dir,
        &repo,
        GuardedPublicCase {
            mode: "valid-colima",
            backend: "colima",
            identity: &identity,
            deadline_ms: PUBLIC_SEMANTIC_BUDGET_MS,
            stdout_limit: 4096,
        },
    );
}

#[cfg(unix)]
fn public_failure_case(mode: &str) -> GuardedPublicOutcome {
    let (dir, bin_dir, repo) = public_fixture(mode, true);
    let identity = format!("release:v{}", effigy_core::build_info::package_version());
    let stale = matches!(
        mode,
        "identity-invalid" | "workspace-hang" | "workspace-flood" | "remove-before-command"
    );
    // The fallback proof needs preparation (the published context, its digest
    // pointer and the pinned executable) to fit, then an inner command that
    // overflows on its second attempt.
    let _ = write_public_guest_context(dir.path(), &repo, mode);
    let fallback_limit = format!(
        "demo-web-app-1\tUp\t\tdemo-web\t{}\tapp\tfalse\n",
        repo.display()
    )
    .len() as u64
        + format!(
            "{}\tUp 1 minute\tFalse\t{}\t\t\n",
            "0123456789abcdef".repeat(4),
            repo.display()
        )
        .len() as u64
        + public_guest_preparation_stdout(dir.path(), mode)
        + 80;
    run_guarded_public_case(
        &dir,
        &bin_dir,
        &repo,
        GuardedPublicCase {
            mode,
            backend: "docker",
            identity: if stale { "release:vstale" } else { &identity },
            // Semantic proofs require their target phase, rather than racing a
            // cold2s discovery budget. Cleanup has its own bounded guard.
            deadline_ms: if matches!(mode, "runtime-hang" | "launch-blocked-discovery") {
                2_000
            } else if mode == "workspace-hang" {
                8_000
            } else {
                PUBLIC_SEMANTIC_BUDGET_MS
            },
            stdout_limit: match mode {
                "fallback-budget" => fallback_limit,
                "runtime-flood" => 64,
                "workspace-hang" | "workspace-flood" => 512,
                _ => 4096,
            },
        },
    )
}

#[cfg(unix)]
macro_rules! public_failure_test {
    ($name:ident, $mode:literal) => {
        #[test]
        fn $name() {
            let _outcome = public_failure_case($mode);
        }
    };
}

#[cfg(unix)]
public_failure_test!(
    public_runtime_deadline_refuses_before_command,
    "runtime-hang"
);
#[cfg(unix)]
public_failure_test!(public_runtime_flood_refuses_before_command, "runtime-flood");
#[cfg(unix)]
public_failure_test!(
    public_runtime_identity_mismatch_refuses_before_command,
    "runtime-mismatch"
);
#[cfg(unix)]
public_failure_test!(
    public_spoofed_hostname_cannot_reuse_an_old_context,
    "hostname-spoof"
);
#[cfg(unix)]
public_failure_test!(
    public_workspace_identity_invalid_requires_refresh,
    "identity-invalid"
);
#[cfg(unix)]
public_failure_test!(public_missing_entry_refuses_before_command, "entry-missing");
#[cfg(unix)]
public_failure_test!(public_wrong_entry_refuses_before_command, "entry-wrong");
#[cfg(unix)]
public_failure_test!(
    public_wrong_entry_bytes_refuse_before_command,
    "entry-wrong-bytes"
);

#[cfg(unix)]
public_failure_test!(
    public_workspace_deadline_refuses_before_command,
    "workspace-hang"
);
#[cfg(unix)]
public_failure_test!(
    public_workspace_flood_refuses_before_command,
    "workspace-flood"
);
#[cfg(unix)]
public_failure_test!(
    public_fallback_draws_from_the_same_cumulative_budget,
    "fallback-budget"
);
#[cfg(unix)]
public_failure_test!(
    public_launch_failure_redacts_human_and_json_diagnostics,
    "remove-before-command"
);
#[cfg(unix)]
public_failure_test!(
    public_earlier_deadline_is_incomplete_launch_evidence,
    "launch-blocked-discovery"
);

#[cfg(unix)]
#[test]
fn public_nonzero_inner_failure_emits_no_captured_secrets() {
    let outcome = public_failure_case("inner-nonzero-secret");
    for marker in [
        "SECRET_ARG_MARKER_142",
        "SECRET_STDOUT_MARKER_142",
        "SECRET_STDERR_MARKER_142",
    ] {
        assert!(!String::from_utf8_lossy(&outcome.stdout).contains(marker));
        assert!(!String::from_utf8_lossy(&outcome.stderr).contains(marker));
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
        deadline,
    )
    .expect_err("expired operation must refuse before discovery");
    let details = failure_details(&error);
    assert_eq!(details["failure"], "deadline");
    assert_eq!(details["phase"], "runtime_discovery");
    assert_eq!(details["preparation"]["captures"], 0);
    // The boundary refusal says explicitly that nothing was attempted and
    // nothing about streams or a leader is known.
    let observations = &details["refusal_observations"];
    assert_eq!(observations["version"], 1);
    assert_eq!(observations["spawn_disposition"], "not_attempted");
    assert_eq!(observations["leader_exit_observed"], "unknown");
    assert_eq!(observations["stdout"]["state"], "unknown");
    assert_eq!(observations["stderr"]["state"], "unknown");
    assert_eq!(observations["remaining_budget_ms"], 0);
    assert_eq!(observations["attempt_elapsed_ms"], serde_json::Value::Null);
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
                target_id: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
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

// ---- Secret-free versioned refusal accounting (effigy#159) ----
//
// Each proof drives the production bounded entry with real owned fake-backend
// processes and readers, then inspects the versioned `refusal_observations`
// block on the actual failure payload. No bytes, argv or errors may surface:
// only structural counts with explicit exact/lower-bound/unknown semantics.

use std::sync::atomic::AtomicU64;

use crate::runner::exec_command::{
    refusal_stream_observation, BoundedLeaderExitObservation, BoundedObservedBytes,
    BoundedRefusalObservations, BoundedSpawnDisposition, BoundedStreamProgress,
    BoundedStreamRefusalObservation, BoundedStreamRefusalState, StreamOutcome,
    BOUNDED_REFUSAL_OBSERVATIONS_VERSION,
};

fn observations_of(details: &serde_json::Value) -> &serde_json::Value {
    &details["refusal_observations"]
}

/// Deadline for the refusal proofs whose fixture must reach an observable
/// point (a write, a marker, a held pipe) inside it. The deadline is
/// absolute from entry, so the window must tolerate machine-load stalls:
/// process spawns on this shared host once stalled for 13 seconds under
/// machine-wide load. Healthy runs still refuse at the deadline itself.
const REFUSAL_PROOF_BUDGET_MS: u64 = 20_000;

fn assert_version_and_unknown_leader(observations: &serde_json::Value) {
    assert_eq!(
        observations["version"], BOUNDED_REFUSAL_OBSERVATIONS_VERSION,
        "observation block must be explicitly versioned: {observations}"
    );
    assert_eq!(
        observations["leader_exit_observed"], "unknown",
        "an unobserved leader must stay explicitly unknown: {observations}"
    );
}

fn assert_count_semantics(count: &serde_json::Value, semantics: &str) -> u64 {
    assert_eq!(
        count["semantics"], semantics,
        "count semantics must be explicit: {count}"
    );
    count["bytes"].as_u64().unwrap_or(0)
}

#[test]
fn refusal_accounting_pre_spawn_expiry_reports_not_attempted_with_unknown_streams() {
    let (_dir, bin_dir, repo) = fixture("refusal-pre-spawn");
    let policy = policy();
    let marker = repo.join("refusal-pre-spawn-spawned");
    {
        let _lock = lock_test();
        let current = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{}", bin_dir.display(), current);
        let _env = EnvGuard::set_many(&[
            ("PATH", Some(path)),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            ("FAKE_BOUNDED_MARKER", Some(marker.display().to_string())),
        ]);
        let origin = Instant::now();
        let error = run_bounded_command_with_deadline(
            BoundedCommandRequest {
                repo_root: &repo,
                policy: &policy,
                service: "app",
                target_id: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                working_dir: Some(Path::new("/workspace")),
                command: "plain",
                output_json: true,
            },
            &limits(30_000, 64, 64),
            origin
                .checked_sub(Duration::from_secs(1))
                .expect("expired deadline"),
        )
        .expect_err("an expired deadline refuses before any spawn");
        let details = failure_details(&error);
        let observations = observations_of(&details);
        assert_version_and_unknown_leader(observations);
        assert_eq!(observations["spawn_disposition"], "not_attempted");
        assert_eq!(observations["attempt_elapsed_ms"], serde_json::Value::Null);
        assert_eq!(
            observations["preparation_elapsed_ms"],
            serde_json::Value::Null
        );
        assert_eq!(observations["remaining_budget_ms"], 0);
        for stream in ["stdout", "stderr"] {
            assert_eq!(observations[stream]["state"], "unknown");
            assert_eq!(
                observations[stream]["retained_prefix_bytes"]["semantics"],
                "unknown"
            );
            assert_eq!(observations[stream]["read_bytes"]["semantics"], "unknown");
        }
        // Legacy fields keep their meanings: no attempt, no completed bytes.
        assert_eq!(details["attempts"], 0);
        assert_eq!(details["retained"]["stdout_bytes"], 0);
    }
    assert!(!marker.exists(), "an expired deadline must not spawn");
}

#[test]
fn refusal_accounting_deadline_pending_streams_report_lower_bounds() {
    let (_dir, bin_dir, repo) = fixture("refusal-pending");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        let error = run_json(&repo, &policy, "sleep-30", &limits(5_000, 64, 64), 5_000)
            .expect_err("slow command must hit the deadline");
        let details = failure_details(&error);
        assert_eq!(details["failure"], "deadline");
        let observations = observations_of(&details);
        assert_version_and_unknown_leader(observations);
        assert_eq!(observations["spawn_disposition"], "spawned");
        // A pending reader is a lower bound, never a zero claim or proof of
        // no effect; end-of-stream is not established either.
        for stream in ["stdout", "stderr"] {
            assert_eq!(observations[stream]["state"], "pending");
            assert_count_semantics(
                &observations[stream]["retained_prefix_bytes"],
                "lower_bound",
            );
            assert_count_semantics(&observations[stream]["read_bytes"], "lower_bound");
        }
        // Clock attribution from the original operation origin: the attempt
        // consumed the budget, and preparation plus attempt make the whole.
        let attempt_elapsed = observations["attempt_elapsed_ms"]
            .as_u64()
            .expect("attempt clock");
        let preparation_elapsed = observations["preparation_elapsed_ms"]
            .as_u64()
            .expect("preparation clock");
        let operation_elapsed = observations["operation_elapsed_ms"]
            .as_u64()
            .expect("operation clock");
        assert!(
            attempt_elapsed >= 3_750,
            "attempt ran to the deadline: {observations}"
        );
        assert_eq!(
            observations["remaining_budget_ms"], 0,
            "an expired budget has no remaining time"
        );
        assert!(
            preparation_elapsed.saturating_add(attempt_elapsed) <= operation_elapsed + 5,
            "clocks must attribute one coherent operation timeline: {observations}"
        );
        // Completed-capture accounting stays separate and unchanged: a
        // refused command contributes nothing to `retained`.
        assert_eq!(details["retained"]["stdout_bytes"], 0);
        assert_eq!(details["retained"]["stderr_bytes"], 0);
    });
}

#[test]
fn refusal_accounting_pending_progress_is_a_lower_bound_not_zero() {
    let (_dir, bin_dir, repo) = fixture("refusal-flood-progress");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        // 100000 bytes cannot fit in any pipe buffer, so the reader must
        // have consumed whole chunks before the child can finish writing;
        // the child then sleeps holding both pipes, keeping them pending.
        let error = run_json(
            &repo,
            &policy,
            "pending-flood",
            &limits(REFUSAL_PROOF_BUDGET_MS, 1_048_576, 64),
            REFUSAL_PROOF_BUDGET_MS,
        )
        .expect_err("the sleeping leader must hit the deadline");
        let details = failure_details(&error);
        assert_eq!(details["failure"], "deadline");
        let observations = observations_of(&details);
        assert_eq!(observations["spawn_disposition"], "spawned");
        assert_eq!(observations["stdout"]["state"], "pending");
        let retained = assert_count_semantics(
            &observations["stdout"]["retained_prefix_bytes"],
            "lower_bound",
        );
        let read = assert_count_semantics(&observations["stdout"]["read_bytes"], "lower_bound");
        // A pending stream's progress is a truthful lower bound, never a
        // zero claim or an unknown: the flood reached the reader.
        assert!(
            retained >= 1,
            "pending retained progress must be a real lower bound: {observations}"
        );
        assert!(
            read >= retained,
            "reads include discarded bytes and never trail retention: {observations}"
        );
        assert_eq!(observations["stderr"]["state"], "pending");
        // The failed prefix is reported but never charged to completed
        // accounting or the remaining budget.
        assert_eq!(details["retained"]["stdout_bytes"], 0);
    });
}

#[test]
fn refusal_accounting_leader_exit_with_inherited_writer_keeps_partial_output_visible() {
    let (_dir, bin_dir, repo) = fixture("refusal-partial-hold");
    let policy = policy();
    let progress_marker = repo.join("refusal-partial-hold-write-point");
    {
        let _lock = lock_test();
        let current = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{}", bin_dir.display(), current);
        let _env = EnvGuard::set_many(&[
            ("PATH", Some(path)),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            ("FAKE_BOUNDED_MARKER", None),
            (
                "FAKE_BOUNDED_PROGRESS_MARKER",
                Some(progress_marker.display().to_string()),
            ),
        ]);
        // The admitted shape: the leader exits fast after private partial
        // output on both channels while an inherited writer keeps the pipes
        // open, so the refusal snapshot is the only record of those bytes.
        // The generous deadline only waits out process start under load; the
        // progress marker proves the fixture reached its write point.
        let error = run_json(
            &repo,
            &policy,
            "partial-both-hold",
            &limits(REFUSAL_PROOF_BUDGET_MS, 64, 64),
            REFUSAL_PROOF_BUDGET_MS,
        )
        .expect_err("an inherited writer must not defeat the deadline");
        let details = failure_details(&error);
        assert!(
            progress_marker.exists(),
            "fixture must reach its write point before the counts mean anything"
        );
        assert_eq!(details["failure"], "deadline");
        let observations = observations_of(&details);
        assert_version_and_unknown_leader(observations);
        assert_eq!(observations["spawn_disposition"], "spawned");
        // Both channels held partial bytes; the lower bounds say at least
        // the three written bytes without exposing them.
        for (stream, written) in [("stdout", 3), ("stderr", 3)] {
            assert_eq!(observations[stream]["state"], "pending");
            let retained = assert_count_semantics(
                &observations[stream]["retained_prefix_bytes"],
                "lower_bound",
            );
            assert!(
                retained >= written,
                "the refused {stream} prefix must show its partial bytes: {observations}"
            );
        }
        // Equal preparation and cumulative counts still establish nothing
        // about command output: `retained` covers completed captures only.
        assert_eq!(details["retained"]["stdout_bytes"], 0);
        assert_eq!(details["retained"]["stderr_bytes"], 0);
    }
}

#[test]
fn refusal_accounting_exact_eof_and_pending_states_coexist_in_one_snapshot() {
    let (_dir, bin_dir, repo) = fixture("refusal-asymmetric");
    let policy = policy();
    let progress_marker = repo.join("refusal-asymmetric-stdout-closed");
    {
        let _lock = lock_test();
        let current = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{}", bin_dir.display(), current);
        let _env = EnvGuard::set_many(&[
            ("PATH", Some(path)),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            ("FAKE_BOUNDED_MARKER", None),
            (
                "FAKE_BOUNDED_PROGRESS_MARKER",
                Some(progress_marker.display().to_string()),
            ),
        ]);
        let error = run_json(
            &repo,
            &policy,
            "eof-hold-stderr",
            &limits(REFUSAL_PROOF_BUDGET_MS, 64, 64),
            REFUSAL_PROOF_BUDGET_MS,
        )
        .expect_err("a held stderr must keep the operation pending");
        let details = failure_details(&error);
        // The marker is written only after the fixture closed stdout, so the
        // EOF expectation below is load-independent.
        assert!(
            progress_marker.exists(),
            "fixture must reach its stdout-closed point"
        );
        assert_eq!(details["failure"], "deadline");
        let observations = observations_of(&details);
        assert_version_and_unknown_leader(observations);
        // stdout: a real EOF with zero bytes is an exact zero, distinct from
        // an unknown or a fabricated combined state.
        assert_eq!(observations["stdout"]["state"], "eof");
        let retained =
            assert_count_semantics(&observations["stdout"]["retained_prefix_bytes"], "exact");
        assert_eq!(retained, 0);
        assert_count_semantics(&observations["stdout"]["read_bytes"], "exact");
        // stderr: still pending, so only lower bounds and no EOF claim.
        assert_eq!(observations["stderr"]["state"], "pending");
        assert_count_semantics(
            &observations["stderr"]["retained_prefix_bytes"],
            "lower_bound",
        );
        assert_count_semantics(&observations["stderr"]["read_bytes"], "lower_bound");
    }
}

#[test]
fn refusal_accounting_overflow_snapshot_is_exact_for_the_failing_stream() {
    let (_dir, bin_dir, repo) = fixture("refusal-overflow");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        let error = run_json(&repo, &policy, "big-stdout", &limits(30_000, 4, 64), 30_000)
            .expect_err("stdout overflow must refuse");
        let details = failure_details(&error);
        assert_eq!(details["failure"], "stdout_overflow");
        let observations = observations_of(&details);
        assert_eq!(observations["spawn_disposition"], "spawned");
        assert_eq!(observations["stdout"]["state"], "overflow");
        let retained =
            assert_count_semantics(&observations["stdout"]["retained_prefix_bytes"], "exact");
        assert!(retained <= 4, "retention stays bounded: {observations}");
        let read = assert_count_semantics(&observations["stdout"]["read_bytes"], "lower_bound");
        assert!(
            read >= retained,
            "reads never trail retention: {observations}"
        );
        // The sibling stream never claims a fabricated combined state.
        let sibling_state = observations["stderr"]["state"]
            .as_str()
            .expect("stderr state");
        assert!(
            sibling_state == "pending" || sibling_state == "eof",
            "sibling must be pending or eof, not a fabricated state: {observations}"
        );
        assert!(!details.to_string().contains("xxxx"));
    });
}

#[test]
fn refusal_accounting_secret_partial_output_never_reaches_any_surface() {
    let (_dir, bin_dir, repo) = fixture("refusal-secrets");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        let error = run_json(
            &repo,
            &policy,
            "hang-secrets",
            &limits(PUBLIC_SEMANTIC_BUDGET_MS, 64, 64),
            PUBLIC_SEMANTIC_BUDGET_MS,
        )
        .expect_err("the hanging secret command must hit the deadline");
        let human = error.to_string();
        let json = error.json_error_details().expect("json details");
        let details = failure_details(&error);
        assert_eq!(details["failure"], "deadline");
        for marker in [
            "SECRET_ARG_MARKER_142",
            "SECRET_STDOUT_MARKER_142",
            "SECRET_STDERR_MARKER_142",
        ] {
            assert!(!human.contains(marker), "human error leaked {marker}");
            assert!(!json.contains(marker), "json details leaked {marker}");
        }
        // The bytes flowed; only structural counts may represent them.
        let observations = observations_of(&details);
        for stream in ["stdout", "stderr"] {
            let retained = assert_count_semantics(
                &observations[stream]["retained_prefix_bytes"],
                "lower_bound",
            );
            assert!(
                retained >= 1,
                "partial {stream} must be counted: {observations}"
            );
        }
        assert_eq!(details["retained"]["stdout_bytes"], 0);
    });
}

#[test]
fn refusal_accounting_success_and_inner_failures_carry_no_observation_block() {
    let (_dir, bin_dir, repo) = fixture("refusal-absent");
    let policy = policy();
    with_fake_backend(&bin_dir, "docker", || {
        let output = run_json(&repo, &policy, "plain", &limits(30_000, 64, 64), 30_000)
            .expect("bounded command succeeds");
        let value: serde_json::Value = serde_json::from_str(&output).expect("json");
        assert!(
            value.get("refusal_observations").is_none(),
            "a complete capture is not a refusal: {value}"
        );
        let error = run_json(
            &repo,
            &policy,
            "SECRET_ARG_MARKER_142",
            &limits(30_000, 64, 64),
            30_000,
        )
        .expect_err("inner nonzero exit must fail");
        let details = failure_details(&error);
        assert_eq!(details["failure"], "inner_command_failed");
        assert!(
            details.get("refusal_observations").is_none(),
            "an inner failure is a complete in-budget capture, not a refusal: {details}"
        );
    });
}

#[test]
fn refusal_accounting_snapshot_mapping_matches_each_reader_outcome() {
    // Capability limit: a genuine pipe read error is not inducible through
    // the spawned production path (Stdio::piped builds the pipe internally,
    // and a child closing a stream yields EOF, not a read error), so the
    // read_error snapshot is proven here from the exact production reader's
    // outcome plus the real reader loop's stop behavior
    // (bounded_reader_reports_read_failure_without_panicking), not by a
    // spawned process. Inducing it in the production transport would need
    // new injection authority that does not exist.
    let progress = |retained: u64, read: u64| BoundedStreamProgress {
        retained: AtomicU64::new(retained),
        read: AtomicU64::new(read),
    };

    // EOF-complete: both counts final and exact.
    let observation = refusal_stream_observation(
        Some(&StreamOutcome::Complete(b"out".to_vec())),
        &progress(3, 3),
        false,
    );
    assert_eq!(observation.state, BoundedStreamRefusalState::Eof);
    assert_eq!(observation.retained_prefix, BoundedObservedBytes::Exact(3));
    assert_eq!(observation.read_total, BoundedObservedBytes::Exact(3));

    // Overflow: the retained prefix is exact and final; reads continue
    // draining discarded bytes, so the read total stays a lower bound.
    let observation = refusal_stream_observation(
        Some(&StreamOutcome::Overflow { retained_bytes: 4 }),
        &progress(4, 4096),
        false,
    );
    assert_eq!(observation.state, BoundedStreamRefusalState::Overflow);
    assert_eq!(observation.retained_prefix, BoundedObservedBytes::Exact(4));
    assert_eq!(
        observation.read_total,
        BoundedObservedBytes::LowerBound(4096)
    );

    // Read failure: the reader stopped, so its counters are final and exact
    // even though the kept prefix was never delivered.
    let observation = refusal_stream_observation(
        Some(&StreamOutcome::ReadFailed),
        &progress(2048, 2048),
        false,
    );
    assert_eq!(observation.state, BoundedStreamRefusalState::ReadError);
    assert_eq!(
        observation.retained_prefix,
        BoundedObservedBytes::Exact(2048)
    );
    assert_eq!(observation.read_total, BoundedObservedBytes::Exact(2048));

    // A reader that ended without reporting: explicit unknown state, with a
    // truthful lower bound on the lost prefix instead of a zero claim.
    let observation = refusal_stream_observation(None, &progress(512, 512), true);
    assert_eq!(observation.state, BoundedStreamRefusalState::Unknown);
    assert_eq!(
        observation.retained_prefix,
        BoundedObservedBytes::LowerBound(512)
    );
    assert_eq!(observation.read_total, BoundedObservedBytes::Exact(512));

    // Pending reader: everything is a lower bound and nothing is claimed
    // about end-of-stream.
    let observation = refusal_stream_observation(None, &progress(0, 0), false);
    assert_eq!(observation.state, BoundedStreamRefusalState::Pending);
    assert_eq!(
        observation.retained_prefix,
        BoundedObservedBytes::LowerBound(0)
    );
    assert_eq!(observation.read_total, BoundedObservedBytes::LowerBound(0));
}

#[test]
fn refusal_accounting_boundary_observations_describe_no_attempt() {
    let origin = Instant::now();
    let deadline = origin + Duration::from_secs(30);
    let observations = BoundedRefusalObservations::budget_boundary(origin, deadline, origin);
    assert_eq!(observations.version, BOUNDED_REFUSAL_OBSERVATIONS_VERSION);
    assert_eq!(
        observations.spawn_disposition,
        BoundedSpawnDisposition::NotAttempted
    );
    assert_eq!(
        observations.leader_exit,
        BoundedLeaderExitObservation::Unknown
    );
    assert_eq!(observations.attempt_elapsed, None);
    assert_eq!(observations.preparation_elapsed, None);
    for stream in [observations.stdout, observations.stderr] {
        let BoundedStreamRefusalObservation {
            state,
            retained_prefix,
            read_total,
        } = stream;
        assert_eq!(state, BoundedStreamRefusalState::Unknown);
        assert_eq!(retained_prefix, BoundedObservedBytes::Unknown);
        assert_eq!(read_total, BoundedObservedBytes::Unknown);
    }
}

/// In-container re-entry for the bounded surface: under the handoff marker
/// the command runs locally through the caller's bounded budget instead of
/// probing a host runtime. Refusal shims for every host runtime binary fail
/// the test if any probe escapes the container.
#[cfg(unix)]
mod handoff_local_tests {
    use super::super::run_container_shell_bounded;
    use super::limits;
    use crate::contract_test_support::{lock_test, EnvGuard};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn handoff_repo(
        name: &str,
    ) -> (
        effigy_test_support::Fixture,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let (dir, bin_dir, repo_root) = super::fixture_in(name, None);
        let workspace = repo_root.join("workspace");
        fs::create_dir_all(&workspace).expect("mkdir handoff workspace");
        fs::write(
            repo_root.join("docker-compose.yml"),
            "services:\n  app:\n    image: busybox\n",
        )
        .expect("write compose file");
        fs::write(
            repo_root.join("effigy.toml"),
            format!(
                "[containers]\ndefault = \"web\"\n\n[containers.web]\ncompose_file = \"docker-compose.yml\"\nproject_name = \"demo-web\"\nprimary_service = \"app\"\nworking_dir = '{}'\n",
                workspace.display(),
            ),
        )
        .expect("write manifest");
        for program in ["colima", "docker", "nerdctl", "compose"] {
            let path = bin_dir.join(program);
            fs::write(
                &path,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 42\n",
                    repo_root
                        .join(format!("host-runtime-{program}.log"))
                        .display()
                ),
            )
            .expect("write refusal shim");
            let mut permissions = fs::metadata(&path).expect("stat shim").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("chmod shim");
        }
        (dir, bin_dir, repo_root)
    }

    fn handoff_env(
        bin_dir: &std::path::Path,
        home: &std::path::Path,
    ) -> crate::contract_test_support::EnvGuard {
        let current = std::env::var("PATH").unwrap_or_default();
        EnvGuard::set_many(&[
            (
                crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_NAME,
                Some("1".to_owned()),
            ),
            ("PATH", Some(format!("{}:{current}", bin_dir.display()))),
            ("HOME", Some(home.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        ])
    }

    fn assert_no_host_runtime_probes(repo_root: &std::path::Path) {
        for program in ["colima", "docker", "nerdctl", "compose"] {
            assert!(
                !repo_root
                    .join(format!("host-runtime-{program}.log"))
                    .exists(),
                "host runtime `{program}` was probed from inside the handoff"
            );
        }
    }

    fn guest_tree(
        root: &std::path::Path,
    ) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
        fn walk(
            dir: &std::path::Path,
            files: &mut std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
        ) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    files.insert(path.clone(), Vec::new());
                    walk(&path, files);
                } else {
                    files.insert(path.clone(), fs::read(path).unwrap());
                }
            }
        }
        let mut files = std::collections::BTreeMap::new();
        walk(root, &mut files);
        files
    }

    // Neither a missing legacy Compose file nor an unavailable host catalog
    // prevents local execution. Both would fail a host policy resolution.
    #[test]
    fn guest_resolves_execution_without_rendering_host_policy() {
        let (dir, bin_dir, repo_root) = handoff_repo("guest-no-host-policy");
        let home = dir.path().join("home");
        fs::create_dir_all(&home).expect("mkdir isolated home");
        let _lock = lock_test();
        let _env = handoff_env(&bin_dir, &home);
        fs::remove_file(repo_root.join("docker-compose.yml")).unwrap();
        // An ephemeral checkout used to mint a scope token during policy load.
        fs::create_dir(repo_root.join(".git")).unwrap();
        fs::write(
            repo_root.join(".git/config"),
            "[effigy]\nruntimeScope = ephemeral\n",
        )
        .unwrap();
        let resolve = || {
            crate::runner::container_runtime::resolve_handoff_local_execution(
                &repo_root, None, None,
            )
            .expect("guest route")
            .expect("manifest coordinates require no host policy")
        };
        let before = guest_tree(dir.path());
        let target = resolve();
        assert_eq!(
            guest_tree(dir.path()),
            before,
            "guest resolution must write nothing"
        );
        assert_eq!(target.container, "web");
        assert_eq!(target.service, "app");
        assert_eq!(target.working_dir, repo_root.join("workspace"));
        assert!(!repo_root.join(".effigy").exists());
        assert!(!repo_root.join(".git/effigy-runtime-scope").exists());
        assert_eq!(fs::read_dir(&home).unwrap().count(), 0);
        assert_no_host_runtime_probes(&repo_root);

        fs::write(
            repo_root.join("effigy.toml"),
            format!(
                "[containers]\ndefault = 'web'\n[containers.web]\nprimary_service = 'app'\nworking_dir = '{}'\n[containers.web.services.app]\ncatalog = 'host-only-unavailable-catalog'\n",
                repo_root.join("workspace").display(),
            ),
        ).unwrap();
        let before = guest_tree(dir.path());
        let target = resolve();
        assert_eq!(
            guest_tree(dir.path()),
            before,
            "guest resolution must write nothing"
        );
        assert_eq!(target.service, "app");
        assert!(!repo_root.join(".effigy").exists());
        assert!(!repo_root.join(".git/effigy-runtime-scope").exists());
        assert_eq!(fs::read_dir(&home).unwrap().count(), 0);
        assert_no_host_runtime_probes(&repo_root);
    }

    #[test]
    fn guest_non_primary_service_keeps_checkout_working_directory() {
        let (dir, bin_dir, repo_root) = handoff_repo("guest-sidecar");
        let home = dir.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let _lock = lock_test();
        let _env = handoff_env(&bin_dir, &home);
        let target = crate::runner::container_runtime::resolve_handoff_local_execution(
            &repo_root,
            Some("web"),
            Some("sidecar"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(target.service, "sidecar");
        assert_eq!(target.working_dir, repo_root);
        assert!(!repo_root.join(".effigy").exists());
        assert_eq!(fs::read_dir(&home).unwrap().count(), 0);
        assert_no_host_runtime_probes(&repo_root);
    }

    #[test]
    fn bounded_shell_command_runs_handoff_local_without_host_probes() {
        let (dir, bin_dir, repo_root) = handoff_repo("bounded-shell-local");
        let home = dir.path().join("home");
        fs::create_dir_all(&home).expect("mkdir isolated home");
        let _lock = lock_test();
        let _env = handoff_env(&bin_dir, &home);
        let marker = repo_root.join("handoff-bounded-marker.txt");

        let output = run_container_shell_bounded(
            &repo_root,
            Some("web"),
            Some("app"),
            &format!("printf handoff-bounded-ok > '{}'", marker.display()),
            limits(30_000, 65536, 65536),
            false,
        )
        .expect("handoff bounded shell should run locally");
        assert!(
            output.contains("bounded container command finished for container `web` service `app`"),
            "got: {output}"
        );
        assert_eq!(
            fs::read_to_string(&marker).expect("read marker"),
            "handoff-bounded-ok"
        );
        assert_no_host_runtime_probes(&repo_root);
    }

    #[test]
    fn bounded_shell_command_handoff_local_keeps_json_schema() {
        let (dir, bin_dir, repo_root) = handoff_repo("bounded-shell-local-json");
        let home = dir.path().join("home");
        fs::create_dir_all(&home).expect("mkdir isolated home");
        let _lock = lock_test();
        let _env = handoff_env(&bin_dir, &home);

        let output = run_container_shell_bounded(
            &repo_root,
            Some("web"),
            Some("app"),
            "printf handoff-json-ok",
            limits(30_000, 65536, 65536),
            true,
        )
        .expect("handoff bounded shell should run locally");
        let payload: serde_json::Value =
            serde_json::from_str(&output).expect("bounded output must stay JSON");
        assert_eq!(
            payload["schema"], "effigy.container.bounded-exec.v1",
            "got: {output}"
        );
        assert_eq!(payload["ok"], true, "got: {output}");
        assert_eq!(payload["container"], "web", "got: {output}");
        assert_eq!(payload["service"], "app", "got: {output}");
        assert_eq!(payload["failure"], "complete", "got: {output}");
        assert_eq!(payload["stdout"], "handoff-json-ok", "got: {output}");
        assert_no_host_runtime_probes(&repo_root);
    }

    #[test]
    fn bounded_shell_command_handoff_stdout_cap_still_applies() {
        let (dir, bin_dir, repo_root) = handoff_repo("bounded-shell-local-overflow");
        let home = dir.path().join("home");
        fs::create_dir_all(&home).expect("mkdir isolated home");
        let _lock = lock_test();
        let _env = handoff_env(&bin_dir, &home);

        let error = run_container_shell_bounded(
            &repo_root,
            Some("web"),
            Some("app"),
            "i=0; while [ \"$i\" -lt 100 ]; do printf 'x'; i=$((i+1)); done",
            limits(30_000, 64, 65536),
            true,
        )
        .expect_err("overflowing stdout must refuse even handoff-local");
        let details = super::failure_details(&error);
        assert_eq!(details["failure"], "stdout_overflow", "got: {details}");
        assert_no_host_runtime_probes(&repo_root);
    }

    #[test]
    fn bounded_shell_command_handoff_expired_deadline_refuses_before_spawn() {
        let (dir, bin_dir, repo_root) = handoff_repo("bounded-shell-local-deadline");
        let home = dir.path().join("home");
        fs::create_dir_all(&home).expect("mkdir isolated home");
        let _lock = lock_test();
        let _env = handoff_env(&bin_dir, &home);
        let marker = repo_root.join("handoff-bounded-deadline-marker.txt");

        let past = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(1))
            .expect("past deadline");
        let error = super::super::run_container_shell_bounded_with_deadline(
            &repo_root,
            Some("web"),
            Some("app"),
            &format!("printf late > '{}'", marker.display()),
            limits(30_000, 65536, 65536),
            true,
            past,
            past,
        )
        .expect_err("expired deadline must refuse before any spawn");
        let details = super::failure_details(&error);
        assert_eq!(details["failure"], "deadline", "got: {details}");
        assert_eq!(details["phase"], "runtime_discovery", "got: {details}");
        assert!(
            !marker.exists(),
            "expired handoff operation must not run the command"
        );
        assert_no_host_runtime_probes(&repo_root);
    }

    /// Plain `container shell --command` under the handoff marker runs
    /// locally without probing a host runtime. Moved here from the lifecycle
    /// tests so it runs under the nextest harness (one process per test):
    /// the plain parallel cargo harness shares process env, and an
    /// unguarded concurrent PATH write can make the PATH-resolved `sh`
    /// spawn fail with ENOENT.
    #[test]
    fn handoff_container_shell_command_runs_locally_without_host_runtime() {
        let (dir, bin_dir, repo_root) = handoff_repo("shell-command-local");
        let home = dir.path().join("home");
        fs::create_dir_all(&home).expect("mkdir isolated home");
        let _lock = lock_test();
        let _env = handoff_env(&bin_dir, &home);
        let marker = repo_root.join("shell-marker.txt");

        let output = crate::runner::container_command::lifecycle::run_container_shell(
            &repo_root,
            Some("web"),
            Some("app"),
            Some(&format!("printf handoff-shell-ok > '{}'", marker.display())),
            false,
        )
        .expect("handoff shell command should run locally");
        assert!(
            output.contains("finished container shell for `web` service `app`"),
            "got: {output}"
        );
        assert_eq!(
            fs::read_to_string(&marker).expect("read marker"),
            "handoff-shell-ok"
        );
        assert_no_host_runtime_probes(&repo_root);
    }

    /// Plain `container exec` capture under the handoff marker runs locally
    /// without probing a host runtime. Same isolation rationale as above.
    #[test]
    fn handoff_container_exec_operation_runs_locally_without_host_runtime() {
        let (dir, bin_dir, repo_root) = handoff_repo("exec-operation-local");
        let home = dir.path().join("home");
        fs::create_dir_all(&home).expect("mkdir isolated home");
        let _lock = lock_test();
        let _env = handoff_env(&bin_dir, &home);
        let marker = repo_root.join("exec-marker.txt");

        let output =
            crate::runner::container_command::lifecycle::run_container_exec_operation_capture(
                &repo_root,
                Some("web"),
                effigy_containers::ContainerCapturedExecOperation {
                    service: Some("app".to_owned()),
                    command: vec![
                        "sh".to_owned(),
                        "-lc".to_owned(),
                        format!("printf handoff-exec-ok > '{}'", marker.display()),
                    ],
                    stdin_file: None,
                    cwd: None,
                    env: std::collections::BTreeMap::new(),
                },
            )
            .expect("handoff exec operation should run locally");
        assert!(output.status.success(), "exec failed: {output:?}");
        assert_eq!(
            fs::read_to_string(&marker).expect("read marker"),
            "handoff-exec-ok"
        );
        assert_no_host_runtime_probes(&repo_root);
    }
}
