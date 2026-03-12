use super::prelude::{render_cli_header_text, render_help_text, HelpTopic};

#[test]
fn render_help_writes_structured_sections() {
    let rendered = render_help_text(HelpTopic::General);
    assert!(rendered.contains("Commands"));
    assert!(rendered.contains("effigy help"));
    assert!(rendered.contains("effigy version"));
    assert!(rendered.contains("effigy config"));
    assert!(rendered.contains("effigy doctor"));
    assert!(rendered.contains("effigy docs"));
    assert!(rendered.contains("effigy contracts"));
    assert!(rendered.contains("effigy distribution"));
    assert!(rendered.contains("effigy release"));
    assert!(rendered.contains("effigy scan"));
    assert!(rendered.contains("effigy test"));
    assert!(rendered.contains("effigy watch"));
    assert!(rendered.contains("effigy init"));
    assert!(rendered.contains("effigy migrate"));
    assert!(rendered.contains("effigy cache"));
    assert!(rendered.contains("effigy completion"));
    assert!(rendered.contains("<catalog>/test fallback"));
    assert!(!rendered.contains("effigy test --plan"));
    assert!(rendered.contains("Use `effigy <built-in-task> --help`"));
    assert!(rendered.contains("--env-schema <PATH>"));
    assert!(rendered.contains("--version"));
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
    assert!(rendered.contains("effigy doctor frontend/build -- --watch"));
}

#[test]
fn render_docs_help_shows_validation_options() {
    let rendered = render_help_text(HelpTopic::Docs);
    assert!(rendered.contains("docs Help"));
    assert!(rendered.contains("effigy docs check-links"));
    assert!(rendered.contains("effigy docs check-json-examples"));
    assert!(rendered.contains("effigy docs check-headings"));
    assert!(rendered.contains("effigy docs check-paths"));
    assert!(rendered.contains("effigy docs check-contains"));
    assert!(rendered.contains("effigy docs check-forbidden"));
    assert!(rendered.contains("effigy docs check-index"));
    assert!(rendered.contains("effigy docs check-next-action"));
    assert!(rendered.contains("effigy docs check-workflow-paths"));
    assert!(rendered.contains("effigy docs add-log-index"));
    assert!(rendered.contains("--file <PATH>"));
    assert!(rendered.contains("--section <TITLE>"));
    assert!(rendered.contains("--min-blocks <N>"));
    assert!(rendered.contains("--require <TEXT>"));
    assert!(rendered.contains("--require-block <N:TEXT>"));
    assert!(rendered.contains("--require-heading <TEXT>"));
    assert!(rendered.contains("--forbid <TEXT>"));
    assert!(rendered.contains("--policy-index <NAME>"));
    assert!(rendered.contains("--policy <NAME>"));
    assert!(rendered.contains("--dir <PATH>"));
    assert!(rendered.contains("--index <PATH>"));
    assert!(rendered.contains("<LOG_FILE>"));
    assert!(rendered.contains("--json"));
}

#[test]
fn render_contracts_help_shows_validation_options() {
    let rendered = render_help_text(HelpTopic::Contracts);
    assert!(rendered.contains("contracts Help"));
    assert!(rendered.contains("effigy contracts check-json"));
    assert!(rendered.contains("effigy contracts validate-selection"));
    assert!(rendered.contains("--index <PATH>"));
    assert!(rendered.contains("--fast"));
    assert!(rendered.contains("--full"));
    assert!(rendered.contains("--changed-only <BASE>"));
    assert!(rendered.contains("--print-selected=json"));
    assert!(rendered.contains("--contract <PATH>"));
    assert!(rendered.contains("--artifact <PATH>"));
    assert!(rendered.contains("--json"));
}

