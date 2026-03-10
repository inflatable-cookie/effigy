#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::runner) enum ManifestScanOutputFormat {
    Text,
    Markdown,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestShellConfig {
    #[serde(default)]
    pub(in crate::runner) run: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestPackageManagerConfig {
    #[serde(default, alias = "js_ts", alias = "typescript")]
    pub(in crate::runner) js: Option<ManifestJsPackageManager>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestScanConfig {
    #[serde(default)]
    pub(in crate::runner) god_files: Option<ManifestGodFilesConfig>,
    #[serde(default)]
    pub(in crate::runner) duplicate_blocks: Option<ManifestDuplicateBlocksConfig>,
    #[serde(default)]
    pub(in crate::runner) comment_ratio: Option<ManifestCommentRatioConfig>,
    #[serde(default)]
    pub(in crate::runner) generated_assets: Option<ManifestGeneratedAssetsConfig>,
    #[serde(default)]
    pub(in crate::runner) generated_in_src: Option<ManifestGeneratedInSrcConfig>,
    #[serde(default)]
    pub(in crate::runner) attention_markers: Option<ManifestAttentionMarkersConfig>,
    #[serde(default)]
    pub(in crate::runner) stale_suppressions: Option<ManifestStaleSuppressionsConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestGodFilesConfig {
    #[serde(default, alias = "threshold")]
    pub(in crate::runner) warn: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) high: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) critical: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestDuplicateBlocksConfig {
    #[serde(default, alias = "threshold")]
    pub(in crate::runner) warn: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) high: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) critical: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) min_occurrences: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestCommentRatioConfig {
    #[serde(default, alias = "threshold")]
    pub(in crate::runner) warn: Option<f64>,
    #[serde(default)]
    pub(in crate::runner) high: Option<f64>,
    #[serde(default)]
    pub(in crate::runner) critical: Option<f64>,
    #[serde(default)]
    pub(in crate::runner) min_code_lines: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestGeneratedAssetsConfig {
    #[serde(default, alias = "threshold")]
    pub(in crate::runner) warn: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) high: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) critical: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestGeneratedInSrcConfig {
    #[serde(default, alias = "threshold")]
    pub(in crate::runner) warn: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) warn_bytes: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) high: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) high_bytes: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) critical: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) critical_bytes: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) source_root: Option<String>,
    #[serde(default)]
    pub(in crate::runner) source_roots: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestAttentionMarkersConfig {
    #[serde(default)]
    pub(in crate::runner) warning: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) high: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) critical: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestStaleSuppressionsConfig {
    #[serde(default)]
    pub(in crate::runner) warning: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) high: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) critical: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) doctor: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) include: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) exclude: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum ManifestJsPackageManager {
    Bun,
    Pnpm,
    Npm,
    Direct,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestEnvSchemaConfig {
    #[serde(default)]
    pub(in crate::runner) enabled: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) schema: Option<String>,
    #[serde(default)]
    pub(in crate::runner) exec_timeout: Option<u64>,
}
