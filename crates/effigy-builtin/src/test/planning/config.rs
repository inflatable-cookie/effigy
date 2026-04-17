use std::path::Path;

use crate::constants::DEFAULT_BUILTIN_TEST_MAX_PARALLEL;
use effigy_manifest::LoadedCatalog;

pub(super) fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    let configured = catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == resolved_root)
        .find_map(|catalog| {
            catalog
                .manifest
                .test
                .as_ref()
                .and_then(|test| test.max_parallel)
        })
        .filter(|value| *value > 0);

    configured.unwrap_or(DEFAULT_BUILTIN_TEST_MAX_PARALLEL)
}
