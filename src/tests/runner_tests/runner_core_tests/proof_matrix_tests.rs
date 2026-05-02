use crate::runner::bootstrap_command::bootstrap_runtime_session_context;
use crate::runner::interactive_session::{
    classify_interactive_session_ownership, should_cleanup_interactive_session,
    InteractiveSessionIntent,
};
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, PublicWorkspaceCleanupOverride};

#[test]
fn bootstrap_and_workspace_proof_matrix_keeps_public_cleanup_contract_visible() {
    let bootstrap_run = bootstrap_runtime_session_context("bootstrap run");
    assert_eq!(
        bootstrap_run.lease_refresh_policy,
        LeaseRefreshPolicy::SkipRefresh
    );
    assert_eq!(
        bootstrap_run.public_workspace_cleanup,
        PublicWorkspaceCleanupOverride::Default
    );

    let bootstrap_start = bootstrap_runtime_session_context("bootstrap start");
    assert_eq!(
        bootstrap_start.lease_refresh_policy,
        LeaseRefreshPolicy::SkipRefresh
    );
    assert_eq!(
        bootstrap_start.public_workspace_cleanup,
        PublicWorkspaceCleanupOverride::ForceStopOnExit
    );

    let public_started = classify_interactive_session_ownership(
        InteractiveSessionIntent::PublicWorkspace,
        false,
        true,
    );
    let public_ready_adopted = classify_interactive_session_ownership(
        InteractiveSessionIntent::PublicWorkspace,
        true,
        true,
    );
    let seeded_started =
        classify_interactive_session_ownership(InteractiveSessionIntent::SeededTask, false, true);

    assert!(should_cleanup_interactive_session(public_started, true));
    assert!(!should_cleanup_interactive_session(
        public_ready_adopted,
        true
    ));
    assert!(should_cleanup_interactive_session(seeded_started, true));
    assert!(!should_cleanup_interactive_session(seeded_started, false));
}
