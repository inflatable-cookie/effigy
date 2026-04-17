use super::fixture_support::setup_root_with_catalog_tasks;
use super::runtime::PathBuf;

pub(in crate::runner::tests) fn setup_root_and_catalog_a_catalog(name: &str) -> PathBuf {
    setup_root_with_catalog_tasks(
        name,
        &[("catalog_a", &[("reset-db", "printf catalog_a")])],
        true,
    )
}

pub(in crate::runner::tests) fn json_task_column(
    parsed: &serde_json::Value,
    field: &str,
) -> Vec<String> {
    parsed[field]
        .as_array()
        .expect("json row array")
        .iter()
        .filter_map(|row| row["task"].as_str())
        .map(|task| task.to_owned())
        .collect::<Vec<_>>()
}

pub(in crate::runner::tests) struct ManagedProfileListingCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) profile: &'static str,
    pub(in crate::runner::tests) filter: Option<&'static str>,
    pub(in crate::runner::tests) output_json: bool,
    pub(in crate::runner::tests) expected_field: &'static str,
}
