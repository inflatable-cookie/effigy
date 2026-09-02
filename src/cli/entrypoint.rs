use crate::{
    command_kind_and_name, emit_json_envelope_error, emit_json_envelope_error_with_warnings,
    emit_json_envelope_success_with_warnings, parse_error_json_details, parse_json_or_string,
    render_cli_header, render_parse_error, run_graph_watch_command, run_help_command,
    run_help_group_command, CliExecutionContext,
};
use effigy_cli::{
    apply_global_cli_flags, command_requests_json, parse_command,
    runtime_flag_present_before_passthrough, strip_global_cli_flags, Command, GlobalCliOptions,
    GraphSubcommand,
};
use effigy_context::EffigyRuntimeContext;
use effigy_core::widgets::MessageBlock;
use effigy_ui::{OutputMode, PlainRenderer, Renderer};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

pub fn run_cli(raw_args: Vec<String>) {
    let requested_root_json = runtime_flag_present_before_passthrough(&raw_args, "--json");
    let (args, global_options) = match strip_global_cli_flags(raw_args) {
        Ok(parsed) => parsed,
        Err(err) => {
            let output_mode = OutputMode::from_env();
            if requested_root_json {
                emit_json_envelope_error(
                    2,
                    "cli",
                    "parse",
                    "CliParseError",
                    &err.to_string(),
                    Some(parse_error_json_details()),
                );
            }
            let mut renderer = PlainRenderer::stderr(output_mode);
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let resolved_root = effigy_core::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            let _ = render_parse_error(&mut renderer, &resolved_root, &err.to_string());
            std::process::exit(2);
        }
    };
    let global_json_mode = global_options.json_mode;
    // Tell the catalog layer how to render its baseline-fallback notice before
    // anything can resolve a catalog fragment.
    effigy_catalog::pack::set_diagnostic_mode(if global_json_mode {
        effigy_catalog::pack::DiagnosticMode::Json
    } else {
        effigy_catalog::pack::DiagnosticMode::Text
    });
    let output_mode = OutputMode::from_env();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let first_word = args.first().cloned();
    let parsed = parse_command_with_builtin_deferral(args, &cwd, &global_options);
    // Displaced direct built-ins warn only after routing proved the built-in
    // owns the invocation; grouped routes and manifest-routed tasks never do.
    let legacy_direct_warning = first_word.as_deref().and_then(|word| {
        crate::cli::legacy_direct::direct_warning_for_parse(word, &parsed)
    });
    let parsed = match parsed {
        Ok(cmd) => cmd,
        Err(err) => {
            let warning_values =
                crate::cli::legacy_direct::warning_values(legacy_direct_warning.as_ref());
            if global_json_mode {
                emit_json_envelope_error_with_warnings(
                    2,
                    "cli",
                    "parse",
                    "CliParseError",
                    &err.to_string(),
                    Some(parse_error_json_details()),
                    &warning_values,
                );
            }
            crate::cli::legacy_direct::print_human_warnings_option(
                legacy_direct_warning.as_ref(),
                global_json_mode,
            );
            let mut renderer = PlainRenderer::stderr(output_mode);
            let resolved_root = effigy_core::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            let _ = render_parse_error(&mut renderer, &resolved_root, &err.to_string());
            std::process::exit(2);
        }
    };
    let cmd = match apply_global_cli_flags(parsed, &global_options) {
        Ok(cmd) => cmd,
        Err(err) => {
            let warning_values =
                crate::cli::legacy_direct::warning_values(legacy_direct_warning.as_ref());
            if global_json_mode {
                emit_json_envelope_error_with_warnings(
                    2,
                    "cli",
                    "parse",
                    "CliParseError",
                    &err.to_string(),
                    Some(parse_error_json_details()),
                    &warning_values,
                );
            }
            crate::cli::legacy_direct::print_human_warnings_option(
                legacy_direct_warning.as_ref(),
                global_json_mode,
            );
            let mut renderer = PlainRenderer::stderr(output_mode);
            let resolved_root = effigy_core::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            let _ = render_parse_error(&mut renderer, &resolved_root, &err.to_string());
            std::process::exit(2);
        }
    };
    let internal_suppress_header = std::env::var("EFFIGY_INTERNAL_SUPPRESS_HEADER")
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"));
    let suppress_header = internal_suppress_header || command_requests_json(&cmd, global_json_mode);
    let emit_json_envelope = !internal_suppress_header && suppress_header;
    let (command_kind, command_name) = command_kind_and_name(&cmd);
    let runtime_context = match EffigyRuntimeContext::capture_lossy(
        Some(cwd.clone()),
        crate::runner::command_repo_override_for_context(&cmd),
    ) {
        Ok(context) => context,
        Err(error) => {
            let message =
                format!("failed to capture runtime context after cwd was resolved: {error}");
            let warning_values =
                crate::cli::legacy_direct::warning_values(legacy_direct_warning.as_ref());
            if emit_json_envelope {
                emit_json_envelope_error_with_warnings(
                    1,
                    command_kind,
                    &command_name,
                    "RuntimeContextError",
                    &message,
                    None,
                    &warning_values,
                );
            }
            crate::cli::legacy_direct::print_human_warnings_option(
                legacy_direct_warning.as_ref(),
                emit_json_envelope,
            );
            let mut renderer = PlainRenderer::stderr(output_mode);
            let _ = renderer.error_block(&MessageBlock::new("Task failed", message));
            std::process::exit(1);
        }
    };
    let command_root = runtime_context.command_root().to_path_buf();
    let context = CliExecutionContext {
        output_mode,
        runtime_context: &runtime_context,
        command_root: &command_root,
        suppress_header,
        emit_json_envelope,
        command_kind,
        command_name: &command_name,
    };

    match cmd {
        Command::Version => {
            crate::run_version_command(&context, legacy_direct_warning.as_ref())
        }
        Command::Help(topic) => {
            let legacy_note =
                crate::cli::legacy_direct::legacy_help_note(first_word.as_deref(), topic);
            run_help_command(&context, topic, legacy_note.as_deref())
        }
        Command::HelpGroup(group) => run_help_group_command(&context, group),
        Command::Graph(
            args @ effigy_cli::GraphArgs {
                subcommand: GraphSubcommand::Watch { .. },
                ..
            },
        ) => run_graph_watch_command(&context, args),
        command @ (Command::Bundle(_)
        | Command::Changelog(_)
        | Command::Deploy(_)
        | Command::Deps(_)
        | Command::Papercuts(_)
        | Command::Defer(_)
        | Command::Exec(_)
        | Command::Secrets(_)
        | Command::State(_)
        | Command::System(_)
        | Command::Workspace(_)
        | Command::Gateway(_)
        | Command::Service(_)
        | Command::Demo(_)
        | Command::Graph(_)
        | Command::Rhai(_)
        | Command::Skill(_)
        | Command::Docs(_)
        | Command::Contracts(_)
        | Command::Artifact(_)
        | Command::Container(_)
        | Command::Bootstrap(_)
        | Command::Uninstall(_)
        | Command::Release(_)
        | Command::Doctor(_)
        | Command::Tasks(_)
        | Command::InternalGateway(_)
        | Command::InternalScriptRun(_)
        | Command::InternalContainerLeaseReaper(_)
        | Command::InternalHostProcessSupervise(_)
        | Command::InternalHostProcessStop(_)
        | Command::Task(_)
        | Command::GroupedBuiltin(_)) => {
            run_and_render_command(&context, command, legacy_direct_warning.as_ref())
        }
    }
}

