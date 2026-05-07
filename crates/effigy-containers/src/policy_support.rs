mod generated_compose;

#[cfg(test)]
pub(crate) use generated_compose::with_test_effigy_home;
pub(crate) use generated_compose::{
    effigy_home_dir, resolve_compose_source, validate_media_mounts,
};
