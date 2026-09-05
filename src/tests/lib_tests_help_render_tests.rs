use super::prelude::{
    render_cli_header_text, render_help_group_text, render_help_text, HelpGroup, HelpTopic,
};

#[test]
fn render_help_writes_structured_sections() {
    let rendered = render_help_text(HelpTopic::General);
    assert!(rendered.contains("Commands"));
    assert!(rendered.contains("effigy help"));
    assert!(rendered.contains("effigy version"));
    assert!(rendered.contains("effigy exec"));
    assert!(rendered.contains("effigy deploy"));
    assert!(rendered.contains("effigy deps"));
    assert!(rendered.contains("effigy graph"));
    assert!(rendered.contains("effigy gateway"));
    assert!(rendered.contains("effigy config"));
    assert!(rendered.contains("effigy demo"));
    assert!(rendered.contains("effigy service"));
    assert!(rendered.contains("effigy doctor"));
    assert!(rendered.contains("effigy docs"));
    assert!(rendered.contains("effigy contracts"));
    assert!(rendered.contains("effigy artifact"));
    assert!(rendered.contains("effigy container"));
    assert!(rendered.contains("effigy bootstrap"));
    assert!(rendered.contains("effigy release"));
    assert!(rendered.contains("effigy defer"));
    assert!(rendered.contains("effigy scan"));
    assert!(rendered.contains("effigy test"));
    assert!(rendered.contains("effigy watch"));
    assert!(rendered.contains("effigy init"));
    assert!(rendered.contains("effigy tasks migrate"));
    assert!(rendered.contains("effigy tasks cache"));
    assert!(rendered.contains("effigy config completion"));
    assert!(rendered.contains("supports <catalog>/test targeting"));
    assert!(!rendered.contains("effigy test --plan"));
    assert!(rendered.contains("Use `effigy <built-in-task> --help`"));
    assert!(rendered.contains("--env-schema <PATH>"));
    assert!(rendered.contains("effigy <managed-task> --headless"));
    assert!(rendered.contains("EFFIGY_MANAGED_HEADLESS=1"));
    assert!(rendered.contains("logs [process] [--follow]"));
    assert!(rendered.contains("--version"));
    assert!(!rendered.contains("Quick Start"));
    assert!(!rendered.contains("effigy Help"));
}

#[test]
fn render_general_help_groups_commands_by_operator_job() {
    let rendered = render_help_text(HelpTopic::General);
    for group in HelpGroup::ALL {
        assert!(
            rendered.contains(group.title()),
            "missing {}",
            group.title()
        );
        assert!(
            rendered.contains(group.summary()),
            "missing {}",
            group.slug()
        );
    }
    assert!(rendered.contains("Use `effigy help <group>` for one group"));

    let work = rendered.find("Work Commands").expect("work section");
    let local = rendered.find("Local Commands").expect("local section");
    let repo = rendered.find("Repo Commands").expect("repo section");
    let deliver = rendered.find("Deliver Commands").expect("deliver section");
    let extend = rendered.find("Extend Commands").expect("extend section");
    let admin = rendered.find("Admin Commands").expect("admin section");
    assert!(work < local && local < repo && repo < deliver && deliver < extend && extend < admin);
}

#[test]
fn render_repo_group_help_lists_only_repository_intelligence_commands() {
    let rendered = render_help_group_text(HelpGroup::Repo);
    for command in [
        "effigy graph",
        "effigy scan",
        "effigy docs",
        "effigy contracts",
        "effigy papercuts",
    ] {
        assert!(rendered.contains(command), "missing {command}: {rendered}");
    }
    for foreign in [
        "effigy container",
        "effigy exec",
        "effigy release",
        "effigy deploy",
        "effigy artifact",
        "effigy skill",
    ] {
        assert!(!rendered.contains(foreign), "leaked {foreign}: {rendered}");
    }
    assert!(rendered.contains("never an `effigy <group> <command>` route"));
}

