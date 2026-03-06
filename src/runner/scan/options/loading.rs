use std::path::{Path, PathBuf};

use crate::runner::error::RunnerError;
use crate::runner::manifest::config_sections::ManifestDuplicateBlocksConfig;
use crate::runner::manifest::config_sections::ManifestGeneratedInSrcConfig;
use crate::runner::manifest::{
    config_sections::{
        ManifestAttentionMarkersConfig, ManifestCommentRatioConfig, ManifestGeneratedAssetsConfig,
        ManifestGodFilesConfig, ManifestScanOutputFormat, ManifestStaleSuppressionsConfig,
    },
    TaskManifest,
};
use crate::runner::model::{catalog::LoadedCatalog, constants::TASK_MANIFEST_FILE};

use super::super::model::{
    AttentionMarkerScanOptions, CommentRatioScanOptions, DuplicateBlockScanOptions,
    GeneratedAssetScanOptions, GeneratedInSrcScanOptions, GodFileScanOptions, ScanRenderFormat,
    StaleSuppressionScanOptions,
};

pub(in crate::runner) fn load_root_god_file_options(
    target_root: &Path,
) -> Result<GodFileScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.god_files.as_ref())
    })
}

pub(in crate::runner) fn load_root_generated_asset_options(
    target_root: &Path,
) -> Result<GeneratedAssetScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_assets.as_ref())
    })
}

pub(in crate::runner) fn load_root_generated_in_src_options(
    target_root: &Path,
) -> Result<GeneratedInSrcScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_in_src.as_ref())
    })
}

pub(in crate::runner) fn load_root_duplicate_block_options(
    target_root: &Path,
) -> Result<DuplicateBlockScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.duplicate_blocks.as_ref())
    })
}

pub(in crate::runner) fn load_root_comment_ratio_options(
    target_root: &Path,
) -> Result<CommentRatioScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.comment_ratio.as_ref())
    })
}

pub(in crate::runner) fn load_root_attention_marker_options(
    target_root: &Path,
) -> Result<AttentionMarkerScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.attention_markers.as_ref())
    })
}

pub(in crate::runner) fn load_root_stale_suppression_options(
    target_root: &Path,
) -> Result<StaleSuppressionScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.stale_suppressions.as_ref())
    })
}

pub(in crate::runner) fn doctor_attention_marker_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<AttentionMarkerScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.attention_markers.as_ref())
    })
}

pub(in crate::runner) fn doctor_stale_suppression_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<StaleSuppressionScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.stale_suppressions.as_ref())
    })
}

pub(in crate::runner) fn doctor_generated_asset_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GeneratedAssetScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_assets.as_ref())
    })
}

pub(in crate::runner) fn doctor_generated_in_src_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GeneratedInSrcScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_in_src.as_ref())
    })
}

pub(in crate::runner) fn doctor_duplicate_block_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<DuplicateBlockScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.duplicate_blocks.as_ref())
    })
}

pub(in crate::runner) fn doctor_comment_ratio_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<CommentRatioScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.comment_ratio.as_ref())
    })
}

pub(in crate::runner) fn doctor_god_file_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GodFileScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.god_files.as_ref())
    })
}

