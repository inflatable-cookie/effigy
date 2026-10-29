//! Private production-adapter proof for a no-port direct-Compose builder.
//!
//! The manifest declares a retired `compose_file` (translated to a project
//! catalog fragment) with no published ports. A Rhai task
//! runs `container::up` and then `container::shell`, so the real policy load,
//! admission guard, detached preparation, owned-binding verification and exec
//! all run. Only the Docker/Colima binaries are fake. The fake `up` applies the
//! labels from the generated render, as Compose would, and `ps` and
//! `inspect` serve those labels back. Each scenario changes only how the labels
//! were applied, so every refusal comes from the production check.

use super::rhai_script_tests::{install_fake_container_runtime, write_executable};
use crate::contract_test_support::lock_test;
#[cfg(test)]
use crate::route_table_fixture::RouteTableFixture;
use crate::runner::tests::prelude::execution::run_manifest_task_with_cwd;
use crate::runner::tests::prelude::{fs, write_manifest};
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
mode="${EFFIGY_TEST_BUILDER_MODE:-applied}"
subcmd=""
for arg in "$@"; do
  case "$arg" in
    up|down|ps|logs|exec|kill|inspect|info)
      subcmd="$arg"
      break
      ;;
  esac
done
case "$subcmd" in
  info) exit 0 ;;
  up)
    ovr=""
    for arg in "$@"; do
      case "$arg" in
        *generated.compose.yml) ovr="$arg" ;;
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
      if [ "$mode" = "wrong_project" ]; then printf foreign-project > "$state/project"; fi
      printf "checkout=%s\n" "$checkout" >> "$state/labels"
      printf "scope=%s\n" "$scope" >> "$state/labels"
    fi
    : > "$state/running"
    printf "compose-up\n"
    ;;
  ps)
    if [ -f "$state/delay-binding" ]; then sleep 1.2; fi
    if [ -f "$state/running" ]; then
      printf "%s\tUp 1 second\t\t%s\t\t%s\tFalse\n" "$cname" "$proj" "$svc"
    fi
    if [ -f "$state/delay-binding" ]; then
      for sibling in dbgate mailpit silo; do
        printf "%s-%s-1\tUp 1 second\t\t%s\t\t%s\tFalse\n" "$proj" "$sibling" "$proj" "$sibling"
      done
    fi
    if [ -f "$state/sibling.json" ]; then
      printf "fixture-sibling-dev-sibling-1\tUp 1 second\t\tfixture-sibling-dev\t\tsibling\tFalse\n"
    fi
    ;;
  inspect)
    if [ -f "$state/delay-binding" ]; then
      sleep 0.9
      printf '%s\n' "$*" >> "$state/delayed-inspects"
    fi
    for arg in "$@"; do
      if [ "$arg" = "fixture-sibling-dev-sibling-1" ] && [ -f "$state/sibling.json" ]; then cat "$state/sibling.json"; exit 0; fi
    done
    count=0
    if [ -f "$state/inspects" ]; then
      count=$(cat "$state/inspects")
    fi
    count=$((count + 1))
    printf "%s\n" "$count" > "$state/inspects"
    id_char="a"
    if [ "$mode" = "changing" ]; then
      # The container is replaced between any two verifications: every
      # inspection reports a different full ID, so a binding taken from one
      # inspection never matches the next verification, however many
      # inspections the activation makes before it.
      id_char=$(printf "\\$(printf '%03o' $((97 + (count - 1) % 26)))")
    elif [ -f "$state/replaced" ]; then
      id_char="b"
    fi
    id=$(printf "%064d" 0 | tr 0 "$id_char")
    checkout=""
    scope=""
    if [ -f "$state/labels" ]; then
      checkout=$(sed -n 's/^checkout=//p' "$state/labels")
      scope=$(sed -n 's/^scope=//p' "$state/labels")
    fi
    if [ "$mode" = changing_scope ] && [ "$count" -gt 1 ]; then scope=foreign-scope-token; fi
    if [ "$mode" = changing_checkout ] && [ "$count" -gt 1 ]; then checkout=/elsewhere/checkout; fi
    if [ "$mode" = delayed_foreign ] && [ -f "$state/delay-binding" ]; then checkout=/elsewhere/checkout; fi
    if [ -f "$state/project" ]; then proj=$(cat "$state/project"); fi
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
    printf '%s\0' "$@" > "$state/native-argv"
    # Only native full-ID execution is admitted. Compose service/name execution
    # is the original counterexample and deliberately fails this fake backend.
    while [ "$1" != exec ]; do shift; done
    shift
    while [ $# -gt 0 ]; do
      case "$1" in
        -i|-t) shift ;;
        -w|-u|-e) shift 2 ;;
        *) target="$1"; shift; break ;;
      esac
    done
    if [ "$mode" = literal ] && [ "$1" = printf ]; then cat > "$state/stdin"; fi
    expected=$(printf "%064d" 0 | tr 0 a)
    if [ -f "$state/replaced" ]; then expected=$(printf "%064d" 0 | tr 0 b); fi
    if [ "$mode" = "replace_at_exec" ] && [ "$1" != true ]; then
      : > "$state/replaced"
      expected=$(printf "%064d" 0 | tr 0 b)
    fi
    if [ "$target" != "$expected" ]; then exit 1; fi
    if [ "$1" = true ]; then
      printf '%s\n' "$target" >> "$state/readiness"
      if [ "$mode" = "replace_after_readiness" ] || [ -f "$state/arm-replacement" ]; then : > "$state/replaced"; fi
    else
      printf '%s\n' "$target" "$@" >> "$state/effects"
    fi
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
    _temp: effigy_test_support::Fixture,
    result: Result<(), String>,
    docker_args: String,
    root: std::path::PathBuf,
}

