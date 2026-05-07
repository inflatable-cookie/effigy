mod colima_runtime;
mod implementation;
mod parse;
mod process;

pub use colima_runtime::{
    colima_is_running, colima_profile_warnings, ensure_colima_running,
    ensure_runtime_backend_running, recover_colima_runtime, reset_colima_runtime,
    running_colima_profiles, runtime_backend_is_running, selected_backend_label,
    ColimaRecoveryReport,
};
pub use implementation::{
    capture_compose_ps, capture_running_container_stats,
    capture_running_container_stats_for_profile, infer_host_working_dir_for_container,
    list_running_compose_containers, list_running_compose_containers_for_policy,
    list_running_compose_containers_for_profile, list_running_compose_containers_profiled,
    run_compose_capture, run_compose_invocation_capture, run_docker_capture, shutdown_container,
    ContainerExecError,
};
pub use parse::{
    RunningComposeContainer, RunningComposeContainerProfiled, RunningContainerStats,
    RunningContainerStatsCapture,
};
pub use process::{run_command_capture, run_command_capture_allow_failure};
