use crate::ManifestTaskRunIn;

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestTaskDefaultsConfig {
    #[serde(default)]
    pub run_in: Option<ManifestTaskRunIn>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestIsolationConfig {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestIsolationAdoption {
    pub repo: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestScanOutputFormat {
    Text,
    Markdown,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestShellConfig {
    #[serde(default)]
    pub run: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPackageManagerConfig {
    #[serde(default, alias = "js_ts", alias = "typescript")]
    pub js: Option<ManifestJsPackageManager>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestScanConfig {
    #[serde(default)]
    pub god_files: Option<ManifestGodFilesConfig>,
    #[serde(default)]
    pub boundary_violations: Option<ManifestBoundaryViolationsConfig>,
    #[serde(default)]
    pub dead_code: Option<ManifestDeadCodeConfig>,
    #[serde(default)]
    pub validation_gaps: Option<ManifestValidationGapsConfig>,
    #[serde(default)]
    pub duplicate_blocks: Option<ManifestDuplicateBlocksConfig>,
    #[serde(default)]
    pub comment_ratio: Option<ManifestCommentRatioConfig>,
    #[serde(default)]
    pub generated_assets: Option<ManifestGeneratedAssetsConfig>,
    #[serde(default)]
    pub generated_in_src: Option<ManifestGeneratedInSrcConfig>,
    #[serde(default)]
    pub attention_markers: Option<ManifestAttentionMarkersConfig>,
    #[serde(default)]
    pub stale_suppressions: Option<ManifestStaleSuppressionsConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestBoundaryViolationsConfig {
    #[serde(default)]
    pub include_heuristic: Option<bool>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
    #[serde(default)]
    pub layers: std::collections::BTreeMap<String, ManifestBoundaryLayerConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestBoundaryLayerConfig {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub may_depend_on: Vec<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestDeadCodeConfig {
    #[serde(default)]
    pub include_heuristic: Option<bool>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub allow_paths: Vec<String>,
    #[serde(default)]
    pub allow_symbols: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestValidationGapsConfig {
    #[serde(default)]
    pub include_heuristic: Option<bool>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub hotspot_threshold: Option<usize>,
    #[serde(default)]
    pub affected_depth: Option<usize>,
    #[serde(default)]
    pub allow_paths: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestGodFilesConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestDuplicateBlocksConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub min_occurrences: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestCommentRatioConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<f64>,
    #[serde(default)]
    pub high: Option<f64>,
    #[serde(default)]
    pub critical: Option<f64>,
    #[serde(default)]
    pub min_code_lines: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestGeneratedAssetsConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestGeneratedInSrcConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub warn_bytes: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub high_bytes: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub critical_bytes: Option<usize>,
    #[serde(default)]
    pub source_root: Option<String>,
    #[serde(default)]
    pub source_roots: Vec<String>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestAttentionMarkersConfig {
    #[serde(default)]
    pub warning: Vec<String>,
    #[serde(default)]
    pub high: Vec<String>,
    #[serde(default)]
    pub critical: Vec<String>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestStaleSuppressionsConfig {
    #[serde(default)]
    pub warning: Vec<String>,
    #[serde(default)]
    pub high: Vec<String>,
    #[serde(default)]
    pub critical: Vec<String>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestJsPackageManager {
    Bun,
    Pnpm,
    Npm,
    Direct,
}

impl ManifestJsPackageManager {
    /// Binary name to look up in `PATH` for this manager.
    ///
    /// Returns `None` for `Direct`, which assumes the runner binary
    /// (`vitest` and friends) is already resolvable without an outer
    /// package-manager invocation.
    pub fn binary_name(self) -> Option<&'static str> {
        match self {
            Self::Bun => Some("bun"),
            Self::Pnpm => Some("pnpm"),
            Self::Npm => Some("npm"),
            Self::Direct => None,
        }
    }

    /// Install command for this manager when Effigy needs to hydrate a
    /// JS workspace before running tools from `node_modules/.bin`.
    pub fn install_command(self) -> Option<&'static str> {
        match self {
            Self::Bun => Some("bun install"),
            Self::Pnpm => Some("pnpm install"),
            Self::Npm => Some("npm install"),
            Self::Direct => None,
        }
    }

    /// Vitest invocation for this manager as a `(command, label)` pair.
    ///
    /// The label is used as stable evidence text in plan output, e.g.
    /// `package_manager.js=pnpm`.
    pub fn vitest_command(self) -> (&'static str, &'static str) {
        match self {
            Self::Bun => ("bun x vitest run", "bun"),
            Self::Pnpm => ("pnpm exec vitest run", "pnpm"),
            Self::Npm => ("npx vitest run", "npm"),
            Self::Direct => ("vitest run", "direct"),
        }
    }
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestEnvSchemaConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub exec_timeout: Option<u64>,
}