pub fn run_and_render_command(
    context: &CliExecutionContext<'_>,
    command: Command,
    legacy_direct_warning: Option<&crate::cli::legacy_direct::LegacyDirectWarning>,
) {
    // `config` and `scan` prove built-in ownership at the runner's manifest
    // selection fallback; open a recording scope around the run so those
    // direct invocations warn exactly when the built-in was selected.
    let opens_registry_scope = matches!(
        &command,
        Command::Task(task)
            if effigy_cli::command_surface::group_for_child_word(&task.name).is_some()
    );
    if opens_registry_scope {
        crate::cli::legacy_direct::open_registry_scope();
    }
    let mut renderer = PlainRenderer::stdout(context.output_mode);
    if !context.suppress_header {
        let _ = render_cli_header(&mut renderer, context.command_root);
    }
    let spinner = if should_show_transient_spinner(context, &command) {
        renderer.spinner(transient_spinner_label(&command)).ok()
    } else {
        None
    };
    let outcome = crate::runner::run_command_with_context(command, context.runtime_context);
    let registry_warnings = if opens_registry_scope {
        crate::cli::legacy_direct::close_registry_scope()
    } else {
        Vec::new()
    };
    let mut warnings = Vec::with_capacity(registry_warnings.len() + 1);
    if let Some(warning) = legacy_direct_warning {
        warnings.push(warning.to_json());
    }
    warnings.extend(registry_warnings.iter().map(|w| w.to_json()));
    crate::cli::legacy_direct::print_human_warning_values(&warnings, context.emit_json_envelope);
    match outcome {
        Ok(output) => {
            if let Some(spinner) = spinner.as_ref() {
                spinner.finish_clear();
            }
            if context.emit_json_envelope {
                emit_json_envelope_success_with_warnings(
                    context.command_kind,
                    context.command_name,
                    &output,
                    &warnings,
                );
                return;
            }
            if !output.trim().is_empty() {
                let _ = renderer.text(&output);
            }
            let _ = renderer.text("");
        }
        Err(err) => {
            if let Some(spinner) = spinner.as_ref() {
                spinner.finish_clear();
            }
            if context.emit_json_envelope {
                emit_json_envelope_error_with_warnings(
                    1,
                    context.command_kind,
                    context.command_name,
                    "RunnerError",
                    &err.to_string(),
                    err.rendered_output().map(parse_json_or_string),
                    &warnings,
                );
            }
            if let Some(rendered) = err.rendered_output() {
                let _ = renderer.text(rendered);
                if context.suppress_header {
                    std::process::exit(1);
                }
            }
            if context.emit_json_envelope {
                emit_json_envelope_error_with_warnings(
                    1,
                    context.command_kind,
                    context.command_name,
                    "RunnerError",
                    &err.to_string(),
                    None,
                    &warnings,
                );
            }
            let mut err_renderer = PlainRenderer::stderr(context.output_mode);
            let _ = err_renderer.error_block(&MessageBlock::new("Task failed", err.to_string()));
            std::process::exit(1);
        }
    }
}

