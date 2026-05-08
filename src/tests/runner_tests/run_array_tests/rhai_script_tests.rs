use crate::contract_test_support::{lock_test, EnvGuard};
use crate::runner::tests::prelude::execution::run_manifest_task_with_cwd;
use crate::runner::tests::prelude::{
    assert_file_text_equals, assert_invocation_error_contains, fs, temp_workspace, write_manifest,
};
use effigy_cli::TaskInvocation;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn run_manifest_task_run_array_supports_file_backed_rhai_steps() {
    let root = temp_workspace("run-array-file-rhai-step");
    fs::create_dir_all(root.join("scripts/rhai")).expect("mkdir rhai script dir");
    fs::write(
        root.join("scripts/rhai/validate.rhai"),
        r#"
let process = process::run("sh", ["-lc", "printf process-ok"]);
task::run("capture", []);
fs::write_file("process.txt", process["stdout"]);
fs::copy("nested-source.txt", "nested.txt");
"#,
    )
    .expect("write rhai script");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.capture]
run = "printf nested-ok > nested-source.txt"

[tasks.validate]
run = [{ rhai = "scripts/rhai/validate.rhai" }]
"#,
    );

    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("file-backed rhai task should run");

    assert_file_text_equals(&root.join("process.txt"), "process-ok");
    assert_file_text_equals(&root.join("nested.txt"), "nested-ok");
}

#[test]
fn run_manifest_task_run_array_rejects_conflicting_rhai_step_keys() {
    let root = temp_workspace("run-array-invalid-rhai-step");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [{ run = "printf invalid", rhai = "print(\"nope\")" }]
"#,
    );

    let error = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect_err("conflicting rhai step should fail");
    assert_invocation_error_contains(error, &["define exactly one of `run`, `task`, or `rhai`"]);
}

#[test]
fn run_manifest_task_run_array_rhai_steps_support_args_and_runtime_helpers() {
    let root = temp_workspace("run-array-rhai-runtime-helpers");
    fs::create_dir_all(root.join("scripts/rhai")).expect("mkdir rhai script dir");
    fs::write(
        root.join("scripts/rhai/helpers.rhai"),
        r#"
let stamp = time::now_utc();
let scratch = fs::make_temp_dir("rhai-runtime-helper");
let ctx = runtime::context();
let exec = exec::run(["sh", "-lc", "printf exec-ok"], #{ run_in: "host" });
let pid = time::process_id().to_string();
fs::write_file("stamp.txt", stamp);
fs::write_file("arg.txt", args[0].to_string());
fs::write_file("scratch.txt", scratch);
fs::write_file("runtime-root.txt", ctx["command_root"]);
fs::write_file("exec.txt", exec["stdout"]);
fs::write_lines("lines.txt", ["one", "two"]);
fs::append_file("append.txt", "alpha\n");
fs::append_file("append.txt", "beta\n");
fs::write_file("pid.txt", pid);
if !fs::exists(scratch) {
    throw "scratch dir was not created";
}
fs::remove(scratch);
"#,
    )
    .expect("write rhai script");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [{ rhai = "scripts/rhai/helpers.rhai" }]
"#,
    );

    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".to_owned(),
            args: vec!["helper-ok".to_owned()],
        },
        root.clone(),
    )
    .expect("rhai helper task should run");

    let stamp = fs::read_to_string(root.join("stamp.txt")).expect("read stamp");
    assert!(stamp.ends_with('Z'), "expected UTC timestamp: {stamp}");
    assert_file_text_equals(&root.join("arg.txt"), "helper-ok");
    assert_file_text_equals(&root.join("exec.txt"), "exec-ok");
    let canonical_root = root.canonicalize().expect("canonical root");
    assert_file_text_equals(
        &root.join("runtime-root.txt"),
        &canonical_root.display().to_string(),
    );
    assert_file_text_equals(&root.join("lines.txt"), "one\ntwo\n");
    assert_file_text_equals(&root.join("append.txt"), "alpha\nbeta\n");
    let pid = fs::read_to_string(root.join("pid.txt")).expect("read pid");
    assert!(
        pid.trim().parse::<u32>().is_ok(),
        "expected numeric pid output: {pid}"
    );
    let scratch = fs::read_to_string(root.join("scratch.txt")).expect("read scratch dir");
    assert!(
        !std::path::Path::new(scratch.trim()).exists(),
        "scratch dir should be removed: {scratch}"
    );
}

