use super::prelude::*;

#[test]
fn run_manifest_task_builtin_catalogs_renders_diagnostics_and_resolution_probe() {
    let cases = [
        CatalogsResolveCase {
            workspace: "builtin-catalogs",
            fixture: CatalogResolveFixture::RootAndFarmyardApi,
            args: &["--resolve", "farmyard/api"],
            expected: &["Resolution: farmyard/api", "catalog: farmyard"],
        },
        CatalogsResolveCase {
            workspace: "builtin-catalogs-resolve-managed-profile",
            fixture: CatalogResolveFixture::ManagedProfileInvocation,
            args: &["--resolve", "dev front"],
            expected: &[
                "Resolution: dev front",
                "status: ok",
                "catalog: root",
                "task: dev",
                "managed profile `front` resolved via invocation `dev front`",
            ],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        match case.fixture {
            CatalogResolveFixture::RootAndFarmyardApi => write_root_and_farmyard_api_catalog(&root),
            CatalogResolveFixture::ManagedProfileInvocation => {
                write_managed_dev_profile_manifest(&root, "front")
            }
        }
        let out = run_catalogs_ok(root, case.args);
        assert_contains_all(&out, case.expected);
    }
}
