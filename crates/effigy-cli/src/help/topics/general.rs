use std::collections::BTreeSet;

use super::super::{HelpRenderer, HelpResult, KeyValue, NoticeLevel, TableSpec};

pub(crate) fn render_general_help<R: HelpRenderer>(
    renderer: &mut R,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    let commands = vec![
        ("effigy help", "Show general help (same as --help)", None),
        (
            "effigy version",
            "Print the current Effigy version (same as --version)",
            None,
        ),
        (
            "effigy tasks",
            "List discovered catalogs/task commands and probe routing",
            Some("tasks"),
        ),
        (
            "effigy service",
            "Inspect the layered service catalog and extract bundled fragments for override ownership",
            Some("service"),
        ),
        (
            "effigy exec",
            "Run one ad-hoc command inside the manifest's dev-context container",
            None,
        ),
        (
            "effigy system",
            "Operate the manifest default system substrate through its default workspace container",
            None,
        ),
        (
            "effigy workspace",
            "Ensure the selected system is up, then open the resolved workspace shell",
            None,
        ),
        (
            "effigy gateway",
            "Operate the host-native local DNS and reverse-proxy gateway",
            None,
        ),
        (
            "effigy config",
            "Show config keys/examples or inspect the effective composed manifest and focused path sources",
            None,
        ),
        (
            "effigy demo",
            "List declared demos and inspect the latest known proof state without starting execution",
            None,
        ),
        (
            "effigy doctor",
            "Run remedial-first health checks for environment, manifests, and task references",
            Some("doctor"),
        ),
        (
            "effigy docs",
            "Run reusable docs QA checks such as markdown link, JSON example, and index validation",
            Some("docs"),
        ),
        (
            "effigy contracts",
            "Validate reusable JSON contract artifacts such as selection payloads",
            Some("contracts"),
        ),
        (
            "effigy distribution",
            "Validate distribution metadata/artifact bundles and generate closeout evidence",
            Some("distribution"),
        ),
        (
            "effigy container",
            "Operate manifest-defined Colima-backed container environments",
            Some("container"),
        ),
        (
            "effigy bootstrap",
            "Clone/update a repo from a git URL and apply its repo-owned `[bootstrap]` contract",
            Some("bootstrap"),
        ),
        (
            "effigy release",
            "Inspect release readiness from changelog, version files, and optional gates",
            Some("release"),
        ),
        (
            "effigy test",
            "Run built-in auto-detected tests (or explicit tasks.test); supports <catalog>/test fallback",
            None,
        ),
        (
            "effigy watch",
            "Watch mode phase-1 runtime with explicit owner policy and debounce/glob controls",
            None,
        ),
        (
            "effigy init",
            "Initialize baseline effigy.toml scaffold with safe overwrite/dry-run controls",
            None,
        ),
        (
            "effigy migrate",
            "Migrate package scripts into `[tasks]` with preview/apply flow",
            None,
        ),
        (
            "effigy unlock",
            "Manually clear lock scopes (`workspace`, `shared:*`, `task:*`, `profile:*/*`)",
            None,
        ),
        (
            "effigy cache",
            "Inspect/invalidate phase-1 task cache metadata (`inspect`, `invalidate`)",
            None,
        ),
        (
            "effigy completion",
            "Generate shell completion scripts and selector candidates",
            None,
        ),
        (
            "effigy scan",
            "Run built-in repository scanners such as `god-files` and `attention-markers`",
            None,
        ),
        ("effigy <task>", "Resolve task across discovered catalogs", None),
        (
            "effigy <catalog>/<task>",
            "Run task from explicit catalog alias",
            None,
        ),
    ];
    renderer.section("Commands")?;
    renderer.table(&TableSpec::new(
        Vec::new(),
        commands
            .into_iter()
            .filter(|(_, _, builtin)| builtin.is_none_or(|name| !deferred_builtins.contains(name)))
            .map(|(command, description, _)| vec![command.to_owned(), description.to_owned()])
            .collect::<Vec<Vec<String>>>(),
    ))?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy <built-in-task> --help` for task-specific flags and examples.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Generic task invocations also accept `--repo <PATH>`, `--verbose-root`, and `--env-schema <PATH>` before passthrough arguments.",
    )?;
    renderer.key_values(&[
        KeyValue::new("-h, --help", "Print this help panel"),
        KeyValue::new("--version", "Print the current Effigy version"),
        KeyValue::new("--json", "Render command-envelope JSON for CI/tooling"),
    ])?;
    Ok(())
}
