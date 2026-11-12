use super::*;

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
General
───────
- dev : effigy.toml
      <managed:tui>
- dev admin : effigy.toml
      <managed:tui profile:admin>
- dev front : effigy.toml
      <managed:tui profile:front>

{}",
        BUILTIN_TASKS_SECTION
    );
    assert_eq!(extract_tail(&stdout, "\nCatalogs\n────────\n"), expected);

    let filtered = run_effigy(&["tasks", "--task", "dev"], Some(&root), false);
    let filtered_expected = "\
Task Matches: dev
─────────────────
General
───────
- dev : effigy.toml
      <managed:tui>
- dev admin : effigy.toml
      <managed:tui profile:admin>
- dev front : effigy.toml
      <managed:tui profile:front>

";
    assert_eq!(
        extract_tail(&filtered, "\nTask Matches: dev\n─────────────────\n"),
        filtered_expected,
        "filtering must preserve every managed profile matching the base name"
    );
}
