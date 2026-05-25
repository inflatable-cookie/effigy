use crate::error::ScanError;
use effigy_manifest::config_sections::ManifestScanOutputFormat;

use super::super::super::model::{
    AttentionMarkerScanOptions, BoundaryViolationScanOptions, CommentRatioScanOptions,
    DeadCodeScanOptions, DuplicateBlockScanOptions, GeneratedAssetScanOptions,
    GeneratedInSrcScanOptions, GodFileScanOptions, ScanRenderFormat, StaleSuppressionScanOptions,
    ValidationGapScanOptions,
};

pub(super) trait CommonScanOptionsMut {
    fn fail_on_findings_mut(&mut self) -> &mut bool;
    fn respect_gitignore_mut(&mut self) -> &mut bool;
    fn doctor_enabled_mut(&mut self) -> &mut bool;
    fn include_mut(&mut self) -> &mut Vec<String>;
    fn exclude_mut(&mut self) -> &mut Vec<String>;
    fn format_mut(&mut self) -> &mut ScanRenderFormat;
    fn out_mut(&mut self) -> &mut Option<String>;
}

pub(super) trait ManifestBackedScanOptions: CommonScanOptionsMut + Default {
    type ManifestConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_>;
    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig);
    fn validate_manifest_options(&self) -> Result<(), ScanError>;
}

pub(super) struct CommonManifestOptions<'a> {
    pub(super) fail_on_findings: Option<bool>,
    pub(super) respect_gitignore: Option<bool>,
    pub(super) doctor_enabled: Option<bool>,
    pub(super) format: Option<ManifestScanOutputFormat>,
    pub(super) include: &'a [String],
    pub(super) exclude: &'a [String],
    pub(super) out: Option<&'a String>,
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
impl_common_scan_options_mut!(BoundaryViolationScanOptions);
impl_common_scan_options_mut!(DeadCodeScanOptions);
impl_common_scan_options_mut!(DuplicateBlockScanOptions);
impl_common_scan_options_mut!(CommentRatioScanOptions);
impl_common_scan_options_mut!(GeneratedAssetScanOptions);
impl_common_scan_options_mut!(GeneratedInSrcScanOptions);
impl_common_scan_options_mut!(AttentionMarkerScanOptions);
impl_common_scan_options_mut!(StaleSuppressionScanOptions);
impl_common_scan_options_mut!(ValidationGapScanOptions);
