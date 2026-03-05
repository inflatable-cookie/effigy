use std::path::Path;

use super::super::LoadedCatalog;

pub(in crate::runner) use super::completion::parse_completion_contract_request;
pub(in crate::runner) use super::completion::CompletionParseContract;
pub(in crate::runner) use super::config::parse_config_contract_request;
pub(in crate::runner) use super::config::ConfigParseContract;
pub(in crate::runner) use super::unlock::parse_unlock_contract_request;
pub(in crate::runner) use super::watch::parse_watch_contract_request;

pub(in crate::runner) fn builtin_test_max_parallel(
    catalogs: &[LoadedCatalog],
    resolved_root: &Path,
) -> usize {
    super::test::builtin_test_max_parallel(catalogs, resolved_root)
}