fn run_direct_builder_activation(label: &str, mode: &str) -> BuilderOutcome {
    let temp = effigy_test_support::Fixture::workspace(&format!("effigy-builder-{label}-"))
        .expect("fresh private root");
    let root = fs::canonicalize(temp.path()).expect("canonical private root");
    fs::create_dir_all(root.join("infra/release")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/release/docker-compose.yml"),
        "services:\n  builder:\n    image: example\n    working_dir: /workspace\n",
    )
    .expect("write compose");
    fs::create_dir_all(root.join("scripts")).expect("mkdir script dir");
    let script = if mode == "literal" {
        r#"container::up("builder", true);
let result = container::exec("builder", ["printf", "%s", "space and $literal; untouched"], #{"stdin_file": path::join(repo_root, "input.txt"), "cwd": repo_root, "env": #{"PAYLOAD": "a b;$raw"}});
if !result["success"] { throw("literal exec failed"); }
"#.to_owned()
    } else if mode.starts_with("delayed_") {
        BUILDER_SCRIPT.replace(
            "let shell =",
            "fs::write_file(\"state/delay-binding\", \"armed\");\nlet shell =",
        )
    } else if mode == "replace_before_exec" {
        BUILDER_SCRIPT.replace(
            "let shell =",
            "fs::write_file(\"state/arm-replacement\", \"armed\");\nlet shell =",
        )
    } else {
        BUILDER_SCRIPT.to_owned()
    };
    fs::write(root.join("scripts/builder.rhai"), script).expect("write rhai script");
    fs::write(root.join("input.txt"), "owned stdin bytes").unwrap();
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
    if mode.starts_with("delayed_") {
        let compose = root.join("infra/release/docker-compose.yml");
        let mut content = fs::read_to_string(&compose).unwrap();
        for sibling in ["dbgate", "mailpit", "silo"] {
            content.push_str(&format!("  {sibling}:\n    image: example\n"));
        }
        fs::write(compose, content).unwrap();
    }
    if mode == "generated" {
        let catalog = root.join("infra/dev/catalog/private-builder");
        fs::create_dir_all(&catalog).unwrap();
        fs::write(
            catalog.join("service.toml"),
            r#"[service]
name = "private-builder"
description = "Private no-port builder fixture"
[capabilities]
exec_target = true
shell = "/bin/sh"
"#,
        )
        .unwrap();
        fs::write(catalog.join("compose.fragment.yml"), "services:\n  {{ service_name }}:\n    image: fixture:private\n    working_dir: /workspace\n    volumes:\n      - {{ repo_root }}:/workspace\n").unwrap();
        let manifest = fs::read_to_string(root.join("effigy.toml"))
            .unwrap()
            .replace("compose_file = \"infra/release/docker-compose.yml\"\n", "")
            .replace(
                "primary_service = \"builder\"",
                "primary_service = \"builder\"\nworking_dir = \"/workspace\"",
            );
        fs::write(root.join("effigy.toml"), format!("{manifest}\n[containers.builder.services.builder]\ncatalog = \"private-builder\"\n")).unwrap();
    }
    let _gateway =
        crate::runner::gateway_command::set_test_gateway_home(&root.join("gateway-home"));
    let state_dir = root.join("state");
    fs::create_dir_all(&state_dir).expect("mkdir state");
    let sibling = serde_json::json!([{
        "Id": "c".repeat(64), "Name": "/fixture-sibling-dev-sibling-1",
        "State": {"Status": "running", "Running": true},
        "Config": {"Labels": {"com.docker.compose.project": "fixture-sibling-dev",
            "com.docker.compose.service": "sibling", "com.docker.compose.oneoff": "False", "com.effigy.checkout": root}},
        "Mounts": [{"Type": "volume", "Name": "fixture-sibling-cache", "Source": root.join("sibling-state"), "Destination": "/cache"}],
        "NetworkSettings": {"Ports": {}}
    }]).to_string();
    let sibling_path = state_dir.join("sibling.json");
    fs::write(&sibling_path, &sibling).unwrap();
    let sibling_root = root.join("sibling-state");
    fs::create_dir_all(&sibling_root).unwrap();
    let sibling_data = sibling_root.join("volume-data");
    let sibling_lease =
        root.join("lease-home/runtime/host-container-leases/dev/fixture-sibling-dev/sibling.json");
    fs::create_dir_all(sibling_lease.parent().unwrap()).unwrap();
    fs::write(&sibling_data, "retained volume bytes").unwrap();
    let sibling_lease_bytes = serde_json::json!({
        "schema": "effigy.host-container-lease.v1", "schema_version": 1,
        "token": "retained-sibling-generation", "container_name": "sibling",
        "profile": "dev", "project_name": "fixture-sibling-dev", "expires_at_epoch_ms": u64::MAX,
    })
    .to_string();
    fs::write(&sibling_lease, &sibling_lease_bytes).unwrap();
    let routes_path = crate::runner::gateway_command::gateway_dir()
        .unwrap()
        .join("routes.json");
    let mut routes = effigy_gateway::routes::RouteTable::new();
    routes.upsert(effigy_gateway::routes::Route {
        domain: "sibling.fixture.test".to_owned(),
        target: Some("127.0.0.1:34567".to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: effigy_gateway::routes::RouteSource::Container,
        project: sibling_root.display().to_string(),
        tls: false,
        managed_listener: None,
        registered: chrono::Utc::now(),
    });
    fs::create_dir_all(routes_path.parent().unwrap()).unwrap();
    routes.save(&routes_path).unwrap();
    let routes_before = effigy_gateway::routes::RouteTable::load(&routes_path)
        .unwrap()
        .routes;
    let bin_dir = install_fake_container_runtime(&root);
    write_executable(
        &bin_dir.join("colima"),
        r#"#!/bin/sh
set -eu
case "$1" in
  status) printf 'INFO[0000] status: Running\n' ;;
  start) exit 0 ;;
  ssh) exit 0 ;;
  nerdctl)
    while [ "$1" != -- ]; do shift; done
    shift
    exec "$(dirname "$0")/docker" "$@"
    ;;
  *) exit 1 ;;
