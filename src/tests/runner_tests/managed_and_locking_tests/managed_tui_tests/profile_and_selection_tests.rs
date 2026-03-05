use super::super::prelude::*;

fn setup_admin_profile_manifest(root: &Path) {
    write_managed_admin_profile_manifest(root);
}

fn setup_profile_specific_concurrent_entries(root: &Path) {
    write_root_manifest(
        root,
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
}

fn setup_unknown_profile_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "cargo run -p api" }]
"#,
    );
}

#[test]
fn run_manifest_task_managed_tui_profile_selection_contract_table() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedOutputCase {
            workspace: "managed-default-profile",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "Managed Task Plan",
                "profile: default",
                "api",
                "front",
                "admin",
                "fail-on-non-zero: enabled",
            ],
            expected_absent: &[],
            setup: setup_admin_profile_manifest,
        },
        ManagedOutputCase {
            workspace: "managed-named-profile",
            invocation: ManagedInvocation::Dev,
            args: &["admin"],
            expected: &["profile: admin", "api", "admin"],
            expected_absent: &["front"],
            setup: setup_admin_profile_manifest,
        },
        ManagedOutputCase {
            workspace: "managed-concurrent-profile-specific",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &["profile: default", "default-api", "default-front"],
            expected_absent: &["admin-api"],
            setup: setup_profile_specific_concurrent_entries,
        },
        ManagedOutputCase {
            workspace: "managed-concurrent-profile-specific-admin",
            invocation: ManagedInvocation::Dev,
            args: &["admin"],
            expected: &["profile: admin", "admin-api"],
            expected_absent: &["default-front"],
            setup: setup_profile_specific_concurrent_entries,
        },
    ];

    assert_managed_output_case_table(&cases);
}

#[test]
fn run_manifest_task_managed_tui_errors_for_unknown_profile() {
    let cases = [ManagedProfileNotFoundCase {
        workspace: "managed-unknown-profile",
        invocation: ManagedInvocation::Dev,
        args: &["admin"],
        setup: setup_unknown_profile_manifest,
        expected_task: "dev",
        expected_profile: "admin",
        expected_available: &["default"],
    }];

    assert_managed_profile_not_found_case_table(&cases);
}
