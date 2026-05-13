use crate::{
    command_kind_and_name, emit_json_envelope_error, emit_json_envelope_success,
    parse_error_json_details, parse_json_or_string, render_cli_header, render_parse_error,
    run_help_command, CliExecutionContext,
};
use effigy_cli::{
    apply_global_json_flag, command_requests_json, parse_command, strip_global_json_flags, Command,
};
use effigy_context::EffigyRuntimeContext;
use effigy_core::widgets::MessageBlock;
use effigy_ui::{OutputMode, PlainRenderer, Renderer};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

pub fn run_cli(raw_args: Vec<String>) {
    let (args, global_json_mode) = strip_global_json_flags(raw_args);
    let output_mode = OutputMode::from_env();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let parsed = match parse_command_with_builtin_deferral(args, &cwd) {
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
    let cmd = apply_global_json_flag(parsed, global_json_mode);
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
        command @ (Command::Bundle(_)
        | Command::Changelog(_)
        | Command::Deploy(_)
        | Command::Defer(_)
        | Command::Exec(_)
        | Command::Secrets(_)
        | Command::State(_)
        | Command::System(_)
        | Command::Workspace(_)
        | Command::Gateway(_)
        | Command::Service(_)
        | Command::Demo(_)
        | Command::Docs(_)
        | Command::Contracts(_)
        | Command::Distribution(_)
        | Command::Artifact(_)
        | Command::Container(_)
        | Command::Bootstrap(_)
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
) -> Result<Command, effigy_cli::CliParseError> {
    let Some(first) = args.first() else {
        return parse_command(args);
    };

    let Some(root) = deferred_builtin_root(&args[1..], cwd) else {
        return parse_command(args);
    };
    let deferred_builtins = crate::runner::deferred_builtins_for_root(&root);
    if !deferred_builtins.contains(first) {
        return parse_command(args);
    }

    Ok(Command::Task(effigy_cli::TaskInvocation {
        name: first.clone(),
        args: args[1..].to_vec(),
    }))
}

fn deferred_builtin_root(tail: &[String], cwd: &Path) -> Option<PathBuf> {
    let repo_override = repo_override_from_args(tail);
    effigy_core::resolver::resolve_target_root(cwd.to_path_buf(), repo_override)
        .ok()
        .map(|resolved| resolved.resolved_root)
        .or_else(|| repo_override_from_args(tail))
        .or_else(|| Some(cwd.to_path_buf()))
}

fn repo_override_from_args(args: &[String]) -> Option<PathBuf> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--repo" {
            return iter.next().map(PathBuf::from);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_command_with_builtin_deferral;
    use effigy_cli::Command;
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

        let parsed =
            parse_command_with_builtin_deferral(vec!["deploy".to_owned()], &root).expect("parse");

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

        let parsed =
            parse_command_with_builtin_deferral(vec!["deploy".to_owned(), "uat".to_owned()], &root)
                .expect("parse");

        assert!(matches!(
            parsed,
            Command::Task(task) if task.name == "deploy" && task.args == vec!["uat".to_owned()]
        ));
    }
}
