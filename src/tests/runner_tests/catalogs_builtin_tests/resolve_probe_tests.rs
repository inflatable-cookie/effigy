use super::prelude::*;

fn setup_root_and_farmyard_api(root: &Path) {
    write_root_and_farmyard_api_catalog(root);
}

fn setup_managed_front_profile(root: &Path) {
    write_managed_dev_profile_manifest(root, "front");
}

#[test]
fn run_manifest_task_builtin_catalogs_renders_diagnostics_and_resolution_probe() {
    let cases = [
        BuiltinInvocationSetupCase {
            workspace: "builtin-catalogs",
            args: &["--resolve", "farmyard/api"],
            expected: &["Resolution: farmyard/api", "catalog: farmyard"],
            setup: setup_root_and_farmyard_api,
        },
        BuiltinInvocationSetupCase {
            workspace: "builtin-catalogs-resolve-managed-profile",
            args: &["--resolve", "dev front"],
            expected: &[
                "Resolution: dev front",
                "status: ok",
                "catalog: root",
                "task: dev",
                "managed profile `front` resolved via invocation `dev front`",
            ],
            setup: setup_managed_front_profile,
        },
    ];

    assert_builtin_ok_case_table_with_case_setup("catalogs", &cases);
}
