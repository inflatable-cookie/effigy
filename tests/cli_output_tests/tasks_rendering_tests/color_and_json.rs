use super::*;

#[test]
fn cli_tasks_supports_colorized_output_when_forced() {
    let root = temp_workspace("cli-color-tasks");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    )
    .expect("write manifest");

    let stdout = run_effigy(&["tasks"], Some(&root), true);
    assert!(stdout.contains("Catalogs"));
    assert!(stdout.contains('\u{1b}'));
}

#[test]
fn cli_config_global_json_mode_emits_machine_readable_payload() {
    let stdout = run_effigy(&["--json", "config"], None, false);
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "config");
    assert_eq!(parsed["result"]["schema"], "effigy.config.v1");
    assert_eq!(parsed["result"]["mode"], "reference");
    assert!(parsed["result"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("effigy.toml Reference")));
}

#[test]
fn cli_tasks_colorized_output_styles_task_name_path_and_signature() {
    let root = write_catalog_build_workspace("cli-color-task-style");

    let stdout = run_effigy(&["tasks"], Some(&root), true);
    assert!(stdout.contains("\u{1b}[1m\u{1b}[37mcattle-grid/build\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[38;5;244mcattle-grid/effigy.toml\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[2m\u{1b}[38;5;117mtsc -p tsconfig.json {args}\u{1b}[0m"));
}

#[test]
fn cli_tasks_colorized_output_styles_builtin_task_description_as_muted() {
    let root = temp_workspace("cli-color-builtin-style");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    )
    .expect("write manifest");

    let stdout = run_effigy(&["tasks"], Some(&root), true);
    assert!(stdout.contains("\u{1b}[1m\u{1b}[37mhelp\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[38;5;244mShow general help (same as --help)\u{1b}[0m"));
}
