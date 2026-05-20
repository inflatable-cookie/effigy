use crate::runner::tests::prelude::{
    assert_file_text_contains_all, assert_file_text_excludes_all, assert_output_contains_all,
    assert_output_excludes_all, assert_path_exists, assert_path_missing,
    assert_task_invocation_error_contains, run_builtin_err, run_builtin_ok, run_tasks,
    temp_workspace, write_root_manifest, TasksArgs,
};

#[test]
fn run_manifest_task_builtin_init_creates_scaffold_when_missing() {
    let root = temp_workspace("builtin-init-create");

    let out = run_builtin_ok(root.to_path_buf(), "init", &[]);
    assert_output_contains_all(
        &out,
        &[
            "Effigy init apply: applied",
            "manifest.effigy_toml [created]",
            "readme.project_intro [created]",
            "agents_md.effigy_contract [created]",
            "skill.codex_project [created]",
        ],
    );
    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &[
            "[tasks]",
            "ping = \"printf ok\"",
            "# [tasks.dev]",
            "# [tasks.validate]",
        ],
    );
    assert_file_text_contains_all(
        &root.join("README.md"),
        &["inflatable-cookie/effigy", "Built-ins"],
    );
    assert_file_text_contains_all(
        &root.join("AGENTS.md"),
        &["BEGIN EFFIGY AGENT CONTRACT", "effigy doctor"],
    );

    let listed = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: Some("ping".to_owned()),
        resolve_selector: None,
        status_selector: None,
        status_all: false,
        output_json: false,
        pretty_json: true,
    })
    .expect("generated scaffold should parse and list tasks");
    assert_output_contains_all(&listed, &["ping"]);
}

#[test]
fn run_manifest_task_builtin_init_refuses_overwrite_without_force() {
    let root = temp_workspace("builtin-init-refuse-overwrite");
    write_root_manifest(&root, "[tasks]\nold = \"printf old\"\n");

    let err = run_builtin_err(root.to_path_buf(), "init", &["minimal"]);
    assert_task_invocation_error_contains(err, &["already exists", "`effigy init --force`"]);
    assert_file_text_contains_all(&root.join("effigy.toml"), &["old = \"printf old\""]);
    assert_path_missing(
        &root.join("README.md"),
        "refuse-overwrite must not add README",
    );
}

#[test]
fn run_manifest_task_builtin_init_force_overwrites_existing_manifest() {
    let root = temp_workspace("builtin-init-force-overwrite");
    write_root_manifest(&root, "[tasks]\nold = \"printf old\"\n");
    std::fs::write(root.join("README.md"), "# old readme\n").expect("readme");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--force"]);
    assert_output_contains_all(&out, &["Overwrote effigy.toml", "Overwrote README.md"]);
    assert_file_text_contains_all(&root.join("effigy.toml"), &["ping = \"printf ok\""]);
    assert_file_text_excludes_all(&root.join("effigy.toml"), &["old = \"printf old\""]);
    assert_file_text_contains_all(&root.join("README.md"), &["inflatable-cookie/effigy"]);
    assert_file_text_excludes_all(&root.join("README.md"), &["# old readme"]);
}

#[test]
fn run_manifest_task_builtin_init_dry_run_prints_scaffold_without_writing() {
    let root = temp_workspace("builtin-init-dry-run");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--dry-run"]);
    assert_output_contains_all(&out, &["=== effigy.toml ===", "[tasks]", "# [tasks.dev]"]);
    assert_output_contains_all(&out, &["=== README.md ===", "inflatable-cookie/effigy"]);
    assert_path_missing(&root.join("effigy.toml"), "dry-run manifest");
    assert_path_missing(&root.join("README.md"), "dry-run readme");
}

#[test]
fn run_manifest_task_builtin_init_dry_run_notes_skip_for_existing_readme() {
    let root = temp_workspace("builtin-init-dry-run-readme-skip");
    std::fs::write(root.join("README.md"), "# exists\n").expect("readme");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--dry-run"]);
    assert_output_contains_all(&out, &["=== README.md ===", "would skip"]);
    assert_path_missing(&root.join("effigy.toml"), "dry-run manifest");
}

