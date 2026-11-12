use super::{
    prepare_check_json, resolve_schema_payload, run_prepared_check_json, split_shell_like_args,
    CheckFailure, ContractBodyFailure, ContractsConfig, ContractsError, SelectionPayload,
    SelectionValidation, CONTRACT_BODY_SCHEMA, SELECTION_PAYLOAD_SCHEMA,
};
use serde_json::{json, Value};
use std::path::Path;

#[test]
fn contract_fixture_arguments_preserve_spaces_quotes_and_paired_roots() {
    let repo = tempfile::tempdir().expect("repo");
    let root = repo.path().join("repo with 'quoted' paths");
    std::fs::create_dir(&root).expect("quoted root");
    let mut fixtures = super::ContractFixtures::new(&root);
    let args = super::expand_contract_fixture_tokens(
        "effigy skill run --path <fixture_skill_source> skill-contract/probe --repo <fixture_skill_consumer>",
        &mut fixtures,
    )
    .expect("fixture arguments");
    assert_eq!(args.len(), 8);
    let source = std::path::PathBuf::from(&args[4]);
    let consumer = std::path::PathBuf::from(&args[7]);
    assert_eq!(source.parent(), consumer.parent());
    assert!(source.starts_with(&root));
    assert!(source.join("effigy.toml").is_file());
    assert!(consumer.join("effigy.toml").is_file());
    assert_eq!(args[5], "skill-contract/probe");
    drop(fixtures);
    assert!(!source.exists() && !consumer.exists());
}

#[test]
fn contract_fixture_invalid_index_command_has_no_fixture_effects() {
    let repo = tempfile::tempdir().expect("repo");
    let mut fixtures = super::ContractFixtures::new(repo.path());
    for command in [
        "effigy tasks --repo <fixture_task_success> '",
        "foreign-program --repo <fixture_task_success>",
    ] {
        assert!(super::expand_contract_fixture_tokens(command, &mut fixtures).is_err());
    }
    assert!(fixtures.directories.is_empty());
    assert!(!fixtures.execution_root.exists());
}

#[test]
fn contract_fixture_all_constructor_roots_are_owned_without_relocating_inspection() {
    let repo = tempfile::tempdir().expect("repo");
    let mut fixtures = super::ContractFixtures::new(repo.path());
    let group = super::create_qa_groups_contract_fixture_dir(&mut fixtures).expect("group");
    let (source, consumer) = super::create_skill_contract_fixtures(&mut fixtures).expect("skill");
    let task = super::create_contract_fixture(&mut fixtures, "[tasks.fail]\nrun = 'exit 9'\n")
        .expect("failure task");
    for path in [&group, &source, &consumer] {
        assert!(path.starts_with(repo.path()));
    }
    assert!(
        !task.starts_with(repo.path()),
        "inspection root stays external"
    );
    assert!(group.join("effigy.toml").is_file());
    let paths: Vec<_> = fixtures
        .directories
        .iter()
        .map(|dir| dir.path().to_path_buf())
        .collect();
    drop(fixtures);
    assert!(paths.iter().all(|path| !path.exists()));
}

#[test]
fn contract_fixture_partial_io_failure_and_unwind_clean_only_owned_directories() {
    let repo = tempfile::tempdir().expect("repo");
    let sentinel = repo.path().join("foreign-sentinel");
    std::fs::write(&sentinel, "preserve").expect("sentinel");
    let failed_path = std::cell::RefCell::new(None);
    let failure = (|| -> std::io::Result<()> {
        let mut fixtures = super::ContractFixtures::new(repo.path());
        let path = fixtures.directory("partial-", super::FixturePlacement::Execution)?;
        *failed_path.borrow_mut() = Some(path.clone());
        std::fs::write(path.join("manifest"), "owned")?;
        // Real IO failure after allocation, while the context owns the root.
        std::fs::create_dir(path.join("manifest"))
    })();
    assert!(failure.is_err());
    assert!(!failed_path
        .borrow()
        .as_ref()
        .expect("allocated root")
        .exists());
    let unwound_path = std::cell::RefCell::new(None);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut fixtures = super::ContractFixtures::new(repo.path());
        let path = fixtures
            .directory("unwind-", super::FixturePlacement::Execution)
            .expect("root");
        *unwound_path.borrow_mut() = Some(path);
        panic!("controlled fixture unwind");
    }));
    assert!(panic.is_err());
    assert!(!unwound_path
        .borrow()
        .as_ref()
        .expect("unwound root")
        .exists());
    assert_eq!(
        std::fs::read_to_string(sentinel).expect("sentinel survived"),
        "preserve"
    );
}

