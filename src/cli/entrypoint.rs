use crate::ui::{OutputMode, PlainRenderer};
use crate::{
    apply_global_json_flag, command_kind_and_name, command_requests_json, emit_json_envelope_error,
    parse_command, parse_error_json_details, render_parse_error, run_and_render_command,
    run_help_command, strip_global_json_flags, CliExecutionContext, Command,
};
use std::path::{Path, PathBuf};

pub fn run_cli(raw_args: Vec<String>) {
    let (args, global_json_mode) = strip_global_json_flags(raw_args);
    let output_mode = OutputMode::from_env();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let parsed = match parse_command_with_explicit_builtin_deferral(args, &cwd) {
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
            let resolved_root = crate::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            let _ = render_parse_error(&mut renderer, &resolved_root, &err.to_string());
            std::process::exit(2);
        }
    };
    let cmd = apply_global_json_flag(parsed, global_json_mode);
    let suppress_header = command_requests_json(&cmd, global_json_mode);
    let emit_json_envelope = suppress_header;
    let (command_kind, command_name) = command_kind_and_name(&cmd);
    let command_root = crate::runner::resolve_command_root(&cmd);
    let context = CliExecutionContext {
        output_mode,
        command_root: &command_root,
        suppress_header,
        emit_json_envelope,
        command_kind,
        command_name: &command_name,
    };

    match cmd {
        Command::Version => crate::run_version_command(&context),
        Command::Help(topic) => run_help_command(&context, topic),
        command @ (Command::Changelog(_)
        | Command::Docs(_)
        | Command::Contracts(_)
        | Command::Distribution(_)
        | Command::Bootstrap(_)
        | Command::Release(_)
        | Command::Doctor(_)
        | Command::Tasks(_)
        | Command::Task(_)) => run_and_render_command(&context, command),
    }
}

fn parse_command_with_explicit_builtin_deferral(
    args: Vec<String>,
    cwd: &Path,
) -> Result<Command, crate::CliParseError> {
    let Some(first) = args.first() else {
        return parse_command(args);
    };

    let Some(root) = explicit_deferred_builtin_root(first, &args[1..], cwd) else {
        return parse_command(args);
    };
    let deferred_builtins = crate::runner::deferred_builtins_for_root(&root);
    if !deferred_builtins.contains(first) {
        return parse_command(args);
    }

    Ok(Command::Task(crate::TaskInvocation {
        name: first.clone(),
        args: args[1..].to_vec(),
    }))
}

fn explicit_deferred_builtin_root(cmd: &str, tail: &[String], cwd: &Path) -> Option<PathBuf> {
    if !crate::runner::builtin_can_be_explicitly_deferred(cmd) {
        return None;
    }
    let repo_override = repo_override_from_args(tail);
    crate::resolver::resolve_target_root(cwd.to_path_buf(), repo_override)
        .ok()
        .map(|resolved| resolved.resolved_root)
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