#[test]
fn run_manifest_task_builtin_init_skips_existing_readme_without_force() {
    let root = temp_workspace("builtin-init-skip-readme");
    std::fs::write(root.join("README.md"), "# My project\n\nkeep this body\n")
        .expect("write pre-existing readme");

    let out = run_builtin_ok(root.to_path_buf(), "init", &[]);
    assert_output_contains_all(
        &out,
        &[
            "manifest.effigy_toml [created]",
            "readme.project_intro [present]",
            "existing file left untouched",
        ],
    );
    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &["[tasks]", "ping = \"printf ok\""],
    );
    assert_file_text_contains_all(&root.join("README.md"), &["# My project", "keep this body"]);
}

#[test]
fn run_manifest_task_builtin_init_json_reports_write_status() {
    let root = temp_workspace("builtin-init-json");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.v1\"",
            "\"mode\": \"apply\"",
            "\"status\": \"applied\"",
            "\"changed\": true",
            "\"checks\":",
            "\"id\": \"manifest.effigy_toml\"",
            "\"id\": \"readme.project_intro\"",
            "\"id\": \"agents_md.effigy_contract\"",
        ],
    );
    assert_path_exists(&root.join("effigy.toml"), "init json manifest");
    assert_path_exists(&root.join("README.md"), "init json readme");
}

#[test]
fn run_manifest_task_builtin_init_agent_check_reports_missing_without_writing() {
    let root = temp_workspace("builtin-init-agent-check");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--check", "--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.v1\"",
            "\"mode\": \"check\"",
            "\"status\": \"needs_changes\"",
            "\"id\": \"manifest.effigy_toml\"",
            "\"id\": \"readme.project_intro\"",
            "\"id\": \"agents_md.effigy_contract\"",
            "\"id\": \"skill.codex_project\"",
            "\"id\": \"gitignore.effigy_local_state\"",
        ],
    );
    assert_path_missing(&root.join("effigy.toml"), "agent check manifest");
    assert_path_missing(&root.join("AGENTS.md"), "agent check instructions");
    assert_path_missing(
        &root.join(".agents/skills/effigy/SKILL.md"),
        "agent check skill",
    );
}

