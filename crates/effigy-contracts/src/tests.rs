use super::{
    prepare_check_json, resolve_schema_payload, run_prepared_check_json, split_shell_like_args,
    ContractsConfig, SelectionPayload, SelectionValidation, CONTRACT_BODY_SCHEMA,
    SELECTION_PAYLOAD_SCHEMA,
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
fn invalid_payload_schema_is_reported_as_an_error() {
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
            "payload_schemas": { "effigy.tasks.v1": "broken.json" },
            "schemas": []
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
    .expect_err("invalid payload schema must fail the run");

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
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["failures"][0]["schema"], CONTRACT_BODY_SCHEMA);
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
