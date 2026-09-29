//! Nested catalog selections keep their declared container target.
//!
//! Contract `037` makes catalog membership explicit; contract `013` owns
//! execution routing. Together they require that a root-qualified or builtin
//! selector owned by a nested catalog executes on the owning catalog's
//! declared container target — never on the root workspace's container and
//! never silently on the host when the declared target is missing.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::contract_test_support::EnvGuard;
use crate::runner::entrypoints::run_command_with_context;
use crate::runner::error::RunnerError;
use crate::runner::execute::api::run_manifest_task_with_cwd;
use crate::runner::tests::prelude::{
    assert_task_invocation_error_contains, lock_test, run_builtin_ok, temp_workspace,
    write_manifest, write_root_manifest,
};
use crate::runner::tests::prelude::TaskInvocation;
use effigy_cli::Command;
use effigy_context::EffigyRuntimeContext;

const ROOT_MANIFEST: &str = r#"[catalog]
alias = "root"

[catalog.members]
cattle-grid = "cattle-grid"
pen = "pen"

[containers]
default = "workspace"

[containers.workspace]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "root-workspace"
primary_service = "workspace"
working_dir = "/workspace"

[systems]
default = "yard"

[systems.yard]
default_workspace = "main"

[systems.yard.workspaces.main]
container = "workspace"

[tasks.host-task]
run = "printf host-ran"
run_in = "host"
"#;

const CATTLE_MANIFEST: &str = r#"[catalog]
alias = "cattle-grid"

[containers]
default = "cattle"

[containers.cattle]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "cattle-grid-cattle"
primary_service = "cattle"
working_dir = "/workspace"

[systems]
default = "ranch"

[systems.ranch]
default_workspace = "main"

[systems.ranch.workspaces.main]
container = "cattle"

[tasks.check]
run = "printf cattle-check"
run_in = "container"

[tasks.ambient]
run = "printf cattle-ambient"

[test.suites.unit]
run = "printf cattle-suite"
"#;

const PEN_MANIFEST: &str = r#"[catalog]
alias = "pen"

[containers]
default = "sty"

[containers.sty]
driver = "colima"
compose_file = "docker-compose.yml"
project_name = "pen-sty"
primary_service = "sty"

[tasks.vet]
run = "printf pen-vet"
run_in = "container"

[test.suites.unit]
run = "printf pen-suite"
"#;

struct NestedContainerFixture {
    root: PathBuf,
    cattle: PathBuf,
    _env: EnvGuard,
}

fn write_compose(root: &Path, service: &str) {
    fs::write(
        root.join("docker-compose.yml"),
        format!("services:\n  {service}:\n    image: alpine:latest\n"),
    )
    .expect("write docker compose");
}

/// Install a fake docker/colima pair that logs every invocation, including
/// the compose `-f`/`-p` selectors, and reports the child project as running.
fn install_logging_fake_runtime(root: &Path, cattle_dir: &Path) {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir fake runtime bin");

    let docker = bin_dir.join("docker");
    let docker_log = root.join("fake-docker.log");
    let root_canonical = fs::canonicalize(root).expect("canonicalize fixture root");
    let cattle_canonical =
        fs::canonicalize(cattle_dir).expect("canonicalize cattle dir for compose rows");
    fs::write(
        &docker,
        format!(
            r#"#!/bin/sh
log='{log}'
printf 'docker:%s\n' "$*" >> "$log"
if [ "$1" = compose ]; then
  shift
  service=''
  saw_exec=0
  while [ $# -gt 0 ]; do
    case "$1" in
      -f|-p) printf 'flag:%s:%s\n' "$1" "$2" >> "$log"; shift 2 ;;
      exec)
        saw_exec=1
        shift
        while [ $# -gt 0 ]; do
          case "$1" in
            -T|--tty=false) shift ;;
            -w|--workdir|-u|--user|-e|--env) shift 2 ;;
            --) shift ;;
            -*) shift ;;
            *) service="$1"; shift; break ;;
          esac
        done
        printf 'compose:exec:%s:%s\n' "$service" "$*" >> "$log"
        if [ "$1" = which ]; then
          exit 1
        fi
        if [ "$1" = sh ] && [ "$2" = -lc ]; then
          sh -lc "$3"
          exit $?
        fi
        exit 0
        ;;
      up) printf 'compose:up\n' >> "$log"; exit 0 ;;
      down) printf 'compose:down\n' >> "$log"; exit 0 ;;
      ps) printf 'compose:ps\n' >> "$log"; printf 'NAME STATUS\n'; exit 0 ;;
      *) shift ;;
    esac
  done
  exit 0
fi
if [ "$1" = ps ]; then
  shift
  while [ $# -gt 0 ]; do
    case "$1" in
      --format)
        printf 'cattle-a1b2\tUp 2 minutes\t\t'
        printf 'cattle-grid-cattle\t{cattle}\tcattle\t0\n'
        printf 'workspace-a1b2\tUp 2 minutes\t\t'
        printf 'root-workspace\t{root}\tworkspace\t0\n'
        exit 0
        ;;
      *) shift ;;
    esac
  done
  exit 0
