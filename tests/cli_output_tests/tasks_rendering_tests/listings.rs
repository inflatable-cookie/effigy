use super::*;

#[test]
fn cli_tasks_text_output_has_stable_section_spacing_and_two_line_task_entries() {
    let root = write_catalog_build_workspace("cli-text-spacing-shape");

    let stdout = run_effigy(&["tasks"], Some(&root), false);
    assert!(stdout.contains("\n\nCatalogs\n"));
    assert!(stdout.contains("\n\nTasks\n"));
    assert!(stdout.contains("\n\nBuilt-in Tasks\n"));
    assert!(stdout.contains(
        "- cattle-grid/build : cattle-grid/effigy.toml\n      tsc -p tsconfig.json {args}"
    ));
}

#[test]
fn cli_tasks_text_output_matches_canonical_fixture_tail() {
    let root = write_catalog_build_workspace("cli-text-fixture-tail");

    let stdout = run_effigy(&["tasks"], Some(&root), false);
    let expected = format!(
        "\
Catalogs
────────
count: 2
- root : effigy.toml
- cattle-grid : cattle-grid/effigy.toml

Tasks
─────
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
