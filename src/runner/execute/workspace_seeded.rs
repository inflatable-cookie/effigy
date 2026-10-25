use std::path::{Path, PathBuf};

use super::api::ContainerExecutionBinding;
pub(super) use crate::runner::container_runtime::inside_container_handoff;
use crate::runner::error::RunnerError;
use crate::runner::runtime_session_context::PublicWorkspaceCleanupOverride;
use crate::runner::util::render_passthrough_args;
use crate::runner::workspace_command::run_workspace_seeded_session;
use effigy_core::shell::shell_quote;

pub(super) fn render_workspace_seeded_task_command(task_name: &str, args: &[String]) -> String {
    let mut rendered = format!("effigy {}", shell_quote(task_name));
    let rendered_args = render_passthrough_args(args);
    if !rendered_args.is_empty() {
        rendered.push(' ');
        rendered.push_str(&rendered_args);
    }
    // A seeded workspace task runs inside a named container, reached by a
    // command string the container runtime carries. Session access never
    // travels that way (effigy#338): the nested Effigy run there resolves its
    // own access, exactly as a top-level invocation does.
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

        assert!(
            rendered.ends_with("effigy 'dev' 'front' '--' '--host' '0.0.0.0'"),
            "got: {rendered}"
        );
    }

    /// A container command string never carries session access, even when
    /// this process holds some (effigy#338).
    #[test]
    fn a_seeded_workspace_command_never_carries_session_access() {
        let _guard = crate::contract_test_support::lock_test();
        unsafe {
            std::env::set_var("EFFIGY_SECRET_PASSPHRASE", "vault-passphrase");
        }
        let rendered = render_workspace_seeded_task_command("dev", &[]);
        unsafe {
            std::env::remove_var("EFFIGY_SECRET_PASSPHRASE");
        }
        assert!(!rendered.contains("vault-passphrase"), "{rendered}");
        assert!(!rendered.contains("EFFIGY_INTERNAL_SECRET"), "{rendered}");
    }
}
