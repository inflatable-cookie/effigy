pub(super) use super::super::prelude::cases::*;
pub(super) use super::super::prelude::fixture_support::*;
pub(super) use super::super::prelude::harness_builtin::*;
pub(super) use super::super::prelude::harness_env::*;
pub(super) use super::super::prelude::harness_tasks::*;
pub(super) use super::super::prelude::harness_workspace::*;
pub(super) use super::super::prelude::json::*;
pub(super) use super::super::prelude::output::*;
pub(super) use super::super::prelude::runtime::*;

pub(super) fn setup_root_and_catalog_a_catalog(name: &str) -> PathBuf {
    setup_root_with_catalog_tasks(
        name,
        &[("catalog_a", &[("reset-db", "printf catalog_a")])],
        true,
    )
}

pub(super) fn json_task_column(parsed: &serde_json::Value, field: &str) -> Vec<String> {
    parsed[field]
        .as_array()
        .expect("json row array")
        .iter()
        .filter_map(|row| row["task"].as_str())
        .map(|task| task.to_owned())
        .collect::<Vec<_>>()
}

pub(super) struct ManagedProfileListingCase {
    pub(super) workspace: &'static str,
    pub(super) profile: &'static str,
    pub(super) filter: Option<&'static str>,
    pub(super) output_json: bool,
    pub(super) expected_field: &'static str,
}
