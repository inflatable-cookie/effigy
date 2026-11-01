mod colima_cleanup;
mod colima_runtime;
mod compose_progress;
mod compose_steps;
mod healthcheck_pidfiles;
mod implementation;
mod managed_stop;
mod mounts;
mod parse;
mod participation;
pub(crate) use participation::docker_config_current_context;
#[cfg(all(test, unix))]
#[path = "exec/exit141_tests.rs"]
mod exit141_tests;
#[cfg(all(test, unix))]
#[path = "exec/tests/interpolation.rs"]
mod interpolation_tests;
#[cfg(all(test, unix))]
#[path = "exec/name_store_tests.rs"]
mod name_store_tests;
mod process;
pub(crate) use process::error_is_timeout;
mod ssh_agent_preflight;

pub use colima_runtime::{
    colima_is_running, colima_is_running_with_deadline, colima_profile_warnings,
    ensure_colima_running, ensure_colima_running_report, ensure_runtime_backend_running,
    ensure_runtime_backend_running_report, recover_colima_runtime,
    recover_colima_runtime_with_progress, reset_colima_runtime, reset_colima_runtime_with_progress,
    running_colima_profiles, runtime_backend_is_running, selected_backend_label,
    ColimaEnsureReport, ColimaRecoveryPhase, ColimaRecoveryReport,
};
pub use compose_progress::{
    parse_compose_step_event, service_from_container_name, ComposeStepEvent,
};
pub use compose_steps::{runtime_wait, ComposeSteps, TimedStep};
pub use healthcheck_pidfiles::{
    parse_health_check_pidfile_prune, prune_stale_health_check_pidfiles, HealthCheckPidFilePrune,
    HEALTHCHECK_PIDFILE_PATTERN, HEALTHCHECK_PIDFILE_STALE_MINUTES, HEALTHCHECK_TASK_ROOT,
};
pub use implementation::{
    capture_compose_ps, capture_running_container_stats,
    capture_running_container_stats_for_profile, compose_stack_is_running,
    container_last_log_line_for_policy, decision_project_inventory,
    discover_compose_containers_all_states, discover_running_compose_containers,
    infer_host_working_dir_for_container, inspect_containers_for_policy, inspect_image_for_policy,
    list_compose_containers_for_policy_including_stopped,
    list_compose_containers_for_policy_including_stopped_with_deadline,
    list_compose_containers_for_policy_including_stopped_with_timeout,
    list_compose_containers_for_project_including_stopped, list_running_compose_containers,
    list_running_compose_containers_for_policy,
    list_running_compose_containers_for_policy_with_deadline,
    list_running_compose_containers_for_policy_with_timeout,
    list_running_compose_containers_for_profile,
    list_running_compose_containers_for_project_with_deadline,
    list_running_compose_containers_profiled, owned_service_listing_args, run_backend_invocation,
    run_backend_invocation_private_until, run_backend_invocation_until, run_compose_capture,
    run_compose_capture_with_env, run_compose_invocation_capture,
    run_compose_invocation_capture_with_env, run_compose_invocation_streaming_with_env,
    run_docker_capture, set_restart_policy, shared_project_containers, shared_render_identity,
    shutdown_container, verify_owned_running_compose_identities, verify_owned_running_compose_rows,
    verify_shared_render_container, verify_workspace_artifact_builder,
    verify_workspace_artifact_builder_declaration, ContainerExecError, OwnershipContext,
    RunningComposeContainerInventory, RuntimeInventoryFailure, SharedProjectContainer,
    VerifiedComposeContainer,
};
pub use managed_stop::{end_owned_container_process_group, verify_owned_stack_for_shutdown};
pub use mounts::ensure_isolated_mount_host_points;
pub use parse::{
    compose_status_is_running, parse_running_compose_containers, RunningComposeContainer,
    RunningComposeContainerProfiled, RunningContainerStats, RunningContainerStatsCapture,
};
pub use process::{
    no_containers_diagnostic, retained_backend_log_path, run_command_capture,
    run_command_capture_allow_failure, run_input_capture_until,
};
pub use ssh_agent_preflight::{
    inspect_colima_ssh_agent_socket, inspect_colima_ssh_agent_socket_for_profile,
    inspect_colima_ssh_agent_socket_for_profile_with_deadline, SshAgentSocketHealth,
};