esac
"#,
    );
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
    fs::create_dir_all(root.join("home")).expect("private subprocess HOME");
    let _env = crate::contract_test_support::EnvGuard::set_many(&[
        ("HOME", Some(root.join("home").display().to_string())),
        (
            "EFFIGY_EXECUTABLE",
            Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("target/debug/effigy")
                    .display()
                    .to_string(),
            ),
        ),
        ("PATH", Some(path)),
        (
            "EFFIGY_COMPOSE_BACKEND",
            Some(
                if mode == "nerdctl" || mode.starts_with("delayed_") {
                    "colima"
                } else {
                    "docker"
                }
                .to_owned(),
            ),
        ),
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

    let result = if mode == "expired_binding" {
        let policy = effigy_containers::load_container_policy(&root, Some("builder"))
            .expect("private declared policy");
        crate::runner::container_runtime_prep::verify_service_binding(
            &root,
            &policy,
            "builder",
            Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    } else {
        run_manifest_task_with_cwd(
            &TaskInvocation {
                name: "validate".to_owned(),
                args: Vec::new(),
            },
            root.clone(),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    };
    assert_eq!(
        fs::read_to_string(&sibling_path).unwrap(),
        sibling,
        "sibling ID and running state changed"
    );
    assert_eq!(
        fs::read_to_string(&sibling_data).unwrap(),
        "retained volume bytes"
    );
    assert_eq!(
        fs::read_to_string(&sibling_lease).unwrap(),
        sibling_lease_bytes
    );
    assert_eq!(
        effigy_gateway::routes::RouteTable::load(&routes_path)
            .unwrap()
            .routes,
        routes_before,
        "sibling route changed"
    );
    let docker_args = fs::read_to_string(&docker_args).unwrap_or_default();
    let result = result.map_err(|error| format!("{error}\nprivate runtime trace:\n{docker_args}"));
    BuilderOutcome {
        _temp: temp,
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
    // A translated `compose_file` carries ownership labels in its one
    // generated document; no separate override exists.
    assert!(
        outcome
            .docker_args
            .contains("builder.generated.compose.yml")
            && !outcome.docker_args.contains("ownership.compose.yml"),
        "the generated render must reach Compose: {}",
        outcome.docker_args
    );
    assert!(
        outcome
            .docker_args
            .lines()
            .any(|line| line.starts_with("exec ")),
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
        !outcome
            .docker_args
            .lines()
            .any(|line| line.starts_with("exec "))
            && !outcome.docker_args.contains(" restart"),
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
        !outcome
            .docker_args
            .lines()
            .any(|line| line.starts_with("exec "))
            && !outcome.docker_args.contains(" restart"),
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
        !outcome
            .docker_args
            .lines()
            .any(|line| line.starts_with("exec "))
            && !outcome.docker_args.contains(" restart"),
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

#[test]
fn direct_builder_full_id_replacement_refuses_before_probe_or_restart() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("changing", "changing");
    let error = outcome
        .result
        .expect_err("a changed full ID must not be adopted");
    assert!(is_owned_binding_refusal(&error), "{error}");
    assert!(
        !outcome
            .docker_args
            .lines()
            .any(|line| line.starts_with("exec ")),
        "no exec may run for a replaced full ID: {}",
        outcome.docker_args
    );
    assert!(!outcome.docker_args.contains("restart"));
}

#[test]
fn direct_builder_replacement_after_readiness_refuses_publication() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("after-readiness", "replace_after_readiness");
    let error = outcome
        .result
        .expect_err("readiness cannot authorize a successor");
    assert!(is_owned_binding_refusal(&error), "{error}");
    assert!(
        outcome.root.join("state/readiness").is_file(),
        "original ID was ready"
    );
    assert!(!outcome.root.join("state/effects").exists());
    assert!(!outcome.docker_args.contains("restart"));
}

#[test]
fn direct_builder_replacement_between_readiness_and_exec_has_no_successor_effect() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("before-exec", "replace_before_exec");
    let error = outcome
        .result
        .expect_err("final exec must retain its readiness ID");
    assert!(is_owned_binding_refusal(&error), "{error}");
    assert!(outcome.root.join("state/replaced").is_file());
    assert!(!outcome.root.join("state/effects").exists());
    assert!(!outcome.docker_args.contains("restart"));
}

#[test]
fn direct_builder_replacement_after_final_inspect_cannot_redirect_native_exec() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("at-exec", "replace_at_exec");
    outcome
        .result
        .expect_err("deleted original ID must fail without executing in successor");
    assert!(outcome.root.join("state/replaced").is_file());
    assert!(!outcome.root.join("state/effects").exists());
}

#[test]
fn generated_builder_without_ports_uses_the_same_verified_native_exec_path() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("generated", "generated");
    outcome
        .result
        .expect("generated service must activate without a guessed route");
    let effects = fs::read_to_string(outcome.root.join("state/effects")).unwrap();
    assert!(effects.contains(&"a".repeat(64)));
    assert!(effects.contains("printf builder-shell"));
}

