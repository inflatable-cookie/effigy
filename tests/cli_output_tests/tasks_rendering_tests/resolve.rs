use super::*;

#[test]
fn cli_tasks_resolve_text_output_matches_canonical_fixture_tail() {
    let root = write_catalog_build_workspace("cli-text-fixture-tail-resolve");

    let stdout = run_effigy(
        &["tasks", "--resolve", "cattle-grid/build"],
        Some(&root),
        false,
    );
    let expected = "\
Resolution: cattle-grid/build
─────────────────────────────
status: ok
catalog: cattle-grid
task: build
lock_scopes: task:cattle-grid/build
evidence:
- selected catalog via explicit prefix `cattle-grid`

";
    assert_eq!(
        extract_tail(&stdout, "\nResolution: cattle-grid/build\n"),
        expected
    );
}

#[test]
fn cli_tasks_resolve_managed_profile_invocation_is_concise() {
    let root = temp_workspace("cli-text-fixture-tail-resolve-managed-profile");
    fs::write(
        root.join("effigy.toml"),
        r#"[catalog]
alias = "root"

[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ run = "printf front-ok" }]
"#,
    )
    .expect("write manifest");

    let stdout = run_effigy(&["tasks", "--resolve", "dev front"], Some(&root), false);
    let expected = "\
Resolution: dev front
─────────────────────
status: ok
catalog: root
task: dev
lock_scopes: task:dev, profile:dev/front
evidence:
- selected shallowest catalog `root` by depth 0 from workspace root
- managed profile `front` resolved via invocation `dev front`

";
    assert_eq!(extract_tail(&stdout, "\nResolution: dev front\n"), expected);
    assert!(!stdout.contains("\nCatalogs\n"));
    assert!(!stdout.contains("\nTasks\n"));
}

#[test]
fn cli_tasks_resolve_managed_profile_missing_is_concise_with_available_profiles() {
    let root = temp_workspace("cli-text-fixture-tail-resolve-managed-profile-missing");
    fs::write(
        root.join("effigy.toml"),
        r#"[catalog]
alias = "root"

[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ run = "printf front-ok" }]
"#,
    )
    .expect("write manifest");

    let stdout = run_effigy(
        &["tasks", "--resolve", "dev missing-profile"],
        Some(&root),
        false,
    );
    let expected = "\
Resolution: dev missing-profile
───────────────────────────────
status: error
catalog: <none>
task: dev
lock_scopes: task:dev, profile:dev/missing-profile
• warn: managed profile `missing-profile` not found for task `dev`; available: default, front

";
    assert_eq!(
        extract_tail(&stdout, "\nResolution: dev missing-profile\n"),
        expected
    );
    assert!(!stdout.contains("\nCatalogs\n"));
    assert!(!stdout.contains("\nTasks\n"));
}

#[test]
fn cli_tasks_resolve_shared_lock_name_is_concise() {
    let root = temp_workspace("cli-text-fixture-tail-resolve-shared-lock");
    fs::write(
        root.join("effigy.toml"),
        r#"[catalog]
alias = "root"

[tasks.api]
run = "printf api"
lock = "backend"
"#,
    )
    .expect("write manifest");

    let stdout = run_effigy(&["tasks", "--resolve", "api"], Some(&root), false);
    let expected = "\
Resolution: api
───────────────
status: ok
catalog: root
task: api
lock_scopes: shared:backend
evidence:
- selected shallowest catalog `root` by depth 0 from workspace root

";
    assert_eq!(extract_tail(&stdout, "\nResolution: api\n"), expected);
}