fn should_show_transient_spinner(context: &CliExecutionContext<'_>, command: &Command) -> bool {
    if context.emit_json_envelope || context.suppress_header {
        return false;
    }
    if !std::io::stdout().is_terminal() || std::env::var_os("CI").is_some() {
        return false;
    }
    matches!(
        command,
        Command::Container(effigy_cli::ContainerArgs {
            subcommand: effigy_cli::ContainerSubcommand::Cache {
                name: _,
                subcommand: effigy_cli::ContainerCacheSubcommand::List {
                    global: true,
                    project: _,
                    kind: _,
                },
            },
            ..
        }) | Command::Container(effigy_cli::ContainerArgs {
            subcommand: effigy_cli::ContainerSubcommand::Volume {
                subcommand: effigy_cli::ContainerVolumeSubcommand::List { .. },
            },
            ..
        })
    )
}

fn transient_spinner_label(command: &Command) -> &'static str {
    match command {
        Command::Container(effigy_cli::ContainerArgs {
            subcommand:
                effigy_cli::ContainerSubcommand::Volume {
                    subcommand: effigy_cli::ContainerVolumeSubcommand::List { .. },
                },
            ..
        }) => "Inspecting managed volumes...",
        _ => "Inspecting cache volumes...",
    }
}

fn parse_command_with_builtin_deferral(
    args: Vec<String>,
    cwd: &Path,
    global_options: &GlobalCliOptions,
) -> Result<Command, effigy_cli::CliParseError> {
    let Some(first) = args.first() else {
        return parse_command(args);
    };

    // The five namespace words are reserved: an exact space-separated
    // namespace enters grouped built-in routing and no manifest task or
    // `[defer] builtins` entry owns the bare word after this preview.
    if effigy_cli::command_surface::group_for_namespace_word(first).is_some() {
        return parse_command(args);
    }

    let Some(root) = deferred_builtin_root(&args[1..], cwd, global_options.repo_override.clone())
    else {
        return parse_command(args);
    };
    let deferred_builtins = crate::runner::deferred_builtins_for_root(&root);
    if first == "help" {
        return reject_help_for_deferred_builtin(parse_command(args)?, &deferred_builtins, &root);
    }
    if !deferred_builtins.contains(first) {
        return parse_command(args);
    }

    Ok(Command::Task(effigy_cli::TaskInvocation {
        name: first.clone(),
        args: args[1..].to_vec(),
    }))
}

