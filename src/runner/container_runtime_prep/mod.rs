//! Preparing a container-backed run, and the read-only observations that prove
//! the stack a run is about to use is the one this checkout owns.
//!
//! Each job has one owner, and this module is only the surface they share:
//!
//! - [`activation`] builds the runtime activation plan, takes the environment
//!   hold, and runs the stages in the order a routed task needs them.
//! - [`exec_prep`] is the exec side of that order: the one apply, then exec
//!   readiness, then alias reconciliation.
//! - [`prep`] names the individual stages and keeps their effects injectable;
//!   [`running`], [`gateway`] and [`validation`] own three of those decisions.
//! - [`binding`] proves the owned binding of one running service.
//! - [`checkout_mount`] and [`cargo_inputs`] prove the selected checkout is
//!   what the container will execute against.
//! - [`host_mounts`] prepares the host side of Compose bind mounts.
//! - [`image_platform`] and [`predecessor`] are the preflight observations, and
//!   [`sealed_render`] with [`compose_hash`] own the sealed render whose bytes
//!   they read.

mod activation;
mod binding;
mod cargo_inputs;
mod checkout_mount;
mod compose_hash;
mod exec_prep;
mod gateway;
mod host_mounts;
mod image_platform;
mod predecessor;
mod prep;
mod running;
mod sealed_render;
mod validation;

pub(in crate::runner) use activation::{
    activate_container_runtime_for_task, activate_routed_container_runtime,
    build_runtime_activation_plan, reconcile_shared_services_for, ActivationRequest,
    ContainerTaskActivation,
};
#[cfg(test)]
pub(in crate::runner) use activation::activate_routed_container_runtime_with;
pub(in crate::runner) use binding::{
    ensure_primary_service_binding, verify_service_binding, OWNED_EXEC_VERIFICATION_TIMEOUT,
};
pub(in crate::runner) use gateway::container_policy_uses_gateway_surface;
pub(in crate::runner) use image_platform::{observed_image_platform, ObservedImagePlatform};
pub(in crate::runner) use predecessor::{
    observed_predecessor_compatibility, PredecessorCompatibility,
};
#[cfg(test)]
pub(in crate::runner) use predecessor::{
    observed_predecessor_compatibility_with, PredecessorBackend,
};
pub(in crate::runner) use sealed_render::{seal_render_for_preflight, SealedRender};
pub(in crate::runner) use crate::runner::workspace_command::provisioning::BoundedReadRefusal;

#[cfg(test)]
use activation::{
    activate_container_runtime_plan_for_task_using, ensure_exec_container_running_with,
    runtime_activation_plan_from_request, runtime_activation_report_for_result,
};
#[cfg(test)]
use cargo_inputs::refresh_changed_cargo_inputs_using;
#[cfg(test)]
use checkout_mount::verify_live_checkout_mount_using;
#[cfg(test)]
use compose_hash::{
    canonical_compose_model, compose_config_args, compose_config_output,
    compose_service_config_hashes, parse_service_config_hashes, verify_frozen_compose_reads,
};
#[cfg(test)]
use exec_prep::{apply_shared_services, run_task_apply};
#[cfg(test)]
use gateway::ensure_runtime_gateway_readiness_stage_using;
#[cfg(test)]
use host_mounts::{
    candidate_host_mount_paths, parse_bind_mount_host_path, prepare_host_bind_mount_dirs,
};
#[cfg(test)]
use image_platform::{image_reference, parse_image_platform};
#[cfg(test)]
use predecessor::inspected_config_hash;
#[cfg(test)]
use prep::{run_runtime_compose_up_stage, run_runtime_prep_steps};
#[cfg(test)]
use running::ensure_runtime_running_stage;
#[cfg(test)]
use sealed_render::verified_compose_inputs;
#[cfg(test)]
use validation::validate_policy_runtime;

#[cfg(test)]
mod tests;
