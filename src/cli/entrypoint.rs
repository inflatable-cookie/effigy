use crate::{
    command_kind_and_name, emit_json_envelope_error, emit_json_envelope_success,
    parse_error_json_details, parse_json_or_string, render_cli_header, render_parse_error,
    run_graph_watch_command, run_help_command, run_help_group_command, CliExecutionContext,
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
    let output_mode = OutputMode::from_env();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let parsed = match parse_command_with_builtin_deferral(args, &cwd, &global_options) {
        Ok(cmd) => cmd,
        Err(err) => {
            if global_json_mode {
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
            let resolved_root = effigy_core::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            let _ = render_parse_error(&mut renderer, &resolved_root, &err.to_string());
            std::process::exit(2);
        }
    };
    let cmd = match apply_global_cli_flags(parsed, &global_options) {
        Ok(cmd) => cmd,
        Err(err) => {
            if global_json_mode {
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
            if emit_json_envelope {
                emit_json_envelope_error(
                    1,
                    command_kind,
                    &command_name,
                    "RuntimeContextError",
                    &message,
                    None,
                );
            }
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
        Command::Version => crate::run_version_command(&context),
        Command::Help(topic) => run_help_command(&context, topic),
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
        | Command::Task(_)) => run_and_render_command(&context, command),
    }
}

pub fn run_and_render_command(context: &CliExecutionContext<'_>, command: Command) {
    let mut renderer = PlainRenderer::stdout(context.output_mode);
    if !context.suppress_header {
        let _ = render_cli_header(&mut renderer, context.command_root);
    }
    let spinner = if should_show_transient_spinner(context, &command) {
        renderer.spinner(transient_spinner_label(&command)).ok()
    } else {
        None
    };
    match crate::runner::run_command_with_context(command, context.runtime_context) {
        Ok(output) => {
            if let Some(spinner) = spinner.as_ref() {
                spinner.finish_clear();
            }
            if context.emit_json_envelope {
                emit_json_envelope_success(context.command_kind, context.command_name, &output);
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
                emit_json_envelope_error(
                    1,
                    context.command_kind,
                    context.command_name,
                    "RunnerError",
                    &err.to_string(),
                    err.rendered_output().map(parse_json_or_string),
                );
            }
            if let Some(rendered) = err.rendered_output() {
                let _ = renderer.text(rendered);
                if context.suppress_header {
                    std::process::exit(1);
                }
            }
            if context.emit_json_envelope {
                emit_json_envelope_error(
                    1,
                    context.command_kind,
                    context.command_name,
                    "RunnerError",
                    &err.to_string(),
                    None,
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

    let Some(root) = deferred_builtin_root(&args[1..], cwd, global_options.repo_override.clone())
    else {
        return parse_command(args);
    };
    let deferred_builtins = crate::runner::deferred_builtins_for_root(&root);
    if first == "help" {
        return reject_help_for_deferred_builtin(parse_command(args)?, &deferred_builtins);
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
) -> Result<Command, effigy_cli::CliParseError> {
    let Command::Help(topic) = command else {
        return Ok(command);
    };
    let deferred = effigy_cli::command_surface::deferred_builtin_for_help_topic(topic)
        .filter(|name| deferred_builtins.contains(*name));
    match deferred {
        Some(name) => Err(effigy_cli::CliParseError::InvalidArguments(format!(
            "`{name}` is deferred to this repository's own routing, so its built-in help panel is unavailable here; run `effigy {name} --help` for what `effigy {name}` actually does"
        ))),
        None => Ok(Command::Help(topic)),
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