#[test]
fn run_manifest_task_builtin_init_agent_apply_is_idempotent_and_preserves_manifest() {
    let root = temp_workspace("builtin-init-agent-apply");
    write_root_manifest(&root, "[tasks]\ncustom = \"printf custom\"\n");

    let applied = run_builtin_ok(root.to_path_buf(), "init", &["--apply", "--json"]);
    assert_output_contains_all(
        &applied,
        &[
            "\"schema\": \"effigy.init.v1\"",
            "\"mode\": \"apply\"",
            "\"status\": \"applied\"",
            "\"changed\": true",
        ],
    );
    assert_file_text_contains_all(&root.join("effigy.toml"), &["custom = \"printf custom\""]);
    assert_file_text_contains_all(
        &root.join("AGENTS.md"),
        &[
            "BEGIN EFFIGY AGENT CONTRACT",
            "effigy doctor",
            "effigy graph",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".agents/skills/effigy/SKILL.md"),
        &["name: effigy", "metadata:", "internal: true", "Agent jobs"],
    );
    assert_file_text_contains_all(
        &root.join(".gitignore"),
        &["BEGIN EFFIGY LOCAL STATE", ".effigy/"],
    );

    let checked = run_builtin_ok(root.to_path_buf(), "init", &["--check", "--json"]);
    assert_output_contains_all(
        &checked,
        &[
            "\"schema\": \"effigy.init.v1\"",
            "\"mode\": \"check\"",
            "\"status\": \"ok\"",
            "\"needs_changes\": false",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_init_checklist_json_reports_setup_inventory() {
    let root = temp_workspace("builtin-init-checklist-json");
    std::fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"build\": \"vite build\" } }\n",
    )
    .expect("write package");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--checklist", "--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.checklist.v1\"",
            "\"mode\": \"checklist\"",
            "\"jobs\":",
            "\"id\": \"task_migration.package_json\"",
            "\"id\": \"graph_status.inspect\"",
            "\"can_run_noninteractive\": true",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_init_apply_actions_json_reports_applied_and_guided_outcomes() {
    let root = temp_workspace("builtin-init-apply-actions-json");

    let out = run_builtin_ok(
        root.to_path_buf(),
        "init",
        &[
            "--apply-actions",
            "manifest.effigy_toml,validation_command.recommend",
            "--json",
        ],
    );
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.actions.v1\"",
            "\"mode\": \"apply_actions\"",
            "\"id\": \"manifest.effigy_toml\"",
            "\"status\": \"applied\"",
            "\"id\": \"validation_command.recommend\"",
            "\"status\": \"guided\"",
        ],
    );
    assert_path_exists(&root.join("effigy.toml"), "apply-actions manifest");
}

#[test]
fn run_manifest_task_builtin_init_apply_actions_can_run_nested_graph_status() {
    let root = temp_workspace("builtin-init-apply-actions-graph-status");
    let _ = run_builtin_ok(root.to_path_buf(), "init", &["--apply"]);

    let out = run_builtin_ok(
        root.to_path_buf(),
        "init",
        &["--apply-actions", "graph_status.inspect", "--json"],
    );
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.actions.v1\"",
            "\"id\": \"graph_status.inspect\"",
            "\"status\": \"inspected\"",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_init_checklist_reports_contextual_bundle_and_secrets_jobs() {
    let root = temp_workspace("builtin-init-checklist-contextual-jobs");
    std::fs::create_dir_all(root.join("bundle")).expect("bundle dir");
    std::fs::write(
        root.join("bundle/bundle.toml"),
        "[bundle]\nname = \"local-test\"\ndescription = \"Local test bundle.\"\n",
    )
    .expect("write bundle descriptor");
    std::fs::write(root.join("bundle/effigy.toml"), "[tasks]\n").expect("write bundle defaults");
    write_root_manifest(
        &root,
        r#"
[bundle]
base = { type = "path", dir = "bundle" }

[secrets]
backend = "effigy-vault"
"#,
    );
    std::fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"dev\": \"vite\", \"build\": \"vite build\" } }\n",
    )
    .expect("write package");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--checklist", "--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"id\": \"task_migration.package_json\"",
            "\"id\": \"bundle_surface.inspect\"",
            "\"id\": \"secrets_surface.inspect\"",
            "\"applicability\": \"applicable\"",
            "\"recommended_command\": \"effigy bundle inspect\"",
            "\"recommended_command\": \"effigy secrets doctor\"",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_init_apply_actions_can_run_contextual_setup_jobs() {
    let root = temp_workspace("builtin-init-apply-actions-contextual-jobs");
    std::fs::create_dir_all(root.join("bundle")).expect("bundle dir");
    std::fs::write(
        root.join("bundle/bundle.toml"),
        "[bundle]\nname = \"local-test\"\ndescription = \"Local test bundle.\"\n",
    )
    .expect("write bundle descriptor");
    std::fs::write(root.join("bundle/effigy.toml"), "[tasks]\n").expect("write bundle defaults");
    write_root_manifest(
        &root,
        r#"
[bundle]
base = { type = "path", dir = "bundle" }

[secrets]
backend = "effigy-vault"
"#,
    );
    std::fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"dev\": \"vite\", \"build\": \"vite build\" } }\n",
    )
    .expect("write package");

    let out = run_builtin_ok(
        root.to_path_buf(),
        "init",
        &[
            "--apply-actions",
            "task_migration.package_json,graph_status.inspect,secrets_surface.inspect,bundle_surface.inspect",
            "--json",
        ],
    );
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.actions.v1\"",
            "\"id\": \"task_migration.package_json\"",
            "\"status\": \"applied\"",
            "\"id\": \"graph_status.inspect\"",
            "\"status\": \"inspected\"",
            "\"id\": \"secrets_surface.inspect\"",
            "\"id\": \"bundle_surface.inspect\"",
        ],
    );
    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &["[tasks]", "dev = \"vite\"", "build = \"vite build\""],
    );
}

#[test]
fn run_manifest_task_builtin_init_rehomes_existing_effigy_gitignore_entry_without_duplication() {
    let root = temp_workspace("builtin-init-agent-gitignore-dedupe");
    std::fs::write(root.join(".gitignore"), ".DS_Store\n.effigy/\n").expect("write gitignore");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--apply", "--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.v1\"",
            "\"id\": \"gitignore.effigy_local_state\"",
            "\"status\": \"applied\"",
        ],
    );

    let gitignore =
        std::fs::read_to_string(root.join(".gitignore")).expect("read normalized gitignore");
    assert!(gitignore.contains("BEGIN EFFIGY LOCAL STATE"));
    assert_eq!(gitignore.matches(".effigy/").count(), 1);
}

