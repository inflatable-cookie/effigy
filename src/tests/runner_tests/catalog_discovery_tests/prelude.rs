pub(super) use super::super::prelude::catalog::*;
pub(super) use super::super::prelude::fixture_support::*;
pub(super) use super::super::prelude::harness_builtin::*;
pub(super) use super::super::prelude::harness_env::*;
pub(super) use super::super::prelude::harness_workspace::*;
pub(super) use super::super::prelude::output::*;
pub(super) use super::super::prelude::runtime::*;

pub(super) fn assert_task_ambiguous_reset_db(err: RunnerError) {
    match err {
        RunnerError::TaskAmbiguous { name, candidates } => {
            assert_eq!(name, "reset-db");
            assert_eq!(candidates.len(), 2);
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(super) fn assert_catalog_alias_conflict(err: RunnerError, expected_alias: &str) {
    match err {
        RunnerError::TaskCatalogAliasConflict {
            alias,
            first_path,
            second_path,
        } => {
            assert_eq!(alias, expected_alias);
            assert!(first_path.ends_with("effigy.toml"));
            assert!(second_path.ends_with("effigy.toml"));
            assert_ne!(first_path, second_path);
        }
        other => panic!("unexpected error: {other}"),
    }
}
