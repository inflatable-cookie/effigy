//! Barge leases through the container lifecycle, against a fake runtime and a
//! temporary Barge account.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use barge::container::{ContainerClass, ContainerEndReason, ContainerLeaseStatus};
use barge::{Barge, FakeOs, ManualClock};
use effigy_containers::environment::Retention;
use effigy_containers::EffectiveContainerPolicy;

use super::down::run_container_down_command;
use super::up::run_container_up;
use crate::contract_test_support::{
    clean_fixture_environment, lock_test, with_cwd, EnvGuard, ReentrantTestLockGuard,
};
use crate::runner::container_command::barge_lifecycle;
use crate::runner::error::RunnerError;

const FAKE_DOCKER: &str = r#"#!/bin/sh
state="@STATE@"
printf '%s\n' "$*" >> "$state/args.log"
subcmd=""
for arg in "$@"; do
  case "$arg" in
    up|down|ps|inspect|logs) subcmd="$arg"; break ;;
  esac
done
case "$subcmd" in
  "") case "$*" in *version*) printf 'Docker Compose version v2.30.0\n' ;; esac ;;
  up)
    previous=""
    : > "$state/up-compose"
    for arg in "$@"; do
      if [ "$previous" = -f ]; then cat "$arg" >> "$state/up-compose"; fi
      previous="$arg"
    done
    if [ -f "$state/fail-up" ]; then echo 'no such image' >&2; exit 1; fi
    : > "$state/running"
    ;;
  ps)
    if [ -f "$state/running" ]; then
      printf 'barge-fixture-app-1\tUp 1 second\t\tbarge-fixture\t@REPO@\tapp\tFalse\n'
    fi
    ;;
  inspect)
    id=$(printf '%064d' 0 | tr 0 a)
    printf '[{"Id":"%s","Name":"/barge-fixture-app-1","State":{"Status":"running","Running":true,"ExitCode":0},"Config":{"Labels":{"com.docker.compose.project":"barge-fixture","com.docker.compose.service":"app","com.docker.compose.oneoff":"False","com.docker.compose.project.working_dir":"@REPO@","com.effigy.checkout":"@REPO@"}},"Mounts":[{"Type":"bind","Source":"@REPO@","Destination":"/workspace","RW":true}],"NetworkSettings":{"Ports":{}}}]\n' "$id"
    ;;
  down) rm -f "$state/running" ;;
esac
exit 0
"#;

struct LeaseFixture {
    _temp: effigy_test_support::Fixture,
    repo: PathBuf,
    state: PathBuf,
    barge: PathBuf,
    _env: EnvGuard,
    _ambient: EnvGuard,
    _lock: ReentrantTestLockGuard,
}

