use crate::runner::tests::prelude::cases::*;
use crate::runner::tests::prelude::execution::run_manifest_task_with_cwd;
use crate::runner::tests::prelude::harness::*;
use crate::runner::tests::prelude::json::*;
use crate::runner::tests::prelude::output::*;
use crate::runner::tests::prelude::setup_fanout_catalog_repo;
use crate::runner::tests::prelude::TaskInvocation;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn run_manifest_task_builtin_test_uses_configured_suites_as_source_of_truth() {
    let root = temp_workspace("builtin-test-configured-suites-source-of-truth");
    let configured_marker = root.join("configured-suite.log");
    let vitest_marker = root.join("vitest-suite.log");
    let manifest = format!(
        r#"[test.suites]
unit = "sh -lc 'printf configured > \"{}\"'"
"#,
        configured_marker.display()
    );
    write_root_manifest(&root, &manifest);
    write_package_json_with_test_script(&root);
    install_local_vitest_marker(&root, &vitest_marker);

    let out = run_builtin_ok(root.to_path_buf(), "test", &["--verbose-results"]);
    assert_output_contains_all(&out, &["Test Results", "runner:unit"]);
    assert_path_exists(&configured_marker, "configured suite marker");
    assert_path_missing(&vitest_marker, "auto-detected vitest marker");
}

#[test]
fn run_manifest_task_builtin_test_plans_and_runs_managed_suite_steps() {
    let root = temp_workspace("builtin-test-managed-suite-steps");
    let prepare_marker = root.join("prepare.log");
    let suite_marker = root.join("suite.log");
    write_root_manifest(
        &root,
        &format!(
            r#"[tasks.prepare]
run = "printf prepared > {}"

[test.suites.composed]
run = [
  {{ task = "prepare" }},
  {{ run = "printf suite > {}" }},
]
"#,
            prepare_marker.display(),
            suite_marker.display()
        ),
    );

    let plan = run_builtin_ok(root.to_path_buf(), "test", &["--plan", "composed"]);
    assert_output_contains_all(&plan, &["Test Plan", "composed", "printf prepared"]);
    assert_path_missing(&prepare_marker, "planned prepare marker");
    assert_path_missing(&suite_marker, "planned suite marker");

    let out = run_builtin_ok(root, "test", &["composed"]);
    assert_output_contains_all(&out, &["Test Results", "root: ok"]);
    assert_path_exists(&prepare_marker, "executed prepare marker");
    assert_path_exists(&suite_marker, "executed suite marker");
}

