use super::prelude::*;

#[test]
fn render_help_writes_structured_sections() {
    let rendered = render_help_text(HelpTopic::General);
    assert!(rendered.contains("Commands"));
    assert!(rendered.contains("effigy help"));
    assert!(rendered.contains("effigy config"));
    assert!(rendered.contains("effigy doctor"));
    assert!(rendered.contains("effigy test"));
    assert!(rendered.contains("effigy watch"));
    assert!(rendered.contains("effigy init"));
    assert!(rendered.contains("effigy migrate"));
    assert!(rendered.contains("effigy cache"));
    assert!(rendered.contains("effigy completion"));
    assert!(rendered.contains("<catalog>/test fallback"));
    assert!(!rendered.contains("effigy test --plan"));
    assert!(rendered.contains("Use `effigy <built-in-task> --help`"));
    assert!(!rendered.contains("Quick Start"));
    assert!(!rendered.contains("effigy Help"));
}

#[test]
fn render_doctor_help_shows_fix_and_json_options() {
    let rendered = render_help_text(HelpTopic::Doctor);
    assert!(rendered.contains("doctor Help"));
    assert!(rendered.contains("--fix"));
    assert!(rendered.contains("--verbose"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("effigy doctor --fix"));
    assert!(rendered.contains("effigy doctor --verbose"));
    assert!(rendered.contains("effigy doctor <task> <args>"));
    assert!(rendered.contains("effigy doctor farmyard/build -- --watch"));
}

#[test]
fn render_tasks_help_shows_resolve_and_json_options() {
    let rendered = render_help_text(HelpTopic::Tasks);
    assert!(rendered.contains("tasks Help"));
    assert!(rendered.contains("--resolve <SELECTOR>"));
    assert!(rendered.contains("routing probes only when debugging selector resolution"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("--pretty <true|false>"));
    assert!(rendered.contains("effigy tasks --resolve <catalog>/<task>"));
    assert!(rendered.contains("effigy tasks --json --resolve test"));
}

#[test]
fn render_test_help_shows_detection_and_config() {
    let rendered = render_help_text(HelpTopic::Test);
    assert!(rendered.contains("test Help"));
    assert!(rendered.contains("built-in test runner detection by default"));
    assert!(rendered.contains("`tasks.test` is defined, it takes precedence"));
    assert!(rendered.contains("<catalog>/test fallback"));
    assert!(rendered.contains("Detection Order"));
    assert!(rendered.contains("--verbose-results"));
    assert!(rendered.contains("--tui"));
    assert!(rendered.contains("[suite] [runner args]"));
    assert!(rendered.contains("effigy test vitest user-service"));
    assert!(rendered.contains("effigy <catalog>/test"));
    assert!(rendered.contains("effigy test --plan user-service"));
    assert!(rendered.contains("effigy test --plan viteest user-service"));
    assert!(rendered.contains("Named Test Selection"));
    assert!(rendered.contains("effigy test user-service"));
    assert!(rendered.contains("prefix the suite explicitly"));
    assert!(rendered.contains("check `available-suites` per target"));
    assert!(rendered.contains("suggests nearest suite names"));
    assert!(rendered.contains("source of truth and auto-detection is skipped"));
    assert!(rendered.contains("Migration"));
    assert!(rendered.contains("ambiguous in multi-suite repos"));
    assert!(rendered.contains("effigy test viteest user-service"));
    assert!(rendered.contains("suggests `effigy test vitest user-service`"));
    assert!(rendered.contains("effigy test nextest user_service --nocapture"));
    assert!(rendered.contains("Error Recovery"));
    assert!(rendered.contains("Ambiguity: `effigy test user-service`"));
    assert!(rendered.contains("Unavailable or mistyped suite"));
    assert!(rendered.contains("[package_manager]"));
    assert!(rendered.contains("js = \"bun\""));
    assert!(rendered.contains("[test]"));
    assert!(rendered.contains("max_parallel = 2"));
    assert!(rendered.contains("cargo_env_match = \"prefix-aware\""));
    assert!(rendered.contains("[test.suites]"));
    assert!(rendered.contains("unit = \"bun x vitest run\""));
    assert!(rendered.contains("[test.runners]"));
    assert!(rendered.contains("vitest = \"bun x vitest run\""));
    assert!(!rendered.contains("[tasks.test]"));
    assert!(rendered.contains("Task-ref chain with quoted args"));
    assert!(rendered.contains(
        "run = [{ task = \"test vitest \\\"user service\\\"\" }, \"printf validate-ok\"]"
    ));
    assert!(rendered.contains("Task-ref chain parsing is shell-like tokenization only"));
}

#[test]
fn render_watch_help_shows_phase_scope() {
    let rendered = render_help_text(HelpTopic::Watch);
    assert!(rendered.contains("watch Help"));
    assert!(rendered.contains("--owner <effigy|external>"));
    assert!(rendered.contains("--debounce-ms <MS>"));
    assert!(rendered.contains("file-triggered reruns for non-watcher tasks"));
}

#[test]
fn render_init_help_shows_phase_scope() {
    let rendered = render_help_text(HelpTopic::Init);
    assert!(rendered.contains("init Help"));
    assert!(rendered.contains("effigy init [--dry-run] [--force] [--json]"));
    assert!(rendered.contains("generate minimal valid effigy.toml"));
    assert!(rendered.contains("--dry-run"));
    assert!(rendered.contains("--force"));
}

#[test]
fn render_migrate_help_shows_phase_scope() {
    let rendered = render_help_text(HelpTopic::Migrate);
    assert!(rendered.contains("migrate Help"));
    assert!(
        rendered.contains("effigy migrate [--from <PATH>] [--script <NAME>]... [--apply] [--json]")
    );
    assert!(rendered.contains("import package.json scripts only"));
    assert!(rendered.contains("--apply"));
    assert!(rendered.contains("--script <NAME>"));
}

#[test]
fn render_cli_header_includes_ascii_and_root() {
    let rendered = render_cli_header_text("/tmp/repo");
    assert!(rendered.contains("╭"));
    assert!(rendered.contains("EFFIGY"));
    assert!(rendered.contains("/tmp/repo"));
    assert!(rendered.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
}
