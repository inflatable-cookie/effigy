mod colima_runtime;
mod compose_progress;
mod compose_steps;
mod healthcheck_pidfiles;
mod healthcheck_timer;
mod implementation;
mod managed_stop;
mod mounts;
mod parse;
mod participation;
pub(crate) use participation::docker_config_current_context;
#[cfg(all(test, unix))]
#[path = "exec/exit141_tests.rs"]
mod exit141_tests;
mod process;
mod restart_recovery;
mod ssh_agent_preflight;

pub use colima_runtime::{
    colima_is_running, colima_is_running_with_deadline, colima_profile_warnings,
    ensure_colima_running, ensure_colima_running_report, ensure_runtime_backend_running,
    ensure_runtime_backend_running_report, recover_colima_runtime, reset_colima_runtime,
    running_colima_profiles, runtime_backend_is_running, selected_backend_label,
    ColimaEnsureReport, ColimaRecoveryReport,
};
pub use compose_progress::{
    parse_compose_step_event, service_from_container_name, ComposeStepEvent,
};
pub use compose_steps::{phase, ComposeSteps, TimedStep};
pub use healthcheck_pidfiles::{
    inspect_run_inode_pressure, parse_health_check_pidfile_prune, parse_run_inode_pressure,
    prune_stale_health_check_pidfiles, HealthCheckPidFilePrune, RunInodePressure,
    HEALTHCHECK_PIDFILE_PATTERN, HEALTHCHECK_PIDFILE_STALE_MINUTES, HEALTHCHECK_TASK_ROOT,
    RUN_INODE_PRESSURE_PERCENT, STALE_PIDFILE_WARNING_COUNT,
};
pub use implementation::{
    capture_compose_ps, capture_running_container_stats,
    capture_running_container_stats_for_profile, compose_stack_is_running,
    container_last_log_line_for_policy, discover_compose_containers_all_states,
    discover_running_compose_containers, infer_host_working_dir_for_container,
    inspect_containers_for_policy, list_compose_containers_for_policy_including_stopped,
    list_compose_containers_for_policy_including_stopped_with_deadline,
    list_compose_containers_for_policy_including_stopped_with_timeout,
    list_compose_containers_for_project_including_stopped, list_running_compose_containers,
    list_running_compose_containers_for_policy,
    list_running_compose_containers_for_policy_with_deadline,
    list_running_compose_containers_for_policy_with_timeout,
    list_running_compose_containers_for_profile,
    list_running_compose_containers_for_project_with_deadline,
    list_running_compose_containers_profiled, owned_service_listing_args,
    reconcile_stale_owned_containers, remove_stale_owned_containers, run_backend_invocation,
    run_compose_capture, run_compose_invocation_capture, run_compose_invocation_capture_with_env,
    run_compose_invocation_streaming_with_env, run_docker_capture,
    running_stack_admits_reactivation, shutdown_container, verify_owned_running_compose_identities,
    verify_owned_running_compose_rows, verify_workspace_artifact_builder,
    verify_workspace_artifact_builder_declaration, ContainerExecError, OwnershipContext,
    ReactivationAdmission, RunningComposeContainerInventory, RuntimeInventoryFailure,
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
pub use restart_recovery::{
    recover_exited_owned_compose_services, recover_exited_owned_compose_services_for_project,
    OwnedServiceStartRecovery,
};
pub use ssh_agent_preflight::{
    inspect_colima_ssh_agent_socket, inspect_colima_ssh_agent_socket_for_profile,
    inspect_colima_ssh_agent_socket_for_profile_with_deadline, SshAgentSocketHealth,
};