fn write_contract_repo(root: &Path, index: &Value) {
    std::fs::write(
        root.join("effigy.toml"),
        "[contracts]\nschema_index = \"index.json\"\nselection_contract = \"selection.json\"\n",
    )
    .expect("write manifest");
    std::fs::write(root.join("index.json"), index.to_string()).expect("write index");
    std::fs::write(
        root.join("selection.json"),
        json!({
            "type": "object",
            "required": ["selected", "count", "changed_only_base"],
            "properties": {
                "selected": { "type": "array", "items": { "type": "string" } },
                "count": { "type": "number" },
                "changed_only_base": { "type": ["string", "null"] },
            }
        })
        .to_string(),
    )
    .expect("write selection contract");
}

#[test]
fn prepare_check_json_requires_configured_schema_index() {
    let repo = tempfile::tempdir().expect("repo");
    std::fs::write(
        repo.path().join("effigy.toml"),
        "[catalog]\nalias = \"bare\"\n",
    )
    .expect("write manifest");

    let error = prepare_check_json(
        repo.path(),
        &ContractsConfig::load(repo.path()).expect("config"),
        None,
        None,
        &SelectionValidation::Skip,
        None,
    )
    .expect_err("unconfigured project must be refused");

    let message = error.to_string();
    assert!(
        message.contains("no JSON schema index configured"),
        "{message}"
    );
    assert!(message.contains("[contracts] schema_index"), "{message}");
    assert!(!message.contains("docs/knowledge/contracts"), "{message}");
}

#[test]
fn prepare_check_json_validates_the_selection_payload_from_config() {
    let repo = tempfile::tempdir().expect("repo");
    write_contract_repo(
        repo.path(),
        &json!({
            "version": 1,
            "schemas": [{
                "schema": "effigy.command.v1",
                "schema_version": 1,
                "command": "effigy --json help",
                "status": "inactive"
            }],
            "payload_schemas": {}
        }),
    );

    let prepared = prepare_check_json(
        repo.path(),
        &ContractsConfig::load(repo.path()).expect("config"),
        None,
        None,
        &SelectionValidation::Payload,
        None,
    )
    .expect("prepare");
    assert!(prepared.validates_selection());
    assert_eq!(prepared.index_path(), repo.path().join("index.json"));

    let report = run_prepared_check_json(repo.path(), &prepared, false).expect("run");
    assert_eq!(report.checks, 1);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
}

#[test]
fn selection_validation_reports_contract_failures() {
    let repo = tempfile::tempdir().expect("repo");
    write_contract_repo(
        repo.path(),
        &json!({ "version": 1, "schemas": [], "payload_schemas": {} }),
    );
    // The selection payload carries no entries, so a contract that requires one
    // fails: the failure is reported as a contract check, not a command failure.
    std::fs::write(
        repo.path().join("selection.json"),
        json!({
            "type": "object",
            "required": ["selected"],
            "properties": { "selected": { "minItems": 1 } }
        })
        .to_string(),
    )
    .expect("write selection contract");

    let prepared = prepare_check_json(
        repo.path(),
        &ContractsConfig::load(repo.path()).expect("config"),
        None,
        None,
        &SelectionValidation::Payload,
        None,
    )
    .expect("prepare");
    let report = run_prepared_check_json(repo.path(), &prepared, false).expect("run");

    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].schema, SELECTION_PAYLOAD_SCHEMA);
    assert!(
        report.failures[0].reason.contains("/selected"),
        "{:?}",
        report.failures
    );
    assert!(report.render_text(None).is_err());
}

