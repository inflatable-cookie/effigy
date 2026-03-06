use std::path::{Path, PathBuf};

use super::{
    AttentionMarkerPatterns, AttentionMarkerScanOptions, GeneratedAssetScanOptions,
    GeneratedAssetThresholds, GodFileScanOptions, GodFileThresholds, LoadedCatalog,
    ManifestAttentionMarkersConfig, ManifestGeneratedAssetsConfig, ManifestGodFilesConfig,
    RunnerError, ScanRenderFormat, TaskManifest, DEFAULT_ATTENTION_MARKER_CRITICAL,
    DEFAULT_ATTENTION_MARKER_HIGH, DEFAULT_ATTENTION_MARKER_WARNING,
    DEFAULT_GENERATED_ASSETS_CRITICAL_BYTES, DEFAULT_GENERATED_ASSETS_HIGH_BYTES,
    DEFAULT_GENERATED_ASSETS_WARN_BYTES, DEFAULT_GOD_FILES_CRITICAL, DEFAULT_GOD_FILES_HIGH,
    DEFAULT_GOD_FILES_WARN, TASK_MANIFEST_FILE,
};

impl Default for GodFileScanOptions {
    fn default() -> Self {
        Self {
            thresholds: GodFileThresholds {
                warn: DEFAULT_GOD_FILES_WARN,
                high: DEFAULT_GOD_FILES_HIGH,
                critical: DEFAULT_GOD_FILES_CRITICAL,
            },
            fail_on_findings: false,
            respect_gitignore: true,
            doctor_enabled: true,
            include: Vec::new(),
            exclude: Vec::new(),
            format: ScanRenderFormat::Text,
            out: None,
        }
    }
}

impl Default for GeneratedAssetScanOptions {
    fn default() -> Self {
        Self {
            thresholds: GeneratedAssetThresholds {
                warn: DEFAULT_GENERATED_ASSETS_WARN_BYTES,
                high: DEFAULT_GENERATED_ASSETS_HIGH_BYTES,
                critical: DEFAULT_GENERATED_ASSETS_CRITICAL_BYTES,
            },
            fail_on_findings: false,
            respect_gitignore: true,
            doctor_enabled: true,
            include: Vec::new(),
            exclude: Vec::new(),
            format: ScanRenderFormat::Text,
            out: None,
        }
    }
}

impl Default for AttentionMarkerScanOptions {
    fn default() -> Self {
        Self {
            patterns: AttentionMarkerPatterns {
                warning: DEFAULT_ATTENTION_MARKER_WARNING
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
                high: DEFAULT_ATTENTION_MARKER_HIGH
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
                critical: DEFAULT_ATTENTION_MARKER_CRITICAL
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            },
            fail_on_findings: false,
            respect_gitignore: true,
            doctor_enabled: true,
            include: Vec::new(),
            exclude: Vec::new(),
            format: ScanRenderFormat::Text,
            out: None,
        }
    }
}

impl GodFileScanOptions {
    pub(super) fn from_manifest(
        config: Option<&ManifestGodFilesConfig>,
    ) -> Result<Self, RunnerError> {
        let mut options = Self::default();
        if let Some(config) = config {
            if let Some(value) = config.warn {
                options.thresholds.warn = value;
            }
            if let Some(value) = config.high {
                options.thresholds.high = value;
            }
            if let Some(value) = config.critical {
                options.thresholds.critical = value;
            }
            if let Some(value) = config.fail_on_findings {
                options.fail_on_findings = value;
            }
            if let Some(value) = config.respect_gitignore {
                options.respect_gitignore = value;
            }
            if let Some(value) = config.doctor {
                options.doctor_enabled = value;
            }
            if let Some(value) = config.format {
                options.format = value.into();
            }
            options.include = config.include.clone();
            options.exclude = config.exclude.clone();
            options.out = config.out.clone();
        }
        options.validate()?;
        Ok(options)
    }

    pub(in crate::runner) fn validate(&self) -> Result<(), RunnerError> {
        if self.thresholds.warn == 0 || self.thresholds.high == 0 || self.thresholds.critical == 0 {
            return Err(RunnerError::task_invocation(
                "`scan.god_files` thresholds must be greater than zero",
            ));
        }
        if self.thresholds.warn > self.thresholds.high
            || self.thresholds.high > self.thresholds.critical
        {
            return Err(RunnerError::task_invocation(
                "`scan.god_files` thresholds must be ordered `warn <= high <= critical`",
            ));
        }
        Ok(())
    }
}

impl GeneratedAssetScanOptions {
    pub(super) fn from_manifest(
        config: Option<&ManifestGeneratedAssetsConfig>,
    ) -> Result<Self, RunnerError> {
        let mut options = Self::default();
        if let Some(config) = config {
            if let Some(value) = config.warn {
                options.thresholds.warn = value;
            }
            if let Some(value) = config.high {
                options.thresholds.high = value;
            }
            if let Some(value) = config.critical {
                options.thresholds.critical = value;
            }
            if let Some(value) = config.fail_on_findings {
                options.fail_on_findings = value;
            }
            if let Some(value) = config.respect_gitignore {
                options.respect_gitignore = value;
            }
            if let Some(value) = config.doctor {
                options.doctor_enabled = value;
            }
            if let Some(value) = config.format {
                options.format = value.into();
            }
            options.include = config.include.clone();
            options.exclude = config.exclude.clone();
            options.out = config.out.clone();
        }
        options.validate()?;
        Ok(options)
    }

