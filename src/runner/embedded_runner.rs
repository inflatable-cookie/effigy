use std::path::Path;

use effigy_cli::{
    apply_global_json_flag, parse_command, strip_global_json_flags, Command, TaskInvocation,
};
use effigy_execution::ExecutionSurface;

use super::command_context::{apply_repo_target_to_embedded_command, EmbeddedRepoOverrideMode};
use super::error::RunnerError;

pub(in crate::runner) fn parse_embedded_command(
    repo_root: &Path,
    args: &[String],
    force_json: bool,
    mode: EmbeddedRepoOverrideMode,
) -> Result<Command, RunnerError> {
    let (stripped_args, requested_json) = strip_global_json_flags(args.to_vec());
    let command = parse_command(stripped_args)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(apply_repo_target_to_embedded_command(
        apply_global_json_flag(command, force_json || requested_json),
        repo_root,
        mode,
    ))
}

pub(in crate::runner) fn run_embedded_command(
    repo_root: &Path,
    command: Command,
    cwd: &Path,
    mode: EmbeddedRepoOverrideMode,
) -> Result<String, RunnerError> {
    crate::runner::entrypoints::run_command_with_cwd(
        apply_repo_target_to_embedded_command(command, repo_root, mode),
        cwd,
    )
}

pub(in crate::runner) fn run_embedded_task(
    task: &TaskInvocation,
    cwd: &Path,
) -> Result<String, RunnerError> {
    crate::runner::execute::api::run_manifest_task_with_surface(
        task,
        cwd.to_path_buf(),
        ExecutionSurface::RunArray,
    )
}

#[cfg(test)]
mod tests {
    use super::parse_embedded_command;
    use crate::runner::command_context::EmbeddedRepoOverrideMode;
    use effigy_cli::{Command, DocsArgs, DocsCheckKind, DocsSubcommand};
    use std::path::Path;

    #[test]
    fn parse_embedded_command_defaults_repo_override_when_missing() {
        let command = parse_embedded_command(
            Path::new("/tmp/repo"),
            &["docs".to_owned(), "check-links".to_owned()],
            false,
            EmbeddedRepoOverrideMode::DefaultIfMissing,
        )
        .expect("parse embedded command");

        assert!(matches!(
            command,
            Command::Docs(DocsArgs {
                subcommand: DocsSubcommand::Check {
                    kind: DocsCheckKind::Links,
                    ..
                },
                repo_override: Some(path),
                output_json: false,
            }) if path == Path::new("/tmp/repo")
        ));
    }

    #[test]
    fn parse_embedded_command_applies_forced_json_flag() {
        let command = parse_embedded_command(
            Path::new("/tmp/repo"),
            &["docs".to_owned(), "check-links".to_owned()],
            true,
            EmbeddedRepoOverrideMode::DefaultIfMissing,
        )
        .expect("parse embedded command");

        assert!(matches!(
            command,
            Command::Docs(DocsArgs {
                output_json: true,
                ..
            })
        ));
    }

    #[test]
    fn parse_embedded_command_respects_existing_json_flag() {
        let command = parse_embedded_command(
            Path::new("/tmp/repo"),
            &[
                "--json".to_owned(),
                "docs".to_owned(),
                "check-links".to_owned(),
            ],
            false,
            EmbeddedRepoOverrideMode::DefaultIfMissing,
        )
        .expect("parse embedded command");

        assert!(matches!(
            command,
            Command::Docs(DocsArgs {
                output_json: true,
                ..
            })
        ));
    }
}