#[test]
fn nerdctl_builder_without_ports_targets_the_verified_full_id() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("nerdctl", "nerdctl");
    outcome
        .result
        .expect("native nerdctl exec must use the inspected ID");
    let effects = fs::read_to_string(outcome.root.join("state/effects")).unwrap();
    assert!(effects.contains(&"a".repeat(64)));
    assert!(effects.contains("printf builder-shell"));
}

#[test]
fn delayed_nerdctl_binding_preserves_verification_and_reaches_exact_id_exec() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("delayed-owned", "delayed_owned");
    outcome
        .result
        .expect("valid transport hops must fit the owned-verification budget");
    let effects = fs::read_to_string(outcome.root.join("state/effects")).unwrap();
    assert!(effects.contains(&"a".repeat(64)));
    let inspects = fs::read_to_string(outcome.root.join("state/delayed-inspects")).unwrap();
    assert!(!inspects.is_empty());
    for sibling in ["dbgate", "mailpit", "silo"] {
        assert!(
            !inspects.contains(sibling),
            "unselected sibling inspected: {inspects}"
        );
    }
    assert!(!outcome.docker_args.contains("restart"));
}

#[test]
fn delayed_nerdctl_binding_still_refuses_foreign_checkout_without_restart() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("delayed-foreign", "delayed_foreign");
    let error = outcome
        .result
        .expect_err("slow foreign inspection must refuse");
    assert!(is_owned_binding_refusal(&error), "{error}");
    assert!(!outcome.root.join("state/effects").exists());
    assert!(!outcome.docker_args.contains("restart"));
}