#[test]
fn artifact_selection_validation_reads_the_artifact_file() {
    let repo = tempfile::tempdir().expect("repo");
    write_contract_repo(
        repo.path(),
        &json!({ "version": 1, "schemas": [], "payload_schemas": {} }),
    );

    let valid = repo.path().join("valid.json");
    std::fs::write(
        &valid,
        json!({
            "selected": ["a"],
            "count": 1,
            "changed_only_base": null,
        })
        .to_string(),
    )
    .expect("write artifact");
    let prepared = prepare_check_json(
        repo.path(),
        &ContractsConfig::load(repo.path()).expect("config"),
        None,
        None,
        &SelectionValidation::Artifact(valid),
        None,
    )
    .expect("prepare");
    assert!(run_prepared_check_json(repo.path(), &prepared, false)
        .expect("run")
        .failures
        .is_empty());

    let invalid = repo.path().join("invalid.json");
    std::fs::write(
        &invalid,
        json!({ "selected": [7], "count": 1, "changed_only_base": null }).to_string(),
    )
    .expect("write artifact");
    let prepared = prepare_check_json(
        repo.path(),
        &ContractsConfig::load(repo.path()).expect("config"),
        None,
        None,
        &SelectionValidation::Artifact(invalid),
        None,
    )
    .expect("prepare");
    let report = run_prepared_check_json(repo.path(), &prepared, false).expect("run");
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert!(
        report.failures[0].reason.contains("/selected/0"),
        "{:?}",
        report.failures
    );
}

#[test]
fn selection_validation_requires_a_configured_contract() {
    let repo = tempfile::tempdir().expect("repo");
    std::fs::write(
        repo.path().join("effigy.toml"),
        "[contracts]\nschema_index = \"index.json\"\n",
    )
    .expect("write manifest");
    std::fs::write(
        repo.path().join("index.json"),
        json!({ "version": 1, "schemas": [] }).to_string(),
    )
    .expect("write index");

    let error = prepare_check_json(
        repo.path(),
        &ContractsConfig::load(repo.path()).expect("config"),
        None,
        None,
        &SelectionValidation::Payload,
        None,
    )
    .expect_err("missing selection contract must be refused");

    let message = error.to_string();
    assert!(
        message.contains("no selection contract configured"),
        "{message}"
    );
    assert!(
        message.contains("[contracts] selection_contract"),
        "{message}"
    );
}

#[test]
fn payload_schemas_compile_relative_to_the_index() {
    let repo = tempfile::tempdir().expect("repo");
    std::fs::create_dir_all(repo.path().join("schemas")).expect("mkdir schemas");
    std::fs::write(
        repo.path().join("schemas/tasks.json"),
        json!({
            "type": "object",
            "required": ["tasks"],
            "properties": { "tasks": { "type": "array" } }
        })
        .to_string(),
    )
    .expect("write payload schema");
    std::fs::write(
        repo.path().join("effigy.toml"),
        "[contracts]\nschema_index = \"index.json\"\n",
    )
    .expect("write manifest");
    std::fs::write(
        repo.path().join("index.json"),
        json!({
            "version": 1,
            "payload_schemas": { "effigy.tasks.v1": "schemas/tasks.json" },
            "schemas": []
        })
        .to_string(),
    )
    .expect("write index");

    let prepared = prepare_check_json(
        repo.path(),
        &ContractsConfig::load(repo.path()).expect("config"),
        None,
        None,
        &SelectionValidation::Skip,
        None,
    )
    .expect("prepare");
    assert!(!prepared.validates_selection());
    assert!(prepared.payload_schemas.contains_key("effigy.tasks.v1"));
}

