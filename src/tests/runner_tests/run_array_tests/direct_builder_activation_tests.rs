//! Private production-adapter proof for a no-port direct-Compose builder.
//!
//! The manifest declares `compose_file` with no published ports. A Rhai task
//! runs `container::up` and then `container::shell`, so the real policy load,
//! admission guard, detached preparation, owned-binding verification and exec
//! all run. Only the Docker/Colima binaries are fake. The fake `up` applies the
//! labels from the rendered ownership override, as Compose would, and `ps` and
//! `inspect` serve those labels back. Each scenario changes only how the labels
//! were applied, so every refusal comes from the production check.

use super::rhai_script_tests::{install_fake_container_runtime, write_executable};
use crate::contract_test_support::lock_test;
use crate::runner::tests::prelude::execution::run_manifest_task_with_cwd;
use crate::runner::tests::prelude::{fs, temp_workspace, write_manifest};
use effigy_cli::TaskInvocation;

/// Fake `docker`. The label state lives in `$EFFIGY_TEST_BUILDER_STATE_DIR`.
/// `up` records the labels Compose would apply, `ps` and `inspect` report them.
pub(super) const FAKE_DOCKER: &str = r#"#!/bin/sh
set -eu
svc="${EFFIGY_TEST_FAKE_SERVICE:-builder}"
proj="${EFFIGY_TEST_FAKE_PROJECT:-fixture-builder-dev}"
cname="$proj-$svc-1"
printf "%s\n" "$*" >> "$EFFIGY_TEST_DOCKER_ARGS_FILE"
state="$EFFIGY_TEST_BUILDER_STATE_DIR"
subcmd=""
for arg in "$@"; do
  case "$arg" in
    up|down|ps|logs|exec|kill|inspect)
      subcmd="$arg"
      break
      ;;
  esac
done
case "$subcmd" in
  up)
    ovr=""
    for arg in "$@"; do
      case "$arg" in
        *ownership.compose.yml) ovr="$arg" ;;
      esac
    done
    mode="${EFFIGY_TEST_BUILDER_MODE:-applied}"
    : > "$state/labels"
    if [ "$mode" != "unlabelled" ] && [ -n "$ovr" ]; then
      checkout=$(sed -n 's/^ *com.effigy.checkout: //p' "$ovr" | head -1)
      scope=$(sed -n 's/^ *com.effigy.scope: //p' "$ovr" | head -1)
      if [ "$mode" = "wrong_checkout" ]; then
        checkout="/elsewhere/checkout"
      fi
      if [ "$mode" = "foreign_scope" ]; then
        scope="foreign-scope-token"
      fi
      printf "checkout=%s\n" "$checkout" >> "$state/labels"
      printf "scope=%s\n" "$scope" >> "$state/labels"
    fi
    : > "$state/running"
    printf "compose-up\n"
    ;;
  ps)
    if [ -f "$state/running" ]; then
      printf "%s\tUp 1 second\t\t%s\t\t%s\tFalse\n" "$cname" "$proj" "$svc"
    fi
    ;;
  inspect)
    count=0
    if [ -f "$state/inspects" ]; then
      count=$(cat "$state/inspects")
    fi
    count=$((count + 1))
    printf "%s\n" "$count" > "$state/inspects"
    id_char="a"
    if [ "${EFFIGY_TEST_BUILDER_MODE:-applied}" = "changing" ] && [ "$count" -gt 1 ]; then
      id_char="b"
    fi
    id=$(printf "%064d" 0 | tr 0 "$id_char")
    checkout=""
    scope=""
    if [ -f "$state/labels" ]; then
      checkout=$(sed -n 's/^checkout=//p' "$state/labels")
      scope=$(sed -n 's/^scope=//p' "$state/labels")
    fi
    extra=""
    if [ -n "$checkout" ]; then
      extra="$extra,\"com.effigy.checkout\":\"$checkout\""
    fi
    if [ -n "$scope" ]; then
      extra="$extra,\"com.effigy.scope\":\"$scope\""
    fi
    printf '[{"Id":"%s","Name":"/%s","State":{"Status":"running","Running":true},"Config":{"Labels":{"com.docker.compose.project":"%s","com.docker.compose.service":"%s","com.docker.compose.oneoff":"False"%s}},"Mounts":[],"NetworkSettings":{"Ports":{}}}]\n' "$id" "$cname" "$proj" "$svc" "$extra"
    ;;
  exec)
    printf "exec-ok\n"
    ;;
  down)
    rm -f "$state/running"
    printf "compose-down\n"
    ;;
  *)
    printf "unexpected docker invocation: %s\n" "$*" >&2
    exit 1
    ;;
esac
"#;

const BUILDER_SCRIPT: &str = r#"
let up = container::up("builder", true);
if up.trim() != "" {
    fs::write_file("up.txt", up);
}
let shell = container::shell("builder", "printf builder-shell");
fs::write_file("shell.txt", shell);
"#;

struct BuilderOutcome {
    result: Result<(), String>,
    docker_args: String,
    root: std::path::PathBuf,
}