#[test]
fn run_manifest_task_builtin_init_repair_removes_loose_effigy_gitignore_duplicates() {
    let root = temp_workspace("builtin-init-agent-gitignore-repair");
    std::fs::write(
        root.join(".gitignore"),
        ".DS_Store\n.effigy/\n\n# BEGIN EFFIGY LOCAL STATE\n.effigy/\n# END EFFIGY LOCAL STATE\n",
    )
    .expect("write duplicated gitignore");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--repair", "--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.v1\"",
            "\"mode\": \"repair\"",
            "\"status\": \"repaired\"",
            "\"id\": \"gitignore.effigy_local_state\"",
        ],
    );

    let gitignore =
        std::fs::read_to_string(root.join(".gitignore")).expect("read repaired gitignore");
    assert_eq!(gitignore.matches(".effigy/").count(), 1);
}

#[test]
fn run_manifest_task_builtin_init_positional_minimal_matches_default() {
    let root = temp_workspace("builtin-init-positional-minimal");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["minimal"]);
    assert_output_contains_all(&out, &["Created effigy.toml", "Created README.md"]);
    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &["[tasks]", "ping = \"printf ok\""],
    );
    assert_path_exists(&root.join("README.md"), "positional minimal readme");
}

#[test]
fn run_manifest_task_builtin_init_unknown_starter_errors_cleanly() {
    let root = temp_workspace("builtin-init-unknown-starter");

    let err = run_builtin_err(root.to_path_buf(), "init", &["does-not-exist"]);
    assert_task_invocation_error_contains(
        err,
        &[
            "failed to load starter",
            "does-not-exist",
            "starter not found",
        ],
    );
    assert_path_missing(
        &root.join("effigy.toml"),
        "unknown-starter run must not write a manifest",
    );
}

#[test]
fn run_manifest_task_builtin_init_extra_positional_errors() {
    let root = temp_workspace("builtin-init-extra-positional");

    let err = run_builtin_err(root.to_path_buf(), "init", &["minimal", "extra"]);
    assert_task_invocation_error_contains(
        err,
        &["accepts at most one starter name", "minimal", "extra"],
    );
}

#[test]
fn run_manifest_task_builtin_init_list_text_reports_registered_starters() {
    let root = temp_workspace("builtin-init-list-text");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--list"]);
    assert_output_contains_all(&out, &["Available starters:", "- minimal", "- northstar"]);
    assert_output_excludes_all(&out, &["- platform", "- php-app"]);
    assert_path_missing(
        &root.join("effigy.toml"),
        "`--list` must not emit a manifest",
    );
}

#[test]
fn run_manifest_task_builtin_init_list_json_reports_schema_and_entries() {
    let root = temp_workspace("builtin-init-list-json");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--list", "--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.list.v1\"",
            "\"starters\":",
            "\"name\": \"minimal\"",
        ],
    );
    assert_path_missing(
        &root.join("effigy.toml"),
        "`--list --json` must not emit a manifest",
    );
}

#[test]
fn run_manifest_task_builtin_init_list_rejects_starter_name() {
    let root = temp_workspace("builtin-init-list-with-name");

    let err = run_builtin_err(root.to_path_buf(), "init", &["--list", "minimal"]);
    assert_task_invocation_error_contains(
        err,
        &["--list", "cannot be combined with a starter name"],
    );
}

#[test]
fn run_manifest_task_builtin_init_list_rejects_force_or_dry_run() {
    let root = temp_workspace("builtin-init-list-with-force");

    let err = run_builtin_err(root.to_path_buf(), "init", &["--list", "--force"]);
    assert_task_invocation_error_contains(
        err,
        &["--list", "cannot be combined with `--force` or `--dry-run`"],
    );
}

