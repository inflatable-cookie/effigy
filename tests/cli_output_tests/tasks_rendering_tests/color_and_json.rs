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

/// Plain and forced-color rows share one listing presentation contract; the
/// adjacent JSON test carries the same task in machine form.
#[test]
fn cli_tasks_color_policy_preserves_plain_and_styled_rows() {
    let root = write_catalog_build_workspace("cli-color-task-style");

    let plain_output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("EFFIGY_COLOR", "always")
        .output()
        .expect("run effigy with NO_COLOR overriding forced color");
    assert!(plain_output.status.success(), "{plain_output:?}");
    let plain = String::from_utf8(plain_output.stdout).expect("utf8 stdout");
    assert!(plain.contains("EFFIGY"));
    assert!(plain.contains("Catalogs"));
    assert!(plain.contains(root.file_name().expect("workspace name").to_str().expect("utf8 name")));
    assert!(plain.contains("cattle-grid/build"));
    assert!(!plain.contains('\u{1b}'), "NO_COLOR output: {plain}");

    let stdout = run_effigy(&["tasks"], Some(&root), true);
    assert!(stdout.contains("\u{1b}[1m\u{1b}[37mcattle-grid/build\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[38;5;244mcattle-grid/effigy.toml\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[2m\u{1b}[38;5;117mtsc -p tsconfig.json {args}\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1m\u{1b}[37mhelp\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[38;5;244mShow general help (same as --help)\u{1b}[0m"));
}
