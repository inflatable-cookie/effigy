use crate::{
    command_kind_and_name, emit_json_envelope_error, emit_json_envelope_error_with_human,
    emit_json_envelope_success, parse_error_json_details, parse_json_or_string, render_cli_header,
    render_parse_error, run_help_command, run_help_group_command, CliExecutionContext,
};
use effigy_cli::usage_hint::usage_hint_for;
use effigy_cli::{
    apply_global_cli_flags, command_requests_json, parse_command,
    runtime_flag_present_before_passthrough, strip_global_cli_flags, Command, GlobalCliOptions,
    TasksQaCommand,
};
use effigy_context::EffigyRuntimeContext;
use effigy_core::widgets::MessageBlock;
use effigy_ui::{OutputMode, PlainRenderer, Renderer};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

pub fn run_cli(raw_args: Vec<String>) {
    let requested_root_json = runtime_flag_present_before_passthrough(&raw_args, "--json");
    // One argv snapshot feeds the targeted usage of every argument error below:
    // the answer is the nearest command's own usage, never the whole help. It
    // holds the command argv, so a global flag can never be read as the command.
    let mut hint_argv = raw_args.clone();
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
                    "refused",
                    &err.to_string(),
                    Some(parse_error_json_details(&usage_hint_for(&hint_argv))),
                );
            }
            let mut renderer = PlainRenderer::stderr(output_mode);
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let resolved_root = effigy_core::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            let _ = render_parse_error(
                &mut renderer,
                &resolved_root,
                &err.to_string(),
                &usage_hint_for(&hint_argv),
            );
            std::process::exit(2);
        }
    };
    hint_argv = args.clone();
    let global_json_mode = global_options.json_mode;
    // `--json` never writes the retired-config notice to stderr; the envelope
    // carries it. Set before any manifest load so parse-time routing cannot
    // print it early.
    effigy_manifest::set_retired_notice_delivery(if global_json_mode {
        effigy_manifest::RetiredNoticeDelivery::JsonEnvelope
    } else {
        effigy_manifest::RetiredNoticeDelivery::OncePerCheckout
    });
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
                    "refused",
                    &err.to_string(),
                    Some(parse_error_json_details(&usage_hint_for(&hint_argv))),
                );
            }
            let mut renderer = PlainRenderer::stderr(output_mode);
            let resolved_root = effigy_core::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            let _ = render_parse_error(
                &mut renderer,
                &resolved_root,
                &err.to_string(),
                &usage_hint_for(&hint_argv),
            );
            std::process::exit(2);
        }
    };
    if global_json_mode && matches!(&parsed, Command::Skill(args) if args.stdio_passthrough()) {
        let mut renderer = PlainRenderer::stderr(output_mode);
        let _ = renderer.error_block(&MessageBlock::new(
            "Task failed",
            "`--json` and `--stdio passthrough` are incompatible: JSON mode lets Effigy own a versioned envelope while passthrough lets the task own raw stdio; choose one".to_owned(),
        ));
        std::process::exit(2);
    }
    let cmd = match apply_global_cli_flags(parsed, &global_options) {
        Ok(cmd) => cmd,
        Err(err) => {
            if global_json_mode {
                emit_json_envelope_error(
                    2,
                    "cli",
                    "parse",
                    "CliParseError",
                    "refused",
                    &err.to_string(),
                    Some(parse_error_json_details(&usage_hint_for(&hint_argv))),
                );
            }
            let mut renderer = PlainRenderer::stderr(output_mode);
            let resolved_root = effigy_core::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            let _ = render_parse_error(
                &mut renderer,
                &resolved_root,
                &err.to_string(),
                &usage_hint_for(&hint_argv),
            );
            std::process::exit(2);
        }
    };
    let internal_suppress_header = std::env::var("EFFIGY_INTERNAL_SUPPRESS_HEADER")
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"));
    // An explicit JSON request is the machine contract: it always gets the
    // `effigy.command.v1` envelope. The nested-invocation marker a parent sets
    // only hides the human furniture (the CLI header and the retired-config
    // notice), so a nested `--json` child reports the same schema as any other
    // caller, including a workload that inherited the marker from a host
    // process supervisor.
    let json_requested = command_requests_json(&cmd, global_json_mode);
    let local_qa_json = !global_json_mode && command_requests_local_qa_json(&cmd);
    let suppress_header = internal_suppress_header
        || effigy_ui::steps::in_managed_pane()
        || json_requested
        || local_qa_json;
    let emit_json_envelope = json_requested;
    effigy_manifest::set_retired_notice_delivery(if emit_json_envelope {
        effigy_manifest::RetiredNoticeDelivery::JsonEnvelope
    } else if matches!(cmd, Command::Doctor(_)) {
        // A doctor report is where an operator looks for config drift, so it
        // names retired config every time it runs.
        effigy_manifest::RetiredNoticeDelivery::Always
    } else {
        effigy_manifest::RetiredNoticeDelivery::OncePerCheckout
    });
    // One step sink for every plane: a terminal draws spinners on stderr,
    // `--json` and pipes stay silent unless the command asked for verbose
    // output, which prints one plain line per step instead.
    effigy_ui::steps::configure_from_terminal(
        emit_json_envelope,
        matches!(&cmd, Command::Doctor(args) if args.verbose),
    );
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
                    "unavailable",
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

    // Raw stdio passthrough bypasses Effigy's renderer, spinner, and JSON
    // envelope entirely: the selected task owns stdin/stdout/stderr and status.
    if matches!(&cmd, Command::Skill(args) if args.stdio_passthrough()) {
        match crate::runner::run_skill_passthrough_with_context(cmd, context.runtime_context) {
            Ok(status) => std::process::exit(status),
            Err(error) => {
                if let Some(status) = error.task_exit_status() {
                    // The task already owns stderr; do not add Effigy text.
                    std::process::exit(status);
                }
                let mut err_renderer = PlainRenderer::stderr(output_mode);
                let _ =
                    err_renderer.error_block(&MessageBlock::new("Task failed", error.to_string()));
                std::process::exit(1);
            }
        }
    }

    match cmd {
        Command::Version => crate::run_version_command(&context),
        Command::Help(topic) => run_help_command(&context, topic),
        Command::HelpGroup(group) => run_help_group_command(&context, group),
        command @ (Command::Bundle(_)
        | Command::Changelog(_)
        | Command::Fmt(_)
        | Command::Deploy(_)
        | Command::Deps(_)
        | Command::Defer(_)
        | Command::Exec(_)
        | Command::Secrets(_)
        | Command::State(_)
        | Command::Workspace(_)
        | Command::Gateway(_)
        | Command::Service(_)
        | Command::Rhai(_)
        | Command::Skill(_)
        | Command::Docs(_)
        | Command::Contracts(_)
        | Command::Container(_)
        | Command::Bootstrap(_)
        | Command::Uninstall(_)
        | Command::Release(_)
        | Command::Doctor(_)
        | Command::Tasks(_)
        | Command::Runs(_)
        | Command::InternalGateway(_)
        | Command::InternalGatewayIdentity(_)
        | Command::InternalScriptRun(_)
        | Command::InternalContainerLeaseReaper(_)
        | Command::Task(_)) => run_and_render_command(&context, command),
    }
}

