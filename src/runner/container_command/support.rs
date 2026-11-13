//! Shared support for the container command family.
//!
//! Each piece here is used by more than one command, and each lives in the
//! module that owns its job:
//!
//! - `workspace_preparation`: which workspace preparation steps may run.
//! - `repo_scope`: which checkout a container command acts on.
//! - `container_readiness`: the declared health gate and its refusal.
//! - `runtime_identity`: what the runtime says about this checkout's
//!   containers, and whether its engine answered at all.
//! - `gateway_aliases`: the gateway's TCP aliases inside a container.
//! - `host_ports`: the host ports a manifest pins.
//! - `shared_services`: the shared data services an environment depends on.
//! - `shared_compose`: one shared service's Compose invocations.
//! - `runtime_commands`: one runtime command on the environment's exact
//!   backend, and the volumes it removes.
//! - `report_annotations`: what a command's report says about that work.
//!
//! The re-exports below are this module's surface: a caller keeps naming
//! `support::<item>`, and the item's own module stays free to change.

mod container_readiness;
mod gateway_aliases;
mod host_ports;
mod repo_scope;
mod report_annotations;
mod runtime_commands;
mod runtime_identity;
mod shared_compose;
mod shared_services;
#[cfg(test)]
mod tests;
mod workspace_preparation;

pub(super) use container_readiness::wait_for_container_ready;
pub(super) use gateway_aliases::annotate_registered_gateway_routes;
pub(in crate::runner) use gateway_aliases::reconcile_primary_service_tcp_alias_hosts;
pub(super) use host_ports::ensure_pinned_host_ports_available;
#[cfg(test)]
pub(super) use repo_scope::repo_root_has_effigy_manifest;
pub(super) use repo_scope::{resolve_repo_root_or_invocation_cwd_scope, ContainerRepoScope};
#[cfg(test)]
pub(super) use report_annotations::annotate_left_running_shared_services;
pub(super) use report_annotations::{
    annotate_data_handover_notes, annotate_shared_service_notes, annotate_source_update_notes,
    annotate_tcp_alias_host_notes, annotate_warning_lines,
};
pub(super) use runtime_commands::{
    remove_reset_volumes, run_runtime_volume_capture, stop_and_remove_container,
};
pub(in crate::runner) use runtime_identity::validate_running_container_runtime_match;
pub(super) use runtime_identity::{
    exec_error_means_runtime_not_running, running_container_runtime_mismatch,
};
pub(in crate::runner) use shared_compose::run_shared_service_exec_capture;
pub(in crate::runner) use shared_services::{
    ensure_shared_services_observed, ensure_shared_services_running,
};

#[cfg(test)]
use self::container_readiness::{
    declared_health_refusal, declared_health_refusal_with_evidence, declared_health_tail,
    declared_health_tail_progress, primary_container_progress, primary_service_container_observed,
};
#[cfg(test)]
use self::gateway_aliases::{render_tcp_alias_hosts_script, tcp_alias_host_pairs};
#[cfg(test)]
use self::shared_compose::{run_shared_compose_bounded, run_shared_compose_capture};
#[cfg(test)]
use self::shared_services::{
    ensure_shared_services_with_renders, ensure_shared_services_with_renders_observed,
    shared_service_subject,
};