fn run_direct_builder_activation(label: &str, mode: &str) -> BuilderOutcome {
    let root = temp_workspace(&format!("direct-builder-activation-{label}"));
    fs::create_dir_all(root.join("infra/release")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/release/docker-compose.yml"),
        "services:\n  builder:\n    image: example\n    working_dir: /workspace\n",
    )
    .expect("write compose");
    fs::create_dir_all(root.join("scripts")).expect("mkdir script dir");
    fs::write(root.join("scripts/builder.rhai"), BUILDER_SCRIPT).expect("write rhai script");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [{ rhai = "scripts/builder.rhai" }]

[containers]
default = "builder"

[containers.builder]
driver = "colima"
startup = "detached"
profile = "dev"
compose_file = "infra/release/docker-compose.yml"
project_name = "fixture-builder-dev"
primary_service = "builder"

[containers.builder.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"
detach_timeout_secs = 1

[containers.builder.host]
mounts = ["./:/workspace"]
"#,
    );
    let state_dir = root.join("state");
    fs::create_dir_all(&state_dir).expect("mkdir state");
    let bin_dir = install_fake_container_runtime(&root);
    write_executable(&bin_dir.join("docker"), FAKE_DOCKER);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let colima_state = root.join("colima-running");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let _env = crate::contract_test_support::EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        (
            "EFFIGY_TEST_DOCKER_ARGS_FILE",
            Some(docker_args.display().to_string()),
        ),
        (
            "EFFIGY_TEST_COLIMA_ARGS_FILE",
            Some(colima_args.display().to_string()),
        ),
        (
            "EFFIGY_TEST_COLIMA_STATE_FILE",
            Some(colima_state.display().to_string()),
        ),
        (
            "EFFIGY_TEST_LOG_FOLLOW_FILE",
            Some(log_follow.display().to_string()),
        ),
        (
            "EFFIGY_TEST_BUILDER_STATE_DIR",
            Some(state_dir.display().to_string()),
        ),
        ("EFFIGY_TEST_BUILDER_MODE", Some(mode.to_owned())),
        (
            "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
            Some(root.join("lease-home").display().to_string()),
        ),
    ]);

    let result = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .map(|_| ())
    .map_err(|error| error.to_string());
    let docker_args = fs::read_to_string(&docker_args).unwrap_or_default();
    BuilderOutcome {
        result,
        docker_args,
        root,
    }
}

#[test]
fn direct_builder_without_ports_activates_and_execs_with_current_checkout_labels() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("applied", "applied");

    outcome
        .result
        .expect("current direct builder must activate and exec without a published route");
    assert!(
        outcome.docker_args.contains("ownership.compose.yml"),
        "the ownership override must reach Compose: {}",
        outcome.docker_args
    );
    assert!(
        outcome.docker_args.contains("exec -T"),
        "the owned builder must receive exec: {}",
        outcome.docker_args
    );
    let shell = fs::read_to_string(outcome.root.join("shell.txt")).expect("read shell output");
    assert!(
        shell.contains("finished container shell for `builder`"),
        "the shell must finish through the owned builder: {shell}"
    );
}

#[test]
fn direct_builder_refuses_exec_when_checkout_label_names_another_checkout() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("wrong-checkout", "wrong_checkout");

    let error = outcome
        .result
        .expect_err("wrong checkout label must refuse before exec");
    assert!(is_owned_binding_refusal(&error), "{error}");
    assert!(
        !outcome.docker_args.contains("exec -T") && !outcome.docker_args.contains(" restart"),
        "no exec may run for a foreign checkout: {}",
        outcome.docker_args
    );
}

#[test]
fn direct_builder_refuses_exec_for_unlabelled_preexisting_container() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("unlabelled", "unlabelled");

    let error = outcome
        .result
        .expect_err("an unlabelled preexisting builder must not be adopted");
    assert!(is_owned_binding_refusal(&error), "{error}");
    assert!(
        !outcome.docker_args.contains("exec -T") && !outcome.docker_args.contains(" restart"),
        "no exec may run for an unlabelled container: {}",
        outcome.docker_args
    );
}

#[test]
fn direct_builder_refuses_exec_for_foreign_scope_label() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("foreign-scope", "foreign_scope");

    let error = outcome
        .result
        .expect_err("a foreign scope label must refuse before exec");
    assert!(is_owned_binding_refusal(&error), "{error}");
    assert!(
        !outcome.docker_args.contains("exec -T") && !outcome.docker_args.contains(" restart"),
        "no exec may run for a foreign scope: {}",
        outcome.docker_args
    );
}

/// Readiness retries the owned-binding check until its deadline, so a refused
/// binding surfaces as a readiness failure that names the inspect step.
fn is_owned_binding_refusal(error: &str) -> bool {
    error.contains("inspect owned binding")
        || error.contains("binding unavailable")
        || error.contains("ownership")
        || error.contains("not exec-ready")
        || error.contains("not verified as owned")
}
