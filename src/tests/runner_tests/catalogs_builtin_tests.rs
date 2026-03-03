use super::*;

fn write_root_manifest(root: &PathBuf, body: &str) {
    write_manifest(&root.join("effigy.toml"), body);
}

#[derive(Clone, Copy)]
enum CatalogResolveFixture {
    RootAndFarmyardApi,
    ManagedProfileInvocation,
}

struct CatalogsResolveCase {
    workspace: &'static str,
    fixture: CatalogResolveFixture,
    args: &'static [&'static str],
    expected: &'static [&'static str],
}

fn write_root_and_farmyard_api_catalog(root: &PathBuf) {
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_root_manifest(root, "[tasks.root]\nrun = \"printf root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );
}

fn write_managed_profile_manifest(root: &PathBuf) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ run = "printf front-ok" }]
"#,
    );
}

fn write_root_task_manifest(root: &PathBuf) {
    write_root_manifest(root, "[tasks.root]\nrun = \"printf root\"\n");
}

fn parse_catalogs_json(out: &str) -> serde_json::Value {
    serde_json::from_str(out).expect("json parse")
}

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
            CatalogResolveFixture::ManagedProfileInvocation => write_managed_profile_manifest(&root),
        }
        let out = run_builtin_ok(root, "catalogs", case.args);
        assert_contains_all(&out, case.expected);
    }
}

#[test]
fn run_manifest_task_builtin_catalogs_json_renders_probe_payload() {
    let root = temp_workspace("builtin-catalogs-json");
    write_root_and_farmyard_api_catalog(&root);

    let out = run_builtin_ok(root, "catalogs", &["--json", "--resolve", "farmyard/api"]);

    let parsed = parse_catalogs_json(&out);
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["catalogs"].is_array());
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "farmyard");
    assert_eq!(parsed["resolve"]["task"], "api");
    assert!(parsed["precedence"].is_array());
}

#[test]
fn run_manifest_task_builtin_catalogs_json_resolve_supports_managed_profile_invocation() {
    let root = temp_workspace("builtin-catalogs-json-resolve-managed-profile");
    write_managed_profile_manifest(&root);

    let out = run_builtin_ok(root, "catalogs", &["--json", "--resolve", "dev front"]);

    let parsed = parse_catalogs_json(&out);
    assert_eq!(parsed["resolve"]["selector"], "dev front");
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "root");
    assert_eq!(parsed["resolve"]["task"], "dev");
    let evidence = parsed["resolve"]["evidence"]
        .as_array()
        .expect("resolve evidence array")
        .iter()
        .filter_map(|line| line.as_str())
        .collect::<Vec<&str>>();
    assert!(evidence
        .iter()
        .any(|line| line.contains("managed profile `front` resolved via invocation `dev front`")));
}

#[test]
fn run_manifest_task_builtin_catalogs_json_reports_resolution_errors() {
    let root = temp_workspace("builtin-catalogs-json-error");
    write_root_task_manifest(&root);

    let out = run_builtin_ok(root, "catalogs", &["--json", "--resolve", "farmyard/api"]);

    let parsed = parse_catalogs_json(&out);
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["resolve"]["status"], "error");
    assert_eq!(parsed["resolve"]["catalog"], serde_json::Value::Null);
    assert!(parsed["resolve"]["error"]
        .as_str()
        .is_some_and(|msg| msg.contains("prefix `farmyard` not found")));
}

#[test]
fn run_manifest_task_builtin_catalogs_json_compact_output_has_no_newlines() {
    let root = temp_workspace("builtin-catalogs-json-compact");
    write_root_and_farmyard_api_catalog(&root);

    let out = run_builtin_ok(
        root,
        "catalogs",
        &["--json", "--pretty", "false", "--resolve", "farmyard/api"],
    );

    assert!(!out.contains('\n'));
    let parsed = parse_catalogs_json(&out);
    assert_eq!(parsed["resolve"]["status"], "ok");
}

#[test]
fn run_manifest_task_builtin_catalogs_pretty_requires_json() {
    let root = temp_workspace("builtin-catalogs-pretty-requires-json");
    write_root_task_manifest(&root);

    let err = run_builtin_err(root, "catalogs", &["--pretty", "false"]);
    assert_task_invocation_error_contains(
        err,
        &["`--pretty` is only supported together with `--json`"],
    );
}

#[test]
fn run_manifest_task_builtin_catalogs_rejects_invalid_pretty_value() {
    let root = temp_workspace("builtin-catalogs-invalid-pretty");
    write_root_task_manifest(&root);

    let err = run_builtin_err(root, "catalogs", &["--json", "--pretty", "nope"]);
    assert_task_invocation_error_contains(err, &["value `nope` is invalid"]);
}
