use std::path::{Path, PathBuf};

use crate::runner::interactive_session::{
    classify_interactive_session_ownership, InteractiveSessionIntent, InteractiveSessionOwnership,
};
use crate::runner::runtime_session_context::{
    current_runtime_session_context, PublicWorkspaceCleanupOverride,
};

use super::{workspace, RunnerError};

pub(super) fn run_workspace_container_session(
    repo_root: &Path,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    initial_command: Option<&str>,
    session_intent: InteractiveSessionIntent,
) -> Result<String, RunnerError> {
    let repo_override = workspace::effective_workspace_repo_override(repo_root, repo_override);
    let container_name = container_name.map(str::to_owned);
    let policy = workspace::load_workspace_session_policy(repo_root, container_name.as_deref())?;
    let system_was_running =
        crate::runner::container_runtime_prep::ensure_container_runtime_prepared(
            repo_root,
            &policy,
            container_name.as_deref(),
            repo_override.clone(),
        )?;
    let routes_were_ready_before_handoff = workspace::prepare_workspace_handoff(
        repo_root,
        &policy,
        container_name.as_deref(),
        repo_override.clone(),
        initial_command,
    )?;
    let ownership = classify_workspace_session_ownership(
        session_intent,
        system_was_running,
        routes_were_ready_before_handoff,
    );
    let shell_result = workspace::run_workspace_handoff_shell(
        repo_root,
        container_name.as_deref(),
        initial_command,
    );
    let cleanup_result = workspace::cleanup_workspace_session(
        ownership,
        shell_result.is_ok(),
        container_name,
        repo_override,
    );

    combine_workspace_session_results(shell_result, cleanup_result)
}

pub(super) fn classify_workspace_session_ownership(
    session_intent: InteractiveSessionIntent,
    system_was_running: bool,
    routes_were_ready_before_handoff: bool,
) -> InteractiveSessionOwnership {
    if matches!(session_intent, InteractiveSessionIntent::PublicWorkspace)
        && matches!(
            current_runtime_session_context().public_workspace_cleanup,
            PublicWorkspaceCleanupOverride::ForceStopOnExit
        )
    {
        return InteractiveSessionOwnership {
            runtime_ownership: crate::runner::interactive_session::RuntimeOwnership::SessionOwned,
            readiness_state:
                crate::runner::interactive_session::SessionReadinessState::CompletedBySession,
            cleanup_policy: crate::runner::interactive_session::CleanupPolicy::StopRuntimeOnExit,
        };
    }

    classify_interactive_session_ownership(
        session_intent,
        system_was_running,
        routes_were_ready_before_handoff,
    )
}

fn combine_workspace_session_results(
    shell_result: Result<String, RunnerError>,
    cleanup_result: Result<(), RunnerError>,
) -> Result<String, RunnerError> {
    match (shell_result, cleanup_result) {
        (Ok(output), Ok(())) => Ok(output),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(shell_error), Err(cleanup_error)) => Err(RunnerError::workspace_session_cleanup(
            shell_error.to_string(),
            cleanup_error.to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::combine_workspace_session_results;
    use crate::runner::error::RunnerError;

    #[test]
    fn combine_workspace_session_results_uses_typed_cleanup_variant() {
        let error = combine_workspace_session_results(
            Err(RunnerError::task_invocation("shell failed")),
            Err(RunnerError::task_invocation("cleanup failed")),
        )
        .expect_err("combined shell and cleanup failure should error");

        match error {
            RunnerError::WorkspaceSessionCleanup {
                shell_error,
                cleanup_error,
            } => {
                assert_eq!(shell_error, "shell failed");
                assert_eq!(cleanup_error, "cleanup failed");
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }
}
