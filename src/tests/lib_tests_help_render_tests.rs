use super::prelude::{render_cli_header_text, render_help_text, HelpTopic};

#[test]
fn render_help_writes_structured_sections() {
    let rendered = render_help_text(HelpTopic::General);
    assert!(rendered.contains("Commands"));
    assert!(rendered.contains("effigy help"));
    assert!(rendered.contains("effigy version"));
    assert!(rendered.contains("effigy exec"));
    assert!(rendered.contains("effigy deploy"));
    assert!(rendered.contains("effigy gateway"));
    assert!(rendered.contains("effigy config"));
    assert!(rendered.contains("effigy demo"));
    assert!(rendered.contains("effigy service"));
    assert!(rendered.contains("effigy doctor"));
    assert!(rendered.contains("effigy docs"));
    assert!(rendered.contains("effigy contracts"));
    assert!(rendered.contains("effigy distribution"));
    assert!(rendered.contains("effigy container"));
    assert!(rendered.contains("effigy bootstrap"));
    assert!(rendered.contains("effigy release"));
    assert!(rendered.contains("effigy defer"));
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
fn render_demo_help_shows_discovery_and_inspection_options() {
    let rendered = render_help_text(HelpTopic::Demo);
    assert!(rendered.contains("demo Help"));
    assert!(rendered.contains("effigy demo browser"));
    assert!(rendered.contains("effigy demo list"));
    assert!(rendered.contains("effigy demo inspect <DEMO_ID>"));
    assert!(rendered.contains("effigy demo run <DEMO_ID>"));
    assert!(rendered.contains("effigy demo stop <DEMO_ID>"));
    assert!(rendered.contains("effigy demo rerun <DEMO_ID>"));
    assert!(rendered.contains("--search <TEXT>"));
    assert!(rendered.contains("--owner <NAME>"));
    assert!(rendered.contains("--tag <TAG>"));
    assert!(rendered.contains("--mode <MODE>"));
    assert!(rendered.contains("--cover <AREA>"));
    assert!(rendered.contains("--status <STATUS>"));
    assert!(rendered.contains("--gap <GAP>"));
    assert!(rendered.contains("--stale-only"));
    assert!(rendered.contains("--group-by <FIELD>"));
    assert!(rendered.contains("--repo <PATH>"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("record a new normalized attempt"));
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
    assert!(rendered.contains("effigy distribution check-glibc-floor"));
    assert!(rendered.contains("effigy distribution first-publish"));
    assert!(rendered.contains("effigy distribution validate-artifacts"));
    assert!(rendered.contains("effigy distribution generate-closeout"));
    assert!(rendered.contains("effigy distribution write-summary"));
    assert!(rendered.contains("--tag <TAG>"));
    assert!(rendered.contains("--binary <PATH>"));
    assert!(rendered.contains("--max-glibc <VER>"));
    assert!(rendered.contains("--artifacts-dir <DIR>"));
    assert!(rendered.contains("--skip-homebrew"));
    assert!(rendered.contains("--skip-docs"));
    assert!(rendered.contains("--skip-smoke"));
    assert!(rendered.contains("--expect-homebrew"));
    assert!(rendered.contains("--output <PATH>"));
    assert!(rendered.contains("--owner <NAME>"));
    assert!(rendered.contains("--log-file <NAME>"));
}

#[test]
fn render_exec_help_shows_service_and_examples() {
    let rendered = render_help_text(HelpTopic::Exec);
    assert!(rendered.contains("exec Help"));
    assert!(rendered.contains("effigy exec [--repo <PATH>] [--service <NAME>]"));
    assert!(rendered.contains("effigy exec composer install"));
    assert!(rendered.contains("[containers.<name>.aliases]"));
}

#[test]
fn render_deploy_help_shows_underlay_json_first_batch() {
    let rendered = render_help_text(HelpTopic::Deploy);
    assert!(rendered.contains("deploy Help"));
    assert!(rendered.contains("effigy deploy model"));
    assert!(rendered.contains("effigy deploy export render"));
    assert!(rendered.contains("--repo <PATH>"));
    assert!(rendered.contains("--path <DIR>"));
    assert!(rendered.contains("--plan"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("Underlay"));
    assert!(rendered.contains("provider-neutral"));
}

#[test]
fn render_gateway_help_shows_lifecycle_examples() {
    let rendered = render_help_text(HelpTopic::Gateway);
    assert!(rendered.contains("gateway Help"));
    assert!(rendered.contains("effigy gateway up"));
    assert!(rendered.contains("effigy gateway down"));
    assert!(rendered.contains("effigy gateway status"));
    assert!(rendered.contains("effigy gateway setup-tls"));
    assert!(rendered.contains("/etc/resolver/test"));
}

#[test]
fn render_service_help_shows_extract_options() {
    let rendered = render_help_text(HelpTopic::Service);
    assert!(rendered.contains("service Help"));
    assert!(rendered.contains("effigy service list"));
    assert!(rendered.contains("effigy service extract <SERVICE>"));
    assert!(rendered.contains("--dir <PATH>"));
    assert!(rendered.contains("project-local"));
}

#[test]
fn render_container_help_shows_runtime_options() {
    let rendered = render_help_text(HelpTopic::Container);
    assert!(rendered.contains("container Help"));
    assert!(rendered.contains("effigy container up"));
    assert!(rendered.contains("effigy container <NAME> up"));
    assert!(rendered.contains("effigy container status --all"));
    assert!(rendered.contains("effigy container stats --all"));
    assert!(rendered.contains("effigy container data list"));
    assert!(rendered.contains("effigy container data export <VOLUME> <PATH>"));
    assert!(rendered.contains("effigy container data import <VOLUME> <PATH>"));
    assert!(rendered.contains("effigy container <NAME> logs"));
    assert!(rendered.contains("effigy container <NAME> shell"));
    assert!(rendered.contains("effigy container <NAME> reset"));
    assert!(rendered.contains("effigy container <NAME> eject"));
    assert!(rendered.contains("--attach"));
    assert!(rendered.contains("--detach"));
    assert!(rendered.contains("--all"));
    assert!(rendered.contains("--service <NAME>"));
    assert!(rendered.contains("--command <CMD>"));
    assert!(rendered.contains("--follow"));
    assert!(rendered.contains("--keep-data"));
    assert!(rendered.contains("effigy container web data list"));
    assert!(rendered.contains("effigy container web data export"));
    assert!(rendered.contains("effigy container web data import"));
    assert!(rendered.contains("effigy container web reset --keep-data"));
    assert!(rendered.contains("attached sessions shut the environment down on owner exit"));
}

#[test]
fn render_bootstrap_help_shows_planning_options() {
    let rendered = render_help_text(HelpTopic::Bootstrap);
    assert!(rendered.contains("bootstrap Help"));
    assert!(rendered.contains("effigy bootstrap <GIT_URL>"));
    assert!(rendered.contains("--path <DIR>"));
    assert!(rendered.contains("--branch <NAME>"));
    assert!(rendered.contains("--start"));
    assert!(rendered.contains("--plan"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("child repo checkout"));
    assert!(rendered.contains("Phase 1"));
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
fn render_defer_help_shows_container_and_repo_options() {
    let rendered = render_help_text(HelpTopic::Defer);
    assert!(rendered.contains("defer Help"));
    assert!(rendered.contains("effigy defer <REQUEST> [args...]"));
    assert!(rendered.contains("effigy --json defer <REQUEST> [args...]"));
    assert!(rendered.contains("--repo <PATH>"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("Use this when you want the configured `[defer]` behavior"));
    assert!(rendered.contains("effigy defer prep"));
    assert!(rendered.contains("effigy defer release -- --dry-run"));
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
fn render_container_help_shows_pull_production_surface() {
    let rendered = render_help_text(HelpTopic::Container);
    assert!(rendered.contains("data pull-production"));
    assert!(rendered.contains("effigy container web data pull-production"));
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
    assert!(rendered.contains("effigy init [<name>] [--dry-run] [--force] [--json]"));
    assert!(rendered.contains("effigy init --list [--json]"));
    assert!(rendered.contains("default `minimal` starter"));
    assert!(rendered.contains("--list"));
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