pub fn run_and_render_command(context: &CliExecutionContext<'_>, command: Command) {
    let task_stdout_owned = command_owns_task_stdout(&command);
    let stdout_is_terminal = std::io::stdout().is_terminal();
    let clean_machine_output = (!stdout_is_terminal && command_emits_release_notes(&command))
        || command_streams_run_logs(&command);
    let suppress_header = context.suppress_header || clean_machine_output;
    let mut renderer = PlainRenderer::stdout(context.output_mode);
    if !suppress_header {
        if task_stdout_owned {
            let mut header_renderer = PlainRenderer::stderr(context.output_mode);
            let _ = render_cli_header(&mut header_renderer, context.command_root);
        } else {
            let _ = render_cli_header(&mut renderer, context.command_root);
        }
    }
    match crate::runner::run_command_with_context(command, context.runtime_context) {
        Ok(output) => {
            if context.emit_json_envelope {
                emit_json_envelope_success(context.command_kind, context.command_name, &output);
                return;
            }
            if !output.trim().is_empty() {
                let _ = renderer.text(&output);
            }
            let output_is_empty = output.trim().is_empty();
            if (!task_stdout_owned || !output_is_empty) && !clean_machine_output {
                let _ = renderer.text("");
            }
        }
        Err(err) => {
            if let crate::runner::RunnerError::RunSettled { code } = err {
                // An attached or followed run already wrote its output; exit
                // with that run's own result and add nothing.
                std::process::exit(code);
            }
            let exit_code = err.process_exit_code();
            let human = err.human_error();
            if context.emit_json_envelope {
                emit_json_envelope_error_with_human(
                    exit_code,
                    context.command_kind,
                    context.command_name,
                    "RunnerError",
                    err.category().as_str(),
                    &err.to_string(),
                    err.json_error_details().map(parse_json_or_string),
                    human.to_json(),
                );
            }
            if let Some(rendered) = err.rendered_output() {
                let _ = renderer.text(rendered);
                if suppress_header {
                    std::process::exit(1);
                }
            }
            let mut err_renderer = PlainRenderer::stderr(context.output_mode);
            let _ = err_renderer.text(&human.render_text());
            std::process::exit(exit_code);
        }
    }
}