/// Keep `effigy help <command>` in step with `effigy <command> --help`.
///
/// When repository routing owns the built-in name, the direct flag already
/// runs the repository's own path, so the built-in panel must not resurface
/// through the help topic either.
fn reject_help_for_deferred_builtin(
    command: Command,
    deferred_builtins: &std::collections::BTreeSet<String>,
    root: &Path,
) -> Result<Command, effigy_cli::CliParseError> {
    let owned_name = match &command {
        Command::Help(topic) => {
            effigy_cli::command_surface::deferred_builtin_for_help_topic(*topic)
                .filter(|name| deferred_builtins.contains(*name))
                .map(str::to_owned)
        }
        // `effigy help config` and `effigy help scan` resolve to the built-in's
        // own `--help` invocation. A repository selector of the same name owns
        // that word, and help must never run repository work, so refuse instead.
        Command::Task(task) => (deferred_builtins.contains(&task.name)
            || crate::runner::root_manifest_declares_task(root, &task.name))
        .then(|| task.name.clone()),
        _ => None,
    };
    match owned_name {
        Some(name) => Err(effigy_cli::CliParseError::InvalidArguments(format!(
            "`{name}` is deferred to this repository's own routing, so its built-in help panel is unavailable here; run `effigy {name} --help` for what `effigy {name}` actually does"
        ))),
        None => Ok(command),
    }
}

fn deferred_builtin_root(
    tail: &[String],
    cwd: &Path,
    global_repo_override: Option<PathBuf>,
) -> Option<PathBuf> {
    let repo_override = global_repo_override.or_else(|| repo_override_from_args(tail));
    effigy_core::resolver::resolve_target_root(cwd.to_path_buf(), repo_override)
        .ok()
        .map(|resolved| resolved.resolved_root)
        .or_else(|| repo_override_from_args(tail))
        .or_else(|| Some(cwd.to_path_buf()))
}

fn repo_override_from_args(args: &[String]) -> Option<PathBuf> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--" {
            return None;
        }
        if arg == "--repo" {
            return iter.next().map(PathBuf::from);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_command_with_builtin_deferral;
    use effigy_cli::{Command, GlobalCliOptions};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("effigy-entrypoint-{name}-{ts}"));
        fs::create_dir_all(&root).expect("mkdir root");
        root
    }

    #[test]
    fn parse_command_with_builtin_deferral_prefers_root_task_name_collision() {
        let root = temp_root("builtin-collision");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.deploy]\nrun = \"printf deploy\"\n",
        )
        .expect("write manifest");

        let parsed = parse_command_with_builtin_deferral(
            vec!["deploy".to_owned()],
            &root,
            &GlobalCliOptions::default(),
        )
        .expect("parse");

        assert!(matches!(
            parsed,
            Command::Task(task) if task.name == "deploy" && task.args.is_empty()
        ));
    }

    #[test]
    fn parse_command_with_builtin_deferral_preserves_passthrough_args_for_root_task_collision() {
        let root = temp_root("builtin-collision-args");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.deploy]\nrun = \"printf deploy\"\n",
        )
        .expect("write manifest");

        let parsed = parse_command_with_builtin_deferral(
            vec!["deploy".to_owned(), "uat".to_owned()],
            &root,
            &GlobalCliOptions::default(),
        )
        .expect("parse");

        assert!(matches!(
            parsed,
            Command::Task(task) if task.name == "deploy" && task.args == vec!["uat".to_owned()]
        ));
    }

    #[test]
    fn parse_command_with_builtin_deferral_honors_leading_repo_override() {
        let root = temp_root("builtin-global-repo");
        let target = root.join("target");
        fs::create_dir_all(&target).expect("mkdir target");
        fs::write(
            target.join("effigy.toml"),
            "[test.suites]\nunit = \"printf ok\"\n",
        )
        .expect("write manifest");

        let parsed = parse_command_with_builtin_deferral(
            vec!["test".to_owned(), "--plan".to_owned()],
            &root,
            &GlobalCliOptions {
                repo_override: Some(target.clone()),
                ..GlobalCliOptions::default()
            },
        )
        .expect("parse");

        assert!(matches!(
            parsed,
            Command::Task(task) if task.name == "test" && task.args == vec!["--plan".to_owned()]
        ));
    }

    #[test]
    fn repo_override_from_args_stops_at_passthrough_delimiter() {
        assert_eq!(
            super::repo_override_from_args(&[
                "--".to_owned(),
                "--repo".to_owned(),
                "/tmp/other".to_owned(),
            ]),
            None
        );
        assert_eq!(
            super::repo_override_from_args(&[
                "--verbose-root".to_owned(),
                "--repo".to_owned(),
                "/tmp/repo".to_owned(),
                "--".to_owned(),
                "--repo".to_owned(),
                "/tmp/other".to_owned(),
            ]),
            Some(std::path::PathBuf::from("/tmp/repo"))
        );
    }
}