#[test]
fn run_manifest_task_run_array_rhai_steps_support_typed_host_helpers() {
    let _guard = lock_test();
    let root = temp_workspace("run-array-rhai-host-helpers");
    fs::create_dir_all(root.join("scripts/rhai")).expect("mkdir rhai script dir");
    fs::write(
        root.join("scripts/rhai/dispatch.rhai"),
        r#"
let bundles = bundle::list();
if bundles["ok"] == false {
    throw "bundle list failed";
}
fs::write_file("bundles.json", json::stringify(bundles));
"#,
    )
    .expect("write rhai script");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.capture]
run = "printf capture-ok"

[tasks.validate]
run = [{ rhai = "scripts/rhai/dispatch.rhai" }]
"#,
    );

    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("rhai dispatch task should run");

    let bundles_json = fs::read_to_string(root.join("bundles.json")).expect("read bundles json");
    assert!(
        bundles_json.contains("\"schema\": \"effigy.bundle.list.v1\""),
        "got: {bundles_json}"
    );
    assert!(bundles_json.contains("\"underlay\""), "got: {bundles_json}");
}

#[test]
fn run_manifest_task_run_array_rhai_steps_support_container_helpers() {
    let _guard = lock_test();
    let root = temp_workspace("run-array-rhai-container-helpers");
    write_container_fixture(&root);
    let bin_dir = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let colima_state = root.join("colima-running");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let _env = EnvGuard::set_many(&[
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
    ]);

    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("rhai container helper task should run");

    let args_log = fs::read_to_string(&docker_args).expect("read docker args");
    assert!(args_log.contains(" up -d"), "got: {args_log}");
    assert!(
        args_log.contains("exec -T -w /workspace"),
        "got: {args_log}"
    );
    assert!(
        args_log.contains("app sh -lc") && args_log.contains("printf helper-shell"),
        "got: {args_log}"
    );
    assert!(
        args_log.contains("down --remove-orphans"),
        "got: {args_log}"
    );
}

fn write_container_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("write compose");
    fs::create_dir_all(root.join("scripts/rhai")).expect("mkdir rhai dir");
    fs::write(
        root.join("scripts/rhai/container-helpers.rhai"),
        r#"
let up = container::up("web", true);
if up.trim() != "" {
    fs::write_file("up.txt", up);
}
let shell = container::shell("web", "printf helper-shell");
fs::write_file("shell.txt", shell);
let down = container::down("web");
fs::write_file("down.txt", down);
"#,
    )
    .expect("write rhai script");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [{ rhai = "scripts/rhai/container-helpers.rhai" }]

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
profile = "dev"
compose_file = "infra/dev/docker-compose.yml"
project_name = "fixture-web-dev"
primary_service = "app"

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"
detach_timeout_secs = 1

[containers.web.host]
mounts = ["./:/workspace"]
"#,
    );
}

fn install_fake_container_runtime(root: &std::path::Path) -> std::path::PathBuf {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let colima = bin_dir.join("colima");
    let docker = bin_dir.join("docker");

    write_executable(
        &colima,
        r#"#!/bin/sh
set -eu
printf "%s\n" "$*" >> "$EFFIGY_TEST_COLIMA_ARGS_FILE"
if [ "${1:-}" = "status" ]; then
  if [ -f "$EFFIGY_TEST_COLIMA_STATE_FILE" ]; then
    printf "Running\n"
  else
    printf "Stopped\n"
  fi
  exit 0
fi
if [ "${1:-}" = "start" ]; then
  : > "$EFFIGY_TEST_COLIMA_STATE_FILE"
  printf "started\n"
  exit 0
fi
printf "unexpected colima invocation: %s\n" "$*" >&2
exit 1
"#,
    );
    write_executable(
        &docker,
        r#"#!/bin/sh
set -eu
printf "%s\n" "$*" >> "$EFFIGY_TEST_DOCKER_ARGS_FILE"
subcmd=""
for arg in "$@"; do
  case "$arg" in
    up|down|ps|logs|exec|kill)
      subcmd="$arg"
      break
      ;;
  esac
done
case "$subcmd" in
  up)
    printf "compose-up\n"
    ;;
  exec)
    printf "exec-ok\n"
    ;;
  down)
    printf "compose-down\n"
    ;;
  ps)
    printf "NAME                STATUS\napp                 running\n"
    ;;
  *)
    printf "unexpected docker invocation: %s\n" "$*" >&2
    exit 1
    ;;
esac
"#,
    );

    bin_dir
}

#[cfg(unix)]
fn write_executable(path: &std::path::Path, body: &str) {
    fs::write(path, body).expect("write executable");
    let mut perms = fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod");
}