fn command_owns_task_stdout(command: &Command) -> bool {
    matches!(command, Command::Task(_))
}

/// `run logs` writes a run's raw output bytes: no header, no trailing line,
/// so a reader's byte offsets match the run's logs exactly.
fn command_streams_run_logs(command: &Command) -> bool {
    matches!(
        command,
        Command::Runs(effigy_cli::RunsArgs {
            action: effigy_cli::RunsAction::Logs { .. },
            ..
        })
    )
}

fn command_emits_release_notes(command: &Command) -> bool {
    matches!(
        command,
        Command::Changelog(args)
            if matches!(
                &args.subcommand,
                effigy_cli::ChangelogSubcommand::Extract { .. }
            )
    )
}

fn command_requests_local_qa_json(command: &Command) -> bool {
    let Command::Tasks(args) = command else {
        return false;
    };
    match args.qa.as_ref() {
        Some(TasksQaCommand::GroupsList { output_json, .. })
        | Some(TasksQaCommand::GroupRun { output_json, .. })
        | Some(TasksQaCommand::GroupStatus { output_json, .. }) => *output_json,
        Some(TasksQaCommand::GroupLogs { .. } | TasksQaCommand::GroupStop { .. }) | None => false,
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
        // The word after `help` is what names the built-in to defer, and it is
        // not always the topic's first built-in (`fmt` and `fmt-rust` share
        // the formatter panel but defer independently).
        let requested = args.get(1).cloned();
        return reject_help_for_deferred_builtin(
            parse_command(args)?,
            requested.as_deref(),
            &deferred_builtins,
            &root,
        );
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
    requested: Option<&str>,
    deferred_builtins: &std::collections::BTreeSet<String>,
    root: &Path,
) -> Result<Command, effigy_cli::CliParseError> {
    let owned_name = match &command {
        Command::Help(_) => requested
            .and_then(effigy_cli::command_surface::deferred_builtin_for_help_argument)
            .filter(|name| deferred_builtins.contains(*name))
            .map(str::to_owned),
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

    /// The #290 regression: a repository `fmt` task receives `fmt` and every
    /// following word, including the built-in's own `rust` subcommand.
    #[test]
    fn parse_command_with_builtin_deferral_routes_fmt_to_the_repository_task() {
        let root = temp_root("builtin-collision-fmt");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.fmt]\nrun = \"cargo fmt {args}\"\n",
        )
        .expect("write manifest");

        let bare = parse_command_with_builtin_deferral(
            vec!["fmt".to_owned()],
            &root,
            &GlobalCliOptions::default(),
        )
        .expect("parse");
        assert!(matches!(
            bare,
            Command::Task(task) if task.name == "fmt" && task.args.is_empty()
        ));

        let built_in_shape = parse_command_with_builtin_deferral(
            vec![
                "fmt".to_owned(),
                "rust".to_owned(),
                "src/main.rs".to_owned(),
            ],
            &root,
            &GlobalCliOptions::default(),
        )
        .expect("parse");
        assert!(matches!(
            built_in_shape,
            Command::Task(task)
                if task.name == "fmt"
                    && task.args == vec!["rust".to_owned(), "src/main.rs".to_owned()]
        ));
    }

    /// Without the task the built-in keeps both spellings.
    #[test]
    fn parse_command_with_builtin_deferral_keeps_the_fmt_builtin_without_a_task() {
        let root = temp_root("builtin-no-collision-fmt");
        fs::write(root.join("effigy.toml"), "[tasks.build]\nrun = \"true\"\n")
            .expect("write manifest");

        let nested = parse_command_with_builtin_deferral(
            vec![
                "fmt".to_owned(),
                "rust".to_owned(),
                "src/main.rs".to_owned(),
            ],
            &root,
            &GlobalCliOptions::default(),
        )
        .expect("parse");
        assert!(matches!(nested, Command::Fmt(_)));

        let escape = parse_command_with_builtin_deferral(
            vec!["fmt-rust".to_owned(), "src/main.rs".to_owned()],
            &root,
            &GlobalCliOptions::default(),
        )
        .expect("parse");
        assert!(matches!(escape, Command::Fmt(_)));
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
            parse(&root, &["help", "service"]).expect("parse"),
            Command::Help(HelpTopic::Service)
        );
    }

    /// `fmt` and its escape `fmt-rust` share one help panel but defer on their
    /// own names, so shadowing one never hides the other's help.
    #[test]
    fn help_command_topic_defers_on_the_requested_builtin_not_the_shared_topic() {
        let shadowing_fmt = temp_root("help-fmt-only");
        fs::write(
            shadowing_fmt.join("effigy.toml"),
            "[tasks.fmt]\nrun = \"cargo fmt\"\n",
        )
        .expect("write manifest");

        let message = parse(&shadowing_fmt, &["help", "fmt"])
            .expect_err("a shadowed `fmt` has no built-in help panel")
            .to_string();
        assert!(message.contains("`fmt` is deferred"), "{message}");
        assert_eq!(
            parse(&shadowing_fmt, &["help", "fmt-rust"]).expect("parse"),
            Command::Help(HelpTopic::Fmt)
        );

        let shadowing_escape = temp_root("help-fmt-rust-only");
        fs::write(
            shadowing_escape.join("effigy.toml"),
            "[tasks.fmt-rust]\nrun = \"printf escape\"\n",
        )
        .expect("write manifest");
        assert_eq!(
            parse(&shadowing_escape, &["help", "fmt"]).expect("parse"),
            Command::Help(HelpTopic::Fmt)
        );
        let message = parse(&shadowing_escape, &["help", "fmt-rust"])
            .expect_err("a shadowed escape spelling has no built-in help panel")
            .to_string();
        assert!(message.contains("`fmt-rust` is deferred"), "{message}");
    }

    #[test]
    fn help_command_topic_defers_for_explicitly_deferred_builtins() {
        let root = temp_root("explicit-defer");
        fs::write(
            root.join("effigy.toml"),
            "[defer]\nrun = \"printf deferred\"\nbuiltins = [\"service\"]\n",
        )
        .expect("write manifest");

        let message = parse(&root, &["help", "service"])
            .expect_err("explicitly deferred built-ins have no help panel")
            .to_string();
        assert!(message.contains("`service` is deferred"), "{message}");
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
