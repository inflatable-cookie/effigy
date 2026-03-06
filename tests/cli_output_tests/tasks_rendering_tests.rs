use serde_json::Value;
use std::fs;
use std::process::Command;

use super::support::temp_workspace;

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
- watch : Watch mode phase-1 runtime with owner policy, debounce, and include/exclude globs
- init : Initialize baseline effigy.toml scaffold with dry-run/force controls
- migrate : Migrate package scripts into [tasks] with preview/apply flow
- unlock : Manually clear lock scopes (`workspace`, `task:*`, `profile:*/*`)
- cache : Inspect and invalidate phase-1 task cache metadata (`inspect`, `invalidate`)
- completion : Generate shell completion scripts (`bash`, `zsh`, `fish`)
- scan : Run built-in repository scanners such as `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-in-src`, `attention-markers`, and `stale-suppressions`

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
- watch : Watch mode phase-1 runtime with owner policy, debounce, and include/exclude globs
- init : Initialize baseline effigy.toml scaffold with dry-run/force controls
- migrate : Migrate package scripts into [tasks] with preview/apply flow
- unlock : Manually clear lock scopes (`workspace`, `task:*`, `profile:*/*`)
- cache : Inspect and invalidate phase-1 task cache metadata (`inspect`, `invalidate`)
- completion : Generate shell completion scripts (`bash`, `zsh`, `fish`)
- scan : Run built-in repository scanners such as `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-in-src`, `attention-markers`, and `stale-suppressions`

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
    let expected = "\
Resolution: cattle-grid/build
─────────────────────────────
status: ok
catalog: cattle-grid
task: build
lock_scopes: workspace, task:build
evidence:
- selected catalog via explicit prefix `cattle-grid`

"
    .to_string();
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
lock_scopes: workspace, task:dev, profile:dev/front
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
lock_scopes: workspace, task:dev, profile:dev/missing-profile
• warn: managed profile `missing-profile` not found for task `dev`; available: default, front

";
    assert_eq!(tail, expected);
    assert!(!stdout.contains("\nCatalogs\n"));
    assert!(!stdout.contains("\nTasks\n"));
}
