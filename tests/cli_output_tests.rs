use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn cli_tasks_no_color_output_has_no_ansi_sequences() {
    let root = temp_workspace("cli-no-color");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("EFFIGY_COLOR", "always")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("EFFIGY"));
    assert!(stdout.contains("╭"));
    assert!(stdout.contains(&root.display().to_string()));
    assert!(stdout.contains("Catalogs"));
    assert!(stdout.contains("catalog"));
    assert!(!stdout.contains('\u{1b}'));
}

#[test]
fn cli_parse_error_includes_usage_in_stderr() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("EFFIGY"));
    assert!(stderr.contains("╭"));
    assert!(stderr.contains("Invalid command arguments"));
    assert!(stderr.contains("Commands"));
    assert!(!stderr.contains('\u{1b}'));
}

#[test]
fn cli_parse_error_json_mode_emits_machine_readable_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("tasks")
        .arg("--repo")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "cli");
    assert_eq!(parsed["command"]["name"], "parse");
    assert_eq!(parsed["error"]["kind"], "CliParseError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("--repo requires a value")));
}

#[test]
fn cli_runner_error_json_mode_emits_machine_readable_payload() {
    let root = temp_workspace("cli-json-runner-error");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("missing-task")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "missing-task");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("missing-task")));
}

#[test]
fn cli_json_mode_tasks_wraps_tasks_payload() {
    let root = temp_workspace("cli-json-tasks-success");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf dev\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "tasks");
    assert_eq!(parsed["command"]["name"], "tasks");
    assert_eq!(parsed["result"]["schema"], "effigy.tasks.v1");
}

#[test]
fn cli_json_mode_doctor_wraps_doctor_payload() {
    let root = temp_workspace("cli-json-doctor-success");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.health]\nrun = \"printf healthy\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("doctor")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "doctor");
    assert_eq!(parsed["command"]["name"], "doctor");
    assert_eq!(parsed["result"]["schema"], "effigy.doctor.v1");
}

#[test]
fn cli_json_mode_config_wraps_config_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("config")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "config");
    assert_eq!(parsed["result"]["schema"], "effigy.config.v1");
}

#[test]
fn cli_json_mode_task_wraps_task_run_payload() {
    let root = temp_workspace("cli-json-task-success");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build-ok\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("build")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "build");
    assert_eq!(parsed["result"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["result"]["task"], "build");
    assert_eq!(parsed["result"]["stdout"], "build-ok");
}

#[test]
fn cli_json_mode_parse_error_wraps_error_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("tasks")
        .arg("--repo")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "cli");
    assert_eq!(parsed["command"]["name"], "parse");
    assert_eq!(parsed["error"]["kind"], "CliParseError");
}

#[test]
fn cli_json_mode_runner_error_wraps_runner_failure() {
    let root = temp_workspace("cli-json-runner-error-envelope");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("missing-task")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "missing-task");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("missing-task")));
}

#[test]
fn cli_json_mode_missing_task_wraps_runner_failure() {
    let root = temp_workspace("cli-json-missing-task");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("does-not-exist")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "does-not-exist");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
}

#[test]
fn cli_help_supports_colorized_sections_when_forced() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--help")
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("EFFIGY"));
    assert!(stdout.contains("Commands"));
    assert!(stdout.contains('\u{1b}'));
}

#[test]
fn cli_general_help_mentions_json_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("--json"));
}

#[test]
fn cli_json_envelope_flag_is_rejected_after_removal() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json-envelope")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("unknown argument: --json-envelope"));
}

#[test]
fn cli_json_raw_flag_is_rejected_after_removal() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json-raw")
        .arg("tasks")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("unknown argument: --json-raw"));
}

#[test]
fn cli_help_global_json_mode_emits_machine_readable_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "help");
    assert_eq!(parsed["command"]["name"], "general");
    assert_eq!(parsed["result"]["schema"], "effigy.help.v1");
    assert_eq!(parsed["result"]["topic"], "general");
    assert!(parsed["result"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("Commands")));
}

