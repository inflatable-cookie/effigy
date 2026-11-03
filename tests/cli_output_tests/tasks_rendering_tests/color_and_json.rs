use super::*;

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

/// Forced-color styling for both row kinds the listing renders: catalog task
/// rows (name, manifest path, run signature) and builtin rows (name, muted
/// description). One spawn carries both claim sets; the folded
/// `supports_colorized_output_when_forced` entry only asserted escapes
/// anywhere in stdout, which the CLI header satisfies even when listing
/// colorization is broken (effigy#602 mutation control mc2).
#[test]
fn cli_tasks_colorized_output_styles_catalog_task_and_builtin_rows() {
    let root = write_catalog_build_workspace("cli-color-task-style");

    let stdout = run_effigy(&["tasks"], Some(&root), true);
    assert!(stdout.contains("\u{1b}[1m\u{1b}[37mcattle-grid/build\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[38;5;244mcattle-grid/effigy.toml\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[2m\u{1b}[38;5;117mtsc -p tsconfig.json {args}\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1m\u{1b}[37mhelp\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[38;5;244mShow general help (same as --help)\u{1b}[0m"));
}
