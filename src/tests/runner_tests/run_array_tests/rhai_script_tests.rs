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
let process = run_process("sh", ["-lc", "printf process-ok"]);
run_task("capture", []);
write_file("process.txt", process["stdout"]);
write_file("nested.txt", read_file("nested-source.txt"));
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
let stamp = now_utc();
let scratch = make_temp_dir("rhai-runtime-helper");
let pid = process_id().to_string();
write_file("stamp.txt", stamp);
write_file("arg.txt", args[0].to_string());
write_file("scratch.txt", scratch);
write_lines("lines.txt", ["one", "two"]);
append_file("append.txt", "alpha\n");
append_file("append.txt", "beta\n");
write_file("pid.txt", pid);
if !path_exists(scratch) {
    throw "scratch dir was not created";
}
remove_path(scratch);
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
fn run_manifest_task_run_array_rhai_steps_support_in_process_effigy_dispatch() {
    let root = temp_workspace("run-array-rhai-effigy-dispatch");
    fs::create_dir_all(root.join("scripts/rhai")).expect("mkdir rhai script dir");
    fs::write(
        root.join("scripts/rhai/dispatch.rhai"),
        r#"
let listing = run_effigy(["tasks"]);
if !listing["success"] {
    throw "tasks listing failed";
}
write_file("tasks.txt", listing["output"].to_string());

let listing_json = run_effigy_json(["tasks"]);
write_file("tasks.json", json_stringify(listing_json));
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

    let tasks_text = fs::read_to_string(root.join("tasks.txt")).expect("read tasks text");
    assert!(tasks_text.contains("capture"), "got: {tasks_text}");
    let tasks_json = fs::read_to_string(root.join("tasks.json")).expect("read tasks json");
    assert!(
        tasks_json.contains("\"schema\": \"effigy.tasks.v1\""),
        "got: {tasks_json}"
    );
    assert!(tasks_json.contains("\"capture\""), "got: {tasks_json}");
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
        args_log.contains("app sh -lc printf helper-shell"),
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
let up = container_up("web", true);
if up.trim() != "" {
    write_file("up.txt", up);
}
let shell = container_shell("web", "printf helper-shell");
write_file("shell.txt", shell);
let down = container_down("web");
write_file("down.txt", down);
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