#[test]
fn cli_help_command_json_mode_emits_machine_readable_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "help");
    assert_eq!(parsed["command"]["name"], "general");
    assert_eq!(parsed["result"]["schema"], "effigy.help.v1");
    assert_eq!(parsed["result"]["topic"], "general");
    assert!(parsed["result"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("Commands")));
}

#[test]
fn cli_tasks_supports_colorized_output_when_forced() {
    let root = temp_workspace("cli-color-tasks");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Catalogs"));
    assert!(stdout.contains('\u{1b}'));
}

#[test]
fn cli_config_global_json_mode_emits_machine_readable_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("config")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
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
    let root = temp_workspace("cli-color-task-style");
    let catalog = root.join("cattle-grid");
    fs::create_dir_all(&catalog).expect("mkdir catalog");
    fs::write(
        catalog.join("effigy.toml"),
        "[catalog]\nalias = \"cattle-grid\"\n[tasks.build]\nrun = \"tsc -p tsconfig.json {args}\"\n",
    )
    .expect("write catalog manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
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

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("\u{1b}[1m\u{1b}[37mhelp\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[38;5;244mShow general help (same as --help)\u{1b}[0m"));
}

#[test]
fn cli_tasks_text_output_has_stable_section_spacing_and_two_line_task_entries() {
    let root = temp_workspace("cli-text-spacing-shape");
    let catalog = root.join("cattle-grid");
    fs::create_dir_all(&catalog).expect("mkdir catalog");
    fs::write(
        catalog.join("effigy.toml"),
        "[catalog]\nalias = \"cattle-grid\"\n[tasks.build]\nrun = \"tsc -p tsconfig.json {args}\"\n",
    )
    .expect("write catalog manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert!(stdout.contains("\n\nCatalogs\n"));
    assert!(stdout.contains("\n\nTasks\n"));
    assert!(stdout.contains("\n\nBuilt-in Tasks\n"));
    assert!(stdout.contains(
        "- cattle-grid/build : cattle-grid/effigy.toml\n      tsc -p tsconfig.json {args}"
    ));
}

#[test]
fn cli_tasks_text_output_matches_canonical_fixture_tail() {
    let root = temp_workspace("cli-text-fixture-tail");
    let catalog = root.join("cattle-grid");
    fs::create_dir_all(&catalog).expect("mkdir catalog");
    fs::write(
        catalog.join("effigy.toml"),
        "[catalog]\nalias = \"cattle-grid\"\n[tasks.build]\nrun = \"tsc -p tsconfig.json {args}\"\n",
    )
    .expect("write catalog manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let anchor = "\nCatalogs\n────────\n";
    let start = stdout.find(anchor).expect("catalogs section anchor");
    let tail = &stdout[start + 1..];
    let expected = "\
Catalogs
────────
count: 1
- cattle-grid : cattle-grid/effigy.toml

Tasks
─────
- cattle-grid/build : cattle-grid/effigy.toml
      tsc -p tsconfig.json {args}

Built-in Tasks
──────────────
- help : Show general help (same as --help)
- config : Show supported project effigy.toml configuration keys and examples
- doctor : Built-in remedial health checks for environment, manifests, and task references
- test : Built-in test runner detection, supports <catalog>/test fallback, optional --plan
- tasks : List discovered catalogs and available tasks

";
    assert_eq!(tail, expected);
}

#[test]
fn cli_tasks_filtered_text_output_matches_canonical_fixture_tail() {
    let root = temp_workspace("cli-text-fixture-tail-filtered");
    let catalog = root.join("cattle-grid");
    fs::create_dir_all(&catalog).expect("mkdir catalog");
    fs::write(
        catalog.join("effigy.toml"),
        "[catalog]\nalias = \"cattle-grid\"\n[tasks.build]\nrun = \"tsc -p tsconfig.json {args}\"\n",
    )
    .expect("write catalog manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .arg("--task")
        .arg("build")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let anchor = "\nTask Matches: build\n───────────────────\n";
    let start = stdout.find(anchor).expect("task matches section anchor");
    let tail = &stdout[start + 1..];
    let expected = "\
Task Matches: build
───────────────────
- cattle-grid/build : cattle-grid/effigy.toml
      tsc -p tsconfig.json {args}

";
    assert_eq!(tail, expected);
}

#[test]
fn cli_tasks_filtered_text_output_managed_profiles_matches_canonical_fixture_tail() {
    let root = temp_workspace("cli-text-fixture-tail-filtered-managed");
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ task = "farmyard/api" }]

[tasks.dev.profiles.front]
concurrent = [{ task = "cream/dev" }]

[tasks.dev.profiles.admin]
concurrent = [{ task = "dairy/dev" }]
"#,
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .arg("--task")
        .arg("dev")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let anchor = "\nTask Matches: dev\n─────────────────\n";
    let start = stdout.find(anchor).expect("task matches section anchor");
    let tail = &stdout[start + 1..];
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
    assert_eq!(tail, expected);
}

#[test]
fn cli_tasks_text_output_managed_profiles_matches_canonical_fixture_tail() {
    let root = temp_workspace("cli-text-fixture-tail-managed");
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ task = "farmyard/api" }]

