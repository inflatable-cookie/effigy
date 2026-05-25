use crate::runner::tests::prelude::{
    assert_output_contains_all, assert_output_excludes_all, run_config_ok,
    workspace_with_empty_manifest,
};

#[test]
fn run_manifest_task_builtin_config_schema_prints_canonical_template() {
    let root = workspace_with_empty_manifest("builtin-config-schema");

    let out = run_config_ok(root, &["--schema"]);
    assert_output_contains_all(
        &out,
        &[
            "Canonical strict-valid effigy.toml schema template",
            "[manifest]",
            "minimum_effigy_version = \"0.6.2\"",
            "include = [",
            "[distribution.package]",
            "repo-url = \"https://github.com/example/my-tool.git\"",
            "[distribution.publish]",
            "binary-name = \"my-tool\"",
            "verify-tag-install = true",
            "verify-binary-json-tasks = true",
            "[distribution.preflight]",
            "[distribution.closeout]",
            "[containers]",
            "[containers.web]",
            "compose_file = \"infra/dev/docker-compose.yml\"",
            "primary_service = \"app\"",
            "[containers.web.host]",
            "mounts = [\"./:/workspace\"]",
            "[demos.login-smoke]",
            "proof = \"Verify the default local login journey succeeds end to end.\"",
            "covers = [\"auth.login\"]",
            "[package_manager]",
            "cargo_env_match = \"prefix-aware\"",
            "[test.suites.managed]",
            "env = \"managed-test\"",
            "teardown_policy = \"always\"",
            "[test.runners]",
            "concurrent = [",
            "container_lifecycle = true",
            "gateway = true",
            "health_wait = true",
            "ready_message = \"http://projectname.test\"",
            "role = \"shell\"",
            "task = \"test vitest \\\"user service\\\"\"",
            "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]",
            "{ name = \"services\", role = \"lifecycle\", start = 1, tab = 1, shutdown_on_exit = true }",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_config_schema_minimal_prints_starter_template() {
    let root = workspace_with_empty_manifest("builtin-config-schema-minimal");

    let out = run_config_ok(root, &["--schema", "--minimal"]);
    assert_output_contains_all(
        &out,
        &[
            "Minimal strict-valid effigy.toml starter",
            "[manifest]",
            "minimum_effigy_version = \"0.6.2\"",
            "[distribution.package]",
            "[distribution.publish]",
            "[distribution.closeout]",
            "[containers]",
            "[containers.web]",
            "[demos.login-smoke]",
            "[package_manager]",
            "[test.runners]",
            "[tasks]",
        ],
    );
    assert_output_excludes_all(&out, &["concurrent = ["]);
}

#[test]
fn run_manifest_task_builtin_config_schema_target_prints_selected_section() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target");

    let out = run_config_ok(root, &["--schema", "--target", "test"]);
    assert_output_contains_all(
        &out,
        &[
            "(test target)",
            "cargo_env_match = \"prefix-aware\"",
            "[test.suites.managed]",
            "setup = [{ run = \"cargo run -p app-db --bin migrate_test_db\" }]",
            "[test.runners]",
        ],
    );
    assert_output_excludes_all(&out, &["[tasks]"]);
}

#[test]
fn run_manifest_task_builtin_config_schema_target_manifest_prints_composition_snippet() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-manifest");

    let out = run_config_ok(root, &["--schema", "--target", "manifest"]);
    assert_output_contains_all(
        &out,
        &[
            "(manifest target)",
            "[manifest]",
            "minimum_effigy_version = \"0.6.2\"",
            "\"effigy.tasks.toml\"",
            "{ path = \"effigy.docs.toml\", override = [\"docs_policy.indexes.vision\"] }",
        ],
    );
    assert_output_excludes_all(&out, &["[tasks]"]);
}

#[test]
fn run_manifest_task_builtin_config_schema_target_bundle_prints_generic_bundle_section() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-bundle");

    let out = run_config_ok(root, &["--schema", "--target", "bundle"]);
    assert_output_contains_all(
        &out,
        &[
            "(bundle target)",
            "[bundle]",
            "# Bundle base selects a local or git-hosted preset.",
            "# base = { type = \"path\", dir = \"bundles/acme\" }",
            "Local bundle directories contain `bundle.toml` metadata plus an `effigy.toml` defaults template under that `dir`.",
            "All other keys are bundle-defined inputs.",
            "Use `effigy bundle inspect` to inspect the active repo bundle source.",
        ],
    );
    assert_output_excludes_all(&out, &["[tasks]"]);
}