impl LeaseFixture {
    /// `profiles` is the Barge account's `container-profiles.json`.
    fn new(profiles: &str, lifecycle: &str) -> Self {
        let lock = lock_test();
        let ambient = clean_fixture_environment();
        let temp = effigy_test_support::Fixture::workspace("effigy-barge-leases-")
            .expect("private fixture root");
        let root = temp.path().canonicalize().expect("canonical root");
        let repo = root.join("checkout");
        let state = root.join("runtime");
        let bin = root.join("bin");
        let home = root.join("home");
        let barge = root.join("barge");
        for dir in [&repo, &state, &bin, &home, &barge] {
            fs::create_dir_all(dir).expect("fixture directory");
        }
        fs::write(barge.join("container-profiles.json"), profiles).expect("profiles");
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
                "[containers]\ndefault = \"dev\"\n\n[containers.dev]\ncompose_file = \"docker-compose.yml\"\nprimary_service = \"app\"\nproject_name = \"barge-fixture\"\nworking_dir = \"/workspace\"\n\n[containers.dev.host]\nmounts = [\"./:/workspace\"]\n{lifecycle}\n"
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
        let env = EnvGuard::set_many(&[
            ("HOME", Some(home.display().to_string())),
            ("PATH", Some(format!("{}:/usr/bin:/bin", bin.display()))),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
            ("BARGE_STATE_DIR", Some(barge.display().to_string())),
            ("CI", None),
            ("EFFIGY_CONTAINER_LEASE_WAIT_SECS", Some("0".to_owned())),
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(home.display().to_string()),
            ),
        ]);
        Self {
            _temp: temp,
            repo,
            state,
            barge,
            _env: env,
            _ambient: ambient,
            _lock: lock,
        }
    }

    fn managed(lifecycle: &str) -> Self {
        Self::new(r#"{"profiles":[{"name":"effigy","max":1}]}"#, lifecycle)
    }

    fn policy(&self) -> EffectiveContainerPolicy {
        effigy_containers::load_container_policy(&self.repo, None).expect("fixture policy")
    }

    fn up(&self, attach: bool, retention: Retention) -> Result<String, RunnerError> {
        with_cwd(&self.repo, || {
            run_container_up(&self.repo, None, attach, !attach, false, retention, &[])
        })
    }

    fn down(&self) {
        with_cwd(&self.repo, || {
            run_container_down_command(Some(self.repo.clone()), None, false, false)
        })
        .expect("container down");
    }

    fn leases(&self) -> Vec<ContainerLeaseStatus> {
        Barge::open_with(
            &self.barge,
            std::sync::Arc::new(FakeOs::new()),
            std::sync::Arc::new(ManualClock::new(1_700_000_000_000)),
        )
        .expect("temporary Barge account")
        .container_status()
        .expect("lease status")
    }

    fn only_lease(&self) -> ContainerLeaseStatus {
        let mut leases = self.leases();
        assert_eq!(leases.len(), 1, "{leases:?}");
        leases.remove(0)
    }

    /// Whether the lease record expects its holder to keep it.
    fn recorded_hold(&self, id: &str) -> bool {
        let record =
            fs::read(self.barge.join(format!("container-leases/{id}.json"))).expect("lease record");
        serde_json::from_slice::<serde_json::Value>(&record).expect("lease JSON")["hold"]
            .as_bool()
            .expect("hold flag")
    }

    fn applied_compose(&self) -> String {
        fs::read_to_string(self.state.join("up-compose")).expect("applied compose")
    }

    fn fail_up(&self) {
        fs::write(self.state.join("fail-up"), "").expect("fail the next apply");
    }
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write fake program");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod fake program");
}

#[test]
fn detached_up_takes_an_unheld_worker_lease_labels_it_and_down_returns_it() {
    let fixture = LeaseFixture::managed("");

    fixture.up(false, Retention::Persistent).expect("up");

    let lease = fixture.only_lease();
    assert_eq!(lease.profile, "effigy");
    assert_eq!(lease.project, "barge-fixture");
    assert_eq!(lease.class, ContainerClass::Worker);
    assert_eq!(lease.ended, None);
    assert!(!fixture.recorded_hold(&lease.id));
    let label = format!("barge.lease: {}", lease.id);
    assert!(
        fixture.applied_compose().contains(&label),
        "{}",
        fixture.applied_compose()
    );

    fixture.up(false, Retention::Persistent).expect("repeat up");
    assert_eq!(fixture.only_lease().id, lease.id, "a repeat keeps its ID");
    // A task's ensure never demotes the persistent stack to a held lease.
    fixture.up(false, Retention::Held).expect("task ensure");
    assert!(!fixture.recorded_hold(&lease.id));

    fixture.down();
    assert_eq!(fixture.leases(), Vec::new());
}

#[test]
fn failed_up_returns_its_new_lease_and_keeps_a_running_stacks_lease() {
    let fixture = LeaseFixture::managed("");
    fixture.fail_up();
    fixture
        .up(false, Retention::Persistent)
        .expect_err("compose up fails");
    assert!(fixture.applied_compose().contains("barge.lease: "));
    assert_eq!(fixture.leases(), Vec::new());

    fs::remove_file(fixture.state.join("fail-up")).expect("restore");
    fixture.up(false, Retention::Persistent).expect("up");
    let lease = fixture.only_lease();
    fixture.fail_up();
    fixture
        .up(false, Retention::Persistent)
        .expect_err("repeat up fails");
    assert_eq!(fixture.only_lease().id, lease.id);
    fixture.down();
}

