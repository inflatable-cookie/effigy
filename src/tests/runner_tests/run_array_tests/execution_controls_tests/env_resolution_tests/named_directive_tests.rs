use crate::runner::tests::prelude::{
    assert_run_array_task_output_derived_case_table, expected_cargo_paths,
    write_root_api_dual_env_capture_manifest, Path, RunArrayTaskOutputDerivedCase,
};

fn setup_compact_env_directive(root: &Path, marker: &Path) {
    write_root_api_dual_env_capture_manifest(
        root,
        marker,
        None,
        &[
            r#"{ CARGO_HOME = "{{project}}/.cargo/home", CARGO_TARGET_DIR = "{{project}}/.cargo/target" }"#,
        ],
        ("CARGO_HOME", "CARGO_TARGET_DIR"),
    );
}

fn setup_named_env_profile_directive(root: &Path, marker: &Path) {
    write_root_api_dual_env_capture_manifest(
        root,
        marker,
        Some(
            r#"[env]
cargo = [
  { CARGO_HOME = "{{project}}/.cargo/home" },
  { CARGO_TARGET_DIR = "{{project}}/.cargo/target" }
]"#,
        ),
        &[r#""cargo""#],
        ("CARGO_HOME", "CARGO_TARGET_DIR"),
    );
}

fn setup_named_env_value_directive(root: &Path, marker: &Path) {
    write_root_api_dual_env_capture_manifest(
        root,
        marker,
        Some(
            r#"[env]
CARGO_HOME = "{{project}}/.cargo/home"
CARGO_TARGET_DIR = "{{project}}/.cargo/target""#,
        ),
        &[r#""CARGO_HOME""#, r#""CARGO_TARGET_DIR""#],
        ("CARGO_HOME", "CARGO_TARGET_DIR"),
    );
}

#[test]
fn run_manifest_task_run_array_named_env_directive_contract_table() {
    let cases = [
        RunArrayTaskOutputDerivedCase {
            workspace: "run-array-compact-env-directive-entry",
            task: "api",
            marker_rel: "compact-env.out",
            expected: expected_cargo_paths,
            setup: setup_compact_env_directive,
        },
        RunArrayTaskOutputDerivedCase {
            workspace: "run-array-env-profile-directive-entry",
            task: "api",
            marker_rel: "env-profile.out",
            expected: expected_cargo_paths,
            setup: setup_named_env_profile_directive,
        },
        RunArrayTaskOutputDerivedCase {
            workspace: "run-array-env-value-directive-entry",
            task: "api",
            marker_rel: "env-value.out",
            expected: expected_cargo_paths,
            setup: setup_named_env_value_directive,
        },
    ];

    assert_run_array_task_output_derived_case_table(&cases);
}