pub(in crate::runner) fn catalog_scan_roots(
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<PathBuf> {
    let mut roots = catalogs
        .iter()
        .filter(|catalog| {
            catalog.catalog_root == target_root || catalog.catalog_root.starts_with(target_root)
        })
        .map(|catalog| catalog.catalog_root.clone())
        .collect::<Vec<PathBuf>>();
    roots.sort();
    roots.dedup();
    if roots.is_empty() {
        roots.push(target_root.to_path_buf());
    }
    roots
}

fn apply_marker_patterns(target: &mut Vec<String>, configured: &[String]) {
    if !configured.is_empty() {
        *target = configured.to_vec();
    }
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

fn load_root_manifest_options<T, F>(target_root: &Path, select: F) -> Result<T, RunnerError>
where
    T: ManifestBackedScanOptions,
    F: FnOnce(&TaskManifest) -> Option<&T::ManifestConfig>,
{
    load_root_scan_options(target_root, T::default, |manifest| {
        build_manifest_backed_options(select(manifest))
    })
}

fn doctor_manifest_options<T, F>(
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

trait CommonScanOptionsMut {
    fn fail_on_findings_mut(&mut self) -> &mut bool;
    fn respect_gitignore_mut(&mut self) -> &mut bool;
    fn doctor_enabled_mut(&mut self) -> &mut bool;
    fn include_mut(&mut self) -> &mut Vec<String>;
    fn exclude_mut(&mut self) -> &mut Vec<String>;
    fn format_mut(&mut self) -> &mut ScanRenderFormat;
    fn out_mut(&mut self) -> &mut Option<String>;
}

trait ManifestBackedScanOptions: CommonScanOptionsMut + Default {
    type ManifestConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_>;
    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig);
    fn validate_manifest_options(&self) -> Result<(), RunnerError>;
}

struct CommonManifestOptions<'a> {
    fail_on_findings: Option<bool>,
    respect_gitignore: Option<bool>,
    doctor_enabled: Option<bool>,
    format: Option<ManifestScanOutputFormat>,
    include: &'a [String],
    exclude: &'a [String],
    out: Option<&'a String>,
}

macro_rules! impl_common_scan_options_mut {
    ($ty:ty) => {
        impl CommonScanOptionsMut for $ty {
            fn fail_on_findings_mut(&mut self) -> &mut bool {
                &mut self.fail_on_findings
            }

            fn respect_gitignore_mut(&mut self) -> &mut bool {
                &mut self.respect_gitignore
            }

            fn doctor_enabled_mut(&mut self) -> &mut bool {
                &mut self.doctor_enabled
            }

            fn include_mut(&mut self) -> &mut Vec<String> {
                &mut self.include
            }

            fn exclude_mut(&mut self) -> &mut Vec<String> {
                &mut self.exclude
            }

            fn format_mut(&mut self) -> &mut ScanRenderFormat {
                &mut self.format
            }

            fn out_mut(&mut self) -> &mut Option<String> {
                &mut self.out
            }
        }
    };
}

impl_common_scan_options_mut!(GodFileScanOptions);
impl_common_scan_options_mut!(DuplicateBlockScanOptions);
impl_common_scan_options_mut!(CommentRatioScanOptions);
impl_common_scan_options_mut!(GeneratedAssetScanOptions);
impl_common_scan_options_mut!(GeneratedInSrcScanOptions);
impl_common_scan_options_mut!(AttentionMarkerScanOptions);
impl_common_scan_options_mut!(StaleSuppressionScanOptions);

impl ManifestBackedScanOptions for GodFileScanOptions {
    type ManifestConfig = ManifestGodFilesConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_> {
        CommonManifestOptions {
            fail_on_findings: config.fail_on_findings,
            respect_gitignore: config.respect_gitignore,
            doctor_enabled: config.doctor,
            format: config.format,
            include: &config.include,
            exclude: &config.exclude,
            out: config.out.as_ref(),
        }
    }

    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig) {
        if let Some(value) = config.warn {
            self.thresholds.warn = value;
        }
        if let Some(value) = config.high {
            self.thresholds.high = value;
        }
        if let Some(value) = config.critical {
            self.thresholds.critical = value;
        }
    }

    fn validate_manifest_options(&self) -> Result<(), RunnerError> {
        self.validate()
    }
}

impl ManifestBackedScanOptions for DuplicateBlockScanOptions {
    type ManifestConfig = ManifestDuplicateBlocksConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_> {
        CommonManifestOptions {
            fail_on_findings: config.fail_on_findings,
            respect_gitignore: config.respect_gitignore,
            doctor_enabled: config.doctor,
            format: config.format,
            include: &config.include,
            exclude: &config.exclude,
            out: config.out.as_ref(),
        }
    }

    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig) {
        if let Some(value) = config.warn {
            self.thresholds.warn = value;
        }
        if let Some(value) = config.high {
            self.thresholds.high = value;
        }
        if let Some(value) = config.critical {
            self.thresholds.critical = value;
        }
        if let Some(value) = config.min_occurrences {
            self.thresholds.min_occurrences = value;
        }
    }

    fn validate_manifest_options(&self) -> Result<(), RunnerError> {
        self.validate()
    }
}