#[test]
fn run_manifest_task_builtin_config_schema_target_demos_prints_demo_registry_snippet() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-demos");

    let out = run_config_ok(root, &["--schema", "--target", "demos"]);
    assert_output_contains_all(
        &out,
        &[
            "(demos target)",
            "[demos.login-smoke]",
            "mode = \"interactive\"",
            "status = \"ready\"",
            "task = \"demo:login-smoke\"",
        ],
    );
    assert_output_excludes_all(&out, &["[tasks]"]);
}

#[test]
fn run_manifest_task_builtin_config_schema_target_tasks_includes_quoted_task_ref_examples() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-tasks");

    let out = run_config_ok(root, &["--schema", "--target", "tasks"]);
    assert_output_contains_all(
        &out,
        &[
            "(tasks target)",
            "[tasks]",
            "task = \"test vitest \\\"user service\\\"\"",
            "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_config_schema_target_scan_prints_god_files_section() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-scan");

    let out = run_config_ok(root, &["--schema", "--target", "scan"]);
    assert_output_contains_all(
        &out,
        &[
            "(scan target)",
            "[scan.god_files]",
            "warn = 250",
            "high = 400",
            "critical = 700",
            "doctor = true",
            "respect_gitignore = true",
            "[scan.boundary_violations]",
            "include_heuristic = false",
            "[scan.boundary_violations.layers.app]",
            "paths = [\"src/app/**\"]",
            "may_depend_on = [\"domain\", \"shared\"]",
            "[scan.dead_code]",
            "allow_paths = [\"src/bin/**\", \"scripts/**\"]",
            "allow_symbols = [\"crate::bootstrap::*\", \"main\"]",
            "[scan.validation_gaps]",
            "hotspot_threshold = 4",
            "affected_depth = 2",
            "allow_paths = [\"src/bin/**\", \"scripts/**\"]",
            "[scan.duplicate_blocks]",
            "warn = 20",
            "high = 40",
            "critical = 80",
            "min_occurrences = 2",
            "doctor = false",
            "[scan.comment_ratio]",
            "warn = 1.5",
            "high = 2.0",
            "critical = 3.0",
            "min_code_lines = 20",
            "doctor = true",
        ],
    );
    assert_output_contains_all(
        &out,
        &[
            "[scan.generated_assets]",
            "warn = 1000000",
            "high = 5000000",
            "critical = 20000000",
            "doctor = true",
            "[scan.generated_in_src]",
            "warn = 1",
            "high = 20000",
            "critical = 200000",
            "source_roots = [\"src/**\", \"app/**\", \"lib/**\", \"crates/**\", \"packages/*/src/**\"]",
            "doctor = true",
        ],
    );
    assert_output_contains_all(
        &out,
        &[
            "[scan.attention_markers]",
            "warning = [\"TODO\", \"REVIEW\", \"NOTE\", \"placeholder\"]",
            "high = [\"FIXME\", \"HACK\", \"@deprecated\", \"workaround\"]",
            "critical = [\"BUG\", \"SECURITY\", \"remove before release\"]",
            "doctor = true",
            "[scan.stale_suppressions]",
            "warning = [\"@ts-ignore\", \"@ts-expect-error\", \"type: ignore\", \"eslint-disable-next-line\"]",
            "high = [\"#[allow(\", \"#[expect(\", \"rubocop:disable\", \"swiftlint:disable\"]",
            "critical = [\"nolint\", \"#[allow(warnings)]\", \"shellcheck disable=\", \"eslint-disable\"]",
            "doctor = false",
        ],
    );
    assert_output_excludes_all(&out, &["[tasks]"]);
}

#[test]
fn run_manifest_task_builtin_config_schema_target_test_runner_prints_single_runner_snippet() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-test-runner");

    let out = run_config_ok(
        root,
        &["--schema", "--target", "test", "--runner", "nextest"],
    );

    assert_output_contains_all(
        &out,
        &[
            "(test target, runner: cargo-nextest)",
            "\"cargo-nextest\" = \"cargo nextest run\"",
        ],
    );
    assert_output_excludes_all(&out, &["vitest = ", "\"cargo-test\" = \"cargo test\""]);
}