#[test]
fn delayed_nerdctl_binding_preserves_explicit_expired_caller_deadline() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("expired-binding", "expired_binding");
    let error = outcome
        .result
        .expect_err("caller deadline must not get a fresh allowance");
    assert!(error.contains("timed out"), "{error}");
    assert!(
        outcome.docker_args.is_empty(),
        "expired verification spawned: {}",
        outcome.docker_args
    );
}

#[test]
fn native_builder_exec_preserves_literal_argv_and_stdin() {
    let _lock = lock_test();
    let outcome = run_direct_builder_activation("literal", "literal");
    outcome.result.expect("captured native exec");
    let argv = fs::read(outcome.root.join("state/native-argv")).unwrap();
    let argv = argv
        .split(|byte| *byte == 0)
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect::<Vec<_>>();
    assert!(
        argv.iter().any(|arg| arg == "-i"),
        "stdin must be attached: {argv:?}"
    );
    assert!(
        !argv.iter().any(|arg| arg == "-t"),
        "captured exec must not request TTY: {argv:?}"
    );
    assert!(argv.windows(4).any(|args| args
        == [
            "a".repeat(64),
            "printf".to_owned(),
            "%s".to_owned(),
            "space and $literal; untouched".to_owned()
        ]));
    assert_eq!(
        fs::read_to_string(outcome.root.join("state/stdin")).unwrap(),
        "owned stdin bytes"
    );
}

#[test]
fn direct_builder_wrong_project_or_changing_owner_refuses_before_effects() {
    let _lock = lock_test();
    for mode in ["wrong_project", "changing_scope", "changing_checkout"] {
        let outcome = run_direct_builder_activation(mode, mode);
        let error = outcome.result.expect_err("changed ownership must refuse");
        assert!(is_owned_binding_refusal(&error), "{error}");
        assert!(
            !outcome
                .docker_args
                .lines()
                .any(|line| line.starts_with("exec ")),
            "{mode}: {}",
            outcome.docker_args
        );
        assert!(!outcome.docker_args.contains("restart"));
    }
}
