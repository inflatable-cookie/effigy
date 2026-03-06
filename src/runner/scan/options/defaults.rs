use super::super::constants::{
    DEFAULT_ATTENTION_MARKER_CRITICAL, DEFAULT_ATTENTION_MARKER_HIGH,
    DEFAULT_ATTENTION_MARKER_WARNING, DEFAULT_COMMENT_RATIO_CRITICAL, DEFAULT_COMMENT_RATIO_HIGH,
    DEFAULT_COMMENT_RATIO_MIN_CODE_LINES, DEFAULT_COMMENT_RATIO_WARN,
    DEFAULT_DUPLICATE_BLOCKS_CRITICAL, DEFAULT_DUPLICATE_BLOCKS_HIGH,
    DEFAULT_DUPLICATE_BLOCKS_MIN_OCCURRENCES, DEFAULT_DUPLICATE_BLOCKS_WARN,
    DEFAULT_GENERATED_ASSETS_CRITICAL_BYTES, DEFAULT_GENERATED_ASSETS_HIGH_BYTES,
    DEFAULT_GENERATED_ASSETS_WARN_BYTES, DEFAULT_GOD_FILES_CRITICAL, DEFAULT_GOD_FILES_HIGH,
    DEFAULT_GOD_FILES_WARN,
};
use super::super::model::{
    AttentionMarkerPatterns, AttentionMarkerScanOptions, CommentRatioScanOptions,
    CommentRatioThresholds, DuplicateBlockScanOptions, DuplicateBlockThresholds,
    GeneratedAssetScanOptions, GeneratedAssetThresholds, GodFileScanOptions, GodFileThresholds,
    ScanRenderFormat,
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

impl Default for CommentRatioScanOptions {
    fn default() -> Self {
        Self {
            thresholds: CommentRatioThresholds {
                warn: DEFAULT_COMMENT_RATIO_WARN,
                high: DEFAULT_COMMENT_RATIO_HIGH,
                critical: DEFAULT_COMMENT_RATIO_CRITICAL,
                min_code_lines: DEFAULT_COMMENT_RATIO_MIN_CODE_LINES,
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
                warning: default_marker_patterns(DEFAULT_ATTENTION_MARKER_WARNING),
                high: default_marker_patterns(DEFAULT_ATTENTION_MARKER_HIGH),
                critical: default_marker_patterns(DEFAULT_ATTENTION_MARKER_CRITICAL),
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

impl Default for DuplicateBlockScanOptions {
    fn default() -> Self {
        Self {
            thresholds: DuplicateBlockThresholds {
                warn: DEFAULT_DUPLICATE_BLOCKS_WARN,
                high: DEFAULT_DUPLICATE_BLOCKS_HIGH,
                critical: DEFAULT_DUPLICATE_BLOCKS_CRITICAL,
                min_occurrences: DEFAULT_DUPLICATE_BLOCKS_MIN_OCCURRENCES,
            },
            fail_on_findings: false,
            respect_gitignore: true,
            doctor_enabled: false,
            include: Vec::new(),
            exclude: Vec::new(),
            format: ScanRenderFormat::Text,
            out: None,
        }
    }
}

fn default_marker_patterns(defaults: &[&str]) -> Vec<String> {
    defaults.iter().map(|value| (*value).to_owned()).collect()
}
