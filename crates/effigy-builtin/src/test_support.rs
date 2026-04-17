//! Test-support fixtures re-exported for runner-side integration tests.
//!
//! Consumed via `src/runner/test_support.rs` by the runner's
//! cross-module tests. Items originate in each subsystem's own
//! `test_support` sibling (`completion`, `config`, `unlock`, `watch`,
//! `test`).

use std::path::Path;

use effigy_manifest::LoadedCatalog;

pub use super::completion::test_support::{
    parse_completion_contract_request, CompletionParseContract,
};
pub use super::config::test_support::{parse_config_contract_request, ConfigParseContract};
pub use super::unlock::test_support::parse_unlock_contract_request;
pub use super::watch::test_support::parse_watch_contract_request;

pub fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    super::test::builtin_test_max_parallel(catalogs, resolved_root)
}
