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
        classify_interactive_session_ownership, CleanupPolicy, InteractiveSessionIntent,
        RuntimeOwnership, SessionReadinessState,
    };

    #[test]
    fn interactive_ownership_matrix_stays_stable_for_workspace_and_seeded_shells() {
        let cases = [
            (
                InteractiveSessionIntent::PublicWorkspace,
                false,
                true,
                RuntimeOwnership::SessionOwned,
                SessionReadinessState::CompletedBySession,
                CleanupPolicy::StopRuntimeOnExit,
            ),
            (
                InteractiveSessionIntent::PublicWorkspace,
                true,
                true,
                RuntimeOwnership::Adopted,
                SessionReadinessState::AlreadyReady,
                CleanupPolicy::PreserveRuntime,
            ),
            (
                InteractiveSessionIntent::PublicWorkspace,
                true,
                false,
                RuntimeOwnership::Adopted,
                SessionReadinessState::CompletedBySession,
                CleanupPolicy::StopRuntimeOnExit,
            ),
            (
                InteractiveSessionIntent::SeededTask,
                false,
                true,
                RuntimeOwnership::SessionOwned,
                SessionReadinessState::CompletedBySession,
                CleanupPolicy::StopRuntimeOnSuccessfulExit,
            ),
            (
                InteractiveSessionIntent::SeededTask,
                true,
                false,
                RuntimeOwnership::Adopted,
                SessionReadinessState::CompletedBySession,
                CleanupPolicy::PreserveRuntime,
            ),
        ];

        for (
            intent,
            system_was_running,
            routes_were_ready_before_entry,
            runtime_ownership,
            readiness_state,
            cleanup_policy,
        ) in cases
        {
            let ownership = classify_interactive_session_ownership(
                intent,
                system_was_running,
                routes_were_ready_before_entry,
            );
            assert_eq!(ownership.runtime_ownership, runtime_ownership);
            assert_eq!(ownership.readiness_state, readiness_state);
            assert_eq!(ownership.cleanup_policy, cleanup_policy);
        }
    }
}
