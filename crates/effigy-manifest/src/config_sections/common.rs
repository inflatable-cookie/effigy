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
    pub duplicate_blocks: Option<ManifestDuplicateBlocksConfig>,
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
