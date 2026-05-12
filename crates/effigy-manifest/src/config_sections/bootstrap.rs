use crate::ManifestManagedRun;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapConfig {
    #[serde(default, alias = "setup")]
    pub run: Option<ManifestManagedRun>,
    #[serde(default)]
    pub start: Option<ManifestBootstrapStart>,
    #[serde(default)]
    pub submodules: Option<ManifestBootstrapSubmodulesPolicy>,
    #[serde(default)]
    pub children: Vec<ManifestBootstrapChildConfig>,
}

/// Selector(s) to run as the bootstrap start task.
///
/// Accepts a single selector string, or an array of entries where each
/// entry is either a selector string (`"dev"`) or a table form
/// (`{ task = "dev" }`). Mixed arrays are allowed, mirroring the
/// flexibility of `[bootstrap].run`. Arrays run sequentially in
/// declaration order; the first failure aborts the chain.
/// Backward-compatible with the original scalar shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestBootstrapStart {
    Single(String),
    Multiple(Vec<ManifestBootstrapStartEntry>),
}

/// One entry in a multi-selector `[bootstrap].start` array.
///
/// Either a bare selector string (`"dev"`) or a table form carrying a
/// `task` selector (`{ task = "dev" }`). Args still travel inside the
/// selector string itself (e.g. `"dev --foo bar"`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestBootstrapStartEntry {
    Selector(String),
    Table(ManifestBootstrapStartTable),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapStartTable {
    pub task: String,
}

impl ManifestBootstrapStartEntry {
    pub fn selector(&self) -> &str {
        match self {
            Self::Selector(selector) => selector.as_str(),
            Self::Table(table) => table.task.as_str(),
        }
    }
}

impl ManifestBootstrapStart {
    /// Selectors in declaration order.
    pub fn selectors(&self) -> Vec<&str> {
        match self {
            Self::Single(selector) => vec![selector.as_str()],
            Self::Multiple(entries) => entries.iter().map(|entry| entry.selector()).collect(),
        }
    }

    /// Owned copy of all selectors in declaration order.
    pub fn to_owned_selectors(&self) -> Vec<String> {
        self.selectors().into_iter().map(String::from).collect()
    }
}

#[derive(Debug, serde::Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestBootstrapSubmodulesPolicy {
    None,
    Init,
    Recursive,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapChildConfig {
    pub path: String,
    pub repo: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default, alias = "setup")]
    pub run: Option<ManifestManagedRun>,
    #[serde(default = "default_bootstrap_child_required")]
    pub required: bool,
}

fn default_bootstrap_child_required() -> bool {
    true
}