#[test]
fn render_distribution_help_shows_validation_options() {
    let rendered = render_help_text(HelpTopic::Distribution);
    assert!(rendered.contains("distribution Help"));
    assert!(rendered.contains("effigy distribution preflight"));
    assert!(rendered.contains("effigy distribution validate-metadata"));
    assert!(rendered.contains("effigy distribution validate-artifacts"));
    assert!(rendered.contains("effigy distribution generate-closeout"));
    assert!(rendered.contains("effigy distribution write-summary"));
    assert!(rendered.contains("--tag <TAG>"));
    assert!(rendered.contains("--artifacts-dir <DIR>"));
    assert!(rendered.contains("--skip-docs"));
    assert!(rendered.contains("--skip-smoke"));
    assert!(rendered.contains("--expect-homebrew"));
    assert!(rendered.contains("--output <PATH>"));
    assert!(rendered.contains("--owner <NAME>"));
    assert!(rendered.contains("--log-file <NAME>"));
}

#[test]
fn render_release_help_shows_status_and_gate_options() {
    let rendered = render_help_text(HelpTopic::Release);
    assert!(rendered.contains("release Help"));
    assert!(rendered.contains("effigy release status"));
    assert!(rendered.contains("effigy release gates"));
    assert!(rendered.contains("effigy release resume"));
    assert!(rendered.contains("effigy release verify-install"));
    assert!(rendered.contains("effigy release simulate"));
    assert!(
        rendered.contains("effigy release simulate [--repo <PATH>] [--version <SEMVER>] [--json]")
    );
    assert!(rendered.contains("effigy release prepare [--repo <PATH>] [--check-gates]"));
    assert!(rendered.contains("effigy release prepare (--plan|--dry-run)"));
    assert!(rendered.contains("effigy release prepare --yes"));
    assert!(rendered.contains("effigy release resume [--repo <PATH>] [--allow-stale] [--json]"));
    assert!(rendered.contains("effigy release execute [--repo <PATH>] [--allow-stale]"));
    assert!(rendered.contains("effigy release execute (--plan|--dry-run)"));
    assert!(rendered.contains("effigy release execute --yes"));
    assert!(rendered.contains("--plan"));
    assert!(rendered.contains("--dry-run"));
    assert!(rendered.contains("--yes"));
    assert!(rendered.contains("--check-gates"));
    assert!(rendered.contains("--version <SEMVER>"));
    assert!(rendered.contains("--allow-stale"));
    assert!(rendered.contains("--tag <TAG>"));
    assert!(rendered.contains("--repo-url <URL>"));
    assert!(rendered.contains("--repo <PATH>"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("compact command legend"));
    assert!(rendered.contains("stale-acknowledgement state"));
    assert!(rendered.contains("selected version"));
    assert!(rendered.contains("suggested remediation actions"));
    assert!(rendered.contains("dedicated prepared-state recovery entrypoint"));
    assert!(rendered.contains("source fingerprints"));
    assert!(rendered.contains("branch drift, HEAD movement, and prepared-file content drift"));
    assert!(rendered.contains("`gates`, `reprepare`, and `discard` shortcuts"));
    assert!(!rendered.contains("simulate remain roadmap work"));
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
    assert!(rendered.contains(
        "Configured suites can declare `env`, `env_file`, `setup`, `teardown`, and `teardown_policy`"
    ));
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
    assert!(rendered.contains("[test.suites.managed]"));
    assert!(rendered.contains("env = \"managed-test\""));
    assert!(rendered.contains("env_file = [\".env\", \".env.test\"]"));
    assert!(rendered.contains("setup = [{ run = \"cargo run -p app-db --bin migrate_test_db\" }]"));
    assert!(rendered.contains("teardown = [{ run = \"cargo run -p app-db --bin reset_test_db\" }]"));
    assert!(rendered.contains("teardown_policy = \"always\""));
    assert!(rendered.contains("[test.runners]"));
    assert!(rendered.contains("vitest = \"bun x vitest run\""));
    assert!(rendered
        .contains("Use `--` when the remaining arguments belong to the underlying test runner"));
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