fi
exit 0
"#,
            log = docker_log.display(),
            cattle = cattle_canonical.display(),
            root = root_canonical.display(),
        ),
    )
    .expect("write fake docker");
    let mut perms = fs::metadata(&docker).expect("stat fake docker").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&docker, perms).expect("chmod fake docker");

    let colima = bin_dir.join("colima");
    fs::write(
        &colima,
        "#!/bin/sh\ncase \"$1\" in\n  status) printf 'INFO[0000] status: Running\\n'; exit 0 ;;\n  start) exit 0 ;;\n  *) exit 0 ;;\nesac\n",
    )
    .expect("write fake colima");
    let mut perms = fs::metadata(&colima).expect("stat fake colima").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&colima, perms).expect("chmod fake colima");
}

fn nested_container_fixture(name: &str) -> NestedContainerFixture {
    // Canonicalize so plan payloads and catalog roots compare equal on macOS
    // symlinked temp directories (/var -> /private/var).
    let root =
        fs::canonicalize(temp_workspace(name)).expect("canonicalize fixture root");
    write_root_manifest(&root, ROOT_MANIFEST);
    write_compose(&root, "workspace");
    let cattle = root.join("cattle-grid");
    fs::create_dir_all(&cattle).expect("mkdir cattle-grid");
    write_manifest(&cattle.join("effigy.toml"), CATTLE_MANIFEST);
    write_compose(&cattle, "cattle");
    let pen = root.join("pen");
    fs::create_dir_all(&pen).expect("mkdir pen");
    write_manifest(&pen.join("effigy.toml"), PEN_MANIFEST);
    write_compose(&pen, "sty");
    install_logging_fake_runtime(&root, &cattle);
    let old_path = std::env::var("PATH").ok().unwrap_or_default();
    let env = EnvGuard::set_many(&[
        (
            "PATH",
            Some(format!("{}:{old_path}", root.join("bin").display())),
        ),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        (
            "EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF",
            Some("1".to_owned()),
        ),
    ]);
    NestedContainerFixture {
        root,
        cattle,
        _env: env,
    }
}

fn docker_log(fixture: &NestedContainerFixture) -> String {
    let log_path = fixture.root.join("fake-docker.log");
    if !log_path.exists() {
        return String::new();
    }
    fs::read_to_string(&log_path).expect("read fake docker log")
}

fn run_task_from_root(
    fixture: &NestedContainerFixture,
    selector: &str,
) -> Result<String, RunnerError> {
    let context = EffigyRuntimeContext::capture(Some(fixture.root.clone()), None)
        .expect("runtime context");
    run_command_with_context(
        Command::Task(TaskInvocation {
            name: selector.to_owned(),
            args: Vec::new(),
        }),
        &context,
    )
}

fn plan_task_from_root(fixture: &NestedContainerFixture, selector: &str) -> serde_json::Value {
    let context = EffigyRuntimeContext::capture(Some(fixture.root.clone()), None)
        .expect("runtime context");
    let output = run_command_with_context(
        Command::Task(TaskInvocation {
            name: selector.to_owned(),
            args: vec!["--json".to_owned(), "--plan".to_owned()],
        }),
        &context,
    )
    .expect("selector plan json");
    serde_json::from_str(&output).expect("plan json")
}

#[test]
fn root_qualified_nested_container_task_executes_in_declared_container() {
    let _lock = lock_test();
    let fixture = nested_container_fixture("nested-root-qualified-container-exec");

    run_task_from_root(&fixture, "cattle-grid/check").expect("nested container run");

    let log = docker_log(&fixture);
    assert!(
        log.contains("compose:exec:cattle:sh -lc printf cattle-check"),
        "nested task did not exec in the declared cattle container: {log}"
    );
    assert!(
        log.contains("flag:-p:cattle-grid-cattle"),
        "nested task exec did not use the child compose project: {log}"
    );
    assert!(
        !log.contains("flag:-p:root-workspace"),
        "nested task fell back to the root workspace compose project: {log}"
    );
    assert!(
        !log.contains("compose:exec:workspace:"),
        "nested task exec reached the root workspace service: {log}"
    );
}

#[test]
fn child_scope_nested_container_task_still_executes_in_declared_container() {
    let _lock = lock_test();
    let fixture = nested_container_fixture("nested-child-scope-container-exec");

    let output = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "check".to_owned(),
            args: Vec::new(),
        },
        fixture.cattle.clone(),
    )
    .expect("child scope container run");

    let log = docker_log(&fixture);
    assert!(
        log.contains("compose:exec:cattle:sh -lc printf cattle-check"),
        "child scope task did not exec in the declared cattle container: {log}"
    );
}

#[test]
fn root_declared_host_task_stays_on_host() {
    let _lock = lock_test();
    let fixture = nested_container_fixture("nested-host-task-stays-host");

    run_task_from_root(&fixture, "host-task").expect("host task run");

    let log = docker_log(&fixture);
    assert!(
        !log.contains("compose:exec"),
        "host task reached a container: {log}"
    );
}

