use std::collections::BTreeMap;
use std::path::Path;

use crate::BuiltinError;
use effigy_managed::run_spec::{render_run_step_sequence, resolve_run_step_env};
use effigy_manifest::config_sections::ManifestJsPackageManager;
use effigy_manifest::task_runtime::ManifestRunStepEnv;
use effigy_manifest::LoadedCatalog;
use effigy_manifest::{
    ManifestCargoEnvMatchMode, ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedRunStep,
    ManifestTestSuiteTeardownPolicy,
};
use effigy_tasks::normalize_builtin_test_suite;

use super::cargo_env::resolve_manifest_cargo_env;

pub(super) struct BuiltinConfiguredSuite {
    pub(super) suite: String,
    pub(super) command: String,
    pub(super) env: BTreeMap<String, String>,
    pub(super) suite_env: Option<String>,
    pub(super) suite_env_files: Vec<String>,
    pub(super) setup_command: Option<String>,
    pub(super) setup_steps: usize,
    pub(super) teardown_command: Option<String>,
    pub(super) teardown_steps: usize,
    pub(super) teardown_policy: ManifestTestSuiteTeardownPolicy,
}

pub(super) struct BuiltinTestTargetConfig {
    pub(super) configured_suites: Vec<BuiltinConfiguredSuite>,
    pub(super) package_manager: Option<ManifestJsPackageManager>,
    pub(super) runner_overrides: BTreeMap<String, String>,
    pub(super) cargo_env: BTreeMap<String, String>,
    pub(super) cargo_env_match: ManifestCargoEnvMatchMode,
}

pub(super) fn resolve_target_test_config(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> Result<BuiltinTestTargetConfig, BuiltinError> {
    let catalog = catalog_for_root(catalogs, target_root);
    let configured_suites = if let Some(entry) = catalog {
        resolve_configured_suites(entry, target_root, catalogs)?
    } else {
        Vec::new()
    };
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
    Ok(BuiltinTestTargetConfig {
        configured_suites,
        package_manager,
        runner_overrides,
        cargo_env,
        cargo_env_match,
    })
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

fn resolve_configured_suites(
    catalog: &LoadedCatalog,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Vec<BuiltinConfiguredSuite>, BuiltinError> {
    let Some(test) = catalog.manifest.test.as_ref() else {
        return Ok(Vec::new());
    };

    test.suites
        .iter()
        .filter_map(|(raw_suite, suite)| {
            suite.run().map(|command| {
                let suite_name = normalize_suite_key(raw_suite);
                let suite_ref = format!("test.suites.{suite_name}");
                let resolved_env = resolve_run_step_env(
                    &suite_ref,
                    suite.env(),
                    suite.env_file(),
                    &catalog.manifest.env,
                    target_root,
                    catalogs,
                    None,
                )?;
                let setup_command = render_suite_lifecycle_sequence(
                    &format!("{suite_ref}.setup"),
                    suite.setup(),
                    &resolved_env,
                    suite.env_file(),
                    &catalog.manifest.env,
                    target_root,
                    catalogs,
                )?;
                let teardown_command = render_suite_lifecycle_sequence(
                    &format!("{suite_ref}.teardown"),
                    suite.teardown(),
                    &resolved_env,
                    suite.env_file(),
                    &catalog.manifest.env,
                    target_root,
                    catalogs,
                )?;
                Ok(BuiltinConfiguredSuite {
                    suite: suite_name,
                    command: command.to_owned(),
                    env: resolved_env,
                    suite_env: render_suite_env_descriptor(suite.env()),
                    suite_env_files: render_suite_env_files(suite.env_file()),
                    setup_command,
                    setup_steps: suite.setup().len(),
                    teardown_command,
                    teardown_steps: suite.teardown().len(),
                    teardown_policy: suite.teardown_policy(),
                })
            })
        })
        .collect::<Result<Vec<BuiltinConfiguredSuite>, BuiltinError>>()
}

fn render_suite_lifecycle_sequence(
    owner_label: &str,
    steps: &[ManifestManagedRunStep],
    task_env: &BTreeMap<String, String>,
    task_env_file: Option<&ManifestEnvFileDirective>,
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, BuiltinError> {
    if steps.is_empty() {
        return Ok(None);
    }

    render_run_step_sequence(
        owner_label,
        steps,
        task_env,
        task_env_file,
        env_profiles,
        target_root,
        catalogs,
        target_root,
        None,
        &effigy_routing::resolve_task_selection,
    )
    .map(Some)
    .map_err(Into::into)
}

fn render_suite_env_descriptor(env: Option<&ManifestRunStepEnv>) -> Option<String> {
    match env {
        Some(ManifestRunStepEnv::Profile(profile)) => Some(format!("profile:{profile}")),
        Some(ManifestRunStepEnv::Inline(entries)) => Some(format!(
            "inline:{}",
            entries.keys().cloned().collect::<Vec<String>>().join(",")
        )),
        None => None,
    }
}

fn render_suite_env_files(env_file: Option<&ManifestEnvFileDirective>) -> Vec<String> {
    match env_file {
        Some(ManifestEnvFileDirective::Single(value)) => vec![value.clone()],
        Some(ManifestEnvFileDirective::Many(values)) => values.clone(),
        None => Vec::new(),
    }
}