#[test]
fn malformed_payload_schema_is_refused_before_fixture_effects() {
    let repo = tempfile::tempdir().expect("repo");
    std::fs::write(
        repo.path().join("broken.json"),
        json!({ "type": "object", "required": 7 }).to_string(),
    )
    .expect("write payload schema");
    std::fs::write(
        repo.path().join("effigy.toml"),
        "[contracts]\nschema_index = \"index.json\"\n",
    )
    .expect("write manifest");
    std::fs::write(
        repo.path().join("index.json"),
        json!({
            "version": 1,
            "payload_schemas": { "effigy.command.v1": "broken.json" },
            "schemas": [{
                "schema": "effigy.command.v1",
                "schema_version": 1,
                "command": "effigy --json doctor --repo <fixture_task_success>",
                "status": "active"
            }]
        })
        .to_string(),
    )
    .expect("write index");

    let error = prepare_check_json(
        repo.path(),
        &ContractsConfig::load(repo.path()).expect("config"),
        None,
        None,
        &SelectionValidation::Skip,
        None,
    )
    .expect_err("malformed payload schema must fail before commands run");

    assert!(
        error
            .to_string()
            .contains("not valid Draft 2020-12 JSON Schema"),
        "{error}"
    );
    let payload = error
        .check_json_payload()
        .expect("contract-body refusal carries check-json identity");
    assert_eq!(payload["schema"], "effigy.contracts.check-json.v1");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["checks"], 1);
    assert_eq!(payload["failures"].as_array().map(Vec::len), Some(1));
    assert_eq!(payload["failures"][0]["schema"], CONTRACT_BODY_SCHEMA);
    assert!(
        payload["failures"][0]["reason"]
            .as_str()
            .expect("reason")
            .contains("not valid Draft 2020-12 JSON Schema"),
        "{}",
        payload["failures"][0]["reason"]
    );
    assert!(!repo.path().join(".effigy/tmp/contract-fixtures").exists());
}

#[test]
fn contract_body_refusal_keeps_its_public_bytes() {
    // A golden regression on the crate's own refusal surface: the human line is
    // the failure reason, and the check-json identity is exactly these bytes.
    // The CLI passes both through unchanged (see the cli_contracts_ golden).
    let error = ContractsError::ContractBody(Box::new(ContractBodyFailure {
        index: Path::new("/repo/docs/knowledge/contracts/json-schema-index.json").to_path_buf(),
        failure: CheckFailure {
            schema: CONTRACT_BODY_SCHEMA.to_owned(),
            command: "/repo/docs/knowledge/contracts/broken.json".to_owned(),
            reason: "payload schema /repo/docs/knowledge/contracts/broken.json is not valid Draft 2020-12 JSON Schema: invalid JSON Schema: required must be an array".to_owned(),
        },
        selection: SelectionPayload {
            selected: vec!["effigy.command.v1".to_owned(), "effigy.task.v1".to_owned()],
            changed_only_base: Some("origin/main".to_owned()),
        },
    }));

    assert_eq!(
        error.to_string(),
        "payload schema /repo/docs/knowledge/contracts/broken.json is not valid Draft 2020-12 JSON Schema: invalid JSON Schema: required must be an array"
    );

    let payload = error
        .check_json_payload()
        .expect("contract-body refusal carries check-json identity");
    assert_eq!(
        serde_json::to_string(&payload).expect("serialize check-json identity"),
        r#"{"changed_only_base":"origin/main","checks":1,"failures":[{"command":"/repo/docs/knowledge/contracts/broken.json","reason":"payload schema /repo/docs/knowledge/contracts/broken.json is not valid Draft 2020-12 JSON Schema: invalid JSON Schema: required must be an array","schema":"effigy.json-contract-body.v1"}],"index":"/repo/docs/knowledge/contracts/json-schema-index.json","ok":false,"schema":"effigy.contracts.check-json.v1","schema_version":1,"selection":{"changed_only_base":"origin/main","count":2,"selected":["effigy.command.v1","effigy.task.v1"]},"selection_artifact":null,"selection_contract":null}"#
    );
}

