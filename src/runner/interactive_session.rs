#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum RuntimeOwnership {
    SessionOwned,
    Adopted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum SessionReadinessState {
    AlreadyReady,
    CompletedBySession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum CleanupPolicy {
    PreserveRuntime,
    StopRuntimeOnExit,
    StopRuntimeOnSuccessfulExit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum InteractiveSessionIntent {
    PublicWorkspace,
    SeededTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) struct InteractiveSessionOwnership {
    pub(in crate::runner) runtime_ownership: RuntimeOwnership,
    pub(in crate::runner) readiness_state: SessionReadinessState,
    pub(in crate::runner) cleanup_policy: CleanupPolicy,
}

pub(in crate::runner) fn classify_interactive_session_ownership(
    intent: InteractiveSessionIntent,
    system_was_running: bool,
    routes_were_ready_before_entry: bool,
) -> InteractiveSessionOwnership {
    let runtime_ownership = if system_was_running {
        RuntimeOwnership::Adopted
    } else {
        RuntimeOwnership::SessionOwned
    };
    let readiness_state = if system_was_running && routes_were_ready_before_entry {
        SessionReadinessState::AlreadyReady
    } else {
        SessionReadinessState::CompletedBySession
    };
    let cleanup_policy = match intent {
        InteractiveSessionIntent::PublicWorkspace => {
            if system_was_running && routes_were_ready_before_entry {
                CleanupPolicy::PreserveRuntime
            } else {
                CleanupPolicy::StopRuntimeOnExit
            }
        }
        InteractiveSessionIntent::SeededTask => {
            if system_was_running {
                CleanupPolicy::PreserveRuntime
            } else {
                CleanupPolicy::StopRuntimeOnSuccessfulExit
            }
        }
    };

    InteractiveSessionOwnership {
        runtime_ownership,
        readiness_state,
        cleanup_policy,
    }
}

pub(in crate::runner) fn should_cleanup_interactive_session(
    ownership: InteractiveSessionOwnership,
    session_succeeded: bool,
) -> bool {
    match ownership.cleanup_policy {
        CleanupPolicy::PreserveRuntime => false,
        CleanupPolicy::StopRuntimeOnExit => true,
        CleanupPolicy::StopRuntimeOnSuccessfulExit => session_succeeded,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_interactive_session_ownership, should_cleanup_interactive_session, CleanupPolicy,
        InteractiveSessionIntent, RuntimeOwnership, SessionReadinessState,
    };

    #[test]
    fn public_workspace_classifies_started_runtime_as_session_owned() {
        let ownership = classify_interactive_session_ownership(
            InteractiveSessionIntent::PublicWorkspace,
            false,
            true,
        );

        assert_eq!(ownership.runtime_ownership, RuntimeOwnership::SessionOwned);
        assert_eq!(
            ownership.readiness_state,
            SessionReadinessState::CompletedBySession
        );
        assert_eq!(ownership.cleanup_policy, CleanupPolicy::StopRuntimeOnExit);
        assert!(should_cleanup_interactive_session(ownership, true));
        assert!(should_cleanup_interactive_session(ownership, false));
    }

    #[test]
    fn public_workspace_classifies_ready_adopted_runtime_as_preserved() {
        let ownership = classify_interactive_session_ownership(
            InteractiveSessionIntent::PublicWorkspace,
            true,
            true,
        );

        assert_eq!(ownership.runtime_ownership, RuntimeOwnership::Adopted);
        assert_eq!(
            ownership.readiness_state,
            SessionReadinessState::AlreadyReady
        );
        assert_eq!(ownership.cleanup_policy, CleanupPolicy::PreserveRuntime);
        assert!(!should_cleanup_interactive_session(ownership, true));
    }

    #[test]
    fn public_workspace_classifies_route_incomplete_adopted_runtime_as_session_prepared() {
        let ownership = classify_interactive_session_ownership(
            InteractiveSessionIntent::PublicWorkspace,
            true,
            false,
        );

        assert_eq!(ownership.runtime_ownership, RuntimeOwnership::Adopted);
        assert_eq!(
            ownership.readiness_state,
            SessionReadinessState::CompletedBySession
        );
        assert_eq!(ownership.cleanup_policy, CleanupPolicy::StopRuntimeOnExit);
        assert!(should_cleanup_interactive_session(ownership, false));
    }

    #[test]
    fn seeded_task_classifies_started_runtime_as_success_only_cleanup() {
        let ownership = classify_interactive_session_ownership(
            InteractiveSessionIntent::SeededTask,
            false,
            true,
        );

        assert_eq!(ownership.runtime_ownership, RuntimeOwnership::SessionOwned);
        assert_eq!(
            ownership.readiness_state,
            SessionReadinessState::CompletedBySession
        );
        assert_eq!(
            ownership.cleanup_policy,
            CleanupPolicy::StopRuntimeOnSuccessfulExit
        );
        assert!(should_cleanup_interactive_session(ownership, true));
        assert!(!should_cleanup_interactive_session(ownership, false));
    }

    #[test]
    fn seeded_task_preserves_adopted_runtime_even_if_routes_were_completed_here() {
        let ownership = classify_interactive_session_ownership(
            InteractiveSessionIntent::SeededTask,
            true,
            false,
        );

        assert_eq!(ownership.runtime_ownership, RuntimeOwnership::Adopted);
        assert_eq!(
            ownership.readiness_state,
            SessionReadinessState::CompletedBySession
        );
        assert_eq!(ownership.cleanup_policy, CleanupPolicy::PreserveRuntime);
        assert!(!should_cleanup_interactive_session(ownership, true));
    }
}
