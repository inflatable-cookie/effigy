use std::path::Path;

use super::super::LoadedCatalog;

pub(in crate::runner) use super::completion::test_support::{
    parse_completion_contract_request, CompletionParseContract,
};
pub(in crate::runner) use super::config::test_support::{
    parse_config_contract_request, ConfigParseContract,
};
pub(in crate::runner) use super::unlock::test_support::parse_unlock_contract_request;
pub(in crate::runner) use super::watch::test_support::parse_watch_contract_request;

pub(in crate::runner) fn builtin_test_max_parallel(
    catalogs: &[LoadedCatalog],
    resolved_root: &Path,
) -> usize {
    super::test::builtin_test_max_parallel(catalogs, resolved_root)
}