#[test]
fn render_selection_payload_json_starts_with_selected_key() {
    let payload = SelectionPayload {
        selected: vec!["a".to_owned(), "b".to_owned()],
        changed_only_base: None,
    };

    let rendered = payload.render_json().expect("render selection payload");
    assert!(rendered.starts_with("{\"selected\":"));
    assert_eq!(
        rendered,
        "{\"selected\":[\"a\",\"b\"],\"count\":2,\"changed_only_base\":null}"
    );
}

#[test]
fn split_shell_like_args_preserves_quoted_groups() {
    let args =
        split_shell_like_args("effigy --json doctor --repo \"/tmp/demo repo\" build -- --watch")
            .expect("args");
    assert_eq!(
        args,
        vec![
            "effigy",
            "--json",
            "doctor",
            "--repo",
            "/tmp/demo repo",
            "build",
            "--",
            "--watch",
        ]
    );
}

#[test]
fn resolve_schema_payload_prefers_nested_result_schema() {
    let payload = json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": true,
        "command": {"kind": "contracts", "name": "contracts"},
        "result": {
            "schema": "effigy.tasks.v1",
            "schema_version": 1,
            "ok": true
        },
        "error": null
    });

    let resolved = resolve_schema_payload("effigy.tasks.v1", &payload);
    assert_eq!(resolved.schema, "effigy.tasks.v1");
    assert_eq!(resolved.schema_version, "1");
    assert_eq!(resolved.payload["ok"], true);
}

#[test]
fn container_render_contract_has_owned_inputs_and_requires_its_public_fields() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixtures = super::ContractFixtures::new(repo.path());
    let args = super::expand_contract_fixture_tokens(
        "effigy container render web --json --repo <fixture_container_render>",
        &mut fixtures,
    )
    .unwrap();
    let root = std::path::PathBuf::from(args.last().unwrap());
    assert!(!root.starts_with(repo.path()));
    assert!(std::fs::read_to_string(root.join("effigy.toml"))
        .unwrap()
        .contains("catalog='redis'"));
    let payload = json!({"schema":"effigy.container.render.v1", "schema_version":1,
        "environment":"web", "path":"/objects/digest", "digest":"digest",
        "compose_files":[], "environment_variables":{}, "files":[]});
    let schema = repo_payload_schema("effigy.container.render.v1");
    assert!(schema.validate(&payload).is_empty());
    for key in [
        "environment",
        "path",
        "digest",
        "compose_files",
        "environment_variables",
        "files",
    ] {
        let mut incomplete = payload.clone();
        incomplete.as_object_mut().unwrap().remove(key);
        assert!(
            !schema.validate(&incomplete).is_empty(),
            "removing {key} must fail the payload schema"
        );
    }
    drop(fixtures);
    assert!(!root.exists());
}

#[test]
fn container_migrate_contract_has_owned_compose_file_inputs_and_requires_its_fields() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixtures = super::ContractFixtures::new(repo.path());
    let args = super::expand_contract_fixture_tokens(
        "effigy container migrate web --json --repo <fixture_container_migrate>",
        &mut fixtures,
    )
    .unwrap();
    let root = std::path::PathBuf::from(args.last().unwrap());
    assert!(!root.starts_with(repo.path()));
    assert!(std::fs::read_to_string(root.join("effigy.toml"))
        .unwrap()
        .contains("compose_file='compose.yml'"));
    assert!(root.join("compose.yml").is_file());
    let payload = json!({"schema":"effigy.container.migrate.v1", "schema_version":1,
        "environment":"web", "compose_file":"compose.yml", "manifest":"effigy.toml",
        "fragments":[], "render_unchanged":true});
    let schema = repo_payload_schema("effigy.container.migrate.v1");
    assert!(schema.validate(&payload).is_empty());
    for key in [
        "environment",
        "compose_file",
        "manifest",
        "fragments",
        "render_unchanged",
    ] {
        let mut incomplete = payload.clone();
        incomplete.as_object_mut().unwrap().remove(key);
        assert!(
            !schema.validate(&incomplete).is_empty(),
            "removing {key} must fail the payload schema"
        );
    }
}

