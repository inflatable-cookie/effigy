use crate::ui::{KeyValue, NoticeLevel, Renderer, TableSpec, UiResult};

pub(crate) fn render_general_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("Commands")?;
    renderer.table(&TableSpec::new(
        Vec::new(),
        vec![
            vec![
                "effigy help".to_owned(),
                "Show general help (same as --help)".to_owned(),
            ],
            vec![
                "effigy version".to_owned(),
                "Print the current Effigy version (same as --version)".to_owned(),
            ],
            vec![
                "effigy tasks".to_owned(),
                "List discovered catalogs/task commands and probe routing".to_owned(),
            ],
            vec![
                "effigy config".to_owned(),
                "Show supported effigy.toml configuration keys and examples".to_owned(),
            ],
            vec![
                "effigy doctor".to_owned(),
                "Run remedial-first health checks for environment, manifests, and task references"
                    .to_owned(),
            ],
            vec![
                "effigy docs".to_owned(),
                "Run reusable docs QA checks such as markdown link, JSON example, and index validation".to_owned(),
            ],
            vec![
                "effigy contracts".to_owned(),
                "Validate reusable JSON contract artifacts such as selection payloads".to_owned(),
            ],
            vec![
                "effigy distribution".to_owned(),
                "Validate distribution metadata/artifact bundles and generate closeout evidence".to_owned(),
            ],
            vec![
                "effigy release".to_owned(),
                "Inspect release readiness from changelog, version files, and optional gates"
                    .to_owned(),
            ],
            vec![
                "effigy test".to_owned(),
                "Run built-in auto-detected tests (or explicit tasks.test); supports <catalog>/test fallback".to_owned(),
            ],
            vec![
                "effigy watch".to_owned(),
                "Watch mode phase-1 runtime with explicit owner policy and debounce/glob controls"
                    .to_owned(),
            ],
            vec![
                "effigy init".to_owned(),
                "Initialize baseline effigy.toml scaffold with safe overwrite/dry-run controls"
                    .to_owned(),
            ],
            vec![
                "effigy migrate".to_owned(),
                "Migrate package scripts into `[tasks]` with preview/apply flow".to_owned(),
            ],
            vec![
                "effigy unlock".to_owned(),
                "Manually clear lock scopes (`workspace`, `shared:*`, `task:*`, `profile:*/*`)"
                    .to_owned(),
            ],
            vec![
                "effigy cache".to_owned(),
                "Inspect/invalidate phase-1 task cache metadata (`inspect`, `invalidate`)"
                    .to_owned(),
            ],
            vec![
                "effigy completion".to_owned(),
                "Generate shell completion scripts and selector candidates".to_owned(),
            ],
            vec![
                "effigy scan".to_owned(),
                "Run built-in repository scanners such as `god-files` and `attention-markers`"
                    .to_owned(),
            ],
            vec![
                "effigy <task>".to_owned(),
                "Resolve task across discovered catalogs".to_owned(),
            ],
            vec![
                "effigy <catalog>/<task>".to_owned(),
                "Run task from explicit catalog alias".to_owned(),
            ],
        ],
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