#[test]
fn render_group_help_covers_every_group_without_execution_grammar() {
    for group in HelpGroup::ALL {
        let rendered = render_help_group_text(*group);
        assert!(rendered.contains(group.title()));
        assert!(rendered.contains(group.summary()));
        assert!(!rendered.contains(&format!("effigy {} ", group.slug())));
    }
}

#[test]
fn render_deps_help_shows_link_and_committed_bun_pin_operations() {
    let rendered = render_help_text(HelpTopic::Deps);
    assert!(rendered.contains("deps Help"));
    assert!(rendered.contains("effigy deps status [cargo|bun]"));
    assert!(rendered.contains("effigy deps link <cargo|bun> <LIBRARY_PATH>"));
    assert!(rendered.contains("effigy deps unlink <cargo|bun> <LIBRARY_PATH>"));
    assert!(rendered.contains("effigy deps pin bun <LIBRARY_PATH>"));
    assert!(rendered.contains("effigy deps unpin bun <LIBRARY_PATH>"));
    assert!(rendered.contains("Bun pin state is committed"));
    assert!(rendered.contains("Apply or preview a verified local Cargo patch closure"));
    assert!(rendered.contains("Remove a local Cargo patch and verify committed-source recovery"));
    assert!(rendered.contains("Apply or preview one verified save-less Bun package closure"));
    assert!(rendered.contains("--dry-run"));
    assert!(rendered.contains("--repo <PATH>"));
    assert!(rendered.contains("--json"));
}