#[test]
fn release_platform_contract_fixture_is_a_ready_single_identity_release() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixtures = super::ContractFixtures::new(repo.path());
    let args = super::expand_contract_fixture_tokens(
        "effigy --json release --plan --repo <fixture_release_platform>",
        &mut fixtures,
    )
    .unwrap();
    assert!(args
        .iter()
        .all(|arg| !arg.contains("<fixture_release_platform>")));
    let root = std::path::PathBuf::from(args.last().unwrap());
    assert!(!root.starts_with(repo.path()));
    // A ready release: `release status` and `release --plan` both exit zero.
    assert!(std::fs::read_to_string(root.join("effigy.toml"))
        .unwrap()
        .contains("changelog = \"CHANGELOG.md\""));
    assert!(std::fs::read_to_string(root.join("Cargo.toml"))
        .unwrap()
        .contains("version = \"0.2.4\""));
    assert!(std::fs::read_to_string(root.join("CHANGELOG.md"))
        .unwrap()
        .contains("## [Unreleased]"));
    drop(fixtures);
    assert!(!root.exists());
}

/// Run a baked status-fixture stub the way a contract row would: with the row
/// environment's PATH, so the fixture's scripts — never any real host runtime —
/// answer the invocation.
fn run_status_fixture_stub(
    root: &Path,
    fixtures: &super::ContractFixtures,
    program: &str,
    args: &[&str],
) -> std::process::Output {
    std::process::Command::new(program)
        .args(args)
        .env(
            "PATH",
            fixtures.row_env.get("PATH").expect("fixture installs PATH"),
        )
        .env(
            "EFFIGY_COMPOSE_BACKEND",
            fixtures
                .row_env
                .get("EFFIGY_COMPOSE_BACKEND")
                .expect("fixture pins the compose backend"),
        )
        .current_dir(root)
        .output()
        .expect("run baked status fixture stub")
}

/// The boundary enforces mode 0o755 on every baked fixture script.
fn assert_scripts_are_executable(root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    for script in ["bin/colima", "bin/docker"] {
        let mode = root
            .join(script)
            .symlink_metadata()
            .unwrap_or_else(|error| panic!("stat {script}: {error}"))
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o755,
            "{script} must carry the boundary's executable mode"
        );
    }
}

/// Discriminating negative: the fixture scripts precede every inherited PATH
/// entry, so the stub — not a real host docker — answers the invocation.
fn assert_docker_stub_refuses(root: &Path, fixtures: &super::ContractFixtures, args: &[&str]) {
    let unexpected = run_status_fixture_stub(root, fixtures, "docker", args);
    assert!(
        !unexpected.status.success(),
        "docker stub must refuse {args:?}"
    );
    assert!(
        String::from_utf8_lossy(&unexpected.stderr).starts_with("unexpected docker invocation:"),
        "docker refusal must be the fixture's own: {:?}",
        String::from_utf8_lossy(&unexpected.stderr)
    );
}