#[test]
fn a_full_pool_refuses_before_compose_runs() {
    let fixture = LeaseFixture::new(r#"{"profiles":[{"name":"effigy","max":0}]}"#, "");

    let error = fixture
        .up(false, Retention::Persistent)
        .expect_err("no slot");

    assert!(error.to_string().contains("is full"), "{error}");
    assert!(!fixture.state.join("up-compose").exists());
    assert_eq!(fixture.leases(), Vec::new());
}

#[test]
fn held_stack_keeps_its_lease_through_idle_linger_and_the_reaper_returns_it() {
    let fixture = LeaseFixture::managed("");
    // A CI checkout's stacks are workers, whose released hold ends the lease.
    let _ci = EnvGuard::set_many(&[("CI", Some("true".to_owned()))]);
    fixture.up(false, Retention::Held).expect("task ensure");
    let lease = fixture.only_lease();
    assert_eq!(lease.class, ContainerClass::Worker);
    assert!(fixture.recorded_hold(&lease.id));
    assert_eq!(lease.ended, None, "this process holds it");
    let policy = fixture.policy();

    // A crashed holder leaves the lease for a sweep; the next session of the
    // same checkout takes it back without rebuilding the stack.
    barge_lifecycle::release_hold(&policy);
    assert_eq!(
        fixture.only_lease().ended.map(|ending| ending.reason),
        Some(ContainerEndReason::HoldReleased)
    );
    barge_lifecycle::hold_for_session(&policy);
    let lease = fixture.only_lease();
    assert_eq!(lease.ended, None);
    assert!(
        fixture.state.join("running").exists(),
        "nothing was torn down"
    );

    let _reaper = EnvGuard::set_many(&[
        (
            "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER",
            Some("1".to_owned()),
        ),
        ("EFFIGY_CONTAINER_IDLE_TIMEOUT_SECS", Some("1".to_owned())),
    ]);
    crate::runner::environment::release(&fixture.repo, &policy);
    assert_eq!(fixture.leases().len(), 1, "the stack is lingering");
    let token = crate::runner::environment::record(&policy)
        .and_then(|record| record.engine().and_then(|e| e.idle_reap_token.clone()))
        .expect("armed idle reap");
    crate::runner::environment::run_idle_reaper(effigy_cli::InternalContainerLeaseReaperArgs {
        repo_root: fixture.repo.clone(),
        container_name: "dev".to_owned(),
        token,
    })
    .expect("idle reap");

    assert!(!fixture.state.join("running").exists());
    assert_eq!(fixture.leases(), Vec::new());
}

#[test]
fn attached_session_returns_its_lease_on_stop_and_keeps_it_for_a_stack_left_running() {
    let _stream = EnvGuard::set_many(&[("EFFIGY_CONTAINER_STREAM", Some("1".to_owned()))]);
    {
        let fixture =
            LeaseFixture::managed("\n[containers.dev.lifecycle]\non_task_exit = \"stop\"");
        fixture
            .up(true, Retention::Persistent)
            .expect("attached up");
        assert!(fixture.applied_compose().contains("barge.lease: "));
        assert_eq!(fixture.leases(), Vec::new());
    }
    let fixture =
        LeaseFixture::managed("\n[containers.dev.lifecycle]\non_task_exit = \"leave-running\"");
    fixture
        .up(true, Retention::Persistent)
        .expect("attached up");
    let lease = fixture.only_lease();
    assert!(!fixture.recorded_hold(&lease.id));
    assert_eq!(lease.ended, None);
    fixture.down();
    assert_eq!(fixture.leases(), Vec::new());
}

/// A long-lived process that held a lease another teardown returned reserves
/// the stack again instead of reusing the returned ID.
#[test]
fn a_returned_lease_is_not_reused_by_its_former_holder() {
    let fixture = LeaseFixture::managed("");
    fixture.up(false, Retention::Held).expect("task ensure");
    let first = fixture.only_lease();
    Barge::open_with(
        &fixture.barge,
        std::sync::Arc::new(FakeOs::new()),
        std::sync::Arc::new(ManualClock::new(1_700_000_000_000)),
    )
    .expect("temporary Barge account")
    .container_return(&first.id)
    .expect("return from elsewhere");

    fixture.up(false, Retention::Held).expect("next ensure");

    let second = fixture.only_lease();
    assert_ne!(second.id, first.id);
    assert!(fixture
        .applied_compose()
        .contains(&format!("barge.lease: {}", second.id)));
    fixture.down();
}

#[test]
fn undeclared_profile_passes_through_without_a_lease_or_label() {
    let fixture = LeaseFixture::new(r#"{"profiles":[{"name":"other","max":1}]}"#, "");

    fixture.up(false, Retention::Persistent).expect("up");

    assert!(!fixture.applied_compose().contains("barge.lease"));
    assert_eq!(fixture.leases(), Vec::new());
    fixture.down();
}
