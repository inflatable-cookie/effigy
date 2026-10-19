use super::{resolve_schema_payload, split_shell_like_args, validate_selection, SelectionPayload};
use serde_json::json;
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
    let docs = super::create_docs_context_fixture(&mut fixtures).expect("docs");
    let task = super::create_contract_fixture(&mut fixtures, "[tasks.fail]\nrun = 'exit 9'\n")
        .expect("failure task");
    for path in [&group, &source, &consumer] {
        assert!(path.starts_with(repo.path()));
    }
    for path in [&docs, &task] {
        assert!(
            !path.starts_with(repo.path()),
            "inspection root stays external"
        );
    }
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

#[test]
fn validate_selection_accepts_valid_payload() {
    let repo = tempfile::tempdir().expect("temp repo");
    let repo_root = repo.path();
    let contract_path = repo_root.join("contract.json");
    let artifact_path = repo_root.join("artifact.json");
    std::fs::write(
        &contract_path,
        json!({
            "required": ["selected", "count", "changed_only_base", "mode"],
            "properties": {
                "mode": {
                    "enum": ["full", "changed-only"]
                }
            }
        })
        .to_string(),
    )
    .expect("contract");
    std::fs::write(
        &artifact_path,
        json!({
            "selected": ["a", "b"],
            "count": 2,
            "changed_only_base": null,
            "mode": "full"
        })
        .to_string(),
    )
    .expect("artifact");

    let report = validate_selection(
        repo_root,
        Some(&Path::new("contract.json").to_path_buf()),
        Some(&Path::new("artifact.json").to_path_buf()),
    )
    .expect("selection validation");
    assert!(report.ok());
}

#[test]
fn validate_selection_rejects_wrong_count() {
    let repo = tempfile::tempdir().expect("temp repo");
    let repo_root = repo.path();
    let contract_path = repo_root.join("contract.json");
    let artifact_path = repo_root.join("artifact.json");
    std::fs::write(
        &contract_path,
        json!({
            "required": ["selected", "count", "changed_only_base", "mode"],
            "properties": {
                "mode": {
                    "enum": ["full"]
                }
            }
        })
        .to_string(),
    )
    .expect("contract");
    std::fs::write(
        &artifact_path,
        json!({
            "selected": ["a", "b"],
            "count": 1,
            "changed_only_base": null,
            "mode": "full"
        })
        .to_string(),
    )
    .expect("artifact");

    let report = validate_selection(
        repo_root,
        Some(&Path::new("contract.json").to_path_buf()),
        Some(&Path::new("artifact.json").to_path_buf()),
    )
    .expect("selection validation");
    assert!(report
        .errors
        .iter()
        .any(|error| error.contains("`count` must equal the number of `selected` entries")));
}

#[test]
fn render_selection_payload_json_starts_with_selected_key() {
    let payload = SelectionPayload {
        selected: vec!["a".to_owned(), "b".to_owned()],
        changed_only_base: None,
        mode: "full".to_owned(),
    };

    let rendered = payload.render_json().expect("render selection payload");
    assert!(rendered.starts_with("{\"selected\":"));
    assert_eq!(
        rendered,
        "{\"selected\":[\"a\",\"b\"],\"count\":2,\"changed_only_base\":null,\"mode\":\"full\"}"
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
            "schema": "effigy.contracts.selection-validation.v1",
            "schema_version": 1,
            "ok": true
        },
        "error": null
    });

    let resolved = resolve_schema_payload("effigy.contracts.selection-validation.v1", &payload);
    assert_eq!(resolved.schema, "effigy.contracts.selection-validation.v1");
    assert_eq!(resolved.schema_version, "1");
    assert_eq!(resolved.payload["ok"], true);
}
