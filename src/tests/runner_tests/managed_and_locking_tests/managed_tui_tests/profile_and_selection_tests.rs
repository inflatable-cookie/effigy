use super::super::prelude::*;

#[test]
fn run_manifest_task_managed_tui_uses_default_profile_when_not_specified() {
    let _guard = lock_test();
    let root = temp_workspace("managed-default-profile");
    let _env = managed_tui_env();
    write_managed_admin_profile_manifest(&root);

    let out = run_dev_with_repo(&root, &[]).expect("managed plan should render");
    assert_contains_all(
        &out,
        &[
            "Managed Task Plan",
            "profile: default",
            "api",
            "front",
            "admin",
            "fail-on-non-zero: enabled",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_accepts_named_profile_argument() {
    let _guard = lock_test();
    let root = temp_workspace("managed-named-profile");
    let _env = managed_tui_env();
    write_managed_admin_profile_manifest(&root);

    let out = run_dev(&root, &["admin"]).expect("managed plan should render");
    assert_contains_all(&out, &["profile: admin", "api", "admin"]);
    assert!(!out.contains("front"));
}

#[test]
fn run_manifest_task_managed_tui_supports_profile_specific_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-concurrent-profile-specific");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { run = "printf default-api", start = 1, tab = 2 },
  { run = "printf default-front", start = 2, tab = 1 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { run = "printf admin-api", start = 1, tab = 1 }
]
"#,
    );

    let out_default = run_dev_with_repo(&root, &[]).expect("default managed plan should render");
    assert_contains_all(
        &out_default,
        &["profile: default", "default-api", "default-front"],
    );
    assert!(!out_default.contains("admin-api"));

    let out_admin = run_dev(&root, &["admin"]).expect("admin managed plan should render");
    assert_contains_all(&out_admin, &["profile: admin", "admin-api"]);
    assert!(!out_admin.contains("default-front"));
}

#[test]
fn run_manifest_task_managed_tui_errors_for_unknown_profile() {
    let root = temp_workspace("managed-unknown-profile");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "cargo run -p api" }]
"#,
    );

    let err = run_dev(&root, &["admin"]).expect_err("unknown profile should fail");
    assert_managed_profile_not_found(err, "dev", "admin", &["default"]);
}