[tasks.dev.profiles.front]
concurrent = [{ task = "cream/dev" }]

[tasks.dev.profiles.admin]
concurrent = [{ task = "dairy/dev" }]
"#,
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let anchor = "\nCatalogs\n────────\n";
    let start = stdout.find(anchor).expect("catalogs section anchor");
    let tail = &stdout[start + 1..];
    let expected = "\
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

Built-in Tasks
──────────────
- help : Show general help (same as --help)
- config : Show supported project effigy.toml configuration keys and examples
- doctor : Built-in remedial health checks for environment, manifests, and task references
- test : Built-in test runner detection, supports <catalog>/test fallback, optional --plan
- tasks : List discovered catalogs and available tasks

";
    assert_eq!(tail, expected);
}

#[test]
fn cli_tasks_text_output_lists_managed_profiles_inline_with_tasks() {
    let root = temp_workspace("cli-text-managed-inline");
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ task = "farmyard/api" }]

[tasks.dev.profiles.front]
concurrent = [{ task = "cream/dev" }]

[tasks.dev.profiles.admin]
concurrent = [{ task = "dairy/dev" }]
"#,
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Tasks"));
    assert!(stdout.contains("- dev : effigy.toml"));
    assert!(stdout.contains("- dev front : effigy.toml"));
    assert!(stdout.contains("- dev admin : effigy.toml"));
    assert!(!stdout.contains("- dev default : effigy.toml"));
    assert!(!stdout.contains("Managed Profiles"));
}

#[test]
fn cli_tasks_resolve_text_output_matches_canonical_fixture_tail() {
    let root = temp_workspace("cli-text-fixture-tail-resolve");
    let catalog = root.join("cattle-grid");
    fs::create_dir_all(&catalog).expect("mkdir catalog");
    fs::write(
        catalog.join("effigy.toml"),
        "[catalog]\nalias = \"cattle-grid\"\n[tasks.build]\nrun = \"tsc -p tsconfig.json {args}\"\n",
    )
    .expect("write catalog manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .arg("--resolve")
        .arg("cattle-grid/build")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let anchor = "\nResolution: cattle-grid/build\n";
    let start = stdout.find(anchor).expect("resolution section anchor");
    let tail = &stdout[start + 1..];
    let expected = format!(
        "\
Resolution: cattle-grid/build
─────────────────────────────
status: ok
catalog: cattle-grid
task: build
evidence:
- selected catalog via explicit prefix `cattle-grid`

"
    );
    assert_eq!(tail, expected);
}

#[test]
fn cli_tasks_resolve_managed_profile_invocation_is_concise() {
    let root = temp_workspace("cli-text-fixture-tail-resolve-managed-profile");
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ run = "printf front-ok" }]
"#,
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .arg("--resolve")
        .arg("dev front")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let anchor = "\nResolution: dev front\n";
    let start = stdout.find(anchor).expect("resolution section anchor");
    let tail = &stdout[start + 1..];
    let expected = "\
Resolution: dev front
─────────────────────
status: ok
catalog: root
task: dev
evidence:
- selected shallowest catalog `root` by depth 0 from workspace root
- managed profile `front` resolved via invocation `dev front`