#[test]
fn render_artifact_help_shows_stage_and_handoff_options() {
    let rendered = render_help_text(HelpTopic::Artifact);
    assert!(rendered.contains("artifact Help"));
    assert!(rendered.contains("effigy artifact inspect <REF|PATH>"));
    assert!(rendered.contains("effigy artifact stage <REF|PATH>"));
    assert!(rendered.contains("effigy artifact capture <SOURCE_PATH|DIR> --ref oci://<REF>"));
    assert!(rendered.contains("--environment <LABEL>"));
    assert!(rendered.contains("--farmyard-handoff"));
    assert!(rendered.contains("--push"));
    assert!(rendered.contains("oci://ghcr.io/acme/private-data:uat"));
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
    assert!(rendered.contains("container.workspace-ownership"));
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
    assert!(rendered.contains("effigy docs check links"));
    assert!(rendered.contains("effigy docs check json-examples"));
    assert!(rendered.contains("effigy docs check headings"));
    assert!(rendered.contains("effigy docs check paths"));
    assert!(rendered.contains("effigy docs check contains"));
    assert!(rendered.contains("effigy docs check forbidden"));
    assert!(rendered.contains("effigy docs check index"));
    assert!(rendered.contains("effigy docs check next-action"));
    assert!(rendered.contains("effigy docs check workflow-paths"));
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
fn render_release_help_shows_distribution_evidence_options() {
    let rendered = render_help_text(HelpTopic::Release);
    assert!(rendered.contains("release Help"));
    assert!(rendered.contains("effigy release preflight"));
    assert!(rendered.contains("effigy release validate"));
    assert!(rendered.contains("effigy release check-binary"));
    assert!(rendered.contains("effigy release proof"));
    assert!(rendered.contains("effigy release evidence validate"));
    assert!(rendered.contains("effigy release evidence closeout"));
    assert!(rendered.contains("effigy release evidence summary"));
    assert!(rendered.contains("--tag <TAG>"));
    assert!(rendered.contains("<BIN> --glibc-floor <VER>"));
    assert!(!rendered.contains("--binary <PATH>"));
    assert!(rendered.contains("--glibc-floor <VER>"));
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
    assert!(rendered.contains("declared workspace user and HOME"));
    assert!(rendered.contains("non-console sessions run without a TTY"));
}

#[test]
fn render_deploy_help_shows_provider_package_export_surface() {
    let rendered = render_help_text(HelpTopic::Deploy);
    assert!(rendered.contains("deploy Help"));
    assert!(rendered.contains("effigy deploy model"));
    assert!(rendered.contains("effigy deploy export <PROVIDER>"));
    assert!(rendered.contains("--repo <PATH>"));
    assert!(rendered.contains("<PROVIDER>"));
    assert!(rendered.contains("--path <DIR>"));
    assert!(rendered.contains("--plan"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("provider-package"));
    assert!(rendered.contains("provider-neutral"));
}

#[test]
fn render_graph_help_shows_index_query_and_context_surface() {
    let rendered = render_help_text(HelpTopic::Graph);
    assert!(rendered.contains("graph Help"));
    assert!(rendered.contains("effigy graph index"));
    assert!(rendered.contains("effigy graph status"));
    assert!(rendered.contains("effigy graph watch"));
    assert!(rendered.contains("effigy graph search"));
    assert!(rendered.contains("effigy graph node"));
    assert!(rendered.contains("effigy graph callers"));
    assert!(rendered.contains("effigy graph callees"));
    assert!(rendered.contains("effigy graph impact"));
    assert!(rendered.contains("effigy graph context"));
    assert!(rendered.contains("effigy graph explore"));
    assert!(rendered.contains("--repo <PATH>"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("--debounce-ms <MS>"));
    assert!(rendered.contains("--refresh"));
    assert!(rendered.contains("EFFIGY_GRAPH_TIMEOUT_MS"));
    assert!(rendered.contains("--language <ID>"));
    assert!(rendered.contains("--path <PREFIX>"));
    assert!(rendered.contains("Use `graph status` only for a report-only freshness check"));
    assert!(rendered.contains("effigy.graph.watch.event.v1"));
    assert!(rendered.contains("effigy graph context \"trace deploy provider export\""));
    assert!(rendered.contains("effigy graph explore \"trace graph watch implementation\""));
    assert!(rendered.contains("graph.backup-"));
    assert!(!rendered.contains("rm -rf .effigy/graph"));
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
    assert!(rendered.contains("effigy service pack update"));
    assert!(rendered.contains("effigy service pack status"));
}

#[test]
fn render_container_help_shows_runtime_options() {
    let rendered = render_help_text(HelpTopic::Container);
    assert!(rendered.contains("container Help"));
    assert!(rendered.contains("effigy container up"));
    assert!(rendered.contains("effigy container <NAME> up"));
    assert!(rendered.contains("effigy container status --global"));
    assert!(rendered.contains("effigy container stats --global"));
    assert!(rendered.contains("effigy container volume list"));
    assert!(rendered.contains("effigy container cache list"));
    assert!(rendered.contains("effigy container cache prune"));
    assert!(rendered.contains("effigy container data list"));
    assert!(rendered.contains("effigy container data export <VOLUME> <PATH>"));
    assert!(rendered.contains("effigy container [<NAME>] data dump"));
    assert!(rendered.contains("effigy container data import <VOLUME> <PATH>"));
    assert!(rendered.contains("effigy container data seed"));
    assert!(rendered.contains("effigy container <NAME> logs"));
    assert!(rendered.contains("effigy container <NAME> shell"));
    assert!(rendered.contains("effigy container <NAME> reset"));
    assert!(rendered.contains("effigy container <NAME> eject"));
    assert!(rendered.contains("--attach"));
    assert!(rendered.contains("--detach"));
    assert!(rendered.contains("--global"));
    assert!(rendered.contains("--orphans"));
    assert!(rendered.contains("--service <NAME>"));
    assert!(rendered.contains("--command <CMD>"));
    assert!(rendered.contains("--follow"));
    assert!(rendered.contains("--keep-data"));
    assert!(rendered.contains("--db-dump <FILE>|<TARGET>|<TARGET>=<FILE>"));
    assert!(rendered.contains("--db-seed <FILE|OCI>|<TARGET>=<FILE|OCI>"));
    assert!(rendered.contains("--no-prompt"));
    assert!(rendered.contains("effigy container web data list"));
    assert!(rendered.contains("effigy container volume list --global --orphans"));
    assert!(rendered.contains("effigy container volume prune --dormant --yes"));
    assert!(rendered.contains("effigy container volume prune --global --orphans --yes"));
    assert!(rendered.contains("effigy container cache list --global"));
    assert!(rendered.contains("effigy container cache list --project acowtancy-dev"));
    assert!(rendered.contains("effigy container cache list --kind rust-target"));
    assert!(rendered.contains("effigy container cache prune --project acowtancy-dev --yes"));
    assert!(rendered.contains("effigy container cache prune --global --yes"));
    assert!(rendered.contains("--project <NAME>"));
    assert!(rendered.contains("--kind <KIND>"));
    assert!(rendered.contains("effigy container web data export"));
    assert!(rendered.contains("effigy container web data import"));
    assert!(rendered.contains("effigy container data dump legacy_mysql"));
    assert!(rendered.contains("effigy container data dump --db-dump legacy_mysql"));
    assert!(rendered.contains("effigy container data dump --db-dump ./latest.sql"));
    assert!(rendered.contains("effigy container data dump --db-dump app=oci://"));
    assert!(rendered.contains("--push"));
    assert!(rendered.contains("effigy container data seed --db-seed ./latest.sql"));
    assert!(rendered
        .contains("effigy container data seed --db-seed app=oci://ghcr.io/acme/private-data:uat"));
    assert!(rendered.contains("effigy container web reset --keep-data"));
    assert!(rendered.contains(
        "interactive workspace/shell exits now ask whether to bring the environment down"
    ));
}

#[test]
fn render_bootstrap_help_shows_planning_options() {
    let rendered = render_help_text(HelpTopic::Bootstrap);
    assert!(rendered.contains("bootstrap Help"));
    assert!(rendered.contains("effigy bootstrap <GIT_URL>"));
    assert!(rendered.contains("effigy bootstrap teardown [--yes] [--json]"));
    assert!(rendered.contains("--path <DIR>"));
    assert!(rendered.contains("--branch <NAME>"));
    assert!(rendered.contains("--backend <containerd|docker>"));
    assert!(rendered.contains("--fresh"));
    assert!(rendered.contains("--no-prompt"));
    assert!(rendered.contains("--reuse-path"));
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
    assert!(rendered.contains("Effigy's self-hosting tagged-binary check"));
    assert!(rendered.contains("repo-owned consumer smoke"));
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
    assert!(rendered.contains(".effigy/reports/release/gates/"));
    assert!(rendered.contains("last 20 lines"));
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
    assert!(rendered.contains("always the built-in orchestrator"));
    assert!(rendered.contains("`tasks.test` was removed in v0.11"));
    assert!(rendered.contains("including `<catalog>/test` targeting"));
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
        "Configured suites can use managed run steps and declare `env`, `env_file`, `setup`, `teardown`, and `teardown_policy`"
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
    assert!(rendered
        .contains("run = [{ task = \"db:test:prepare\" }, \"cargo nextest run --workspace\"]"));
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
    assert!(rendered.contains("effigy init [--check|--apply|--repair] [--json]"));
    assert!(rendered.contains("effigy init --checklist [--json]"));
    assert!(rendered.contains("effigy init --apply-actions <ID>[,<ID>...] [--json]"));
    assert!(rendered.contains("effigy init <name> [--dry-run] [--force] [--json]"));
    assert!(rendered.contains("effigy init --list [--json]"));
    assert!(rendered.contains("plain `effigy init` creates missing baseline"));
    assert!(rendered.contains("real TTYs"));
    assert!(rendered.contains("--list"));
    assert!(rendered.contains("--check"));
    assert!(rendered.contains("--force"));
    assert!(rendered.contains("--apply"));
    assert!(rendered.contains("--checklist"));
    assert!(rendered.contains("--apply-actions <ID>[,<ID>...]"));
    assert!(rendered.contains("An existing root `README.md` is never overwritten"));
}

#[test]
fn render_migrate_help_shows_phase_scope() {
    let rendered = render_help_text(HelpTopic::Migrate);
    assert!(rendered.contains("tasks migrate Help"));
    assert!(rendered
        .contains("effigy tasks migrate [--from <PATH>] [--script <NAME>]... [--apply] [--json]"));
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
