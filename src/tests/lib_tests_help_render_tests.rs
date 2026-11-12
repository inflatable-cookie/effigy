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
fn render_command_help_preserves_each_option_panel() {
    // These are data-only command panels: each named row keeps its own markers
    // and exclusions while sharing the rendering assertion shape.
    let cases = [
        (
            "state",
            HelpTopic::State,
            &[
                "effigy state artifact inspect <REF|PATH>",
                "effigy state artifact stage <REF|PATH>",
                "effigy state artifact capture <SOURCE_PATH|DIR> --ref oci://<REF>",
                "--environment <LABEL>",
                "--farmyard-handoff",
                "--push",
                "oci://ghcr.io/acme/private-data:uat",
            ][..],
            &["effigy artifact "][..],
        ),
        (
            "doctor",
            HelpTopic::Doctor,
            &[
                "doctor Help",
                "--fix",
                "--verbose",
                "--json",
                "effigy doctor --fix",
                "effigy doctor --verbose",
                "effigy doctor <task> <args>",
                "effigy doctor frontend/build -- --watch",
                "container.workspace-ownership",
            ][..],
            &[][..],
        ),
        (
            "contracts",
            HelpTopic::Contracts,
            &[
                "contracts Help",
                "effigy contracts check-json",
                "--validate-selection",
                "--index <PATH>",
                "--changed-only <BASE>",
                "--print-selected=json",
                "--contract <PATH>",
                "--artifact <PATH>",
                "--json",
            ][..],
            &["effigy contracts validate-selection", "--fast", "--full"][..],
        ),
        (
            "release status and run",
            HelpTopic::Release,
            &[
                "release Help",
                "effigy release status",
                "effigy release gates",
                "effigy release run",
                "effigy release resume [--repo <PATH>] <RUN>",
                "effigy release receipt",
                "effigy release:verify:install",
                "repo tasks",
                "effigy release --plan",
                "effigy release --plan [--repo <PATH>] [--version <SEMVER>] [--json]",
                "effigy release run [--repo <PATH>] [--to <STAGE>] [--json]",
                "--to <STAGE>",
                "--plan",
                "--version <SEMVER>",
                "--repo <PATH>",
                "--json",
                "checkpoint at every boundary",
                "reconciling an unknown tag push before any retry",
                "Effigy-owned state outside the repo",
                ".effigy/reports/release/gates/",
            ][..],
            &["simulate remain roadmap work"][..],
        ),
        (
            "release retired commands",
            HelpTopic::Release,
            &[
                "release Help",
                "effigy release:metadata",
                "effigy release:preflight",
                "effigy release:check-binary",
                "effigy release:proof",
                "effigy release:verify:install",
                "`release:evidence:validate|closeout`",
                "retired in v0.15",
                "effigy release receipt",
            ][..],
            &["--binary <PATH>"][..],
        ),
        (
            "defer",
            HelpTopic::Defer,
            &[
                "defer Help",
                "effigy defer <REQUEST> [args...]",
                "effigy --json defer <REQUEST> [args...]",
                "--repo <PATH>",
                "--json",
                "Use this when you want the configured `[defer]` behavior",
                "effigy defer prep",
                "effigy defer release -- --dry-run",
            ][..],
            &[][..],
        ),
        (
            "tasks",
            HelpTopic::Tasks,
            &[
                "tasks Help",
                "--resolve <SELECTOR>",
                "List public tasks grouped by `group`",
                "--all",
                "visibility_reason",
                "--json",
                "--pretty <true|false>",
                "effigy tasks --resolve <catalog>/<task>",
                "effigy tasks --json --resolve test",
                "`selectors` inventory",
            ][..],
            &[][..],
        ),
        (
            "container",
            HelpTopic::Container,
            &[
                "data pull-production",
                "effigy container web data pull-production",
            ][..],
            &[][..],
        ),
        (
            "test",
            HelpTopic::Test,
            &[
                "test Help",
                "always the built-in orchestrator",
                "`tasks.test` was removed in v0.11",
                "including `<catalog>/test` targeting",
                "Detection Order",
                "passthrough like `-p <package>` narrows to the named packages",
                "--verbose-results",
                "--tui",
                "[suite] [runner args]",
                "effigy test vitest user-service",
                "effigy <catalog>/test",
                "effigy test --plan user-service",
                "effigy test --plan viteest user-service",
                "Named Test Selection",
                "effigy test user-service",
                "prefix the suite explicitly",
                "check `available-suites` per target",
                "Configured suites can use managed run steps and declare `env`, `env_file`, `setup`, `teardown`, and `teardown_policy`",
                "suggests nearest suite names",
                "source of truth and auto-detection is skipped",
                "Migration",
                "ambiguous in multi-suite repos",
                "effigy test viteest user-service",
                "suggests `effigy test vitest user-service`",
                "effigy test nextest user_service --nocapture",
                "Error Recovery",
                "Ambiguity: `effigy test user-service`",
                "Unavailable or mistyped suite",
                "[package_manager]",
                "js = \"bun\"",
                "[test]",
                "max_parallel = 2",
                "cargo_env_match = \"prefix-aware\"",
                "[test.suites]",
                "unit = \"bun x vitest run\"",
                "[test.suites.managed]",
                "run = [{ task = \"db:test:prepare\" }, \"cargo nextest run --workspace\"]",
                "env = \"managed-test\"",
                "env_file = [\".env\", \".env.test\"]",
                "setup = [{ run = \"cargo run -p app-db --bin migrate_test_db\" }]",
                "teardown = [{ run = \"cargo run -p app-db --bin reset_test_db\" }]",
                "teardown_policy = \"always\"",
                "[test.runners]",
                "vitest = \"bun x vitest run\"",
                "Use `--` when the remaining arguments belong to the underlying test runner",
                "Task-ref chain with quoted args",
                "run = [{ task = \"test vitest \\\"user service\\\"\" }, \"printf validate-ok\"]",
                "Task-ref chain parsing is shell-like tokenization only",
            ][..],
            &["[tasks.test]"][..],
        ),
        (
            "init",
            HelpTopic::Init,
            &[
                "init Help",
                "effigy init [--check|--apply|--repair] [--json]",
                "effigy init --checklist [--json]",
                "effigy init --apply-actions <ID>[,<ID>...] [--json]",
                "effigy init <name> [--dry-run] [--force] [--json]",
                "effigy init --list [--json]",
                "plain `effigy init` creates missing baseline",
                "does not create a project-local Effigy skill copy",
                "skill.codex_project",
                "real TTYs",
                "--list",
                "--check",
                "--force",
                "--apply",
                "--checklist",
                "--apply-actions <ID>[,<ID>...]",
                "An existing root `README.md` is never overwritten",
            ][..],
            &["task migration"][..],
        ),
    ];

    for (panel, topic, required, absent) in cases {
        let rendered = render_help_text(topic);
        for marker in required {
            assert!(rendered.contains(marker), "{panel} help missing {marker:?}");
        }
        for marker in absent {
            assert!(!rendered.contains(marker), "{panel} help leaked {marker:?}");
        }
    }
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
fn render_cli_header_includes_ascii_and_root() {
    let rendered = render_cli_header_text("/tmp/repo");
    assert!(rendered.contains("╭"));
    assert!(rendered.contains("EFFIGY"));
    assert!(rendered.contains("/tmp/repo"));
    assert!(rendered.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
}
