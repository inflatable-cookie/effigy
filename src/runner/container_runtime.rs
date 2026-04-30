pub(in crate::runner) const CONTAINER_HANDOFF_ENV_NAME: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF";
pub(in crate::runner) const CONTAINER_HANDOFF_ENV_ASSIGNMENT: &str =
    "EFFIGY_INTERNAL_CONTAINER_HANDOFF=1";

pub(in crate::runner) fn inside_container_handoff() -> bool {
    std::env::var_os(CONTAINER_HANDOFF_ENV_NAME).is_some()
}
