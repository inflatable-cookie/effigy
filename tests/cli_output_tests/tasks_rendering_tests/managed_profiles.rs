use super::*;

#[test]
fn cli_tasks_filtered_text_output_managed_profiles_matches_canonical_fixture_tail() {
    let root = temp_workspace("cli-text-fixture-tail-filtered-managed");
    write_managed_profiles_manifest(&root);

    let stdout = run_effigy(&["tasks", "--task", "dev"], Some(&root), false);
    let expected = "\
Task Matches: dev
─────────────────
- dev : effigy.toml
      <managed:tui>
- dev front : effigy.toml
      <managed:tui profile:front>
- dev admin : effigy.toml
      <managed:tui profile:admin>

";
    assert_eq!(
        extract_tail(&stdout, "\nTask Matches: dev\n─────────────────\n"),
        expected
    );
}

#[test]
fn cli_tasks_text_output_managed_profiles_matches_canonical_fixture_tail() {
    let root = temp_workspace("cli-text-fixture-tail-managed");
    write_managed_profiles_manifest(&root);

    let stdout = run_effigy(&["tasks"], Some(&root), false);
    let expected = format!(
        "\
Catalogs
────────
count: 1
- root : effigy.toml

Tasks
─────
- dev : effigy.toml
      <managed:tui>
- dev front : effigy.toml
      <managed:tui profile:front>
- dev admin : effigy.toml
      <managed:tui profile:admin>

{}",
        BUILTIN_TASKS_SECTION
    );
    assert_eq!(extract_tail(&stdout, "\nCatalogs\n────────\n"), expected);
}

#[test]
fn cli_tasks_text_output_lists_managed_profiles_inline_with_tasks() {
    let root = temp_workspace("cli-text-managed-inline");
    write_managed_profiles_manifest(&root);

    let stdout = run_effigy(&["tasks"], Some(&root), false);
    assert!(stdout.contains("Tasks"));
    assert!(stdout.contains("- dev : effigy.toml"));
    assert!(stdout.contains("- dev front : effigy.toml"));
    assert!(stdout.contains("- dev admin : effigy.toml"));
    assert!(!stdout.contains("- dev default : effigy.toml"));
    assert!(!stdout.contains("Managed Profiles"));
}