#[cfg(test)]
mod help_deferral_tests {
    use super::parse_command_with_builtin_deferral;
    use effigy_cli::{Command, GlobalCliOptions, HelpTopic};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("effigy-help-deferral-{name}-{ts}"));
        fs::create_dir_all(&root).expect("mkdir root");
        root
    }

    fn parse(root: &std::path::Path, args: &[&str]) -> Result<Command, effigy_cli::CliParseError> {
        parse_command_with_builtin_deferral(
            args.iter().map(|arg| (*arg).to_owned()).collect(),
            root,
            &GlobalCliOptions::default(),
        )
    }

    #[test]
    fn help_command_topic_resolves_when_the_builtin_owns_its_name() {
        let root = temp_root("not-deferred");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.dev]\nrun = \"printf dev\"\n",
        )
        .expect("write manifest");

        assert_eq!(
            parse(&root, &["help", "docs"]).expect("parse"),
            Command::Help(HelpTopic::Docs)
        );
    }

    #[test]
    fn help_command_topic_defers_with_the_direct_command_when_a_selector_shadows_it() {
        let root = temp_root("selector-shadow");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.docs]\nrun = \"printf docs-task\"\n",
        )
        .expect("write manifest");

        let direct = parse(&root, &["docs", "--help"]).expect("parse");
        assert!(
            matches!(&direct, Command::Task(task) if task.name == "docs"),
            "`effigy docs --help` should route to the repository task: {direct:?}"
        );

        let message = parse(&root, &["help", "docs"])
            .expect_err("`effigy help docs` should not resurface the built-in panel")
            .to_string();
        assert!(message.contains("`docs` is deferred"), "{message}");
        assert!(message.contains("run `effigy docs --help`"), "{message}");

        assert_eq!(
            parse(&root, &["help", "graph"]).expect("parse"),
            Command::Help(HelpTopic::Graph),
            "unrelated built-in help topics stay available"
        );
    }

    #[test]
    fn help_command_topic_defers_for_explicitly_deferred_builtins() {
        let root = temp_root("explicit-defer");
        fs::write(
            root.join("effigy.toml"),
            "[defer]\nrun = \"printf deferred\"\nbuiltins = [\"graph\"]\n",
        )
        .expect("write manifest");

        let message = parse(&root, &["help", "graph"])
            .expect_err("explicitly deferred built-ins have no help panel")
            .to_string();
        assert!(message.contains("`graph` is deferred"), "{message}");
    }

    #[test]
    fn general_and_group_help_topics_stay_reachable_under_deferral() {
        let root = temp_root("group-still-reachable");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.docs]\nrun = \"printf docs-task\"\n",
        )
        .expect("write manifest");

        assert_eq!(
            parse(&root, &["help"]).expect("parse"),
            Command::Help(HelpTopic::General)
        );
        assert_eq!(
            parse(&root, &["help", "repo"]).expect("parse"),
            Command::HelpGroup(effigy_cli::HelpGroup::Repo)
        );
    }
}