#[test]
fn run_manifest_task_builtin_init_northstar_emits_full_consumer_contract_and_guidance() {
    let root = temp_workspace("builtin-init-northstar-emit");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["northstar"]);
    assert_output_contains_all(
        &out,
        &[
            "Created effigy.toml",
            "Created README.md",
            "Created AGENTS.md",
            "Created CHANGELOG.md",
            "Created docs/README.md",
            "Created docs/vision/README.md",
            "Created docs/vision/001-product-vision.md",
            "Created docs/roadmaps/README.md",
            "Created docs/logs/README.md",
            "Created docs/policy/vision-next-task-verbs.txt",
            "Next steps:",
            "<PROJECT_NAME>",
        ],
    );
    assert_path_exists(&root.join("effigy.toml"), "northstar root manifest");
    assert_path_exists(&root.join("AGENTS.md"), "northstar agent contract");
    assert_path_exists(
        &root.join("docs/vision/001-product-vision.md"),
        "northstar first vision document (nested dirs must be created)",
    );
    assert_path_exists(
        &root.join("docs/policy/vision-next-task-verbs.txt"),
        "northstar next-task verb allowlist (nested dirs must be created)",
    );
    // Starter docs_policy wiring + qa:northstar bundle should be present
    // in the emitted manifest.
    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &[
            "[docs_policy.indexes.vision]",
            "[docs_policy.next_actions.vision]",
            "qa:northstar",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_init_northstar_refuses_overwrite_without_force() {
    let root = temp_workspace("builtin-init-northstar-refuse-overwrite");
    // Pre-populate one of the northstar targets so the pre-scan trips.
    write_root_manifest(&root, "[tasks]\nold = \"printf old\"\n");

    let err = run_builtin_err(root.to_path_buf(), "init", &["northstar"]);
    assert_task_invocation_error_contains(
        err,
        &[
            "already exists",
            "effigy.toml",
            "`effigy init --force`",
            "`effigy init --dry-run`",
        ],
    );
    assert_file_text_contains_all(&root.join("effigy.toml"), &["old = \"printf old\""]);
    assert_path_missing(
        &root.join("AGENTS.md"),
        "northstar refuse-overwrite must not write peer files",
    );
    assert_path_missing(
        &root.join("docs/README.md"),
        "northstar refuse-overwrite must not create nested dirs",
    );
}

#[test]
fn run_manifest_task_builtin_init_northstar_force_overwrites_all_targets() {
    let root = temp_workspace("builtin-init-northstar-force");
    write_root_manifest(&root, "[tasks]\nold = \"printf old\"\n");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["northstar", "--force"]);
    assert_output_contains_all(
        &out,
        &[
            "Overwrote effigy.toml",
            "Created AGENTS.md",
            "Created docs/vision/001-product-vision.md",
        ],
    );
    assert_file_text_excludes_all(&root.join("effigy.toml"), &["old = \"printf old\""]);
    assert_path_exists(&root.join("docs/logs/README.md"), "northstar logs index");
}

#[test]
fn run_manifest_task_builtin_init_northstar_dry_run_prints_fenced_sections_without_writing() {
    let root = temp_workspace("builtin-init-northstar-dry-run");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["northstar", "--dry-run"]);
    assert_output_contains_all(
        &out,
        &[
            "=== effigy.toml ===",
            "=== AGENTS.md ===",
            "=== docs/vision/001-product-vision.md ===",
            "=== docs/policy/vision-next-task-verbs.txt ===",
        ],
    );
    assert_path_missing(
        &root.join("effigy.toml"),
        "northstar dry-run must not write the root manifest",
    );
    assert_path_missing(
        &root.join("docs/vision/001-product-vision.md"),
        "northstar dry-run must not write nested docs",
    );
}

#[test]
fn run_manifest_task_builtin_init_northstar_json_reports_files_array_and_guidance() {
    let root = temp_workspace("builtin-init-northstar-json");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["northstar", "--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.v1\"",
            "\"starter\": \"northstar\"",
            "\"written\": true",
            "\"overwritten\": false",
            "\"files\":",
            "\"target\": \"effigy.toml\"",
            "\"target\": \"docs/vision/001-product-vision.md\"",
            "\"target\": \"docs/policy/vision-next-task-verbs.txt\"",
            "\"guidance\":",
            "<PROJECT_NAME>",
        ],
    );
    assert_path_exists(&root.join("AGENTS.md"), "northstar json agent contract");
}