impl ManifestBackedScanOptions for CommentRatioScanOptions {
    type ManifestConfig = ManifestCommentRatioConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_> {
        CommonManifestOptions {
            fail_on_findings: config.fail_on_findings,
            respect_gitignore: config.respect_gitignore,
            doctor_enabled: config.doctor,
            format: config.format,
            include: &config.include,
            exclude: &config.exclude,
            out: config.out.as_ref(),
        }
    }

    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig) {
        if let Some(value) = config.warn {
            self.thresholds.warn = value;
        }
        if let Some(value) = config.high {
            self.thresholds.high = value;
        }
        if let Some(value) = config.critical {
            self.thresholds.critical = value;
        }
        if let Some(value) = config.min_code_lines {
            self.thresholds.min_code_lines = value;
        }
    }

    fn validate_manifest_options(&self) -> Result<(), RunnerError> {
        self.validate()
    }
}

impl ManifestBackedScanOptions for GeneratedAssetScanOptions {
    type ManifestConfig = ManifestGeneratedAssetsConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_> {
        CommonManifestOptions {
            fail_on_findings: config.fail_on_findings,
            respect_gitignore: config.respect_gitignore,
            doctor_enabled: config.doctor,
            format: config.format,
            include: &config.include,
            exclude: &config.exclude,
            out: config.out.as_ref(),
        }
    }

    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig) {
        if let Some(value) = config.warn {
            self.thresholds.warn = value;
        }
        if let Some(value) = config.high {
            self.thresholds.high = value;
        }
        if let Some(value) = config.critical {
            self.thresholds.critical = value;
        }
    }

    fn validate_manifest_options(&self) -> Result<(), RunnerError> {
        self.validate()
    }
}

impl ManifestBackedScanOptions for GeneratedInSrcScanOptions {
    type ManifestConfig = ManifestGeneratedInSrcConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_> {
        CommonManifestOptions {
            fail_on_findings: config.fail_on_findings,
            respect_gitignore: config.respect_gitignore,
            doctor_enabled: config.doctor,
            format: config.format,
            include: &config.include,
            exclude: &config.exclude,
            out: config.out.as_ref(),
        }
    }

    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig) {
        if let Some(value) = config.warn.or(config.warn_bytes) {
            self.thresholds.warn = value;
        }
        if let Some(value) = config.high.or(config.high_bytes) {
            self.thresholds.high = value;
        }
        if let Some(value) = config.critical.or(config.critical_bytes) {
            self.thresholds.critical = value;
        }
        if let Some(value) = config.source_root.as_ref() {
            self.source_roots = vec![value.clone()];
        }
        if !config.source_roots.is_empty() {
            self.source_roots = config.source_roots.clone();
        }
    }

    fn validate_manifest_options(&self) -> Result<(), RunnerError> {
        self.validate()
    }
}

impl ManifestBackedScanOptions for AttentionMarkerScanOptions {
    type ManifestConfig = ManifestAttentionMarkersConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_> {
        CommonManifestOptions {
            fail_on_findings: config.fail_on_findings,
            respect_gitignore: config.respect_gitignore,
            doctor_enabled: config.doctor,
            format: config.format,
            include: &config.include,
            exclude: &config.exclude,
            out: config.out.as_ref(),
        }
    }

    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig) {
        apply_marker_patterns(&mut self.patterns.warning, &config.warning);
        apply_marker_patterns(&mut self.patterns.high, &config.high);
        apply_marker_patterns(&mut self.patterns.critical, &config.critical);
    }

    fn validate_manifest_options(&self) -> Result<(), RunnerError> {
        self.validate()
    }
}

impl ManifestBackedScanOptions for StaleSuppressionScanOptions {
    type ManifestConfig = ManifestStaleSuppressionsConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_> {
        CommonManifestOptions {
            fail_on_findings: config.fail_on_findings,
            respect_gitignore: config.respect_gitignore,
            doctor_enabled: config.doctor,
            format: config.format,
            include: &config.include,
            exclude: &config.exclude,
            out: config.out.as_ref(),
        }
    }

    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig) {
        apply_marker_patterns(&mut self.patterns.warning, &config.warning);
        apply_marker_patterns(&mut self.patterns.high, &config.high);
        apply_marker_patterns(&mut self.patterns.critical, &config.critical);
    }

    fn validate_manifest_options(&self) -> Result<(), RunnerError> {
        self.validate()
    }
}