/// One table over the three baked runtime states: the running fixture
/// answers the known-running probe set, the stopped fixture answers status
/// but refuses probes, and the unavailable fixture refuses every probe with
/// the runtime's own sentence.
#[test]
fn container_status_contract_fixtures_bake_each_runtime_state() {
    struct Case {
        token: &'static str,
        status: Result<&'static str, &'static str>,
        ps_answers: bool,
        ps_args: &'static [&'static str],
        docker_args: &'static [&'static str],
    }
    for case in [
        Case {
            token: "<fixture_container_status>",
            status: Ok("colima is running\n"),
            ps_answers: true,
            ps_args: &["ps", "label=com.docker.compose.project=fixture-web-dev"],
            docker_args: &["--version"],
        },
        Case {
            token: "<fixture_container_status_stopped>",
            status: Ok("colima is not running\n"),
            ps_answers: false,
            ps_args: &["ps"],
            docker_args: &["info"],
        },
        Case {
            token: "<fixture_container_status_unavailable>",
            status: Err("error retrieving current runtime: empty value"),
            ps_answers: false,
            ps_args: &["ps"],
            docker_args: &["ps"],
        },
    ] {
        let repo = tempfile::tempdir().unwrap();
        let mut fixtures = super::ContractFixtures::new(repo.path());
        let args = super::expand_contract_fixture_tokens(
            &format!("effigy --json container web status --repo {}", case.token),
            &mut fixtures,
        )
        .unwrap();
        let root = std::path::PathBuf::from(args.last().unwrap());
        assert!(root.join("effigy.toml").is_file(), "{}", case.token);
        assert_scripts_are_executable(&root);
        if case.ps_answers {
            // The running fixture's exact inputs: manifest, Compose and stub
            // bytes plus PATH precedence so the stub answers, not a host docker.
            assert!(
                std::fs::read_to_string(root.join("effigy.toml"))
                    .unwrap()
                    .contains("working_dir='/workspace'"),
                "status fixture must satisfy CWD mapping so known-running can probe exec"
            );
            assert!(root.join("bin/colima").is_file());
            assert_eq!(
                fixtures
                    .row_env
                    .get("EFFIGY_COMPOSE_BACKEND")
                    .map(String::as_str),
                Some("colima")
            );
            let path = fixtures.row_env.get("PATH").expect("PATH");
            assert!(
                path.starts_with(&format!("{}:", root.join("bin").display()))
                    || path == &root.join("bin").display().to_string(),
                "{path}"
            );
            assert_eq!(
                std::fs::read_to_string(root.join("effigy.toml")).unwrap(),
                "[containers]\ndefault='web'\n[containers.web]\ncompose_file='compose.yml'\nprimary_service='app'\nprofile='effigy'\nproject_name='fixture-web-dev'\nworking_dir='/workspace'\n"
            );
            assert_eq!(
                std::fs::read_to_string(root.join("compose.yml")).unwrap(),
                "services:\n  app:\n    image: example\n"
            );
            assert_eq!(
                std::fs::read_to_string(root.join("bin/docker")).unwrap(),
                "#!/bin/sh\nprintf 'unexpected docker invocation: %s\\n' \"$*\" >&2\nexit 1\n"
            );
        }
        let status = run_status_fixture_stub(&root, &fixtures, "colima", &["status"]);
        match case.status {
            Ok(expected) => {
                assert!(status.status.success(), "{}", case.token);
                assert_eq!(
                    String::from_utf8_lossy(&status.stdout),
                    expected,
                    "{}",
                    case.token
                );
            }
            Err(sentence) => {
                assert!(!status.status.success(), "{}", case.token);
                assert!(
                    String::from_utf8_lossy(&status.stderr).contains(sentence),
                    "unavailable refusal must carry the runtime's own sentence: {:?}",
                    String::from_utf8_lossy(&status.stderr)
                );
            }
        }
        // Only the running runtime answers probes; the others refuse with
        // the fixture's own sentence.
        let ps = run_status_fixture_stub(&root, &fixtures, "colima", case.ps_args);
        assert_eq!(ps.status.success(), case.ps_answers, "{}", case.token);
        if case.ps_answers {
            assert!(
                String::from_utf8_lossy(&ps.stdout).contains("fixture-web-dev-app-1"),
                "known-running ps must synthesize the project's app row: {:?}",
                String::from_utf8_lossy(&ps.stdout)
            );
            assert!(
                run_status_fixture_stub(&root, &fixtures, "colima", &["exec", "web", "true"])
                    .status
                    .success()
            );
            let refused = run_status_fixture_stub(&root, &fixtures, "colima", &["version"]);
            assert!(!refused.status.success());
            assert!(
                String::from_utf8_lossy(&refused.stderr)
                    .starts_with("unexpected colima invocation:"),
                "unexpected colima refusal must be the fixture's own: {:?}",
                String::from_utf8_lossy(&refused.stderr)
            );
        } else {
            assert!(
                String::from_utf8_lossy(&ps.stderr).starts_with("unexpected colima invocation:"),
                "refused ps must be the fixture's own: {:?}",
                String::from_utf8_lossy(&ps.stderr)
            );
        }
        assert_docker_stub_refuses(&root, &fixtures, case.docker_args);
        drop(fixtures);
        assert!(!root.exists());
    }
}

