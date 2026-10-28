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
    assert!(!rendered.contains("effigy graph"));
    assert!(rendered.contains("effigy gateway"));
    assert!(rendered.contains("effigy config"));
    assert!(rendered.contains("effigy service"));
    assert!(rendered.contains("effigy doctor"));
    assert!(rendered.contains("effigy docs"));
    assert!(rendered.contains("effigy contracts"));
    assert!(rendered.contains("effigy container"));
    assert!(rendered.contains("effigy bootstrap"));
    assert!(rendered.contains("effigy release"));
    assert!(rendered.contains("effigy defer"));
    assert!(rendered.contains("effigy scan"));
    assert!(rendered.contains("effigy test"));
    assert!(!rendered.contains("effigy watch"));
    assert!(rendered.contains("effigy init"));
    assert!(!rendered.contains("effigy tasks migrate"));
    assert!(!rendered.contains("effigy tasks cache"));
    assert!(!rendered.contains("effigy tasks unlock"));
    assert!(rendered.contains("effigy config completion"));
    assert!(rendered.contains("supports <catalog>/test targeting"));
    assert!(!rendered.contains("effigy test --plan"));
    assert!(rendered.contains("Use `effigy <task> --help`"));
    assert!(rendered.contains("--env-schema <PATH>"));
    assert!(rendered.contains("<task> --plan"));
    assert!(rendered.contains("never a planning or safety flag"));
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
    for command in ["effigy scan", "effigy docs", "effigy contracts"] {
        assert!(rendered.contains(command), "missing {command}: {rendered}");
    }
    for foreign in [
        "effigy container",
        "effigy exec",
        "effigy release",
        "effigy deploy",
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
    assert!(rendered.contains("including a gone --repo checkout"));
    assert!(rendered.contains("effigy deps unlink bun ../poodle --repo /path/to/gone-consumer"));
    assert!(rendered.contains("--dry-run"));
    assert!(rendered.contains("--repo <PATH>"));
    assert!(rendered.contains("--json"));
}

#[test]
fn render_state_help_shows_the_moved_artifact_verbs_and_handoff_options() {
    let rendered = render_help_text(HelpTopic::State);
    assert!(rendered.contains("effigy state artifact inspect <REF|PATH>"));
    assert!(rendered.contains("effigy state artifact stage <REF|PATH>"));
    assert!(rendered.contains("effigy state artifact capture <SOURCE_PATH|DIR> --ref oci://<REF>"));
    assert!(rendered.contains("--environment <LABEL>"));
    assert!(rendered.contains("--farmyard-handoff"));
    assert!(rendered.contains("--push"));
    assert!(rendered.contains("oci://ghcr.io/acme/private-data:uat"));
    assert!(!rendered.contains("effigy artifact "), "{rendered}");
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
    assert!(!rendered.contains("effigy contracts validate-selection"));
    assert!(rendered.contains("--validate-selection"));
    assert!(rendered.contains("--index <PATH>"));
    assert!(
        !rendered.contains("--fast"),
        "there is one check, not a fast one"
    );
    assert!(
        !rendered.contains("--full"),
        "there is one check, not a fast and a full one"
    );
    assert!(rendered.contains("--changed-only <BASE>"));
    assert!(rendered.contains("--print-selected=json"));
    assert!(rendered.contains("--contract <PATH>"));
    assert!(rendered.contains("--artifact <PATH>"));
    assert!(rendered.contains("--json"));
}

#[test]
fn render_release_help_names_the_retired_commands_and_their_repo_tasks() {
    let rendered = render_help_text(HelpTopic::Release);
    assert!(rendered.contains("release Help"));
    assert!(rendered.contains("effigy release:metadata"));
    assert!(rendered.contains("effigy release:preflight"));
    assert!(rendered.contains("effigy release:check-binary"));
    assert!(rendered.contains("effigy release:proof"));
    assert!(rendered.contains("effigy release:verify:install"));
    assert!(rendered.contains("`release:evidence:validate|closeout`"));
    assert!(rendered.contains("retired in v0.15"));
    assert!(rendered.contains("effigy release receipt"));
    assert!(!rendered.contains("--binary <PATH>"));
}

#[test]
fn render_release_help_shows_status_and_run_options() {
    let rendered = render_help_text(HelpTopic::Release);
    assert!(rendered.contains("release Help"));
    assert!(rendered.contains("effigy release status"));
    assert!(rendered.contains("effigy release gates"));
    assert!(rendered.contains("effigy release run"));
    assert!(rendered.contains("effigy release resume [--repo <PATH>] <RUN>"));
    assert!(rendered.contains("effigy release receipt"));
    assert!(rendered.contains("effigy release:verify:install"));
    assert!(rendered.contains("repo tasks"));
    assert!(rendered.contains("effigy release --plan"));
    assert!(
        rendered.contains("effigy release --plan [--repo <PATH>] [--version <SEMVER>] [--json]")
    );
    assert!(rendered.contains("effigy release run [--repo <PATH>] [--to <STAGE>] [--json]"));
    assert!(rendered.contains("--to <STAGE>"));
    assert!(rendered.contains("--plan"));
    assert!(rendered.contains("--version <SEMVER>"));
    assert!(rendered.contains("--repo <PATH>"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("checkpoint at every boundary"));
    assert!(rendered.contains("reconciling an unknown tag push before any retry"));
    assert!(rendered.contains("Effigy-owned state outside the repo"));
    assert!(rendered.contains(".effigy/reports/release/gates/"));
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
    assert!(rendered.contains("List public tasks grouped by `group`"));
    assert!(rendered.contains("--all"));
    assert!(rendered.contains("visibility_reason"));
    assert!(rendered.contains("--json"));
    assert!(rendered.contains("--pretty <true|false>"));
    assert!(rendered.contains("effigy tasks --resolve <catalog>/<task>"));
    assert!(rendered.contains("effigy tasks --json --resolve test"));
    assert!(rendered.contains("`selectors` inventory"));
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
    assert!(rendered.contains("passthrough like `-p <package>` narrows to the named packages"));
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
fn render_init_help_shows_phase_scope() {
    let rendered = render_help_text(HelpTopic::Init);
    assert!(!rendered.contains("task migration"));
    assert!(rendered.contains("init Help"));
    assert!(rendered.contains("effigy init [--check|--apply|--repair] [--json]"));
    assert!(rendered.contains("effigy init --checklist [--json]"));
    assert!(rendered.contains("effigy init --apply-actions <ID>[,<ID>...] [--json]"));
    assert!(rendered.contains("effigy init <name> [--dry-run] [--force] [--json]"));
    assert!(rendered.contains("effigy init --list [--json]"));
    assert!(rendered.contains("plain `effigy init` creates missing baseline"));
    assert!(rendered.contains("does not create a project-local Effigy skill copy"));
    assert!(rendered.contains("skill.codex_project"));
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
fn render_cli_header_includes_ascii_and_root() {
    let rendered = render_cli_header_text("/tmp/repo");
    assert!(rendered.contains("╭"));
    assert!(rendered.contains("EFFIGY"));
    assert!(rendered.contains("/tmp/repo"));
    assert!(rendered.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
}