";
    assert_eq!(tail, expected);
    assert!(!stdout.contains("\nCatalogs\n"));
    assert!(!stdout.contains("\nTasks\n"));
}

#[test]
fn cli_tasks_resolve_managed_profile_missing_is_concise_with_available_profiles() {
    let root = temp_workspace("cli-text-fixture-tail-resolve-managed-profile-missing");
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ run = "printf front-ok" }]
"#,
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .arg("--resolve")
        .arg("dev missing-profile")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let anchor = "\nResolution: dev missing-profile\n";
    let start = stdout.find(anchor).expect("resolution section anchor");
    let tail = &stdout[start + 1..];
    let expected = "\
Resolution: dev missing-profile
───────────────────────────────
status: error
catalog: <none>
task: dev
• warn: managed profile `missing-profile` not found for task `dev`; available: default, front

";
    assert_eq!(tail, expected);
    assert!(!stdout.contains("\nCatalogs\n"));
    assert!(!stdout.contains("\nTasks\n"));
}

#[test]
fn cli_doctor_supports_colorized_output_when_forced() {
    let root = temp_workspace("cli-color-doctor");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.health]\nrun = \"sh -lc 'printf doctor-color; exit 4'\"\n",
    )
    .expect("write manifest");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("doctor")
        .arg("--repo")
        .arg(&root)
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let combined = format!("{stdout}\n{stderr}");
    assert!(combined.contains("health.task.execute"));
    assert!(combined.contains('\u{1b}'));
}

#[test]
fn cli_catalog_task_json_mode_renders_captured_output_payload() {
    let root = temp_workspace("cli-json-task-success");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build-ok\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("build")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "build");
    assert_eq!(parsed["result"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["result"]["task"], "build");
    assert_eq!(parsed["result"]["exit_code"], 0);
    assert_eq!(parsed["result"]["stdout"], "build-ok");
}

#[test]
fn cli_catalog_task_json_mode_failure_emits_json_and_non_zero_exit() {
    let root = temp_workspace("cli-json-task-failure");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.fail]\nrun = \"sh -lc 'printf fail-out; exit 7'\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("fail")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "fail");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert_eq!(parsed["error"]["details"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["error"]["details"]["task"], "fail");
    assert_eq!(parsed["error"]["details"]["exit_code"], 7);
    assert_eq!(parsed["error"]["details"]["stdout"], "fail-out");
}

#[test]
fn cli_test_plan_json_mode_wraps_test_plan_payload() {
    let root = temp_workspace("cli-json-test-plan-envelope");
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("test")
        .arg("--plan")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "test");
    assert_eq!(parsed["result"]["schema"], "effigy.test.plan.v1");
}

#[test]
fn cli_test_json_mode_wraps_test_failure_payload() {
    let root = temp_workspace("cli-json-test-envelope-failure");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 1\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("test")
        .arg("vitest")
        .arg("user-service")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "test");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.test.results.v1"
    );
}

#[test]
fn cli_deferral_outputs_runner_result_with_cli_preamble_header() {
    let root = temp_workspace("cli-defer-header");
    fs::write(
        root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred-runner-output\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("unknown-task")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("deferred-runner-output"));
    assert!(!stdout.contains("Task Deferral"));
    assert!(stdout.contains("EFFIGY"));
}

#[test]
fn cli_tasks_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("tasks Help"));
    assert!(
        stdout.contains("effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>]")
    );
    assert!(!stdout.contains("doctor Help"));
}

#[test]
fn cli_doctor_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("doctor")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("doctor Help"));
    assert!(stdout.contains("effigy doctor [--repo <PATH>] [--fix] [--json]"));
    assert!(!stdout.contains("tasks Help"));
}

#[test]
fn cli_repo_pulse_prints_migration_guidance() {
    let root = temp_workspace("cli-repo-pulse-migration");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("repo-pulse")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("no longer a built-in command"));
    assert!(stderr.contains("effigy doctor"));
}

fn temp_workspace(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-{name}-{ts}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}
