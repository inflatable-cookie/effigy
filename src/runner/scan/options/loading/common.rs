use std::path::Path;

use crate::runner::error::RunnerError;
use crate::runner::manifest::TaskManifest;
use crate::runner::model::{catalog::LoadedCatalog, constants::TASK_MANIFEST_FILE};

use super::traits::{CommonManifestOptions, CommonScanOptionsMut, ManifestBackedScanOptions};

fn apply_common_manifest_options<T>(options: &mut T, config: CommonManifestOptions<'_>)
where
    T: CommonScanOptionsMut,
{
    if let Some(value) = config.fail_on_findings {
        *options.fail_on_findings_mut() = value;
    }
    if let Some(value) = config.respect_gitignore {
        *options.respect_gitignore_mut() = value;
    }
    if let Some(value) = config.doctor_enabled {
        *options.doctor_enabled_mut() = value;
    }
    if let Some(value) = config.format {
        *options.format_mut() = value.into();
    }
    *options.include_mut() = config.include.to_vec();
    *options.exclude_mut() = config.exclude.to_vec();
    *options.out_mut() = config.out.cloned();
}

fn build_manifest_backed_options<T>(config: Option<&T::ManifestConfig>) -> Result<T, RunnerError>
where
    T: ManifestBackedScanOptions,
{
    let mut options = T::default();
    if let Some(config) = config {
        options.apply_manifest_specific(config);
        apply_common_manifest_options(&mut options, T::common_manifest_options(config));
    }
    options.validate_manifest_options()?;
    Ok(options)
}

pub(super) fn load_root_manifest_options<T, F>(
    target_root: &Path,
    select: F,
) -> Result<T, RunnerError>
where
    T: ManifestBackedScanOptions,
    F: FnOnce(&TaskManifest) -> Option<&T::ManifestConfig>,
{
    load_root_scan_options(target_root, T::default, |manifest| {
        build_manifest_backed_options(select(manifest))
    })
}

pub(super) fn doctor_manifest_options<T, F>(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    select: F,
) -> Result<T, RunnerError>
where
    T: ManifestBackedScanOptions,
    F: FnOnce(&TaskManifest) -> Option<&T::ManifestConfig>,
{
    build_manifest_backed_options(catalog_scan_manifest(resolved_root, catalogs).and_then(select))
}

fn load_root_scan_options<T, FDefault, FBuild>(
    target_root: &Path,
    default: FDefault,
    build: FBuild,
) -> Result<T, RunnerError>
where
    FDefault: FnOnce() -> T,
    FBuild: FnOnce(&TaskManifest) -> Result<T, RunnerError>,
{
    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    if !manifest_path.exists() {
        return Ok(default());
    }
    let manifest = load_scan_manifest(&manifest_path)?;
    build(&manifest)
}

fn catalog_scan_manifest<'a>(
    resolved_root: &Path,
    catalogs: &'a [LoadedCatalog],
) -> Option<&'a TaskManifest> {
    catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == resolved_root)
        .map(|catalog| &catalog.manifest)
}

fn load_scan_manifest(manifest_path: &Path) -> Result<TaskManifest, RunnerError> {
    let manifest_text =
        std::fs::read_to_string(manifest_path).map_err(|error| RunnerError::TaskManifestRead {
            path: manifest_path.to_path_buf(),
            error,
        })?;
    toml::from_str::<TaskManifest>(&manifest_text).map_err(|error| RunnerError::TaskManifestParse {
        path: manifest_path.to_path_buf(),
        error,
    })
}
