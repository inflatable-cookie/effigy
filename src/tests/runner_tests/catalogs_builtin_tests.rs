use super::*;

#[test]
fn run_manifest_task_builtin_catalogs_renders_diagnostics_and_resolution_probe() {
    let root = temp_workspace("builtin-catalogs");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");

    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec!["--resolve".to_owned(), "farmyard/api".to_owned()],
        },
        root,
    )
    .expect("builtin catalogs");

    assert!(out.contains("Resolution: farmyard/api"));
    assert!(out.contains("catalog: farmyard"));
}

#[test]
fn run_manifest_task_builtin_catalogs_resolve_supports_managed_profile_invocation() {
    let root = temp_workspace("builtin-catalogs-resolve-managed-profile");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ run = "printf front-ok" }]
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec!["--resolve".to_owned(), "dev front".to_owned()],
        },
        root,
    )
    .expect("builtin catalogs managed profile resolve");

    assert!(out.contains("Resolution: dev front"));
    assert!(out.contains("status: ok"));
    assert!(out.contains("catalog: root"));
    assert!(out.contains("task: dev"));
    assert!(out.contains("managed profile `front` resolved via invocation `dev front`"));
}

#[test]
fn run_manifest_task_builtin_catalogs_json_renders_probe_payload() {
    let root = temp_workspace("builtin-catalogs-json");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");

    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--resolve".to_owned(),
                "farmyard/api".to_owned(),
            ],
        },
        root,
    )
    .expect("builtin catalogs json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json parse");
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
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ run = "printf front-ok" }]
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--resolve".to_owned(),
                "dev front".to_owned(),
            ],
        },
        root,
    )
    .expect("builtin catalogs json managed profile resolve");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json parse");
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
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--resolve".to_owned(),
                "farmyard/api".to_owned(),
            ],
        },
        root,
    )
    .expect("builtin catalogs json error");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json parse");
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
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--pretty".to_owned(),
                "false".to_owned(),
                "--resolve".to_owned(),
                "farmyard/api".to_owned(),
            ],
        },
        root,
    )
    .expect("builtin catalogs compact json");

    assert!(!out.contains('\n'));
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json parse");
    assert_eq!(parsed["resolve"]["status"], "ok");
}

#[test]
fn run_manifest_task_builtin_catalogs_pretty_requires_json() {
    let root = temp_workspace("builtin-catalogs-pretty-requires-json");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec!["--pretty".to_owned(), "false".to_owned()],
        },
        root,
    )
    .expect_err("expected --pretty requires --json");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--pretty` is only supported together with `--json`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_catalogs_rejects_invalid_pretty_value() {
    let root = temp_workspace("builtin-catalogs-invalid-pretty");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--pretty".to_owned(),
                "nope".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected invalid --pretty value");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("value `nope` is invalid"));
        }
        other => panic!("unexpected error: {other}"),
    }
}
