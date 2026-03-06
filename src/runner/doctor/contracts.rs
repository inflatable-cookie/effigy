#[path = "contracts/metadata.rs"]
mod metadata;

pub(in crate::runner) use metadata::{
    check_id, install_tool, remediation, schema_supported_value, ALL_CHECK_IDS,
};