#[test]
fn nested_task_without_own_target_falls_back_to_the_workspace_runtime() {
    let _lock = lock_test();
    let fixture = nested_container_fixture("nested-ancestor-runtime-fallback");

    // `pen` declares containers but no runtime target of its own, so the
    // documented ancestor fallback routes its container task through the
    // root workspace runtime that owns catalogs without a declared target.
    run_task_from_root(&fixture, "pen/vet").expect("ancestor fallback run");

    let log = docker_log(&fixture);
    assert!(
        log.contains("compose:exec:workspace:sh -lc printf pen-vet"),
        "ancestor fallback task did not exec in the root workspace container: {log}"
    );
}

#[test]
fn nested_container_task_without_any_declared_target_fails_before_execution() {
    let _lock = lock_test();
    let root = temp_workspace("nested-no-target-anywhere-fails");
    write_root_manifest(
        &root,
        r#"[catalog]
alias = "root"

[catalog.members]
barn = "barn"

[containers]
default = "loose"

[containers.loose]
driver = "colima"
compose_file = "docker-compose.yml"
project_name = "loose-sty"
primary_service = "loose"

[tasks.host-task]
run = "printf host-ran"
run_in = "host"
"#,
    );
    let barn = root.join("barn");
    fs::create_dir_all(&barn).expect("mkdir barn");
    write_manifest(
        &barn.join("effigy.toml"),
        r#"[catalog]
alias = "barn"

[tasks.vet]
run = "printf barn-vet"
run_in = "container"
"#,
    );
    let context =
        EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let error = run_command_with_context(
        Command::Task(TaskInvocation {
            name: "barn/vet".to_owned(),
            args: Vec::new(),
        }),
        &context,
    )
    .expect_err("container task without any declared target must fail");

    assert_task_invocation_error_contains(
        error,
        &[
            "vet",
            "run_in = \"container\"",
            "no container target is defined",
        ],
    );
}

#[test]
fn selector_plans_expose_the_resolved_runtime_target() {
    let _lock = lock_test();
    let fixture = nested_container_fixture("nested-plan-runtime-target");

    let container_plan = plan_task_from_root(&fixture, "cattle-grid/check");
    assert_eq!(container_plan["runtime"]["target"], "container");
    assert_eq!(container_plan["runtime"]["container"], "cattle");
    assert_eq!(container_plan["runtime"]["service"], "cattle");
    assert_eq!(
        container_plan["runtime"]["root"],
        fixture.cattle.display().to_string()
    );

    let host_plan = plan_task_from_root(&fixture, "host-task");
    assert_eq!(host_plan["runtime"]["target"], "host");

    let ambient_plan = plan_task_from_root(&fixture, "cattle-grid/ambient");
    assert_eq!(ambient_plan["runtime"]["target"], "container");
    assert_eq!(ambient_plan["runtime"]["container"], "cattle");
}

#[test]
fn builtin_nested_test_selection_executes_in_declared_container() {
    let _lock = lock_test();
    let fixture = nested_container_fixture("nested-builtin-test-container-exec");

    let _ = run_builtin_ok(fixture.root.clone(), "test", &[]);

    let log = docker_log(&fixture);
    assert!(
        log.contains("compose:exec:cattle:sh -lc printf cattle-suite"),
        "builtin nested test suite did not exec in the declared cattle container: {log}"
    );
    assert!(
        !log.contains("flag:-p:root-workspace"),
        "builtin nested test suite used the root workspace compose project: {log}"
    );
}

#[test]
fn builtin_test_plan_exposes_nested_declared_runtime() {
    let _lock = lock_test();
    let fixture = nested_container_fixture("nested-builtin-test-plan-runtime");

    let json = run_builtin_ok(fixture.root.clone(), "test", &["--plan", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("test plan json");
    let targets = parsed["targets"].as_array().expect("targets").clone();
    let by_name = |name: &str| {
        targets
            .iter()
            .find(|target| target["name"] == name)
            .unwrap_or_else(|| panic!("missing target {name} in {targets:?}"))
            .clone()
    };

    let cattle = by_name("cattle-grid");
    assert_eq!(cattle["runtime"]["target"], "container");
    assert_eq!(cattle["runtime"]["container"], "cattle");

    // `pen` declares containers but no systems runtime target, so its suites
    // stay on the host instead of inheriting a location-based container.
    let pen = by_name("pen");
    assert_eq!(pen["runtime"]["target"], "host");
}

#[test]
fn builtin_host_target_suites_stay_on_host_while_container_target_routes() {
    let _lock = lock_test();
    let fixture = nested_container_fixture("nested-builtin-test-mixed-targets");

    let output = run_builtin_ok(fixture.root.clone(), "test", &[]);

    assert!(
        output.contains("pen: ok"),
        "host pen suite did not run: {output}"
    );
    assert!(
        output.contains("cattle-grid: ok"),
        "container cattle suite did not run: {output}"
    );
    let log = docker_log(&fixture);
    assert!(
        log.contains("compose:exec:cattle:sh -lc printf cattle-suite"),
        "cattle suite did not exec in the declared container: {log}"
    );
    assert!(
        !log.contains("compose:exec:sty:"),
        "pen suite execed in the pen containers registry without a declared runtime target: {log}"
    );
}
