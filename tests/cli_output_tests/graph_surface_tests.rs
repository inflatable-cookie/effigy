use super::support::{parse_stdout_json, run_cli_command, run_json_cli_command};

#[test]
fn retired_graph_spellings_return_replacement_errors_without_indexing() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
    for (command, replacement) in [
        ("watch", "graph index"),
        ("files", "graph search"),
        ("callers", "--in"),
        ("callees", "--out"),
        ("impact", "graph node"),
        ("context", "--excerpts-only"),
    ] {
        let output = run_json_cli_command(fixture.path(), &["graph", command]);
        assert!(!output.status.success());
        let payload = parse_stdout_json(&output);
        assert_eq!(payload["ok"], false);
        assert!(payload.to_string().contains(replacement), "{payload}");
        assert!(payload.to_string().contains("removed in v0.15"));
        assert!(!fixture.path().join(".effigy").exists());
    }
}

#[test]
fn graph_warns_once_for_orphaned_catalog_stores_without_touching_them() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
    for alias in ["a", "b"] {
        let dir = fixture
            .path()
            .join(format!(".effigy/graph/catalogs/{alias}"));
        std::fs::create_dir_all(&dir).expect("orphan store dir");
        std::fs::write(dir.join("graph.db"), "obsolete").expect("orphan store");
    }
    let output = run_json_cli_command(fixture.path(), &["graph", "status"]);
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("orphaned catalog graph stores").count(),
        1,
        "{stderr}"
    );
    for alias in ["a", "b"] {
        assert_eq!(
            std::fs::read_to_string(
                fixture
                    .path()
                    .join(format!(".effigy/graph/catalogs/{alias}/graph.db"))
            )
            .expect("untouched store"),
            "obsolete"
        );
    }
    assert!(!fixture.path().join(".effigy/graph/graph.db").exists());
}

#[test]
fn graph_help_has_six_commands_and_no_catalog_selectors() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
    let output = run_cli_command(fixture.path(), &["graph", "--help"]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    for command in ["index", "status", "search", "node", "affected", "explore"] {
        assert!(text.contains(&format!("effigy graph {command}")));
    }
    for command in ["watch", "files", "callers", "callees", "impact", "context"] {
        assert!(!text.contains(&format!("effigy graph {command}")));
    }
    assert!(!text.contains("--catalog") && !text.contains("--all-catalogs"));
}