#[test]
fn changed_only_compares_whole_row_documents_not_schema_id() {
    let row = |command: &str| super::ContractRow {
        schema: "effigy.command.v1".to_owned(),
        schema_version: 1,
        command: command.to_owned(),
        expect_failure: false,
        isolated_home: false,
        payload_schema: None,
        raw: json!({"schema": "effigy.command.v1", "command": command}),
    };
    let hosts = row("effigy --json container web hosts --repo live");
    let old_status = row("effigy --json container web status --repo live");
    let new_status = row("effigy --json container web status --repo <fixture_container_status>");
    let added =
        row("effigy --json container web status --repo <fixture_container_status_unavailable>");
    let selected = super::rows_changed_from(
        &[hosts.clone(), new_status.clone(), added.clone()],
        &[hosts.clone(), old_status],
    );
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[0].command, new_status.command);
    assert_eq!(selected[1].command, added.command);
}

/// Compile one of the shipped payload schema documents.
fn repo_payload_schema(schema_id: &str) -> effigy_json_schema::CompiledSchema {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/knowledge/contracts/payload-schemas")
        .join(format!("{schema_id}.json"));
    let document = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("read {}: {error}", path.display());
    }))
    .expect("parse payload schema");
    effigy_json_schema::CompiledSchema::compile(&document).expect("compile payload schema")
}

/// The index's own rows and payload schemas must agree: every row that names a
/// payload schema points at a schema the map carries, so `qa:json` validates the
/// payload the row claims instead of silently skipping it (effigy#374).
#[test]
fn every_named_payload_schema_is_in_the_index_map() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf();
    let prepared = prepare_check_json(
        &root,
        &ContractsConfig::load(&root).expect("config"),
        None,
        None,
        &SelectionValidation::Skip,
        None,
    )
    .expect("prepare the repository's own index");

    let mut named = 0;
    for row in &prepared.selected_rows {
        if let Some(id) = row.payload_schema.as_deref() {
            named += 1;
            assert!(
                prepared.payload_schemas.contains_key(id),
                "row `{}` names payload schema `{id}`, which the index does not carry",
                row.command
            );
        }
    }
    assert!(
        named >= 9,
        "expected the deps, plan, help, state-stack and container rows to name payload schemas, got {named}"
    );
}

#[test]
fn an_unexpected_failure_keeps_the_command_own_refusal_sentence() {
    let stdout = serde_json::to_vec(&json!({
        "ok": false,
        "error": { "message": "container `web` refused: the backend is not running" },
    }))
    .expect("json");
    assert_eq!(
        super::unexpected_failure_cause(&stdout, b""),
        "container `web` refused: the backend is not running"
    );
    assert_eq!(
        super::unexpected_failure_cause(b"", b"error: nothing to run\n"),
        "error: nothing to run"
    );
}

#[test]
fn json_contract_home_isolation_is_explicit_and_typed() {
    let mut row = json!({"schema":"effigy.run.v1","schema_version":1,"command":"effigy run status <run> --json","status":"active"});
    assert!(
        !super::load_active_contract_rows(&json!({"schemas":[row.clone()]})).unwrap()[0]
            .isolated_home
    );
    row["isolated_home"] = json!(true);
    assert!(
        super::load_active_contract_rows(&json!({"schemas":[row.clone()]})).unwrap()[0]
            .isolated_home
    );
    row["isolated_home"] = json!("true");
    assert!(super::load_active_contract_rows(&json!({"schemas":[row]})).is_err());
}
