use std::path::{Path, PathBuf};

use crate::runner::interactive_session::{
    classify_interactive_session_ownership, InteractiveSessionIntent, InteractiveSessionOwnership,
};
use crate::runner::runtime_session_context::{
    current_runtime_session_context, LeaseRefreshPolicy, PublicWorkspaceCleanupOverride,
    RuntimeSessionContext,
};

/// The workspace entry's front ends (contract 052 §8.11). The TUI, a plain text
/// client and a headless one are front ends of one managed session: they differ
/// only in how they render it, never in which run they talk to. The
/// workspace-client glue task drives them; until it lands they are proven by
/// `effigy_kernel::workspace_session::tests` and this file's own tests.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the workspace-client glue task drives these front ends"
    )
)]
pub(super) mod front_end {
    use effigy_kernel::workspace_session::{SessionHandle, SessionRef, TerminalKind};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum WorkspaceFrontEnd {
        Tui,
        Text,
        Headless,
    }

    impl WorkspaceFrontEnd {
        pub(super) fn terminal_kind(self) -> TerminalKind {
            match self {
                Self::Tui => TerminalKind::Tui,
                Self::Text => TerminalKind::Text,
                Self::Headless => TerminalKind::Headless,
            }
        }

        /// The one line a front end prints for the session it entered. Every
        /// kind prints the same session identity, so a UI and a text log agree.
        pub(super) fn session_line(self, session: &SessionRef) -> String {
            format!("{}: {}", self.terminal_kind().as_str(), session.label())
        }
    }

    /// An interactive terminal with no initial command is the TUI; an
    /// interactive terminal running a command is a text client; a workspace
    /// entry with no terminal at all is headless.
    pub(super) fn workspace_front_end(
        interactive: bool,
        initial_command: Option<&str>,
    ) -> WorkspaceFrontEnd {
        match (interactive, initial_command) {
            (false, _) => WorkspaceFrontEnd::Headless,
            (true, Some(_)) => WorkspaceFrontEnd::Text,
            (true, None) => WorkspaceFrontEnd::Tui,
        }
    }

    /// The handle a front end drives: durable session identity and journalled
    /// requests only, never the supervisor's owning pipe. A front end can close
    /// at any time without settling the session.
    pub(super) fn workspace_session_handle(session: SessionRef) -> SessionHandle {
        SessionHandle::new(session)
    }
}

use super::RunnerError;

pub(super) fn run_workspace_container_session(
    repo_root: &Path,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    initial_command: Option<&str>,
    session_intent: InteractiveSessionIntent,
    cleanup_override: Option<PublicWorkspaceCleanupOverride>,
) -> Result<String, RunnerError> {
    use effigy_runtime_plan::RuntimeActivationRoute;

    let repo_override = super::effective_workspace_repo_override(repo_root, repo_override);
    let container_name = container_name.map(str::to_owned);
    let policy = super::load_workspace_session_policy(repo_root, container_name.as_deref())?;
    let routes_were_ready_before_handoff =
        super::gateway_routes_ready_before_handoff(repo_root, &policy)?;
    let activation = crate::runner::container_runtime_prep::activate_container_runtime_for_task(
        repo_root,
        &policy,
        crate::runner::container_runtime_prep::ActivationRequest {
            container_name: container_name.as_deref(),
            repo_override: repo_override.clone(),
            route: RuntimeActivationRoute::Workspace,
            session_context: workspace_activation_session_context(),
            reconcile_shared_services:
                crate::runner::container_runtime_prep::reconcile_shared_services_for(
                    &policy,
                    workspace_activation_session_context(),
                ),
        },
    )?;
    let crate::runner::container_runtime_prep::ContainerTaskActivation {
        policy,
        system_was_running,
        hold,
        ..
    } = activation;
    super::finish_workspace_handoff_after_activation(
        repo_root,
        &policy,
        container_name.as_deref(),
        repo_override.clone(),
        initial_command,
        routes_were_ready_before_handoff,
    )?;
    let ownership = classify_workspace_session_ownership(
        session_intent,
        system_was_running,
        routes_were_ready_before_handoff,
        cleanup_override,
    );
    let shell_result =
        super::run_workspace_handoff_shell(repo_root, container_name.as_deref(), initial_command);
    // The session's use of the environment ends with its shell. Settle the
    // hold first, so the closeout stop is not refused by this command's own
    // session (contract 053 §4).
    drop(hold);
    let cleanup_result = super::cleanup_workspace_session(
        ownership,
        shell_result.is_ok(),
        &policy.name,
        container_name,
        repo_override,
    );

    combine_workspace_session_results(shell_result, cleanup_result)
}

fn workspace_activation_session_context() -> RuntimeSessionContext {
    RuntimeSessionContext {
        lease_refresh_policy: LeaseRefreshPolicy::SkipRefresh,
        public_workspace_cleanup: current_runtime_session_context().public_workspace_cleanup,
        data_seed_session: current_runtime_session_context().data_seed_session,
    }
}

pub(super) fn classify_workspace_session_ownership(
    session_intent: InteractiveSessionIntent,
    system_was_running: bool,
    routes_were_ready_before_handoff: bool,
    cleanup_override: Option<PublicWorkspaceCleanupOverride>,
) -> InteractiveSessionOwnership {
    let cleanup_override =
        cleanup_override.unwrap_or(current_runtime_session_context().public_workspace_cleanup);
    if matches!(
        cleanup_override,
        PublicWorkspaceCleanupOverride::ForceStopOnExit
    ) {
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
    use super::front_end::{workspace_front_end, workspace_session_handle, WorkspaceFrontEnd};
    use crate::runner::error::RunnerError;
    use effigy_kernel::id::{CheckoutId, CheckoutKind, Generation, ResourceId, RunId};
    use effigy_kernel::run::Fence;
    use effigy_kernel::workspace_session::{SessionRef, TerminalKind};

    fn session() -> SessionRef {
        let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, [5; 16]);
        let environment = ResourceId::environment(checkout, "stack").expect("environment");
        let run = RunId::new(1, [3; 10]).expect("run id");
        SessionRef::new(
            run,
            Fence {
                resource: environment.clone(),
                generation: Generation::new(7),
            },
            environment,
            Generation::new(4),
        )
    }

    #[test]
    fn front_ends_map_to_their_terminal_kind() {
        assert_eq!(
            workspace_front_end(true, None).terminal_kind(),
            TerminalKind::Tui
        );
        assert_eq!(
            workspace_front_end(true, Some("effigy test")).terminal_kind(),
            TerminalKind::Text
        );
        assert_eq!(
            workspace_front_end(false, None).terminal_kind(),
            TerminalKind::Headless
        );
    }

    #[test]
    fn every_front_end_reports_one_managed_session_identity() {
        let session = session();
        let lines: Vec<String> = [
            WorkspaceFrontEnd::Tui,
            WorkspaceFrontEnd::Text,
            WorkspaceFrontEnd::Headless,
        ]
        .iter()
        .map(|front_end| front_end.session_line(&session))
        .collect();
        for line in &lines {
            assert!(
                line.ends_with(&session.label()),
                "`{line}` names the same session"
            );
        }
        let identities: Vec<String> = lines
            .iter()
            .map(|line| {
                line.split_once(": ")
                    .expect("kind and identity")
                    .1
                    .to_owned()
            })
            .collect();
        assert!(identities.iter().all(|identity| identity == &identities[0]));
        let handle = workspace_session_handle(session.clone());
        assert_eq!(handle.identity(), &session);
    }

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
