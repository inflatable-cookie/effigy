use super::super::text_doc::TextDoc;

pub(super) fn render_init_scaffold() -> String {
    let mut doc = TextDoc::new();
    for line in [
        "# Baseline effigy.toml scaffold (phase 1)",
        "",
        "[tasks]",
        "ping = \"printf ok\"",
        "",
        "# Example managed dev task (uncomment to use)",
        "# [tasks.dev]",
        "# mode = \"tui\"",
        "# fail_on_non_zero = true",
        "# workspace = \"app\"",
        "# concurrent = [",
        "#   { task = \"api\", start = 1, tab = 1 },",
        "#   { run = \"printf worker\", start = 2, tab = 2 }",
        "# ]",
        "",
        "# Example DAG-style validation chain (uncomment to use)",
        "# [tasks.validate]",
        "# run = [",
        "#   { id = \"lint\", run = \"printf lint-ok\" },",
        "#   { id = \"tests\", task = \"test vitest\", depends_on = [\"lint\"] },",
        "#   { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }",
        "# ]",
        "",
    ] {
        doc.line(line);
    }
    doc.finish()
}
