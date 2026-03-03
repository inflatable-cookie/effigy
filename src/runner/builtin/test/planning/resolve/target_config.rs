use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::manifest::ManifestCargoEnvMatchMode;
use crate::runner::util::normalize_builtin_test_suite;
use crate::runner::{LoadedCatalog, ManifestJsPackageManager};

use super::cargo_env::resolve_manifest_cargo_env;

pub(super) struct BuiltinTestTargetConfig {
    pub(super) configured_suites: BTreeMap<String, String>,
    pub(super) package_manager: Option<ManifestJsPackageManager>,
    pub(super) runner_overrides: BTreeMap<String, String>,
    pub(super) cargo_env: BTreeMap<String, String>,
    pub(super) cargo_env_match: ManifestCargoEnvMatchMode,
}

pub(super) fn resolve_target_test_config(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> BuiltinTestTargetConfig {
    let catalog = catalog_for_root(catalogs, target_root);
    let configured_suites = catalog
        .and_then(|entry| entry.manifest.test.as_ref())
        .map(|test| {
            test.suites
                .iter()
                .filter_map(|(raw_suite, suite)| {
                    suite
                        .run()
                        .map(|command| (normalize_suite_key(raw_suite), command.to_owned()))
                })
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default();
    let package_manager =
        catalog.and_then(|entry| entry.manifest.package_manager.as_ref().and_then(|pm| pm.js));
    let runner_overrides = catalog
        .and_then(|entry| entry.manifest.test.as_ref())
        .map(|test| {
            test.runners
                .iter()
                .filter_map(|(raw_runner, override_config)| {
                    override_config
                        .command()
                        .map(|command| (normalize_suite_key(raw_runner), command.to_owned()))
                })
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default();
    let cargo_env = catalog.map(resolve_manifest_cargo_env).unwrap_or_default();
    let cargo_env_match = catalog
        .and_then(|entry| {
            entry
                .manifest
                .test
                .as_ref()
                .map(|test| test.cargo_env_match)
        })
        .unwrap_or_default();
    BuiltinTestTargetConfig {
        configured_suites,
        package_manager,
        runner_overrides,
        cargo_env,
        cargo_env_match,
    }
}

fn catalog_for_root<'a>(
    catalogs: &'a [LoadedCatalog],
    target_root: &Path,
) -> Option<&'a LoadedCatalog> {
    catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == target_root)
}

fn normalize_suite_key(raw: &str) -> String {
    normalize_builtin_test_suite(raw).unwrap_or(raw).to_owned()
}
