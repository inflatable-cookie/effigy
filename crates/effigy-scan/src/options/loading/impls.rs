use crate::error::ScanError;
use effigy_manifest::config_sections::{
    ManifestAttentionMarkersConfig, ManifestBoundaryViolationsConfig, ManifestCommentRatioConfig,
    ManifestDeadCodeConfig, ManifestDuplicateBlocksConfig, ManifestGeneratedAssetsConfig,
    ManifestGeneratedInSrcConfig, ManifestGodFilesConfig, ManifestStaleSuppressionsConfig,
    ManifestValidationGapsConfig,
};

use super::super::super::model::{
    AttentionMarkerScanOptions, BoundaryLayerRule, BoundaryViolationScanOptions,
    CommentRatioScanOptions, DeadCodeScanOptions, DuplicateBlockScanOptions,
    GeneratedAssetScanOptions, GeneratedInSrcScanOptions, GodFileScanOptions,
    StaleSuppressionScanOptions, ValidationGapScanOptions,
};
use super::traits::{CommonManifestOptions, ManifestBackedScanOptions};

fn apply_marker_patterns(target: &mut Vec<String>, configured: &[String]) {
    if !configured.is_empty() {
        *target = configured.to_vec();
    }
}

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

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
        self.validate()
    }
}

impl ManifestBackedScanOptions for BoundaryViolationScanOptions {
    type ManifestConfig = ManifestBoundaryViolationsConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_> {
        CommonManifestOptions {
            fail_on_findings: config.fail_on_findings,
            respect_gitignore: None,
            doctor_enabled: config.doctor,
            format: config.format,
            include: &[],
            exclude: &[],
            out: config.out.as_ref(),
        }
    }

    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig) {
        if let Some(value) = config.include_heuristic {
            self.include_heuristic = value;
        }
        self.layers = config
            .layers
            .iter()
            .map(|(name, layer)| {
                (
                    name.clone(),
                    BoundaryLayerRule {
                        paths: layer.paths.clone(),
                        may_depend_on: layer.may_depend_on.clone(),
                    },
                )
            })
            .collect();
    }

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
        self.validate()
    }
}

impl ManifestBackedScanOptions for DeadCodeScanOptions {
    type ManifestConfig = ManifestDeadCodeConfig;

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
        if let Some(value) = config.include_heuristic {
            self.include_heuristic = value;
        }
        self.allow_paths = config.allow_paths.clone();
        self.allow_symbols = config.allow_symbols.clone();
    }

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
        self.validate()
    }
}

impl ManifestBackedScanOptions for ValidationGapScanOptions {
    type ManifestConfig = ManifestValidationGapsConfig;

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
        if let Some(value) = config.include_heuristic {
            self.include_heuristic = value;
        }
        if let Some(value) = config.hotspot_threshold {
            self.hotspot_threshold = value;
        }
        if let Some(value) = config.affected_depth {
            self.affected_depth = value;
        }
        self.allow_paths = config.allow_paths.clone();
    }

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
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

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
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

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
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

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
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

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
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

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
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

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
        self.validate()
    }
}