    pub(in crate::runner) fn validate(&self) -> Result<(), RunnerError> {
        if self.thresholds.warn == 0 || self.thresholds.high == 0 || self.thresholds.critical == 0 {
            return Err(RunnerError::task_invocation(
                "`scan.generated_assets` thresholds must be greater than zero",
            ));
        }
        if self.thresholds.warn > self.thresholds.high
            || self.thresholds.high > self.thresholds.critical
        {
            return Err(RunnerError::task_invocation(
                "`scan.generated_assets` thresholds must be ordered `warn <= high <= critical`",
            ));
        }
        Ok(())
    }
}

impl AttentionMarkerScanOptions {
    pub(super) fn from_manifest(
        config: Option<&ManifestAttentionMarkersConfig>,
    ) -> Result<Self, RunnerError> {
        let mut options = Self::default();
        if let Some(config) = config {
            if !config.warning.is_empty() {
                options.patterns.warning = config.warning.clone();
            }
            if !config.high.is_empty() {
                options.patterns.high = config.high.clone();
            }
            if !config.critical.is_empty() {
                options.patterns.critical = config.critical.clone();
            }
            if let Some(value) = config.fail_on_findings {
                options.fail_on_findings = value;
            }
            if let Some(value) = config.respect_gitignore {
                options.respect_gitignore = value;
            }
            if let Some(value) = config.doctor {
                options.doctor_enabled = value;
            }
            if let Some(value) = config.format {
                options.format = value.into();
            }
            options.include = config.include.clone();
            options.exclude = config.exclude.clone();
            options.out = config.out.clone();
        }
        options.validate()?;
        Ok(options)
    }

    pub(in crate::runner) fn validate(&self) -> Result<(), RunnerError> {
        let total_patterns =
            self.patterns.warning.len() + self.patterns.high.len() + self.patterns.critical.len();
        if total_patterns == 0 {
            return Err(RunnerError::task_invocation(
                "`scan.attention_markers` requires at least one configured marker",
            ));
        }
        if self
            .patterns
            .warning
            .iter()
            .chain(self.patterns.high.iter())
            .chain(self.patterns.critical.iter())
            .any(|value| value.trim().is_empty())
        {
            return Err(RunnerError::task_invocation(
                "`scan.attention_markers` markers must be non-empty strings",
            ));
        }
        Ok(())
    }
}

pub(in crate::runner) fn load_root_god_file_options(
    target_root: &Path,
) -> Result<GodFileScanOptions, RunnerError> {
    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    if !manifest_path.exists() {
        return Ok(GodFileScanOptions::default());
    }
    let manifest = load_scan_manifest(&manifest_path)?;
    GodFileScanOptions::from_manifest(
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.god_files.as_ref()),
    )
}

pub(in crate::runner) fn load_root_generated_asset_options(
    target_root: &Path,
) -> Result<GeneratedAssetScanOptions, RunnerError> {
    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    if !manifest_path.exists() {
        return Ok(GeneratedAssetScanOptions::default());
    }
    let manifest = load_scan_manifest(&manifest_path)?;
    GeneratedAssetScanOptions::from_manifest(
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_assets.as_ref()),
    )
}

pub(in crate::runner) fn load_root_attention_marker_options(
    target_root: &Path,
) -> Result<AttentionMarkerScanOptions, RunnerError> {
    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    if !manifest_path.exists() {
        return Ok(AttentionMarkerScanOptions::default());
    }
    let manifest = load_scan_manifest(&manifest_path)?;
    AttentionMarkerScanOptions::from_manifest(
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.attention_markers.as_ref()),
    )
}

pub(in crate::runner) fn doctor_attention_marker_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<AttentionMarkerScanOptions, RunnerError> {
    let config = catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == resolved_root)
        .and_then(|catalog| catalog.manifest.scan.as_ref())
        .and_then(|scan| scan.attention_markers.as_ref());
    AttentionMarkerScanOptions::from_manifest(config)
}

pub(in crate::runner) fn doctor_generated_asset_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GeneratedAssetScanOptions, RunnerError> {
    let config = catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == resolved_root)
        .and_then(|catalog| catalog.manifest.scan.as_ref())
        .and_then(|scan| scan.generated_assets.as_ref());
    GeneratedAssetScanOptions::from_manifest(config)
}

pub(in crate::runner) fn doctor_god_file_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GodFileScanOptions, RunnerError> {
    let config = catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == resolved_root)
        .and_then(|catalog| catalog.manifest.scan.as_ref())
        .and_then(|scan| scan.god_files.as_ref());
    GodFileScanOptions::from_manifest(config)
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
