mod generated_compose;

#[cfg(any(test, feature = "test-support"))]
pub use generated_compose::with_test_effigy_home;
pub(crate) use generated_compose::{
    effigy_home_dir, resolve_compose_source, validate_media_mounts,
};
