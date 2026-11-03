use super::*;

/// R2 (invocation resolve) and R3 (missing-profile resolve) probe the same
/// two-profile manifest; one writer keeps their fixtures identical.
fn write_resolve_managed_profiles_manifest(root: &Path) {
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
}

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
    write_resolve_managed_profiles_manifest(&root);

    let stdout = run_effigy(&["tasks", "--resolve", "dev front"], Some(&root), false);
    let expected = "\
Resolution: dev front
─────────────────────
status: ok
catalog: root
task: dev
lock_scopes: task:dev
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
    write_resolve_managed_profiles_manifest(&root);

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
lock_scopes: <none>
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
lock_scopes: task:api, shared:backend
evidence:
- selected shallowest catalog `root` by depth 0 from workspace root

";
    assert_eq!(extract_tail(&stdout, "\nResolution: api\n"), expected);
}

/// Two bare builtin words resolve without any catalog task: `doctor` parses
/// as a typed command (early typed-route probe naming its precedence) and
/// `config` falls to the task engine's builtin fallback (Route::Builtin
/// evidence). Neither evidence arm had a detector anywhere before
/// effigy#602 — the prefix, managed-profile, missing-profile and lock arms
/// were each covered by the tests above.
#[test]
fn cli_tasks_resolve_builtin_words_explain_their_route() {
    let root = temp_workspace("cli-text-fixture-tail-resolve-typed-builtin");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf dev\"\n",
    )
    .expect("write manifest");

    let typed = run_effigy(&["tasks", "--resolve", "doctor"], Some(&root), false);
    let typed_expected = "\
Resolution: doctor
──────────────────
status: ok
catalog: <none>
task: doctor
lock_scopes: <none>
evidence:
- `doctor` is the built-in command `effigy doctor`; it runs before any catalog task of that name

";
    assert_eq!(
        extract_tail(&typed, "\nResolution: doctor\n"),
        typed_expected
    );
    assert!(!typed.contains("\nCatalogs\n"));
    assert!(!typed.contains("\nTasks\n"));

    let builtin = run_effigy(&["tasks", "--resolve", "config"], Some(&root), false);
    let builtin_expected = "\
Resolution: config
──────────────────
status: ok
catalog: <none>
task: config
lock_scopes: <none>
evidence:
- no task matched; built-in `config` runs

";
    assert_eq!(
        extract_tail(&builtin, "\nResolution: config\n"),
        builtin_expected
    );
    assert!(!builtin.contains("\nCatalogs\n"));
    assert!(!builtin.contains("\nTasks\n"));
}
