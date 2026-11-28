//! The file-event relay through the container lifecycle, against a fake
//! runtime that records what each guest stream receives.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_containers::environment::Retention;
use effigy_containers::fs_relay::detect::Invocation;
use effigy_containers::EffectiveContainerPolicy;

use super::down::run_container_down_command;
use super::status::run_container_status_command;
use super::up::run_container_up;
use crate::contract_test_support::{clean_fixture_environment, with_cwd, EnvGuard};
use crate::runner::fs_relay::{start_session, RelayInvocation};
use crate::runner::host_process::stack_fs_relay_running;

const FAKE_DOCKER: &str = r#"#!/bin/sh
state="@STATE@"
printf '%s\n' "$*" >> "$state/args.log"
if [ "$1" = exec ] && [ "$2" = -i ]; then
  echo $$ >> "$state/stream.pids"
  exec cat >> "$state/received"
fi
subcmd=""
for arg in "$@"; do
  case "$arg" in
    up|down|ps|inspect|logs|exec) subcmd="$arg"; break ;;
  esac
done
case "$subcmd" in
  "") case "$*" in *version*) printf 'Docker Compose version v2.30.0\n' ;; esac ;;
  up)
    if [ -f "$state/fail-up" ]; then echo 'no such image' >&2; exit 1; fi
    : > "$state/running"
    case "$*" in *--force-recreate*)
      if [ -f "$state/reattach" ]; then printf '1 0 0:1 / /workspace/.effigy/state rw - ext4 /dev/a rw\n2 0 0:2 / /workspace/.effigy/secrets rw - ext4 /dev/b rw\n' > "$state/mountinfo"; fi ;;
    esac
    ;;
  exec)
    marker=""
    for arg in "$@"; do
      case "$arg" in
        effigy-named-volume-mounts) cat "$state/mountinfo"; exit 0 ;;
        effigy-workspace-identity) printf '501\n20\n'; exit 0 ;;
        effigy-perm-inspect-batch|effigy-perm-find-batch|effigy-perm-access-batch) marker="$arg"; continue ;;
      esac
      case "$marker:$arg" in
        effigy-perm-inspect-batch:/*) printf 'dir 501 20 755\n' ;;
        effigy-perm-find-batch:/*) printf 'clean\n' ;;
        effigy-perm-access-batch:/*) printf 'ready\n' ;;
      esac
    done
    ;;
  ps)
    if [ -f "$state/running" ]; then
      printf 'fs-relay-fixture-app-1\tUp 1 second\t\tfs-relay-fixture\t@REPO@\tapp\tFalse\n'
    fi
    ;;
  inspect)
    id=$(printf '%064d' 0 | tr 0 a)
    printf '[{"Id":"%s","Name":"/fs-relay-fixture-app-1","State":{"Status":"running","Running":true,"ExitCode":0},"Config":{"Labels":{"com.docker.compose.project":"fs-relay-fixture","com.docker.compose.service":"app","com.docker.compose.oneoff":"False","com.docker.compose.project.working_dir":"@REPO@","com.effigy.checkout":"@REPO@"}},"Mounts":[{"Type":"bind","Source":"@REPO@","Destination":"/workspace","RW":true}],"NetworkSettings":{"Ports":{}}}]\n' "$id"
    ;;
  logs) sleep 2 ;;
  down) rm -f "$state/running" ;;
esac
exit 0
"#;

struct RelayFixture {
    _temp: effigy_test_support::Fixture,
    repo: PathBuf,
    state: PathBuf,
    _env: EnvGuard,
    _ambient: EnvGuard,
}

impl RelayFixture {
    /// `host` is the body of `[containers.dev.host]`.
    fn new(host: &str) -> Self {
        let ambient = clean_fixture_environment();
        let temp = effigy_test_support::Fixture::workspace("effigy-fs-relay-")
            .expect("private fixture root");
        let root = temp.path().canonicalize().expect("canonical root");
        let repo = root.join("checkout");
        let state = root.join("runtime");
        let bin = root.join("bin");
        let home = root.join("home");
        for dir in [&repo, &state, &bin, &home] {
            fs::create_dir_all(dir).expect("fixture directory");
        }
        let git = std::process::Command::new("git")
            .args(["init", "-q", "--initial-branch=main"])
            .current_dir(&repo)
            .status()
            .expect("git");
        assert!(git.success(), "fixture git init");
        effigy_core::worktree_scope::prepare_checkout_token(&repo).expect("checkout identity");
        fs::write(
            repo.join("docker-compose.yml"),
            "services:\n  app:\n    image: example:one\n",
        )
        .expect("compose");
        fs::write(
            repo.join("effigy.toml"),
            format!(
                "[containers]\ndefault = \"dev\"\n\n[containers.dev]\ncompose_file = \"docker-compose.yml\"\nprimary_service = \"app\"\nproject_name = \"fs-relay-fixture\"\nworking_dir = \"/workspace\"\n\n[containers.dev.host]\nmounts = [\"./:/workspace\"]\n{host}\n"
            ),
        )
        .expect("manifest");
        crate::runner::test_support::migrate_declared_compose_files(&repo);
        write_executable(
            &bin.join("docker"),
            &FAKE_DOCKER
                .replace("@STATE@", &state.display().to_string())
                .replace("@REPO@", &repo.display().to_string()),
        );
        write_executable(&bin.join("colima"), "#!/bin/sh\nexit 0\n");
        // The supervised relay's executable: it records its argv, publishes its
        // stack record after a short start, then stays up. Its path reaches the
        // launch as a classified environment value.
        let relay = r#"#!/bin/sh
printf '%s\n' "$*" >> 'STATE/relay.argv'
sleep 0.5
record='REPO/.effigy/runtime/fs-events/dev.json'
mkdir -p "$(dirname "$record")"
cat > "$record" <<EOF
{"schema":"effigy.fs-events.relay.v1","container":"dev","owner":"stack","pid":$$,"state":"active","reason":"explicit_on","targets":[]}
EOF
exec sleep 120
"#
        .replace("STATE", &state.display().to_string())
        .replace("REPO", &repo.display().to_string());
        write_executable(&bin.join("effigy"), &relay);
        let env = EnvGuard::set_many(&[
            ("HOME", Some(home.display().to_string())),
            ("PATH", Some(format!("{}:/usr/bin:/bin", bin.display()))),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
            (
                "EFFIGY_EXECUTABLE",
                Some(bin.join("effigy").display().to_string()),
            ),
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(home.display().to_string()),
            ),
        ]);
        RelayFixture {
            _temp: temp,
            repo,
            state,
            _env: env,
            _ambient: ambient,
        }
    }

    fn policy(&self) -> EffectiveContainerPolicy {
        effigy_containers::load_container_policy(&self.repo, None).expect("fixture policy")
    }

    fn up(&self, attach: bool) -> Result<String, crate::runner::error::RunnerError> {
        with_cwd(&self.repo, || {
            run_container_up(
                &self.repo,
                None,
                attach,
                !attach,
                false,
                Retention::Persistent,
                &[],
            )
        })
    }

    fn down(&self) {
        with_cwd(&self.repo, || {
            run_container_down_command(Some(self.repo.clone()), None, false, false)
        })
        .expect("container down");
    }

    fn status(&self) -> serde_json::Value {
        let json = with_cwd(&self.repo, || {
            run_container_status_command(Some(self.repo.clone()), None, false, true)
        })
        .expect("container status");
        serde_json::from_str(&json).expect("status JSON")
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.state.join(name)).unwrap_or_default()
    }

    fn record(&self) -> PathBuf {
        self.repo.join(".effigy/runtime/fs-events/dev.json")
    }
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write fake program");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod fake program");
}

fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn process_gone(pid: &str) -> bool {
    !std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .expect("kill -0")
        .success()
}

fn terminal_shell() -> RelayInvocation {
    RelayInvocation {
        invocation: Invocation::Shell,
        attached: true,
        interactive_tty: true,
    }
}

#[test]
fn attached_session_relays_saves_reports_status_and_stops_with_its_owner() {
    let fixture = RelayFixture::new("");
    fs::write(fixture.state.join("running"), "").expect("running stack");
    let policy = fixture.policy();

    let headless = RelayInvocation {
        interactive_tty: false,
        ..terminal_shell()
    };
    assert!(start_session(&fixture.repo, &policy, headless).is_none());
    assert_eq!(fixture.read("args.log"), "", "auto probes nothing headless");

    let session = start_session(&fixture.repo, &policy, terminal_shell()).expect("session relay");
    assert!(
        start_session(&fixture.repo, &policy, terminal_shell()).is_none(),
        "one environment has one watcher"
    );
    wait_for("the save to reach the guest stream", || {
        fs::write(fixture.repo.join("saved"), "saved").expect("save");
        std::thread::sleep(Duration::from_millis(300));
        fixture.read("received").contains("/workspace/saved")
    });
    let status = fixture.status();
    assert_eq!(status["schema"], "effigy.container.status.v1");
    assert_eq!(status["fs_events"]["mode"], "auto");
    assert_eq!(status["fs_events"]["state"], "active");
    assert_eq!(status["fs_events"]["reason"], "interactive_shell");
    assert_eq!(status["fs_events"]["owner"], "session");
    assert_eq!(status["fs_events"]["targets"][0]["guest"], "/workspace");

    drop(session);
    assert!(!fixture.record().exists());
    for pid in fixture.read("stream.pids").lines() {
        assert!(process_gone(pid), "guest stream {pid} survived its session");
    }
    let status = fixture.status();
    assert_eq!(status["fs_events"]["state"], "inactive");
    assert_eq!(status["fs_events"]["reason"], "not_running");
}

#[test]
fn foreground_attached_up_stops_its_relay_before_compose_down() {
    let fixture = RelayFixture::new(
        "fs_events = \"on\"\n\n[containers.dev.lifecycle]\non_task_exit = \"stop\"",
    );
    let _stream = EnvGuard::set_many(&[("EFFIGY_CONTAINER_STREAM", Some("1".to_owned()))]);

    fixture.up(true).expect("attached up");

    let log = fixture.read("args.log");
    let stream = log
        .lines()
        .position(|line| line.starts_with("exec -i "))
        .expect("the session relayed");
    let down = log
        .lines()
        .position(|line| line.split_whitespace().any(|arg| arg == "down"))
        .expect("the session stopped its stack");
    assert!(stream < down, "{log}");
    assert!(!fixture.record().exists());
    for pid in fixture.read("stream.pids").lines() {
        assert!(process_gone(pid), "guest stream {pid} survived its session");
    }
    assert_eq!(fixture.read("relay.argv"), "", "no stack-owned relay");
}

#[test]
fn detached_on_up_supervises_a_stack_relay_that_down_and_a_failed_up_stop() {
    let fixture = RelayFixture::new("fs_events = \"on\"");
    let policy = fixture.policy();

    fixture.up(false).expect("detached up");
    wait_for("the relay command", || {
        fixture
            .read("relay.argv")
            .contains("container dev __fs-relay --repo")
    });
    assert!(stack_fs_relay_running(&fixture.repo, &policy));
    assert!(
        start_session(&fixture.repo, &policy, terminal_shell()).is_none(),
        "a session reuses the supervised relay"
    );

    fixture.down();
    assert!(!stack_fs_relay_running(&fixture.repo, &policy));

    fixture.up(false).expect("detached up again");
    assert!(stack_fs_relay_running(&fixture.repo, &policy));
    fs::write(fixture.state.join("fail-up"), "").expect("fail the next apply");
    fixture.up(false).expect_err("compose up fails");
    assert!(!stack_fs_relay_running(&fixture.repo, &policy));
    fs::remove_file(fixture.state.join("fail-up")).expect("restore");
    fixture.down();
}

#[test]
fn detached_up_returns_once_the_stack_relay_is_visible() {
    let fixture = RelayFixture::new("fs_events = \"on\"");

    fixture.up(false).expect("detached up");

    // The relay publishes its record half a second after it starts, so a status
    // read right after `up` would say `not_running` if `up` returned first.
    let status = fixture.status();
    assert_eq!(status["fs_events"]["state"], "active", "{status}");
    fixture.down();
}

#[test]
fn detached_auto_up_starts_no_relay_process() {
    let fixture = RelayFixture::new("");
    let policy = fixture.policy();

    fixture.up(false).expect("detached up");

    assert!(!stack_fs_relay_running(&fixture.repo, &policy));
    assert_eq!(fixture.read("relay.argv"), "");
    assert!(!fixture.repo.join(".effigy/runtime/host-processes").exists());
    assert!(!fixture.record().exists());
    let log = fixture.read("args.log");
    assert!(
        !log.lines().any(|line| line.starts_with("exec -i ")),
        "{log}"
    );
    fixture.down();
}

#[test]
fn up_repairs_detached_named_volume_once_and_preserves_data() {
    for (mounted, reattach, succeeds, expected_recreates) in [
        (false, true, true, 1),
        (false, false, false, 1),
        (true, false, true, 0),
    ] {
        for mode in [0, 1, 2] {
            let fixture = RelayFixture::new("");
            let _skip_handoff = EnvGuard::set_many(&[(
                "EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF",
                Some("1".to_owned()),
            )]);
            let mut manifest =
                fs::read_to_string(fixture.repo.join("effigy.toml")).expect("manifest");
            manifest.push_str("\n[systems]\ndefault = 'dev'\n[systems.dev]\ndefault_workspace = 'app'\n[systems.dev.workspaces.app]\ncontainer = 'dev'\nworking_dir = '/workspace'\nuser = 'dev'\nhome = '/home/dev'\n");
            fs::write(fixture.repo.join("effigy.toml"), manifest).expect("manifest");
            crate::runner::test_support::migrate_declared_compose_files(&fixture.repo);
            fs::write(fixture.state.join("running"), "").expect("running");
            fs::write(
            fixture.state.join("mountinfo"),
            if mounted {
                "1 0 0:1 / /workspace/.effigy/state rw - ext4 /dev/a rw\n2 0 0:2 / /workspace/.effigy/secrets rw - ext4 /dev/b rw\n"
            } else {
                "1 0 0:1 / /workspace/.effigy/state rw - ext4 /dev/a rw\n"
            },
        )
        .expect("mountinfo");
            if reattach {
                fs::write(fixture.state.join("reattach"), "").expect("reattach");
            }
            let host = fixture.repo.join(".effigy/secrets");
            let witness = fixture.state.join("host-witness");
            fs::create_dir_all(&witness).expect("witness directory");
            fs::write(witness.join("witness"), "host data").expect("host data");
            // A missing or replaced host mountpoint must not be inspected or
            // repaired when the container's named submount is detached.
            let host_missing = !mounted && reattach;
            if !host_missing {
                fs::create_dir_all(host.parent().expect("host parent"))
                    .expect("host parent directory");
                std::os::unix::fs::symlink(&witness, &host).expect("host symlink");
            }
            fs::write(fixture.state.join("volume"), "volume data").expect("volume data");
            let result = if mode == 2 {
                let policy = effigy_containers::policy::render::prepare_resolved(&fixture.policy())
                    .expect("prepared policy");
                with_cwd(&fixture.repo, || {
                    crate::runner::container_runtime_prep::prepare_container_exec_runtime(
                        &fixture.repo,
                        &policy,
                        Some("dev"),
                        crate::runner::container_runtime_prep::StartPolicy {
                            retention: Retention::Persistent,
                            holders: &[],
                            reconcile_shared_services: true,
                        },
                    )
                })
                .map(|()| String::new())
            } else {
                fixture.up(mode == 1)
            };
            let log = fixture.read("args.log");
            assert_eq!(result.is_ok(), succeeds, "result: {result:?}\n{log}");
            if let Err(error) = result {
                assert!(
                    error
                        .to_string()
                        .contains("detached from the running container"),
                    "{error}"
                );
            }
            let recreates: Vec<_> = log
                .lines()
                .filter(|line| line.contains("--force-recreate"))
                .collect();
            assert_eq!(recreates.len(), expected_recreates, "{log}");
            for line in recreates {
                assert!(
                    line.contains("--no-deps app") && !line.contains("--volumes"),
                    "{line}"
                );
            }
            assert_eq!(
                fs::read_to_string(witness.join("witness")).expect("host"),
                "host data"
            );
            if host_missing {
                assert!(!host.exists(), "detached host mountpoint was created");
            } else {
                assert!(fs::symlink_metadata(&host)
                    .expect("host symlink")
                    .file_type()
                    .is_symlink());
            }
            assert_eq!(fixture.read("volume"), "volume data");
        }
    }
}

#[test]
fn a_live_holder_prevents_detached_volume_recreation() {
    for mode in [0, 1, 2] {
        let fixture = RelayFixture::new("");
        let _skip_handoff = EnvGuard::set_many(&[(
            "EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF",
            Some("1".to_owned()),
        )]);
        let mut manifest = fs::read_to_string(fixture.repo.join("effigy.toml")).expect("manifest");
        manifest.push_str("\n[systems]\ndefault = 'dev'\n[systems.dev]\ndefault_workspace = 'app'\n[systems.dev.workspaces.app]\ncontainer = 'dev'\nworking_dir = '/workspace'\nuser = 'dev'\nhome = '/home/dev'\n");
        fs::write(fixture.repo.join("effigy.toml"), manifest).expect("manifest");
        crate::runner::test_support::migrate_declared_compose_files(&fixture.repo);
        fs::write(fixture.state.join("running"), "").expect("running");
        fs::write(fixture.state.join("mountinfo"), "1 0 0:1 / /workspace/.effigy/state rw - ext4 /dev/a rw\n2 0 0:2 / /workspace/.effigy/secrets rw - ext4 /dev/b rw\n").expect("healthy mountinfo");
        fixture.up(false).expect("activate environment");
        let policy = fixture.policy();
        let mut held =
            crate::runner::environment::Held::begin(&fixture.repo, &policy, "fixture task")
                .expect("begin holder")
                .expect("host holder");
        assert!(held.attach_active().expect("attach holder"));
        let holder = held.id().expect("holder ID").to_string();
        fs::write(
            fixture.state.join("mountinfo"),
            "1 0 0:1 / /workspace/.effigy/state rw - ext4 /dev/a rw\n",
        )
        .expect("detached mountinfo");
        fs::write(fixture.state.join("args.log"), "").expect("clear log");
        let result = if mode == 2 {
            let policy = effigy_containers::policy::render::prepare_resolved(&policy)
                .expect("prepared policy");
            with_cwd(&fixture.repo, || {
                crate::runner::container_runtime_prep::prepare_container_exec_runtime(
                    &fixture.repo,
                    &policy,
                    Some("dev"),
                    crate::runner::container_runtime_prep::StartPolicy {
                        retention: Retention::Persistent,
                        holders: &[],
                        reconcile_shared_services: true,
                    },
                )
            })
            .map(|()| String::new())
        } else {
            fixture.up(mode == 1)
        };
        let error = result
            .expect_err("held detached volume must refuse")
            .to_string();
        assert!(
            error.contains(&holder)
                && error.contains("after active holders release")
                && error.contains("detached"),
            "{error}"
        );
        let log = fixture.read("args.log");
        assert!(
            !log.lines().any(|line| line.contains("--force-recreate")
                || line
                    .split_whitespace()
                    .any(|arg| arg == "down" || arg == "rm")),
            "{log}"
        );
        assert!(held.holds(), "the existing task remains attached");
        drop(held);
    }
}
