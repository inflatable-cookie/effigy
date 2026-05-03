use std::path::{Path, PathBuf};

use super::api::ContainerExecutionBinding;
pub(super) use crate::runner::container_runtime::inside_container_handoff;
use crate::runner::error::RunnerError;
use crate::runner::runtime_session_context::PublicWorkspaceCleanupOverride;
use crate::runner::system_command::run_workspace_seeded_session;
use crate::runner::util::render_passthrough_args;
use effigy_core::shell::shell_quote;

pub(super) fn render_workspace_seeded_task_command(task_name: &str, args: &[String]) -> String {
    let mut rendered = format!("effigy {}", shell_quote(task_name));
    let rendered_args = render_passthrough_args(args);
    if !rendered_args.is_empty() {
        rendered.push(' ');
        rendered.push_str(&rendered_args);
    }
    rendered
}

pub(super) fn run_workspace_seeded_task_session(
    repo_root: &Path,
    container_binding: &ContainerExecutionBinding,
    repo_override: Option<PathBuf>,
    task_name: &str,
    args: &[String],
    cleanup_override: Option<PublicWorkspaceCleanupOverride>,
) -> Result<String, RunnerError> {
    run_workspace_seeded_session(
        repo_root,
        container_binding.container_name(),
        repo_override,
        &render_workspace_seeded_task_command(task_name, args),
        cleanup_override,
    )
}

#[cfg(test)]
mod tests {
    use super::render_workspace_seeded_task_command;

    #[test]
    fn workspace_seeded_task_command_preserves_passthrough_args() {
        let rendered = render_workspace_seeded_task_command(
            "dev",
            &[
                "front".to_owned(),
                "--".to_owned(),
                "--host".to_owned(),
                "0.0.0.0".to_owned(),
            ],
        );

        assert_eq!(rendered, "effigy 'dev' 'front' '--' '--host' '0.0.0.0'");
    }
}
