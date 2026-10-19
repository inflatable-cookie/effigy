mod generated_compose;

#[cfg(any(test, feature = "test-support"))]
pub use generated_compose::with_test_effigy_home;
#[cfg(test)]
pub(crate) use generated_compose::with_test_loopback_inventory;
pub(crate) use generated_compose::{
    effigy_home_dir, parse_media_mount, prepare_container_policy_loopbacks, resolve_compose_source,
    shared_service_output_dir, validate_media_mounts,
};