#[test]
fn run_manifest_task_builtin_test_with_configured_multi_suite_requires_explicit_suite() {
    let root = temp_workspace("builtin-test-configured-multi-suite-ambiguous");
    write_test_suites_manifest(&root, &[("unit", "true"), ("integration", "true")]);

    let err = run_builtin_err(root, "test", &["user-service"]);
    assert_task_invocation_error_contains(
        err,
        &[
            "ambiguous",
            "unit",
            "integration",
            "effigy test unit user-service",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_supports_configured_custom_suite_selector() {
    let root = temp_workspace("builtin-test-configured-custom-suite-selector");
    let unit_marker = root.join("unit-suite.log");
    let integration_marker = root.join("integration-suite.log");
    let manifest = format!(
        r#"[test.suites]
unit = "sh -lc 'printf unit > \"{}\"'"
integration = "sh -lc 'printf integration > \"{}\"'"
"#,
        unit_marker.display(),
        integration_marker.display()
    );
    write_root_manifest(&root, &manifest);

    let out = run_builtin_ok(root, "test", &["unit"]);
    assert_output_contains_all(&out, &["Test Results"]);
    assert_path_exists(&unit_marker, "unit suite marker");
    assert_path_missing(&integration_marker, "integration suite marker");
}

#[test]
fn run_manifest_task_builtin_test_skips_on_demand_suites_by_default() {
    let root = temp_workspace("builtin-test-on-demand-suite");
    let unit_marker = root.join("unit-suite.log");
    let focused_marker = root.join("focused-suite.log");
    write_root_manifest(
        &root,
        &format!(
            r#"[test.suites.unit]
run = "sh -lc 'printf unit > \"{}\"'"

[test.suites.focused]
run = "sh -lc 'printf focused > \"{}\"'"
default = false
"#,
            unit_marker.display(),
            focused_marker.display()
        ),
    );

    let default_out = run_builtin_ok(root.to_path_buf(), "test", &[]);
    assert_output_contains_all(&default_out, &["Test Results", "root/unit: ok"]);
    assert_path_exists(&unit_marker, "default suite marker");
    assert_path_missing(&focused_marker, "on-demand suite marker");

    let focused_out = run_builtin_ok(root, "test", &["focused"]);
    assert_output_contains_all(&focused_out, &["Test Results", "root/focused: ok"]);
    assert_path_exists(&focused_marker, "selected on-demand suite marker");
}

#[test]
fn run_manifest_task_builtin_test_multi_suite_selector_errors_include_recovery_hints() {
    let cases = [
        BuiltinInvocationCase {
            workspace: "builtin-test-multi-suite-ambiguous",
            args: &["user-service"],
            expected: &[
                "ambiguous",
                "vitest",
                "cargo-",
                "Try one of:",
                "Use `effigy test --plan <args>`",
                "effigy test vitest user-service",
                "effigy test cargo-",
            ],
        },
        BuiltinInvocationCase {
            workspace: "builtin-test-mistyped-suite-suggestion",
            args: &["viteest", "user-service"],
            expected: &[
                "runner `viteest` is not available",
                "Did you mean `vitest`?",
                "Try: effigy test vitest user-service",
                "Use `effigy test --plan <args>`",
            ],
        },
    ];

    assert_builtin_error_case_table_with_setup("test", &cases, setup_multi_suite_repo);
}

#[test]
fn run_manifest_task_builtin_test_supports_positional_suite_selector() {
    let root = temp_workspace("builtin-test-suite-selector");
    setup_multi_suite_repo(&root);
    let vitest_marker = root.join("vitest-called.log");
    install_local_vitest_marker(&root, &vitest_marker);

    let out = run_builtin_ok(root.to_path_buf(), "test", &["vitest", "user-service"]);
    assert_output_contains_all(&out, &["Test Results", "root/vitest"]);
    assert_output_excludes_all(&out, &["root/cargo-"]);
    assert_path_exists(&vitest_marker, "vitest suite marker");
}

#[test]
fn run_manifest_task_builtin_test_treats_package_name_as_catalog_not_filter() {
    let root = temp_workspace("builtin-test-catalog-not-filter");
    let (catalog_a, catalog_b) = setup_fanout_catalog_repo(&root);

    let json = run_builtin_ok(root, "test", &["--plan", "--json", "vitest", "catalog_a"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    let names = parsed["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .filter_map(|target| target["name"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["catalog_a"]);
    let commands = parsed["targets"][0]["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    assert!(
        commands
            .iter()
            .all(|command| !command.contains("catalog_a")),
        "package name was forwarded as a vitest filter: {commands:?}"
    );
    assert!(catalog_a.exists() && catalog_b.exists());
}

#[test]
fn run_manifest_task_builtin_test_suite_task_ref_keeps_container_run_in() {
    let root = temp_workspace("builtin-test-suite-task-ref-run-in");
    let api = root.join("api");
    fs::create_dir_all(&api).expect("mkdir api");
    write_root_manifest(
        &root,
        r#"[catalog.members]
api = "api"

[test.suites.api]
run = [{ task = "api/test:unit" }]
"#,
    );
    write_manifest(
        &api.join("effigy.toml"),
        r#"[catalog]
alias = "api"

[tasks."test:unit"]
run = "cargo test --workspace --all-features"
run_in = "container"
"#,
    );

    let json = run_builtin_ok(root, "test", &["--plan", "--json", "api"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    let command = parsed["targets"][0]["commands"][0]
        .as_str()
        .expect("command");
    assert!(
        command.contains("api/test:unit") || command.contains("test:unit"),
        "expected nested task invocation, got {command}"
    );
    assert!(
        !command.contains("cargo test --workspace --all-features"),
        "container task-ref was inlined onto the host: {command}"
    );
}

#[test]
fn run_manifest_task_builtin_test_suite_task_ref_honors_inherited_container_run_in() {
    let root = temp_workspace("builtin-test-suite-task-ref-inherited-run-in");
    let api = root.join("api");
    fs::create_dir_all(&api).expect("mkdir api");
    write_root_manifest(
        &root,
        r#"[catalog.members]
api = "api"

[test.suites.api]
run = [{ task = "api/test:unit" }]
"#,
    );
    write_manifest(
        &api.join("effigy.toml"),
        r#"[catalog]
alias = "api"

[task_defaults]
run_in = "container"

[tasks."test:unit"]
run = "printf inherited-container"
"#,
    );

    let json = run_builtin_ok(root, "test", &["--plan", "--json", "api"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    let command = parsed["targets"][0]["commands"][0]
        .as_str()
        .expect("command");
    assert!(
        command.contains("api/test:unit") || command.contains("test:unit"),
        "expected nested task invocation, got {command}"
    );
    assert!(
        !command.contains("printf inherited-container"),
        "inherited container task-ref was inlined onto the host: {command}"
    );
}

#[test]
fn run_manifest_task_builtin_test_suite_task_ref_honors_workspace_binding() {
    let root = temp_workspace("builtin-test-suite-task-ref-workspace-bound");
    let api = root.join("api");
    fs::create_dir_all(&api).expect("mkdir api");
    write_root_manifest(
        &root,
        r#"[catalog.members]
api = "api"

[test.suites.api]
run = [{ task = "api/test:unit" }]
"#,
    );
    write_manifest(
        &api.join("effigy.toml"),
        r#"[catalog]
alias = "api"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[tasks."test:unit"]
run = "printf workspace-bound"
workspace = "app"
"#,
    );

    let json = run_builtin_ok(root, "test", &["--plan", "--json", "api"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    let command = parsed["targets"][0]["commands"][0]
        .as_str()
        .expect("command");
    assert!(
        command.contains("api/test:unit") || command.contains("test:unit"),
        "expected nested task invocation, got {command}"
    );
    assert!(
        !command.contains("printf workspace-bound"),
        "workspace-bound task-ref was inlined onto the host: {command}"
    );
}

#[test]
fn run_manifest_task_builtin_test_child_task_ref_pins_ancestor_container_registry() {
    let fixture = setup_ancestor_container_child_task_fixture(
        "builtin-test-child-task-ref-ancestor-registry",
        ParentSuiteSpec {
            containers: ANCESTOR_WORKSPACE_CONTAINERS,
            child_containers: "",
        },
    );

    let json = run_builtin_ok(fixture.root.clone(), "test", &["--plan", "--json", "api"]);
    let command = planned_suite_command(&json);
    assert_nested_child_task_ref_keeps_ancestor_discovery(&fixture, &command);
    assert_nested_invocation_container_default(&command, "workspace");
    assert!(
        !planned_suite_root(&json).ends_with("/api"),
        "parent suite target should stay the originating catalog, got {}",
        planned_suite_root(&json)
    );
}

#[test]
fn run_manifest_task_builtin_test_child_owned_suite_task_ref_pins_ancestor_container_registry() {
    let fixture = setup_child_owned_suite_ancestor_container_fixture(
        "builtin-test-child-owned-suite-ancestor-registry",
    );

    let json = run_builtin_ok(
        fixture.root.clone(),
        "api/test",
        &["--plan", "--json", "unit"],
    );
    let command = planned_suite_command(&json);
    assert_nested_child_task_ref_keeps_ancestor_discovery(&fixture, &command);
    assert_nested_invocation_container_default(&command, "workspace");
    assert!(
        planned_suite_root(&json).ends_with("/api"),
        "child-owned suite should keep the child catalog cwd, got {}",
        planned_suite_root(&json)
    );
}

#[test]
fn run_manifest_task_builtin_test_child_explicit_container_registry_still_nests() {
    let fixture = setup_ancestor_container_child_task_fixture(
        "builtin-test-child-explicit-container-registry",
        ParentSuiteSpec {
            containers: ANCESTOR_WORKSPACE_CONTAINERS,
            child_containers: CHILD_EXPLICIT_CONTAINERS,
        },
    );

    let json = run_builtin_ok(fixture.root.clone(), "test", &["--plan", "--json", "api"]);
    let command = planned_suite_command(&json);
    assert_nested_child_task_ref_keeps_ancestor_discovery(&fixture, &command);
    assert!(
        quoted_arg_after(&command, "--repo '").is_some(),
        "child explicit registry must keep ancestor fallback available: {command}"
    );
    assert_nested_invocation_container_default(&command, "child");
}

#[test]
fn run_manifest_task_direct_child_task_does_not_inherit_undeclared_ancestor_containers() {
    let fixture = setup_ancestor_container_child_task_fixture(
        "builtin-test-direct-child-no-ambient-ancestor",
        ParentSuiteSpec {
            containers: ANCESTOR_WORKSPACE_CONTAINERS,
            child_containers: "",
        },
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test:unit".to_owned(),
            args: Vec::new(),
        },
        fixture.api,
    )
    .expect_err("direct child invocation should not inherit undeclared ancestor containers");
    assert_task_invocation_error_contains(
        err,
        &[
            "test:unit",
            "run_in = \"container\"",
            "no container target is defined",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_host_child_task_ref_stays_inlined_at_child_cwd() {
    let root = temp_workspace("builtin-test-host-child-task-ref-cwd");
    let api = root.join("api");
    fs::create_dir_all(&api).expect("mkdir api");
    write_root_manifest(
        &root,
        r#"[catalog.members]
api = "api"

[test.suites.api]
run = [{ task = "api/echo" }]
"#,
    );
    write_manifest(
        &api.join("effigy.toml"),
        r#"[catalog]
alias = "api"

[tasks.echo]
run = "printf host-child"
"#,
    );

    let json = run_builtin_ok(root.clone(), "test", &["--plan", "--json", "api"]);
    let command = planned_suite_command(&json);
    assert!(
        command.contains("printf host-child"),
        "host child task-ref should stay inlined: {command}"
    );
    assert!(
        command.contains(&api.display().to_string()),
        "inlined host task-ref lost the child cwd: {command}"
    );
    assert!(
        !command.contains("--repo"),
        "host inlined task-ref should not pin discovery: {command}"
    );
}

#[test]
fn run_manifest_task_builtin_test_child_command_suite_does_not_pin_repo() {
    let root = temp_workspace("builtin-test-child-command-suite-no-repo-pin");
    let api = root.join("api");
    fs::create_dir_all(&api).expect("mkdir api");
    write_root_manifest(
        &root,
        r#"[catalog.members]
api = "api"
"#,
    );
    write_manifest(
        &api.join("effigy.toml"),
        r#"[catalog]
alias = "api"

[test.suites.unit]
run = "printf command-suite"
"#,
    );

    let json = run_builtin_ok(root, "api/test", &["--plan", "--json", "unit"]);
    let command = planned_suite_command(&json);
    assert_eq!(command, "printf command-suite");
    assert!(!command.contains("--repo"));
}

#[test]
fn run_manifest_task_builtin_test_errors_for_unavailable_positional_suite_selector() {
    let root = temp_workspace("builtin-test-suite-selector-unavailable");
    write_package_json_with_test_script(&root);

    let err = run_builtin_err(root, "test", &["nextest"]);
    assert_task_invocation_error_contains(
        err,
        &[
            "not available",
            "nextest",
            "vitest",
            "Try one of:",
            "Use `effigy test --plan <args>`",
            "effigy test vitest",
        ],
    );
}

const ANCESTOR_WORKSPACE_CONTAINERS: &str = r#"
[containers]
default = "workspace"

[containers.workspace]
primary_service = "workspace"
"#;

const CHILD_EXPLICIT_CONTAINERS: &str = r#"
[containers]
default = "child"

[containers.child]
primary_service = "app"
"#;

struct ParentSuiteSpec {
    containers: &'static str,
    child_containers: &'static str,
}

struct AncestorContainerFixture {
    root: PathBuf,
    api: PathBuf,
}

fn setup_ancestor_container_child_task_fixture(
    workspace: &str,
    spec: ParentSuiteSpec,
) -> AncestorContainerFixture {
    let root = temp_workspace(workspace);
    let api = root.join("api");
    fs::create_dir_all(&api).expect("mkdir api");
    write_root_manifest(
        &root,
        &format!(
            r#"[catalog.members]
api = "api"
{containers}
[test.suites.api]
run = [{{ task = "api/test:unit" }}]
"#,
            containers = spec.containers,
        ),
    );
    write_manifest(
        &api.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "api"
{child_containers}
[task_defaults]
run_in = "container"

[tasks."test:unit"]
run = "printf inherited-container"
"#,
            child_containers = spec.child_containers,
        ),
    );
    AncestorContainerFixture { root, api }
}

fn setup_child_owned_suite_ancestor_container_fixture(workspace: &str) -> AncestorContainerFixture {
    let root = temp_workspace(workspace);
    let api = root.join("api");
    fs::create_dir_all(&api).expect("mkdir api");
    write_root_manifest(
        &root,
        &format!(
            r#"[catalog.members]
api = "api"
{ANCESTOR_WORKSPACE_CONTAINERS}
"#
        ),
    );
    write_manifest(
        &api.join("effigy.toml"),
        r#"[catalog]
alias = "api"

[task_defaults]
run_in = "container"

[tasks."test:unit"]
run = "printf inherited-container"

[test.suites.unit]
run = [{ task = "test:unit" }]
"#,
    );
    AncestorContainerFixture { root, api }
}

fn planned_suite_command(json: &str) -> String {
    let parsed = parse_json_output_with_schema(json, "effigy.test.plan.v1");
    parsed["targets"][0]["commands"][0]
        .as_str()
        .expect("command")
        .to_owned()
}

fn planned_suite_root(json: &str) -> String {
    let parsed = parse_json_output_with_schema(json, "effigy.test.plan.v1");
    parsed["targets"][0]["root"]
        .as_str()
        .expect("root")
        .to_owned()
}

fn assert_nested_child_task_ref_keeps_ancestor_discovery(
    fixture: &AncestorContainerFixture,
    command: &str,
) {
    assert!(
        command.contains("api/test:unit") || command.contains("test:unit"),
        "expected nested task invocation, got {command}"
    );
    assert!(
        command.contains(&fixture.api.display().to_string()) || command.contains("/api'"),
        "expanded task lost the child catalog cwd: {command}"
    );
    let cwd = quoted_arg_after(command, "(cd '").unwrap_or("");
    let repo = quoted_arg_after(command, "--repo '").unwrap_or("");
    assert!(
        !repo.is_empty(),
        "nested task-ref did not pin the originating repository: {command}"
    );
    assert!(
        Path::new(cwd).starts_with(repo) && cwd != repo,
        "nested task-ref pinned {repo:?} instead of the ancestor of {cwd:?}: {command}"
    );
    assert!(
        !command.contains("printf inherited-container"),
        "container task-ref was inlined onto the host: {command}"
    );
}

fn quoted_arg_after<'a>(command: &'a str, marker: &str) -> Option<&'a str> {
    let start = command.find(marker)? + marker.len();
    let end = command[start..].find('\'')?;
    Some(&command[start..start + end])
}

fn nested_selector_before_repo(command: &str) -> Option<&str> {
    let before = command.rsplit_once(" --repo ")?.0;
    let end = before.rfind('\'')?;
    let start = before[..end].rfind('\'')?;
    Some(&before[start + 1..end])
}

fn assert_nested_invocation_container_default(command: &str, expected_default: &str) {
    let cwd = PathBuf::from(
        quoted_arg_after(command, "(cd '").expect("nested command should wrap a child cwd"),
    );
    let repo = quoted_arg_after(command, "--repo '")
        .expect("nested command should pin the originating repository")
        .to_owned();
    let selector = nested_selector_before_repo(command)
        .expect("nested command should keep the child task selector")
        .to_owned();
    let task = TaskInvocation {
        name: selector,
        args: vec!["--repo".to_owned(), repo.clone()],
    };
    let preflight = crate::runner::execute::api::build_execution_preflight(&task, cwd.clone())
        .expect("nested invocation should rediscover catalogs");
    assert_eq!(
        preflight.invocation_cwd,
        crate::runner::command_context::canonicalize_or_original(&cwd),
        "nested discovery must keep the child invocation cwd: {command}"
    );
    assert!(
        preflight.catalogs.iter().any(|catalog| {
            catalog
                .manifest
                .containers
                .as_ref()
                .and_then(|config| config.default.as_deref())
                == Some("workspace")
        }),
        "pinned --repo {repo:?} did not reload the ancestor containers registry: {command}"
    );
    let selection = effigy_routing::select_catalog_and_task(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    )
    .expect("nested invocation should select the child task");
    let (_default_run_in, _systems, containers) =
        crate::runner::execute::api::effective_task_binding_inputs(
            &preflight.invocation_cwd,
            &preflight.catalogs,
            &selection,
        );
    let containers = containers.expect("nested catalogs should expose a containers registry");
    assert_eq!(
        containers.default.as_deref(),
        Some(expected_default),
        "nested effective container default: {command}"
    );
}