#[cfg(test)]
mod namespace_reservation_tests {
    use super::parse_command_with_builtin_deferral;
    use effigy_cli::{Command, GlobalCliOptions, HelpGroup};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("effigy-namespace-reservation-{name}-{ts}"));
        fs::create_dir_all(&root).expect("mkdir root");
        root
    }

    fn parse(root: &std::path::Path, args: &[&str]) -> Result<Command, effigy_cli::CliParseError> {
        parse_command_with_builtin_deferral(
            args.iter().map(|arg| (*arg).to_owned()).collect(),
            root,
            &GlobalCliOptions::default(),
        )
    }

    #[test]
    fn namespace_words_beat_explicit_deferral_entries() {
        let root = temp_root("defer-namespace");
        fs::write(
            root.join("effigy.toml"),
            "[defer]\nrun = \"printf deferred\"\nbuiltins = [\"repo\", \"graph\"]\n",
        )
        .expect("write manifest");

        // The namespace word is reserved: `effigy repo docs` enters grouped
        // built-in routing even though `[defer] builtins` names `repo`.
        assert_eq!(
            parse(&root, &["repo", "docs"]).expect("parse"),
            Command::Help(effigy_cli::HelpTopic::Docs),
            "bare grouped child renders the built-in typed panel"
        );
        assert_eq!(
            parse(&root, &["repo"]).expect("parse"),
            Command::HelpGroup(HelpGroup::Repo),
            "bare namespace word renders the group inventory"
        );

        // The retained direct spelling keeps its deferral: the manifest owns
        // `graph` through `[defer] builtins`.
        assert!(
            matches!(
                parse(&root, &["graph"]).expect("parse"),
                Command::Task(task) if task.name == "graph"
            ),
            "direct `effigy graph` must keep manifest deferral"
        );
    }

    #[test]
    fn grouped_routes_escape_root_task_shadowing_at_parse_time() {
        let root = temp_root("grouped-escape");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.docs]\nrun = \"printf repo-docs-task\"\n",
        )
        .expect("write manifest");

        // Direct `effigy docs` defers to the repository task...
        assert!(
            matches!(
                parse(&root, &["docs"]).expect("parse"),
                Command::Task(task) if task.name == "docs"
            ),
            "direct `effigy docs` must defer to the manifest task"
        );
        // ...while the grouped route keeps the typed built-in value (the
        // built-in docs panel for a bare child).
        assert!(
            matches!(
                parse(&root, &["repo", "docs"]).expect("parse"),
                Command::Help(effigy_cli::HelpTopic::Docs)
            ),
            "`effigy repo docs` must reach the typed built-in owner"
        );
    }

    #[test]
    fn repo_override_still_resolves_for_grouped_invocations() {
        let root = temp_root("grouped-repo-override");
        let target = root.join("target");
        fs::create_dir_all(&target).expect("mkdir target");
        fs::write(
            target.join("effigy.toml"),
            "[tasks.docs]\nrun = \"printf target-docs\"\n",
        )
        .expect("write manifest");

        // The target repo shadows `docs`, so the grouped route must keep the
        // typed built-in even under the override; the direct spelling defers.
        let override_options = GlobalCliOptions {
            repo_override: Some(target.clone()),
            ..GlobalCliOptions::default()
        };
        let grouped = parse_command_with_builtin_deferral(
            vec!["repo".to_owned(), "docs".to_owned()],
            &root,
            &override_options,
        )
        .expect("parse");
        assert!(
            matches!(grouped, Command::Help(effigy_cli::HelpTopic::Docs)),
            "grouped route must ignore target-repo shadowing: {grouped:?}"
        );

        let direct = parse_command_with_builtin_deferral(
            vec!["docs".to_owned()],
            &root,
            &override_options,
        )
        .expect("parse");
        assert!(
            matches!(&direct, Command::Task(task) if task.name == "docs"),
            "direct route must keep target-repo deferral: {direct:?}"
        );
    }
}
