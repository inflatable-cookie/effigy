use super::*;

#[test]
fn cli_tasks_text_output_matches_canonical_fixture_tail() {
    let root = write_catalog_build_workspace("cli-text-fixture-tail");

    let stdout = run_effigy(&["tasks"], Some(&root), false);
    // The CLI header block must stay separated from the listing by a blank
    // line (the #556-era spacing-shape entry's one distinct claim, folded
    // here by effigy#602; every other claim it made lives in the exact tail
    // below).
    assert!(
        stdout.contains("\n\nCatalogs\n"),
        "the CLI header must separate from the Catalogs section: {stdout}"
    );
    let expected = format!(
        "\
Catalogs
────────
count: 2
- root : effigy.toml
- cattle-grid : cattle-grid/effigy.toml

Tasks
─────
General
───────
- cattle-grid/build : cattle-grid/effigy.toml
      tsc -p tsconfig.json {{args}}

{}",
        BUILTIN_TASKS_SECTION
    );
    assert_eq!(extract_tail(&stdout, "\nCatalogs\n────────\n"), expected);
}

#[test]
fn cli_tasks_filtered_text_output_matches_canonical_fixture_tail() {
    let root = write_catalog_build_workspace("cli-text-fixture-tail-filtered");

    let stdout = run_effigy(&["tasks", "--task", "build"], Some(&root), false);
    let expected = "\
Task Matches: build
───────────────────
General
───────
- cattle-grid/build : cattle-grid/effigy.toml
      tsc -p tsconfig.json {args}

";
    assert_eq!(
        extract_tail(&stdout, "\nTask Matches: build\n───────────────────\n"),
        expected
    );
}

#[test]
fn cli_tasks_hides_explicitly_deferred_builtins() {
    let root = temp_workspace("cli-tasks-hidden-deferred-builtin");
    fs::write(
        root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n\n[defer]\nrun = \"printf deferred\"\nbuiltins = [\"release\"]\n",
    )
    .expect("write manifest");

    let stdout = run_effigy(&["tasks"], Some(&root), false);
    assert!(stdout.contains("\n\nBuilt-in Tasks\n"));
    assert!(!stdout.contains("- release :"), "got: {stdout}");
    assert!(stdout.contains("- doctor :"), "got: {stdout}");
}

#[test]
fn cli_tasks_coalesces_default_and_declared_general_groups() {
    let fixture = effigy_test_support::Fixture::new("effigy-cli-private-").expect("group fixture");
    fs::write(
        fixture.path().join("effigy.toml"),
        r#"
[tasks.one]
run = "printf one"
[tasks.two]
run = "printf two"
group = "General"
"#,
    )
    .expect("write manifest");
    let text = run_effigy(&["tasks"], Some(fixture.path()), false);
    assert_eq!(text.matches("\nGeneral\n").count(), 1, "{text}");
    assert!(text.contains("- one :"), "{text}");
    assert!(text.contains("- two :"), "{text}");
}

#[test]
fn cli_tasks_explains_internal_only_inventory() {
    let fixture = effigy_test_support::Fixture::new("effigy-cli-private-")
        .expect("internal inventory fixture");
    fs::write(
        fixture.path().join("effigy.toml"),
        "[tasks.hidden]\nrun = 'printf hidden'\nvisibility = 'internal'\n",
    )
    .expect("write manifest");
    let text = run_effigy(&["tasks"], Some(fixture.path()), false);
    assert!(text.contains("no public tasks"), "{text}");
    assert!(text.contains("effigy tasks --all"), "{text}");
    assert!(!text.contains("- hidden :"), "{text}");
}
